use std::time::Instant;

use freya_core::prelude::*;
use torin::{
    geometry::{
        Point2D,
        Size2D,
        Vector2D,
    },
    prelude::{
        Area,
        Direction,
    },
};

use crate::scrollviews::{
    ItemSize,
    shared::get_corrected_scroll_position,
};

/// Where along an axis a scroll should land, the beginning or the end.
#[derive(Default, PartialEq, Eq)]
pub enum ScrollPosition {
    #[default]
    Start,
    End,
}

impl ScrollPosition {
    /// Scroll offset in pixels of this position.
    fn offset(&self) -> i32 {
        match self {
            Self::Start => 0,
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
/// For an exact pixel offset use [`scroll_to_y`](ScrollController::scroll_to_y) or
/// [`scroll_to_x`](ScrollController::scroll_to_x). The current position is available by converting
/// the controller into a `(i32, i32)` tuple of `(x, y)` pixels.
///
/// # Keeping scrollviews in sync
///
/// Because a [`ScrollController`] is a cheap [`Copy`] handle, you can pass the same one to several
/// scrollviews and they share a single scroll position, moving any of them moves the rest.
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
    pub(crate) scroll: State<(i32, i32)>,
    bounds: State<Option<(Size2D, Size2D)>>,
    /// The scrollable's viewport rectangle in window space, refreshed on every layout. Lets
    /// [`scroll_to_item`](Self::scroll_to_item) reveal a target from its own measured rectangle
    /// without the caller knowing the viewport.
    viewport: State<Area>,
    pub(crate) damp: State<SmoothDamp>,
    pub(crate) drag: State<Drag>,
    pub(crate) task: State<Option<TaskHandle>>,
}

impl From<ScrollController> for (i32, i32) {
    /// Reads the current `(x, y)` scroll position in pixels.
    fn from(val: ScrollController) -> Self {
        *val.scroll.read()
    }
}

impl ScrollController {
    /// Offset of an axis scrolled to its end, resolved against the content size on the next layout.
    const END: i32 = i32::MIN;

    /// Creates a controller starting at the scroll position `(x, y)`.
    pub fn new(x: i32, y: i32) -> Self {
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
    ///
    /// `viewport` is the scrollable's visible frame in window space: the content box is sized to
    /// the viewport, and its own offset scrolls its children rather than itself.
    pub(crate) fn apply_layout(&mut self, content_size: Size2D, viewport: Area) {
        self.bounds
            .set_if_modified(Some((content_size, viewport.size)));
        self.viewport.set_if_modified(viewport);

        let (x, y) = *self.scroll.peek();
        self.scroll_to_x(x);
        self.scroll_to_y(y);
    }

    pub(crate) fn position(self) -> Point2D {
        let (x, y): (i32, i32) = self.into();
        Point2D::new(x as f32, y as f32)
    }

    /// Scrolls the horizontal axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_x(&mut self, to: i32) -> bool {
        let to = self.bounded_position(to, Direction::Horizontal);
        let changed = self.scroll.peek().0 != to;
        if changed {
            self.scroll.write().0 = to;
        }
        changed
    }

    /// Scrolls the vertical axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_y(&mut self, to: i32) -> bool {
        let to = self.bounded_position(to, Direction::Vertical);
        let changed = self.scroll.peek().1 != to;
        if changed {
            self.scroll.write().1 = to;
        }
        changed
    }

    fn bounded_position(&self, position: i32, direction: Direction) -> i32 {
        let Some((content_size, viewport_size)) = *self.bounds.peek() else {
            return position;
        };

        let (content_size, viewport_size) = match direction {
            Direction::Horizontal => (content_size.width, viewport_size.width),
            Direction::Vertical => (content_size.height, viewport_size.height),
        };
        get_corrected_scroll_position(content_size, viewport_size, position as f32) as i32
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
    fn extents(bounds: Option<(Size2D, Size2D)>, direction: Direction) -> (f32, f32) {
        let Some((content_size, viewport_size)) = bounds else {
            return (0., 0.);
        };
        match direction {
            Direction::Horizontal => (content_size.width, viewport_size.width),
            Direction::Vertical => (content_size.height, viewport_size.height),
        }
    }

    /// Whether the scrollable overflows its viewport along `direction`, i.e. there is content to
    /// scroll to on that axis. Reads the content size and viewport the scrollable last laid out
    /// and subscribes the caller, so a sibling can reactively show or hide a scroll affordance as
    /// the area's content grows and shrinks. Unmeasured (zero-viewport) reads as not scrollable.
    pub fn is_scrollable(&self, direction: Direction) -> bool {
        let (content, viewport) = Self::extents(*self.bounds.read(), direction);
        crate::scrollviews::shared::is_scrollable(content, viewport)
    }

    /// Whether `direction` is scrolled to its end, within a pixel.
    ///
    /// The predicate a **stick-to-the-end** surface is built on, such as a chat transcript or a log tail:
    /// follow the content while the reader is at the end, and stop the moment they scroll away
    /// from it.
    ///
    /// **Peeks, like [`scroll_to_item`](Self::scroll_to_item), and for a sharper reason than
    /// looping.** A follower asks this to decide whether to keep following, and the two things it
    /// compares move for two different reasons: the reader scrolls, and the content grows under
    /// them. Subscribing would answer "not at the end" the instant the content outgrew the
    /// viewport, before the follower had scrolled, and a follower that read it reactively would
    /// conclude the reader had scrolled away and stop, on the very first message too long to fit.
    /// So the caller chooses what to re-ask on, and the honest trigger is the **scroll position**
    /// (`(i32, i32)::from(controller)`), which only the reader and the follower move.
    ///
    /// Content that does not overflow is **at** its end: there is nowhere else to be, and a
    /// follower gated on this must keep following as the first lines arrive.
    pub fn is_at_end(&self, direction: Direction) -> bool {
        let (content, viewport) = Self::extents(*self.bounds.peek(), direction);
        if !crate::scrollviews::shared::is_scrollable(content, viewport) {
            return true;
        }
        let (x, y) = *self.scroll.peek();
        let position = match direction {
            Direction::Horizontal => x,
            Direction::Vertical => y,
        } as f32;
        // Bounded on every layout, so once there are bounds to compare against the stored
        // position is already within them.
        // The scroll position is negative-going: the content is offset up by how far down the
        // reader is, so the end is where that offset covers everything the viewport does not.
        (position.abs() + viewport) >= content - 1.0
    }

    /// Scrolls the minimum amount needed to bring `item` fully into view, on whichever axes it
    /// overflows the viewport. `item` is the target's own measured window-space rectangle, e.g.
    /// straight from an [`on_sized`](freya_core::prelude::EventHandlersExt::on_sized)
    /// [`Area`](torin::prelude::Area), so the caller never has to know the viewport or scroll
    /// position. A no-op once the item is already visible, so it is safe to call every render (an
    /// item larger than the viewport aligns to its start and stops, rather than oscillating).
    ///
    /// Jumps straight there, freezing any smooth scroll in flight first: the item's rectangle
    /// was measured where the content is drawn, which trails the target while an animation runs.
    /// See [`animate_to_item`](Self::animate_to_item) for a smooth reveal.
    ///
    /// Peeks rather than reads: it is imperative, and reading inside a reactive effect would
    /// subscribe that effect to the viewport and loop it against its own scroll write.
    pub fn scroll_to_item(&mut self, item: impl Into<Area>) {
        self.reveal_item(item.into(), false);
    }

    /// [`scroll_to_item`](Self::scroll_to_item), moving there with the same smooth scroll the
    /// wheel and the arrow keys use.
    pub fn animate_to_item(&mut self, item: impl Into<Area>) {
        self.reveal_item(item.into(), true);
    }

    /// [`scroll_to_item`](Self::scroll_to_item) for a target that has no measured rectangle: scrolls
    /// the minimum amount needed to bring the span `[offset, offset + size]` of the content into view
    /// along `direction`.
    ///
    /// This is the [`VirtualScrollView`](crate::scrollviews::VirtualScrollView) half of the pair. A
    /// virtualized view only builds the items inside its viewport, so the row a caller wants to
    /// reveal usually does not exist yet and can report no [`Area`] to reveal against. What the
    /// caller does know is where the row sits in the content, which
    /// [`scroll_to_index`](Self::scroll_to_index) works out from an [`ItemSize`]. Everything else,
    /// the viewport and the current position, is this controller's own, exactly as it is for
    /// `scroll_to_item`.
    ///
    /// A no-op once the span is already visible, so it is safe to call every render.
    ///
    /// **Imperative, so it is only meaningful once the scrollable has been laid out**: before the
    /// first layout there is no viewport and no content extent to reveal against, and the call does
    /// nothing. Reveal from a gesture, or from an effect that runs once the target could have been
    /// drawn. A caller that must move the view on the frame it mounts wants
    /// [`scroll_to`](Self::scroll_to), which is resolved against the content on the next layout.
    ///
    /// The content spans `0..content` along the axis and the position is negative-going, so the
    /// visible span of the content starts at `-position`. Everything is peeked rather than read,
    /// for [`scroll_to_item`](Self::scroll_to_item)'s reason.
    pub fn scroll_to_offset(&mut self, offset: f32, size: f32, direction: Direction) {
        self.reveal_offset(offset, size, direction, false);
    }

    /// [`scroll_to_offset`](Self::scroll_to_offset), moving there with the same smooth scroll the
    /// wheel and the arrow keys use.
    pub fn animate_to_offset(&mut self, offset: f32, size: f32, direction: Direction) {
        self.reveal_offset(offset, size, direction, true);
    }

    /// [`scroll_to_offset`](Self::scroll_to_offset) for the item at `index` of a
    /// [`VirtualScrollView`](crate::scrollviews::VirtualScrollView) sized by `item_size`.
    ///
    /// With [`ItemSize::Dynamic`] the offset is summed from the sizes before `index`, while the
    /// view extrapolates its total size from the items it has measured, so near the end of a list
    /// of uneven items the two can disagree by the error in that estimate.
    pub fn scroll_to_index(&mut self, index: usize, item_size: &ItemSize, direction: Direction) {
        self.scroll_to_offset(item_size.offset_of(index), item_size.at(index), direction);
    }

    /// Where the content is drawn right now: the animated position while a smooth scroll runs,
    /// the target otherwise.
    fn drawn_position(&self) -> Point2D {
        if self.task.peek().is_some() {
            self.damp.peek().position
        } else {
            let (x, y) = *self.scroll.peek();
            Point2D::new(x as f32, y as f32)
        }
    }

    /// Moves the drawn position by `delta`, smoothly or at once.
    fn reveal_by(&mut self, delta: Vector2D, animate: bool) {
        if delta == Vector2D::zero() {
            return;
        }
        let from = self.drawn_position();
        if animate {
            self.animate_from(from);
        } else if self.task.peek().is_some() {
            self.stop();
        }
        if delta.x != 0.0 {
            self.scroll_to_x((from.x + delta.x).round() as i32);
        }
        if delta.y != 0.0 {
            self.scroll_to_y((from.y + delta.y).round() as i32);
        }
    }

    fn reveal_item(&mut self, item: Area, animate: bool) {
        let viewport = *self.viewport.peek();
        // Not laid out yet: nothing meaningful to reveal against.
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        let delta = Vector2D::new(
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

    fn reveal_offset(&mut self, offset: f32, size: f32, direction: Direction, animate: bool) {
        let (content, viewport) = Self::extents(*self.bounds.peek(), direction);
        if viewport <= 0.0 || content <= 0.0 {
            return;
        }
        let drawn = self.drawn_position();
        let position = match direction {
            Direction::Horizontal => drawn.x,
            Direction::Vertical => drawn.y,
        };
        let delta = reveal_delta(offset, offset + size, -position, -position + viewport);
        self.reveal_by(
            match direction {
                Direction::Horizontal => Vector2D::new(delta, 0.0),
                Direction::Vertical => Vector2D::new(0.0, delta),
            },
            animate,
        );
    }
}

/// The signed distance to add to the scroll offset on one axis to reveal `[item_min, item_max]`
/// within `[vp_min, vp_max]`. Only a *clipped* item moves: if the item sits anywhere inside the
/// visible span (hugging the start, hugging the end, or covering the whole viewport) it's a no-op.
/// A clipped item is pulled in by the minimum amount (aligning the offending edge). The covering
/// case (item wider than the viewport, spanning it) is caught first, so an over-large item settles
/// once its edge is reached instead of oscillating start↔end.
fn reveal_delta(item_min: f32, item_max: f32, vp_min: f32, vp_max: f32) -> f32 {
    if item_min <= vp_min && item_max >= vp_max {
        0.0 // item already covers the viewport, visible
    } else if item_min < vp_min {
        vp_min - item_min // clipped at the start edge → pull it in
    } else if item_max > vp_max {
        vp_max - item_max // clipped at the end edge → pull it in
    } else {
        0.0 // fully inside the viewport
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
const SETTLE_DISTANCE: f32 = 0.5;
/// Speed under which the animation is slow enough to stop, in pixels per second.
const SETTLE_SPEED: f32 = 20.0;

/// Slowest drag release speed that still starts a fling, in pixels per second.
const FLING_MIN_SPEED: f32 = 50.0;

/// Scrolling feel of a [`TargetPlatform`].
pub(crate) trait ScrollFeel {
    /// Seconds wheel and keyboard scrolls take to reach their destination.
    fn scroll_smoothing_time(&self) -> f32;
    /// Seconds a fling takes to stop, which also scales how far it travels.
    fn scroll_fling_time(&self) -> f32;
}

impl ScrollFeel for TargetPlatform {
    fn scroll_smoothing_time(&self) -> f32 {
        if self.is_mobile() { 0.1 } else { 0.06 }
    }

    fn scroll_fling_time(&self) -> f32 {
        if self.is_mobile() { 0.35 } else { 0.5 }
    }
}

/// Moves a value towards a target with a smooth and continuous animation.
#[derive(Clone, Copy, Default)]
pub(crate) struct SmoothDamp {
    position: Point2D,
    velocity: Vector2D,
    smooth_time: f32,
}

impl SmoothDamp {
    pub(crate) fn new() -> Self {
        Self {
            position: Point2D::zero(),
            velocity: Vector2D::zero(),
            smooth_time: TargetPlatform::Unknown.scroll_smoothing_time(),
        }
    }

    /// Returns whether it has settled onto `target`.
    fn advance(&mut self, target: Point2D, elapsed_seconds: f32) -> bool {
        let omega = 2.0 / self.smooth_time;
        let decay = (-omega * elapsed_seconds).exp();
        let change = self.position - target;
        let linear_term = (self.velocity + change * omega) * elapsed_seconds;

        let velocity = (self.velocity - linear_term * omega) * decay;
        let position = target + (change + linear_term) * decay;

        if (target - position).length() < SETTLE_DISTANCE && velocity.length() < SETTLE_SPEED {
            self.position = target;
            self.velocity = Vector2D::zero();
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
    velocity: Vector2D,
    last_move: Instant,
}

impl Default for Drag {
    fn default() -> Self {
        Self {
            velocity: Vector2D::zero(),
            last_move: Instant::now(),
        }
    }
}

impl Drag {
    fn track(&mut self, delta: Vector2D) {
        let now = Instant::now();
        let elapsed_seconds = now.duration_since(self.last_move).as_secs_f32();
        if elapsed_seconds > 0.0 {
            self.velocity = self.velocity.lerp(-delta / elapsed_seconds, 0.5);
        }
        self.last_move = now;
    }
}

/// Follows the target held by a [`ScrollController`].
impl ScrollController {
    /// Position to render, the animated one while a scroll animation is running.
    pub fn animated_position(&self, target: Point2D) -> Point2D {
        if self.task.read().is_some() {
            self.damp.read().position
        } else {
            target
        }
    }

    /// Chases the controller position from `current`, keeping the current velocity.
    pub fn animate_from(&mut self, current: Point2D) {
        self.start(current, None, TargetPlatform::get().scroll_smoothing_time());
    }

    /// Like [`Self::animate_from`] but launched at `velocity` and slower to stop.
    fn fling_from(&mut self, current: Point2D, velocity: Vector2D) {
        self.start(
            current,
            Some(velocity),
            TargetPlatform::get().scroll_fling_time(),
        );
    }

    fn start(&mut self, current: Point2D, velocity: Option<Vector2D>, smooth_time: f32) {
        let is_animating = self.task.peek().is_some();
        {
            let mut damp = self.damp.write();
            damp.smooth_time = smooth_time;
            if let Some(velocity) = velocity {
                damp.velocity = velocity;
            }
            if !is_animating {
                damp.position = current;
            }
        }
        if is_animating {
            return;
        }

        let ticker = RenderingTicker::get();
        let platform = Platform::get();
        let animation_clock = AnimationClock::get();
        let scroll_controller = *self;
        let mut damp = self.damp;
        let mut task = self.task;

        let animation_task = spawn(async move {
            platform.send(UserEvent::RequestRedraw);
            let mut previous_frame = Instant::now();

            loop {
                ticker.tick().await;

                let elapsed_seconds = animation_clock
                    .correct_elapsed_duration(previous_frame.elapsed())
                    .as_secs_f32();
                previous_frame = Instant::now();

                let target = scroll_controller.position();
                if damp.write().advance(target, elapsed_seconds) {
                    break;
                }

                platform.send(UserEvent::RequestRedraw);
            }

            task.write().take();
        });
        task.write().replace(animation_task);
    }

    /// Freezes the animation and starts a drag from the momentum it caught.
    pub fn begin_drag(&mut self) {
        let caught_velocity = self.stop();
        self.drag.set(Drag {
            velocity: caught_velocity,
            last_move: Instant::now(),
        });
    }

    /// Feeds a drag movement into the tracked velocity.
    pub fn drag(&mut self, delta: Vector2D) {
        self.stop();
        self.drag.write().track(delta);
    }

    /// Ends a drag, flinging when it was fast enough to be a flick.
    pub fn release_drag(&mut self, from: Point2D, content: Size2D, viewport: Size2D) {
        let velocity = self.drag.peek().velocity;
        if velocity.length() < FLING_MIN_SPEED {
            return;
        }

        let projected = from + velocity * TargetPlatform::get().scroll_fling_time();
        let target_x = get_corrected_scroll_position(content.width, viewport.width, projected.x);
        let target_y = get_corrected_scroll_position(content.height, viewport.height, projected.y);

        self.fling_from(from, velocity);
        self.scroll_to_x(target_x as i32);
        self.scroll_to_y(target_y as i32);
    }

    /// Freezes the scroll where it is, returning the velocity it was moving at.
    pub fn stop(&mut self) -> Vector2D {
        let task = self.task.write().take();
        if let Some(task) = task {
            task.cancel();

            let position = self.damp.peek().position.to_i32();
            self.scroll_to_x(position.x);
            self.scroll_to_y(position.y);
        }

        let velocity = self.damp.peek().velocity;
        self.damp.write().velocity = Vector2D::zero();
        velocity
    }
}
