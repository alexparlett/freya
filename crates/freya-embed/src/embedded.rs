use std::{
    cell::RefCell,
    rc::Rc,
    task::Waker,
};

use freya_clipboard::prelude::{
    Clipboard,
    ClipboardProvider,
};
use freya_components::{
    cache::AssetCacher,
    integration::integration,
};
use freya_core::{
    integration::*,
    prelude::{
        Color,
        CursorIcon,
        IntoElement,
        MouseButton,
        PreferredTheme,
        ScreenReader,
        TargetPlatform,
    },
};
use freya_engine::prelude::Canvas;
use futures_util::{
    FutureExt,
    StreamExt,
    select,
};
use ragnarok::{
    EventsExecutorRunner,
    EventsMeasurerRunner,
    NodesState,
};
use torin::prelude::{
    Point2D,
    Size2D,
};

use crate::Fonts;

/// How a host starts an [`Embedded`] app.
pub struct EmbedConfig {
    pub app: AppComponent,
    /// The drawing area in physical pixels.
    pub size: Size2D,
    pub scale_factor: f64,
    /// The accessibility label of the root node.
    pub name: String,
    /// Woken whenever the app has work: an event handler, a task, a timer, a state change.
    /// The host answers by calling [`Embedded::update`] from its own loop.
    pub waker: Waker,
    pub preferred_theme: PreferredTheme,
    pub clipboard: Option<Box<dyn ClipboardProvider>>,
}

/// What an [`Embedded::update`] changed, for the host to act on.
#[derive(Default)]
pub struct Update {
    /// The tree must be drawn again.
    pub redraw: bool,
    /// The accessibility tree changed; present when [`Embedded::set_screen_reader`] is on.
    pub accessibility: Option<accesskit::TreeUpdate>,
    /// The pointer's icon over the app changed.
    pub cursor: Option<CursorIcon>,
}

/// A Freya app driven by its host: the host feeds it events, calls [`Self::update`] when its waker
/// fires and when it gave it events, draws it into a canvas of its own, and reports each present.
pub struct Embedded {
    runner: Runner,
    tree: Tree,
    nodes_state: NodesState<NodeId>,
    accessibility: AccessibilityTree,
    platform: Platform,
    ticker: RenderingTickerSender,
    animation_clock: AnimationClock,
    screen_reader: ScreenReader,
    events_sender: futures_channel::mpsc::UnboundedSender<EventsChunk>,
    events_receiver: futures_channel::mpsc::UnboundedReceiver<EventsChunk>,
    user_events: Rc<RefCell<Vec<UserEvent>>>,
    waker: Waker,
    name: String,
    size: Size2D,
    cursor: CursorIcon,
    pointer: Point2D,
    layout_dirty: bool,
    accessibility_dirty: bool,
    accessibility_mode: Option<NavigationMode>,
    needs_render: bool,
}

impl Embedded {
    /// `provide` runs before the first render, for the host's own root contexts.
    pub fn new(config: EmbedConfig, fonts: &mut Fonts, provide: impl FnOnce(&mut Runner)) -> Self {
        let (events_sender, events_receiver) = futures_channel::mpsc::unbounded();
        let app = config.app;
        let mut runner = Runner::new(move || integration(app.clone()).into_element());

        let screen_reader = runner.provide_root_context(ScreenReader::new);
        runner.provide_root_context(|| {
            let global_contexts = GlobalContexts::default();
            global_contexts.insert_context(Clipboard::create(config.clipboard));
            global_contexts
        });
        let (ticker, ticker_receiver) = RenderingTicker::new();
        runner.provide_root_context(|| ticker_receiver);
        let animation_clock = runner.provide_root_context(AnimationClock::new);
        runner.provide_root_context(AssetCacher::create);
        runner.provide_root_context(TargetPlatform::detect);

        let user_events = Rc::new(RefCell::new(Vec::new()));
        let scale_factor = config.scale_factor;
        let size = config.size;
        let platform = runner.provide_root_context({
            let user_events = user_events.clone();
            move || Platform {
                focused_accessibility_id: State::create(ACCESSIBILITY_ROOT_ID),
                focused_accessibility_node: State::create(accesskit::Node::new(
                    accesskit::Role::Window,
                )),
                root_size: State::create(logical(size, scale_factor)),
                // An embedded surface is placed by its host, never moved, filled or made
                // fullscreen as far as the app can tell.
                window_position: State::create(Point2D::default()),
                is_maximized: State::create(false),
                is_fullscreen: State::create(false),
                scale_factor: State::create(scale_factor),
                custom_scale_factor: State::create(1.),
                navigation_mode: State::create(NavigationMode::NotKeyboard),
                preferred_theme: State::create(config.preferred_theme),
                is_app_focused: State::create(false),
                accent_color: State::create(AccentColor::default()),
                sender: Rc::new(move |event| user_events.borrow_mut().push(event)),
            }
        });

        let tree = Tree::default();
        runner.provide_root_context(|| tree.accessibility_generator.clone());
        runner.provide_root_context(|| fonts.collection.clone());
        provide(&mut runner);

        let mut embedded = Self {
            runner,
            tree,
            nodes_state: NodesState::default(),
            accessibility: AccessibilityTree::default(),
            platform,
            ticker,
            animation_clock,
            screen_reader,
            events_sender,
            events_receiver,
            user_events,
            waker: config.waker,
            name: config.name,
            size,
            cursor: CursorIcon::Default,
            pointer: Point2D::default(),
            layout_dirty: true,
            accessibility_dirty: true,
            accessibility_mode: None,
            needs_render: true,
        };
        embedded.update(fonts);
        embedded
    }

