use freya_clipboard::prelude::{
    ClipboardError,
    ClipboardImage,
    ClipboardProvider,
};

use crate::emscripten::{
    run_script,
    run_script_string,
};

/// Clipboard backed by the hidden IME input.
///
/// Text only: the browser hands a paste over as a string through the IME, with no image data to
/// read or write, so the image half of the provider reports that there is none.
pub struct WebClipboard;

impl ClipboardProvider for WebClipboard {
    fn get_text(&mut self) -> Result<String, ClipboardError> {
        run_script_string("window.__freyaClipboardPaste;").ok_or(ClipboardError::FailedToRead)
    }

    fn set_text(&mut self, contents: String) -> Result<(), ClipboardError> {
        run_script(&format!("window.__freyaIme.copy({contents:?});"));

        Ok(())
    }

    fn get_image(&mut self) -> Result<ClipboardImage, ClipboardError> {
        Err(ClipboardError::NotAvailable)
    }

    fn set_image(&mut self, _image: ClipboardImage) -> Result<(), ClipboardError> {
        Err(ClipboardError::NotAvailable)
    }
}
