use freya_clipboard::prelude::{
    Clipboard,
    ClipboardContext,
    ClipboardProvider,
};

/// Open the desktop clipboard once for the whole renderer, so every window shares one connection.
///
/// [`ClipboardContext`] is arboard, which picks its own Wayland or X11 backend from the session,
/// so the renderer has no display handle to hand it. Where there is no clipboard to open (Android,
/// a headless host) there is no provider and every [`Clipboard`] call reports that.
pub(crate) fn create_clipboard() -> Clipboard {
    let provider = ClipboardContext::new()
        .ok()
        .map(|clipboard| Box::new(clipboard) as Box<dyn ClipboardProvider>);
    Clipboard::create(provider)
}
