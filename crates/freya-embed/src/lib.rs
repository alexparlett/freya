//! Freya driven by a host that owns the loop, the GL context and the input, as a Wayland
//! compositor drawing its own shell does.
//!
//! The host creates one [`Fonts`] and, on GL, one [`Gpu`], and an [`Embedded`] per surface. It
//! feeds each surface Freya's own [`PlatformEvent`]s,
//! calls [`Embedded::update`] whenever the surface's waker fires or it was given events, draws
//! it when the update asks into a target it owns, and calls [`Embedded::presented`] once that
//! frame is on screen.

mod embedded;
mod fonts;
mod gpu;

pub use embedded::{
    EmbedConfig,
    Embedded,
    Update,
};
pub use fonts::Fonts;
pub use freya_core::integration::{
    AccessibilityFocusMovement,
    AccessibilityFocusStrategy,
    AccessibilityId,
    AppComponent,
    KeyboardEventName,
    MouseEventName,
    NavigationMode,
    PlatformEvent,
    Runner,
    WheelEventName,
};
pub use gpu::RasterTarget;
#[cfg(feature = "gl")]
pub use gpu::{
    GlTarget,
    Gpu,
};
