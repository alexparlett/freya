use std::time::{
    Duration,
    Instant,
};

use freya_core::prelude::*;
use torin::prelude::{
    Area,
    Direction,
};

use crate::scrollviews::{
    ItemSize,
    shared::{
        get_corrected_scroll_position,
        is_scrollable,
    },
};

/// Logical scroll position in pixels.
pub type ScrollPoint = euclid::Point2D<f64, ()>;

/// Logical content or viewport size in pixels.
pub type ScrollSize = euclid::Size2D<f64, ()>;

/// Logical scroll movement or velocity.
pub type ScrollVector = euclid::Vector2D<f64, ()>;

/// Where along an axis a scroll should land, the beginning or the end.
#[derive(Default, PartialEq, Eq)]
pub enum ScrollPosition {
    #[default]
    Start,
    End,
}

impl ScrollPosition {
    /// Scroll offset in pixels of this position.
    fn offset(&self) -> f64 {
        match self {
            Self::Start => 0.,
            Self::End => ScrollController::END,
        }
    }
}

/// Initial configuration for a [`ScrollController`] created with [`use_scroll_controller`].
#[derive(Default)]
pub struct ScrollConfig {
    /// Where the vertical axis starts scrolled to when first laid out.
    pub default_vertical_position: ScrollPosition,
    /// Where the horizontal axis starts scrolled to when first laid out.
    pub default_horizontal_position: ScrollPosition,
}

/// Handle to drive a scrollview programmatically.
///
/// By default a scrollview owns its scroll position and only the user can move it, through the
/// wheel, the scrollbar, arrow keys or dragging. A [`ScrollController`] lets your own code read and
/// change that position instead. Create one with [`use_scroll_controller`] and hand it to a
/// scrollview through its `new_controlled` constructor.
///
/// Some cases where a controller is needed:
///
/// - Jumping to the top or bottom in response to an action, for example scrolling a chat to the
///   newest message after sending one.
/// - Keeping several scrollviews in sync, like a diff view with two panes that move together.
/// - Reading the current scroll position to drive something else, such as a "scroll to top" button
///   that only appears once the user has scrolled down.
///
/// # Scrolling from code
///
/// [`scroll_to`](ScrollController::scroll_to) jumps to the start or end of an axis, with the end
/// resolved against the content on the next layout. This is the common way to snap a list to its
/// top or bottom.
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
///
///     rect()
///         .child(
///             Button::new()
///                 .on_press(move |_| {
///                     scroll_controller.scroll_to(ScrollPosition::End, Direction::Vertical);
///                 })
///                 .child("Scroll to bottom"),
///         )
///         .child(
///             ScrollView::new_controlled(scroll_controller)
///                 .children((0..100).map(|i| label().key(i).text(format!("Item {i}")))),
///         )
/// }
/// ```
///
/// For an exact pixel position use [`scroll_to_y`](ScrollController::scroll_to_y) or
/// [`scroll_to_x`](ScrollController::scroll_to_x). The current position is available by converting
/// the controller into a `(f64, f64)` tuple of `(x, y)` pixels, or calling
/// [`position`](ScrollController::position).
///
/// # Animated scrolling
///
/// Scrolling to a position is instant by default. To animate, call
/// [`animate_from`](ScrollController::animate_from) with the current position before setting
/// the target offset.
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let mut scroll_controller = use_scroll_controller(ScrollConfig::default);
///
///     rect()
///         .child(
///             Button::new()
///                 .on_press(move |_| {
///                     let current = scroll_controller.position();
///                     scroll_controller.animate_from(current);
///                     scroll_controller.scroll_to_y(500.);
///                 })
///                 .child("Animate scroll"),
///         )
///         .child(
///             ScrollView::new_controlled(scroll_controller)
///                 .children((0..100).map(|i| label().key(i).text(format!("Item {i}")))),
///         )
/// }
/// ```
///
/// # Keeping scrollviews in sync
///
/// Pass the same [`ScrollController`] to several scrollviews to share a single scroll position.
/// Moving any of them moves the rest.
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let scroll_controller = use_scroll_controller(ScrollConfig::default);
///
///     rect()
///         .horizontal()
///         .spacing(6.)
///         .child(
///             ScrollView::new_controlled(scroll_controller)
///                 .width(Size::flex(1.))
///                 .children((0..30).map(|i| label().key(i).text(format!("Left {i}")))),
///         )
///         .child(
///             ScrollView::new_controlled(scroll_controller)
///                 .width(Size::flex(1.))
///                 .children((0..30).map(|i| label().key(i).text(format!("Right {i}")))),
///         )
/// }
/// ```
///
/// # Starting position
///
/// The [`ScrollConfig`] passed to [`use_scroll_controller`] also decides where each axis starts.
/// Set [`default_vertical_position`](ScrollConfig::default_vertical_position) to
/// [`ScrollPosition::End`] to open a list already scrolled to the bottom.
///
/// ```rust
/// # use freya::prelude::*;
/// fn app() -> impl IntoElement {
///     let scroll_controller = use_scroll_controller(|| ScrollConfig {
///         default_vertical_position: ScrollPosition::End,
///         ..Default::default()
///     });
///
///     ScrollView::new_controlled(scroll_controller)
///         .children((0..100).map(|i| label().key(i).text(format!("Item {i}"))))
/// }
/// ```
#[derive(PartialEq, Clone, Copy)]
pub struct ScrollController {
    pub(crate) scroll: State<(f64, f64)>,
    pub bounds: State<Option<(ScrollSize, ScrollSize)>>,
    /// The scrollable's viewport rectangle in window space, refreshed on every layout. Lets
    /// [`scroll_to_item`](Self::scroll_to_item) reveal a target from its own measured rectangle
    /// without the caller knowing the viewport.
    viewport: State<Area>,
    pub(crate) damp: State<SmoothDamp>,
    pub(crate) drag: State<Drag>,
    pub(crate) task: State<Option<TaskHandle>>,
}