    /// Feeds one input event. Positions are in physical pixels from the drawing area's origin.
    pub fn event(&mut self, event: PlatformEvent) {
        match &event {
            PlatformEvent::Mouse {
                cursor,
                name: MouseEventName::MouseDown,
                ..
            } => {
                self.pointer = Point2D::new(cursor.x as f32, cursor.y as f32);
                self.platform
                    .navigation_mode
                    .set_if_modified(NavigationMode::NotKeyboard);
            }
            PlatformEvent::Mouse { cursor, .. } => {
                self.pointer = Point2D::new(cursor.x as f32, cursor.y as f32);
            }
            _ => {}
        }
        let scale_factor = self.scale_factor();
        let processed = EventsMeasurerAdapter {
            tree: &mut self.tree,
            scale_factor,
        }
        .run(
            &mut vec![event],
            &mut self.nodes_state,
            self.accessibility.focused_node_id(),
        );
        EventsExecutorAdapter {
            runner: &mut self.runner,
        }
        .run(&mut self.nodes_state, processed);
    }

    /// A click at `at` with the left button: down, then up.
    pub fn click(&mut self, at: (f64, f64)) {
        for name in [MouseEventName::MouseDown, MouseEventName::MouseUp] {
            self.event(PlatformEvent::Mouse {
                name,
                cursor: at.into(),
                button: Some(MouseButton::Left),
            });
        }
    }

    /// Runs whatever work is pending: handlers, tasks, the diff, layout, focus and the
    /// accessibility tree. The waker is armed again before it returns.
    pub fn update(&mut self, fonts: &mut Fonts) -> Update {
        // A handler can queue more events and dirty more scopes, so work until the runner has
        // nothing left and its future is pending, which leaves the waker armed.
        let mut armed = false;
        for _ in 0..8 {
            let worked = self.sync();
            let requested = self.fulfil_user_events(fonts);
            if !worked && !requested && self.poll_runner() {
                armed = true;
                break;
            }
        }
        if !armed {
            self.waker.wake_by_ref();
        }

        if self.layout_dirty {
            self.layout_dirty = false;
            let scale_factor = self.scale_factor();
            self.tree.measure_layout(
                self.size,
                &mut fonts.collection,
                &fonts.manager,
                &self.events_sender,
                &mut self.nodes_state,
                scale_factor,
                &fonts.default_families,
            );
            self.accessibility_dirty = true;
        }

        let mut update = Update {
            redraw: self.needs_render,
            ..Default::default()
        };
        if self.accessibility_dirty {
            self.accessibility_dirty = false;
            let tree_update = self.process_accessibility();
            if self.screen_reader.is_on() {
                update.accessibility = Some(tree_update);
            }
        }
        let cursor = self.tree.cursor_icon(&self.nodes_state);
        if cursor != self.cursor {
            self.cursor = cursor;
            update.cursor = Some(cursor);
        }
        update
    }

    /// Draws the tree into `canvas`, clearing it to transparent first.
    pub fn render(&mut self, fonts: &mut Fonts, canvas: &Canvas) {
        self.needs_render = false;
        RenderPipeline {
            font_collection: &mut fonts.collection,
            font_manager: &fonts.manager,
            canvas,
            tree: &self.tree,
            scale_factor: self.scale_factor(),
            background: Color::TRANSPARENT,
        }
        .render();
    }

