use freya_core::prelude::*;
use torin::prelude::{
    Area,
    Direction,
};

use crate::scrollviews::shared::get_corrected_scroll_position;

/// Where along an axis a scroll should land, the beginning or the end.
#[derive(Default, PartialEq, Eq)]
pub enum ScrollPosition {
    #[default]
    Start,
    End,
}

/// Initial configuration for a [`ScrollController`] created with [`use_scroll_controller`].
#[derive(Default)]
pub struct ScrollConfig {
    /// Where the vertical axis starts scrolled to when first laid out.
    pub default_vertical_position: ScrollPosition,
    /// Where the horizontal axis starts scrolled to when first laid out.
    pub default_horizontal_position: ScrollPosition,
}

/// A pending request to scroll an axis to a given [`ScrollPosition`], consumed on the next layout.
pub struct ScrollRequest {
    pub(crate) position: ScrollPosition,
    pub(crate) direction: Direction,
    pub(crate) init: bool,
}

impl ScrollRequest {
    /// Creates a request to scroll `direction` to `position`.
    pub fn new(position: ScrollPosition, direction: Direction) -> ScrollRequest {
        ScrollRequest {
            position,
            direction,
            init: false,
        }
    }
}

/// An absolute scroll movement along one axis, in pixels.
pub enum ScrollEvent {
    X(i32),
    Y(i32),
}

/// Handle to drive and read a scrollable area programmatically.
///
/// By default a scrollable owns its scroll position and only the user can move it, through the
/// wheel, the scrollbar, arrow keys or dragging. A [`ScrollController`] lets your own code read and
/// change that position instead. Create one with [`use_scroll_controller`] and hand it to a
/// scrollable through its `new_controlled` constructor.
///
/// Some cases where a controller is needed:
///
/// - Jumping to the top or bottom in response to an action, for example scrolling a chat to the
///   newest message after sending one.
/// - Keeping several scrollables in sync, like a diff view with two panes that move together.
/// - Reading the current scroll position to drive something else, such as a "scroll to top" button
///   that only appears once the user has scrolled down.
///
/// # Scrolling from code
///
/// [`scroll_to`](ScrollController::scroll_to) queues a jump to the start or end of an axis, applied
/// on the next layout. This is the common way to snap a list to its top or bottom.
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
/// # Keeping scrollables in sync
///
/// Because a [`ScrollController`] is a cheap [`Copy`] handle, pass the same one to several
/// scrollables and they share a single scroll position: moving any of them moves the rest.
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
    notifier: State<()>,
    requests: State<Vec<ScrollRequest>>,
    on_scroll: State<Callback<ScrollEvent, bool>>,
    get_scroll: State<Callback<(), (i32, i32)>>,
    /// The scrollable's current viewport rectangle (window space), refreshed by the scrollable each
    /// layout via [`set_viewport`](Self::set_viewport). Lets [`scroll_to_item`](Self::scroll_to_item)
    /// reveal a target from its own measured rectangle without the caller knowing the viewport.
    viewport: State<Area>,
    /// The current scroll position, mirroring [`on_scroll`](Self::on_scroll)/[`get_scroll`](Self::get_scroll).
    /// Held here so [`scroll_to_item`](Self::scroll_to_item) can *peek* it: that method is imperative,
    /// so reading via `get_scroll` inside a reactive effect would subscribe the effect to the scroll
    /// and loop it against its own write.
    scroll: State<(i32, i32)>,
    /// The scrollable's last content (inner) size `(width, height)`, refreshed every layout by the
    /// scrollable via [`use_apply`](Self::use_apply). Paired with [`viewport`](Self::viewport) it
    /// answers [`is_scrollable`](Self::is_scrollable), so a consumer holding the controller can gate
    /// UI on whether the area actually overflows, without measuring it a second time.
    inner: State<(f32, f32)>,
    /// Whether [`scroll`](Self::scroll) has to be refreshed from [`get_scroll`](Self::get_scroll),
    /// which is true only of a [`managed`](Self::managed) controller: the one [`new`](Self::new)
    /// builds reads its own mirror, so refreshing it would compare a value with itself.
    mirrored: bool,
}

impl From<ScrollController> for (i32, i32) {
    /// Reads the current `(x, y)` scroll position in pixels.
    fn from(val: ScrollController) -> Self {
        val.get_scroll.read().call(())
    }
}