impl From<ScrollController> for (f64, f64) {
    /// Reads the current `(x, y)` scroll position in pixels.
    fn from(val: ScrollController) -> Self {
        *val.scroll.read()
    }
}

impl ScrollController {
    /// Offset of an axis scrolled to its end, resolved against the content size on the next layout.
    const END: f64 = f64::INFINITY;

    /// Creates a controller starting at the scroll position `(x, y)`.
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            scroll: State::create((x, y)),
            bounds: State::create(None),
            viewport: State::create(Area::default()),
            damp: State::create(SmoothDamp::new()),
            drag: State::create(Drag::default()),
            task: State::create(None),
        }
    }

    /// Updates the content and viewport bounds used to clamp scroll positions.
    pub fn apply_layout(&mut self, content_size: ScrollSize, viewport_size: ScrollSize) {
        self.bounds
            .set_if_modified(Some((content_size, viewport_size)));

        let (x, y) = *self.scroll.peek();
        self.scroll_to_x(x);
        self.scroll_to_y(y);
    }

    /// Records the scrollable's visible frame in window space, which
    /// [`scroll_to_item`](Self::scroll_to_item) reveals targets against.
    pub(crate) fn apply_viewport(&mut self, viewport: Area) {
        self.viewport.set_if_modified(viewport);
    }

    /// Reads the current scroll position in pixels.
    pub fn position(self) -> ScrollPoint {
        let (x, y) = self.into();
        ScrollPoint::new(x, y)
    }

    /// Scrolls the horizontal axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_x(&mut self, to: f64) -> bool {
        let to = self.bounded_position(to, Direction::Horizontal);
        let changed = self.scroll.peek().0 != to;
        if changed {
            self.scroll.write().0 = to;
        }
        changed
    }

    /// Scrolls the vertical axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_y(&mut self, to: f64) -> bool {
        let to = self.bounded_position(to, Direction::Vertical);
        let changed = self.scroll.peek().1 != to;
        if changed {
            self.scroll.write().1 = to;
        }
        changed
    }

    fn bounded_position(&self, position: f64, direction: Direction) -> f64 {
        let Some((content_size, viewport_size)) = *self.bounds.peek() else {
            return position;
        };

        let (content_size, viewport_size) = match direction {
            Direction::Horizontal => (content_size.width, viewport_size.width),
            Direction::Vertical => (content_size.height, viewport_size.height),
        };
        get_corrected_scroll_position(content_size, viewport_size, position)
    }

    /// Scrolls `scroll_direction` to `scroll_position`.
    pub fn scroll_to(&mut self, scroll_position: ScrollPosition, scroll_direction: Direction) {
        let to = scroll_position.offset();
        match scroll_direction {
            Direction::Vertical => self.scroll_to_y(to),
            Direction::Horizontal => self.scroll_to_x(to),
        };
    }

    /// The content and viewport extents along `direction`, as last laid out.
    fn extents(bounds: Option<(ScrollSize, ScrollSize)>, direction: Direction) -> (f64, f64) {
        let Some((content_size, viewport_size)) = bounds else {
            return (0., 0.);
        };
        match direction {
            Direction::Horizontal => (content_size.width, viewport_size.width),
            Direction::Vertical => (content_size.height, viewport_size.height),
        }
    }

    /// Whether the scrollable overflows its viewport along `direction`, i.e. there is content to
    /// scroll to on that axis. Subscribes the caller, so a sibling can reactively show or hide a
    /// scroll affordance as the content grows and shrinks. Unmeasured reads as not scrollable.
    pub fn is_scrollable(&self, direction: Direction) -> bool {
        let (content, viewport) = Self::extents(*self.bounds.read(), direction);
        is_scrollable(content, viewport)
    }

    /// Whether `direction` is scrolled to its end, within a pixel.
    ///
    /// The predicate a stick-to-the-end surface is built on, such as a chat transcript or a log
    /// tail: follow the content while the reader is at the end, and stop once they scroll away.
    ///
    /// Peeks rather than reads: the content grows under the reader too, and subscribing would
    /// answer "not at the end" the moment the content outgrew the viewport, before a follower had
    /// scrolled. Re-ask on the scroll position, which only the reader and the follower move.
    ///
    /// Content that does not overflow is at its end.
    pub fn is_at_end(&self, direction: Direction) -> bool {
        let (content, viewport) = Self::extents(*self.bounds.peek(), direction);
        if !is_scrollable(content, viewport) {
            return true;
        }
        let (x, y) = *self.scroll.peek();
        let position = match direction {
            Direction::Horizontal => x,
            Direction::Vertical => y,
        };
        position + viewport >= content - 1.0
    }

    /// Scrolls the minimum amount needed to bring `item` fully into view, on whichever axes it
    /// overflows the viewport. `item` is the target's own measured window-space rectangle, e.g.
    /// from an [`on_sized`](freya_core::prelude::EventHandlersExt::on_sized) area, so the caller
    /// never has to know the viewport or scroll position. A no-op once the item is visible, so it
    /// is safe to call every render.
    ///
    /// Jumps straight there, freezing any smooth scroll in flight first, see
    /// [`animate_to_item`](Self::animate_to_item) for a smooth reveal.
    pub fn scroll_to_item(&mut self, item: impl Into<Area>) {
        self.reveal_item(item.into(), false);
    }

    /// [`scroll_to_item`](Self::scroll_to_item), moving there with the same smooth scroll the
    /// wheel and the arrow keys use.
    pub fn animate_to_item(&mut self, item: impl Into<Area>) {
        self.reveal_item(item.into(), true);
    }

    /// [`scroll_to_item`](Self::scroll_to_item) for a target with no measured rectangle, such as a
    /// row a [`VirtualScrollView`](crate::scrollviews::VirtualScrollView) has not built: scrolls the
    /// minimum amount needed to bring the span `[offset, offset + size]` of the content into view
    /// along `direction`.
    ///
    /// Does nothing before the scrollable has been laid out. A caller that must move the view on
    /// the frame it mounts wants [`scroll_to`](Self::scroll_to).
    pub fn scroll_to_offset(&mut self, offset: f64, size: f64, direction: Direction) {
        self.reveal_offset(offset, size, direction, false);
    }

    /// [`scroll_to_offset`](Self::scroll_to_offset), moving there with the same smooth scroll the
    /// wheel and the arrow keys use.
    pub fn animate_to_offset(&mut self, offset: f64, size: f64, direction: Direction) {
        self.reveal_offset(offset, size, direction, true);
    }

    /// [`scroll_to_offset`](Self::scroll_to_offset) for the item at `index` of a
    /// [`VirtualScrollView`](crate::scrollviews::VirtualScrollView) sized by `item_size`.
    pub fn scroll_to_index(&mut self, index: usize, item_size: &ItemSize, direction: Direction) {
        self.scroll_to_offset(
            item_size.offset_of(index),
            item_size.at(index) as f64,
            direction,
        );
    }

    /// Where the content is drawn right now: the animated position while a smooth scroll runs,
    /// the target otherwise.
    fn drawn_position(&self) -> ScrollPoint {
        if self.task.peek().is_some() {
            self.damp.peek().absolute_position()
        } else {
            let (x, y) = *self.scroll.peek();
            ScrollPoint::new(x, y)
        }
    }

    /// Moves the content by `delta`, smoothly or at once. A positive delta moves the content
    /// towards the end of the viewport, so the scroll position goes down by it.
    fn reveal_by(&mut self, delta: ScrollVector, animate: bool) {
        if delta == ScrollVector::zero() {
            return;
        }
        let from = self.drawn_position();
        if animate {
            self.animate_from(from);
        } else if self.task.peek().is_some() {
            self.stop();
        }
        if delta.x != 0.0 {
            self.scroll_to_x(from.x - delta.x);
        }
        if delta.y != 0.0 {
            self.scroll_to_y(from.y - delta.y);
        }
    }

    fn reveal_item(&mut self, item: Area, animate: bool) {
        let viewport = *self.viewport.peek();
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        let item = item.to_f64();
        let viewport = viewport.to_f64();
        let delta = ScrollVector::new(
            reveal_delta(
                item.min_x(),
                item.max_x(),
                viewport.min_x(),
                viewport.max_x(),
            ),
            reveal_delta(
                item.min_y(),
                item.max_y(),
                viewport.min_y(),
                viewport.max_y(),
            ),
        );
        self.reveal_by(delta, animate);
    }

    fn reveal_offset(&mut self, offset: f64, size: f64, direction: Direction, animate: bool) {
        let (content, viewport) = Self::extents(*self.bounds.peek(), direction);
        if viewport <= 0.0 || content <= 0.0 {
            return;
        }
        let drawn = self.drawn_position();
        let position = match direction {
            Direction::Horizontal => drawn.x,
            Direction::Vertical => drawn.y,
        };
        let delta = reveal_delta(offset, offset + size, position, position + viewport);
        self.reveal_by(
            match direction {
                Direction::Horizontal => ScrollVector::new(delta, 0.0),
                Direction::Vertical => ScrollVector::new(0.0, delta),
            },
            animate,
        );
    }
}