    /// Tells the app a drawn frame reached the screen, which advances its animations.
    pub fn presented(&mut self) {
        self.ticker.notify();
    }

    /// Whether the tree changed since it was last drawn.
    pub fn needs_render(&self) -> bool {
        self.needs_render
    }

    /// A new drawing area, in physical pixels, and scale.
    pub fn resize(&mut self, size: Size2D, scale_factor: f64) {
        let scale_changed = (scale_factor - self.scale_factor()).abs() > f64::EPSILON;
        if size == self.size && !scale_changed {
            return;
        }
        self.size = size;
        self.platform.scale_factor.set_if_modified(scale_factor);
        self.platform
            .root_size
            .set_if_modified(logical(size, scale_factor));
        if scale_changed {
            self.tree.layout.reset();
            self.tree.text_cache.reset();
        } else {
            self.tree.layout.clear_dirty();
            self.tree.layout.invalidate(NodeId::ROOT);
        }
        self.layout_dirty = true;
        self.needs_render = true;
        self.waker.wake_by_ref();
    }

    /// Drops measured text, after fonts changed under it.
    pub fn invalidate_text(&mut self) {
        self.tree.layout.reset();
        self.tree.text_cache.reset();
        self.layout_dirty = true;
        self.needs_render = true;
        self.waker.wake_by_ref();
    }

    /// Whether the app holds its host's keyboard focus.
    pub fn set_focused(&mut self, focused: bool) {
        self.platform.is_app_focused.set_if_modified(focused);
    }

    pub fn set_preferred_theme(&mut self, theme: PreferredTheme) {
        self.platform.preferred_theme.set_if_modified(theme);
    }

    /// Moves keyboard focus.
    pub fn request_focus(&mut self, strategy: AccessibilityFocusStrategy) {
        if let AccessibilityFocusStrategy::Node(id) = strategy {
            self.platform.focused_accessibility_id.set_if_modified(id);
        }
        self.tree.accessibility_diff.request_focus(strategy);
        self.accessibility_dirty = true;
        self.waker.wake_by_ref();
    }

    /// Turns the accessibility export on or off. While on, every [`Update`] carries the changes,
    /// and turning it on returns the whole tree.
    pub fn set_screen_reader(&mut self, on: bool) -> Option<accesskit::TreeUpdate> {
        self.screen_reader.set(on);
        if !on {
            return None;
        }
        let update = self.accessibility.init(&mut self.tree, &self.name);
        self.platform
            .focused_accessibility_id
            .set_if_modified(update.focus);
        Some(update)
    }

    /// The whole accessibility tree as it stands, whether or not the export is on.
    pub fn accessibility_snapshot(&mut self) -> accesskit::TreeUpdate {
        let update = self.accessibility.init(&mut self.tree, &self.name);
        self.platform
            .focused_accessibility_id
            .set_if_modified(update.focus);
        update
    }

    /// The accessibility id of the focused node.
    pub fn focused(&self) -> AccessibilityId {
        self.accessibility.focused_id
    }

    /// Acts on an assistive technology's request: focus moves; a click is delivered at the
    /// node's centre.
    pub fn accessibility_action(&mut self, request: accesskit::ActionRequest) {
        let Some(node_id) = self.accessibility.map.get(&request.target_node).copied() else {
            return;
        };
        match request.action {
            accesskit::Action::Focus => {
                self.request_focus(AccessibilityFocusStrategy::Node(request.target_node));
            }
            accesskit::Action::Click => {
                if let Some(layout) = self.tree.layout.get(&node_id) {
                    let centre = layout.visible_area().center();
                    let scale = self.scale_factor() as f32;
                    self.click(((centre.x * scale) as f64, (centre.y * scale) as f64));
                }
            }
            _ => {}
        }
    }

    pub fn animation_clock(&self) -> &AnimationClock {
        &self.animation_clock
    }

    pub fn runner(&mut self) -> &mut Runner {
        &mut self.runner
    }

    pub fn tree(&self) -> &Tree {
        &self.tree
    }

    /// Runs `f` with the app's root scope current, so it can read and write root contexts.
    pub fn with_root<T>(&mut self, f: impl FnOnce() -> T) -> T {
        self.runner.with_root_context(f)
    }

    fn scale_factor(&self) -> f64 {
        *self.platform.scale_factor.peek()
    }