impl ScrollController {
    /// Creates a controller starting at scroll position `(x, y)` with a list of requests to apply.
    pub fn new(x: i32, y: i32, initial_requests: Vec<ScrollRequest>) -> Self {
        let mut scroll = State::create((x, y));
        Self {
            notifier: State::create(()),
            requests: State::create(initial_requests),
            on_scroll: State::create(Callback::new(move |ev| {
                // Peek, not read: this callback runs from `scroll_to_x`/`scroll_to_item`, which can be
                // driven inside a reactive effect. Reading here would subscribe that effect to the
                // scroll and loop it against this very write. Consumers subscribe via `get_scroll`.
                let current = *scroll.peek();
                match ev {
                    ScrollEvent::X(x) => {
                        scroll.write().0 = x;
                    }
                    ScrollEvent::Y(y) => {
                        scroll.write().1 = y;
                    }
                }
                current != *scroll.peek()
            })),
            get_scroll: State::create(Callback::new(move |_| *scroll.read())),
            viewport: State::create(Area::default()),
            scroll,
            inner: State::create((0., 0.)),
            mirrored: false,
        }
    }
    /// Builds a controller from externally owned state, letting the caller manage its storage.
    pub fn managed(
        notifier: State<()>,
        requests: State<Vec<ScrollRequest>>,
        on_scroll: State<Callback<ScrollEvent, bool>>,
        get_scroll: State<Callback<(), (i32, i32)>>,
    ) -> Self {
        Self {
            notifier,
            requests,
            on_scroll,
            get_scroll,
            viewport: State::create(Area::default()),
            scroll: State::create((0, 0)),
            inner: State::create((0., 0.)),
            mirrored: true,
        }
    }

    /// Applies any pending requests against the given content size. Called by the scrollable on every layout.
    pub fn use_apply(&mut self, width: f32, height: f32) {
        let _ = self.notifier.read();
        if self.mirrored {
            self.scroll.set_if_modified(self.get_scroll.read().call(()));
        }
        // Retain the content size so `is_scrollable` can compare it against the viewport. Guarded
        // so an unchanged layout doesn't notify overflow subscribers.
        self.inner.set_if_modified((width, height));
        for request in self.requests.write().drain(..) {
            match request {
                ScrollRequest {
                    position: ScrollPosition::Start,
                    direction: Direction::Vertical,
                    ..
                } => {
                    self.on_scroll.write().call(ScrollEvent::Y(0));
                }
                ScrollRequest {
                    position: ScrollPosition::Start,
                    direction: Direction::Horizontal,
                    ..
                } => {
                    self.on_scroll.write().call(ScrollEvent::X(0));
                }
                ScrollRequest {
                    position: ScrollPosition::End,
                    direction: Direction::Vertical,
                    init,
                    ..
                } => {
                    if init && height == 0. {
                        continue;
                    }
                    let (_x, y) = self.get_scroll.read().call(());
                    self.on_scroll
                        .write()
                        .call(ScrollEvent::Y(y - height as i32));
                }
                ScrollRequest {
                    position: ScrollPosition::End,
                    direction: Direction::Horizontal,
                    init,
                    ..
                } => {
                    if init && width == 0. {
                        continue;
                    }

                    let (x, _y) = self.get_scroll.read().call(());
                    self.on_scroll
                        .write()
                        .call(ScrollEvent::X(x - width as i32));
                }
            }
        }
    }