/// The signed distance to move the content along one axis to reveal `[item_min, item_max]`
/// within `[vp_min, vp_max]`. Only a clipped item moves, by the minimum amount. An item that
/// already covers the viewport is caught first, so it settles instead of oscillating.
fn reveal_delta(item_min: f64, item_max: f64, vp_min: f64, vp_max: f64) -> f64 {
    if item_min <= vp_min && item_max >= vp_max {
        0.0
    } else if item_min < vp_min {
        vp_min - item_min
    } else if item_max > vp_max {
        vp_max - item_max
    } else {
        0.0
    }
}

/// Creates a [`ScrollController`], configured by the returned [`ScrollConfig`].
pub fn use_scroll_controller(config: impl FnOnce() -> ScrollConfig) -> ScrollController {
    use_hook(|| {
        let config = config();

        ScrollController::new(
            config.default_horizontal_position.offset(),
            config.default_vertical_position.offset(),
        )
    })
}

/// Distance under which the animation is close enough to snap, in pixels.
const SETTLE_DISTANCE: f64 = 0.5;

/// Speed under which the animation is slow enough to stop, in pixels per second.
const SETTLE_SPEED: f64 = 20.0;

/// Slowest drag release speed that still starts a fling, in pixels per second.
const FLING_MIN_SPEED: f64 = 50.0;