    /// Polls the runner's own event future once with the host's waker. True when it is pending,
    /// so later work wakes the host; false when it ran something instead.
    fn poll_runner(&mut self) -> bool {
        let mut cx = std::task::Context::from_waker(&self.waker);
        let runner = &mut self.runner;
        let nodes_state = &mut self.nodes_state;
        let receiver = &mut self.events_receiver;
        let fut = std::pin::pin!(async {
            select! {
                chunk = receiver.next() => match chunk {
                    Some(EventsChunk::Processed(events)) => {
                        EventsExecutorAdapter { runner: &mut *runner }.run(nodes_state, events);
                    }
                    Some(EventsChunk::Batch(events)) => {
                        for event in events {
                            runner.handle_event(event.node_id, event.name, event.data, event.bubbles);
                        }
                    }
                    None => {}
                },
                _ = runner.handle_events().fuse() => {},
            }
        });
        fut.poll(&mut cx).is_pending()
    }

    /// Runs queued events and applies the diff; true if either did anything.
    fn sync(&mut self) -> bool {
        let mut worked = false;
        while let Ok(chunk) = self.events_receiver.try_recv() {
            worked = true;
            match chunk {
                EventsChunk::Processed(events) => {
                    EventsExecutorAdapter {
                        runner: &mut self.runner,
                    }
                    .run(&mut self.nodes_state, events);
                }
                EventsChunk::Batch(events) => {
                    for event in events {
                        self.runner.handle_event(
                            event.node_id,
                            event.name,
                            event.data,
                            event.bubbles,
                        );
                    }
                }
            }
        }
        let mutations = self.runner.sync_and_update();
        worked |= !mutations.is_empty();
        let scale_factor = self.scale_factor() as f32;
        let tree = &mut self.tree;
        let result = self
            .runner
            .run_in(|| tree.apply_mutations(mutations, scale_factor));
        if let Some(strategy) = result.auto_focus {
            self.tree.accessibility_diff.request_focus(strategy);
            self.accessibility_dirty = true;
        }
        self.layout_dirty |= result.needs_render;
        self.needs_render |= result.needs_render;
        self.accessibility_dirty |= result.needs_accessibility;
        worked
    }

    /// Acts on what the app asked of its platform; true if it asked anything.
    fn fulfil_user_events(&mut self, fonts: &mut Fonts) -> bool {
        let events = std::mem::take(&mut *self.user_events.borrow_mut());
        let requested = !events.is_empty();
        for event in events {
            match event {
                UserEvent::RequestRedraw => self.needs_render = true,
                UserEvent::FocusAccessibilityNode(strategy) => {
                    if matches!(
                        strategy,
                        AccessibilityFocusStrategy::Forward(_)
                            | AccessibilityFocusStrategy::Backward(_)
                    ) {
                        self.accessibility_mode = Some(NavigationMode::Keyboard);
                    }
                    if let AccessibilityFocusStrategy::Node(id) = &strategy {
                        self.platform.focused_accessibility_id.set_if_modified(*id);
                    }
                    self.tree.accessibility_diff.request_focus(strategy);
                    self.accessibility_dirty = true;
                }
                UserEvent::SetCustomScaleFactor(factor) => {
                    self.platform.custom_scale_factor.set_if_modified(factor);
                }
                UserEvent::LoadFont {
                    font_name,
                    font_data,
                } => {
                    if fonts.register(&font_name, &font_data) {
                        self.tree.layout.reset();
                        self.tree.text_cache.reset();
                        self.layout_dirty = true;
                        self.needs_render = true;
                    }
                }
                // A shell surface has no browser to open a link in; the host offers its own.
                UserEvent::OpenUrl(url) => tracing::debug!("open {url} ignored"),
                UserEvent::Erased(_) => {}
            }
        }
        requested
    }

    fn process_accessibility(&mut self) -> accesskit::TreeUpdate {
        let update =
            self.accessibility
                .process_updates(&mut self.tree, &self.events_sender, &self.name);
        self.platform
            .focused_accessibility_id
            .set_if_modified(update.focus);
        if let Some(node_id) = self.accessibility.focused_node_id()
            && let Some(layout_node) = self.tree.layout.get(&node_id)
        {
            let node = AccessibilityTree::create_node(node_id, layout_node, &self.tree, &self.name);
            self.platform
                .focused_accessibility_node
                .set_if_modified(node);
        }
        if let Some(mode) = self.accessibility_mode.take() {
            self.platform.navigation_mode.set_if_modified(mode);
        }
        update
    }
}

fn logical(size: Size2D, scale_factor: f64) -> Size2D {
    Size2D::new(
        size.width / scale_factor as f32,
        size.height / scale_factor as f32,
    )
}