    /// Scrolls the horizontal axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_x(&mut self, to: i32) -> bool {
        self.on_scroll.write().call(ScrollEvent::X(to))
    }

    /// Scrolls the vertical axis to `to` pixels. Returns whether the position actually changed.
    pub fn scroll_to_y(&mut self, to: i32) -> bool {
        self.on_scroll.write().call(ScrollEvent::Y(to))
    }

    /// Queues a scroll of `scroll_direction` to `scroll_position`, applied on the next layout.
    pub fn scroll_to(&mut self, scroll_position: ScrollPosition, scroll_direction: Direction) {
        self.requests
            .write()
            .push(ScrollRequest::new(scroll_position, scroll_direction));
        self.notifier.write();
    }

    /// Records the scrollable's current viewport rectangle (window space). The scrollable calls this
    /// every layout so [`scroll_to_item`](Self::scroll_to_item) can reveal a target against it.
    pub fn set_viewport(&mut self, viewport: Area) {
        self.viewport.set_if_modified(viewport);
    }

    /// Whether the scrollable overflows its viewport along `direction`, i.e. there is content to
    /// scroll to on that axis. Reads the content size and viewport the scrollable last reported
    /// (via [`use_apply`](Self::use_apply) / [`set_viewport`](Self::set_viewport)) and subscribes
    /// the caller, so a sibling can reactively show or hide a scroll affordance as the area's
    /// content grows and shrinks. Unmeasured (zero-viewport) reads as not scrollable.
    pub fn is_scrollable(&self, direction: Direction) -> bool {
        let (inner_width, inner_height) = *self.inner.read();
        let viewport = *self.viewport.read();
        match direction {
            Direction::Horizontal => {
                crate::scrollviews::shared::is_scrollable(inner_width, viewport.width())
            }
            Direction::Vertical => {
                crate::scrollviews::shared::is_scrollable(inner_height, viewport.height())
            }
        }
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
        let (inner_width, inner_height) = *self.inner.peek();
        let viewport = *self.viewport.peek();
        let (position, inner, shown) = match direction {
            Direction::Horizontal => (self.scroll.peek().0, inner_width, viewport.width()),
            Direction::Vertical => (self.scroll.peek().1, inner_height, viewport.height()),
        };
        if !crate::scrollviews::shared::is_scrollable(inner, shown) {
            return true;
        }
        // The scroll position is negative-going: the content is offset up by how far down the
        // reader is, so the end is where that offset covers everything the viewport does not.
        (position.unsigned_abs() as f32 + shown) >= inner - 1.0
    }

    /// Scrolls the minimum amount needed to bring `item` fully into view, on whichever axes it
    /// overflows the viewport. The position is clamped against the content extent first, because the
    /// views correct it only for what they paint: the stored position can sit outside the range the
    /// content actually has, and revealing against it would answer for a viewport that is not on
    /// screen. `item` is the target's own measured window-space rectangle, e.g.
    /// straight from an [`on_sized`](freya_core::prelude::EventHandlersExt::on_sized)
    /// [`Area`](torin::prelude::Area), so the caller never has to know the viewport or scroll
    /// position. A no-op once the item is already visible, so it is safe to call every render (an
    /// item larger than the viewport aligns to its start and stops, rather than oscillating).
    ///
    /// Peeks rather than reads: it is imperative, and reading inside a reactive effect would
    /// subscribe that effect to the viewport and loop it against the `on_scroll` write.
    pub fn scroll_to_item(&mut self, item: impl Into<Area>) {
        let item = item.into();
        let viewport = *self.viewport.peek();
        // Not laid out yet: nothing meaningful to reveal against.
        if viewport.width() <= 0.0 || viewport.height() <= 0.0 {
            return;
        }
        let (x, y) = *self.scroll.peek();

        let dx = reveal_delta(
            item.min_x(),
            item.max_x(),
            viewport.min_x(),
            viewport.max_x(),
        );
        let dy = reveal_delta(
            item.min_y(),
            item.max_y(),
            viewport.min_y(),
            viewport.max_y(),
        );

        if dx != 0.0 {
            self.on_scroll
                .write()
                .call(ScrollEvent::X((x as f32 + dx).round() as i32));
        }
        if dy != 0.0 {
            self.on_scroll
                .write()
                .call(ScrollEvent::Y((y as f32 + dy).round() as i32));
        }
    }

    /// [`scroll_to_item`](Self::scroll_to_item) for a target that has no measured rectangle: scrolls
    /// the minimum amount needed to bring the span `[offset, offset + size]` of the content into view
    /// along `direction`.
    ///
    /// This is the [`VirtualScrollView`](crate::scrollviews::VirtualScrollView) half of the pair. A
    /// virtualized view only builds the items inside its viewport, so the row a caller wants to
    /// reveal usually does not exist yet and can report no [`Area`] to reveal against. What the
    /// caller does know is where the row sits in the content: for a fixed item size that is
    /// `index * item_size`. Everything else, the viewport and the current position, is this
    /// controller's own, exactly as it is for `scroll_to_item`.
    ///
    /// A no-op once the span is already visible, so it is safe to call every render.
    ///
    /// **Imperative, so it is only meaningful once the scrollable has been laid out**: before the
    /// first layout there is no viewport and no content extent to reveal against, and the call does
    /// nothing. Reveal from a gesture, or from an effect that runs once the target could have been
    /// drawn. A caller that must move the view on the frame it mounts wants
    /// [`scroll_to`](Self::scroll_to), whose request is queued and applied at the next layout.
    ///
    /// The content spans `0..inner` along the axis and the position is negative-going, so the
    /// visible span of the content starts at `-position`. Everything is peeked rather than read,
    /// for [`scroll_to_item`](Self::scroll_to_item)'s reason.
    pub fn scroll_to_offset(&mut self, offset: f32, size: f32, direction: Direction) {
        let viewport = *self.viewport.peek();
        let (x, y) = *self.scroll.peek();
        let (inner_width, inner_height) = *self.inner.peek();
        let (position, shown, inner) = match direction {
            Direction::Horizontal => (x as f32, viewport.width(), inner_width),
            Direction::Vertical => (y as f32, viewport.height(), inner_height),
        };
        if shown <= 0.0 || inner <= 0.0 {
            return;
        }
        let position = get_corrected_scroll_position(inner, shown, position);
        let delta = reveal_delta(offset, offset + size, -position, -position + shown);
        if delta == 0.0 {
            return;
        }
        let to = (position + delta).round() as i32;
        self.on_scroll.write().call(match direction {
            Direction::Horizontal => ScrollEvent::X(to),
            Direction::Vertical => ScrollEvent::Y(to),
        });
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

/// Creates a [`ScrollController`] tied to the component, configured by the returned [`ScrollConfig`].
pub fn use_scroll_controller(init: impl FnOnce() -> ScrollConfig) -> ScrollController {
    use_hook(|| {
        let config = init();

        ScrollController::new(
            0,
            0,
            vec![
                ScrollRequest {
                    position: config.default_vertical_position,
                    direction: Direction::Vertical,
                    init: true,
                },
                ScrollRequest {
                    position: config.default_horizontal_position,
                    direction: Direction::Horizontal,
                    init: true,
                },
            ],
        )
    })
}