/// Fastest drag release speed, in pixels per second.
const FLING_MAX_SPEED: f64 = 8000.0;

/// Minimum sample interval for touch events delivered together.
/// 120 is rather a workaround right now, its requires an integration that winit doesnt expose yet.
const DRAG_MIN_SAMPLE_TIME: f64 = 1.0 / 120.0;

/// Maximum time since the last drag movement that still allows a fling.
const FLING_RELEASE_TIMEOUT: Duration = Duration::from_millis(100);

/// Scrolling feel of a [`TargetPlatform`].
pub(crate) trait ScrollFeel {
    /// Seconds wheel and keyboard scrolls take to reach their destination.
    fn scroll_smoothing_time(&self) -> f64;
    /// Seconds a fling takes to stop, which also scales how far it travels.
    fn scroll_fling_time(&self) -> f64;
}

impl ScrollFeel for TargetPlatform {
    fn scroll_smoothing_time(&self) -> f64 {
        if self.is_mobile() { 0.1 } else { 0.06 }
    }

    fn scroll_fling_time(&self) -> f64 {
        if self.is_mobile() { 0.35 } else { 0.5 }
    }
}

/// Moves a value towards a target with a smooth and continuous animation.
#[derive(Clone, Copy, Default)]
pub(crate) struct SmoothDamp {
    origin: ScrollPoint,
    position: ScrollPoint,
    velocity: ScrollVector,
    smooth_time: f64,
}

impl SmoothDamp {
    pub(crate) fn new() -> Self {
        Self {
            origin: ScrollPoint::zero(),
            position: ScrollPoint::zero(),
            velocity: ScrollVector::zero(),
            smooth_time: TargetPlatform::Unknown.scroll_smoothing_time(),
        }
    }

    fn absolute_position(&self) -> ScrollPoint {
        self.origin + self.position.to_vector()
    }

    /// Returns whether it has settled onto `target`.
    fn advance(&mut self, target: ScrollPoint, elapsed_seconds: f64) -> bool {
        let target = (target - self.origin).to_point();
        let omega = 2.0 / self.smooth_time;
        let decay = (-omega * elapsed_seconds).exp();
        let change = self.position - target;
        let linear_term = (self.velocity + change * omega) * elapsed_seconds;

        let velocity = (self.velocity - linear_term * omega) * decay;
        let position = target + (change + linear_term) * decay;

        if (target - position).length() < SETTLE_DISTANCE && velocity.length() < SETTLE_SPEED {
            self.position = target;
            self.velocity = ScrollVector::zero();
            return true;
        }

        self.position = position;
        self.velocity = velocity;
        false
    }
}

/// Velocity tracked while the content is dragged, to fling with on release.
#[derive(Clone, Copy)]
pub(crate) struct Drag {
    velocity: ScrollVector,
    last_move: Instant,
}

impl Default for Drag {
    fn default() -> Self {
        Self {
            velocity: ScrollVector::zero(),
            last_move: Instant::now(),
        }
    }
}

impl Drag {
    fn track(&mut self, delta: ScrollVector) {
        let now = Instant::now();
        let elapsed_seconds = now
            .duration_since(self.last_move)
            .as_secs_f64()
            .max(DRAG_MIN_SAMPLE_TIME);

        let velocity = (delta / elapsed_seconds).with_max_length(FLING_MAX_SPEED);
        self.velocity = self.velocity.lerp(velocity, 0.5);
        self.last_move = now;
    }
}

/// Follows the target held by a [`ScrollController`].
impl ScrollController {
    /// Position to render, the animated one while a scroll animation is running.
    pub fn animated_position(&self, target: ScrollPoint) -> ScrollPoint {
        if self.task.read().is_some() {
            self.damp.read().absolute_position()
        } else {
            target
        }
    }

    /// Chases the controller position from `current`, keeping the current velocity.
    pub fn animate_from(&mut self, current: ScrollPoint) {
        self.start(current, None, TargetPlatform::get().scroll_smoothing_time());
    }

    fn start(&mut self, current: ScrollPoint, velocity: Option<ScrollVector>, smooth_time: f64) {
        let is_animating = self.task.peek().is_some();
        {
            let mut damp = self.damp.write();
            damp.smooth_time = smooth_time;
            if let Some(velocity) = velocity {
                damp.velocity = velocity;
            }
            if !is_animating {
                damp.origin = current;
                damp.position = ScrollPoint::zero();
            }
        }
        if is_animating {
            return;
        }

        let ticker = RenderingTicker::get();
        let platform = Platform::get().current_window();
        let animation_clock = AnimationClock::get();
        let scroll_controller = *self;
        let mut damp = self.damp;
        let mut task = self.task;

        let animation_task = spawn(async move {
            platform.request_redraw();
            let mut previous_frame = Instant::now();

            loop {
                ticker.tick().await;

                let elapsed_seconds = animation_clock
                    .correct_elapsed_duration(previous_frame.elapsed())
                    .as_secs_f64();
                previous_frame = Instant::now();

                let target = scroll_controller.position();
                if damp.write().advance(target, elapsed_seconds) {
                    break;
                }

                platform.request_redraw();
            }

            task.write().take();
        });
        task.write().replace(animation_task);
    }

    /// Returns whether the gesture ended or was cancelled, so callers skip applying the scroll delta.
    pub(crate) fn scroll_gesture_phase(&mut self, phase: TouchPhase, delta: ScrollVector) -> bool {
        match phase {
            TouchPhase::Started => {
                self.stop();
                self.drag.set(Drag::default());
            }
            TouchPhase::Moved => self.drag.write().track(-delta),
            TouchPhase::Ended => {
                let bounds = *self.bounds.peek();
                if self.drag.peek().last_move.elapsed() < FLING_RELEASE_TIMEOUT
                    && let Some((content, viewport)) = bounds
                {
                    self.release_drag(self.animated_position(self.position()), content, viewport);
                }
                self.drag.set(Drag::default());
                return true;
            }
            TouchPhase::Cancelled => {
                self.stop();
                self.drag.set(Drag::default());
                return true;
            }
        }
        false
    }

    /// Freezes the animation and starts a drag from the momentum it caught.
    pub fn begin_drag(&mut self) {
        let caught_velocity = self.stop();
        self.drag.set(Drag {
            velocity: caught_velocity,
            last_move: Instant::now(),
        });
    }

    /// Tracks a drag delta in scroll coordinates, positive toward the end.
    pub fn drag(&mut self, delta: ScrollVector) {
        self.stop();
        self.drag.write().track(delta);
    }

    /// Ends a drag, flinging when it was fast enough to be a flick.
    pub fn release_drag(&mut self, from: ScrollPoint, content: ScrollSize, viewport: ScrollSize) {
        let velocity = self.drag.peek().velocity;
        if velocity.length() < FLING_MIN_SPEED {
            return;
        }

        let projected = from + velocity * TargetPlatform::get().scroll_fling_time();
        let target_x = get_corrected_scroll_position(content.width, viewport.width, projected.x);
        let target_y = get_corrected_scroll_position(content.height, viewport.height, projected.y);

        self.start(
            from,
            Some(velocity),
            TargetPlatform::get().scroll_fling_time(),
        );
        self.scroll_to_x(target_x);
        self.scroll_to_y(target_y);
    }

    /// Freezes the scroll where it is, returning the velocity it was moving at.
    pub fn stop(&mut self) -> ScrollVector {
        if self.task.peek().is_some() {
            let task = self.task.write().take();
            if let Some(task) = task {
                task.cancel();
            }

            let position = self.damp.peek().absolute_position();
            self.scroll_to_x(position.x);
            self.scroll_to_y(position.y);
        }

        let velocity = self.damp.peek().velocity;
        if velocity != ScrollVector::zero() {
            self.damp.write().velocity = ScrollVector::zero();
        }
        velocity
    }
}
