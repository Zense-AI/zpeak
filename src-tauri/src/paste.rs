//! Puts the transcript into whatever text box has focus: clipboard + a
//! synthetic paste keystroke, then the previous clipboard is restored.

use arboard::Clipboard;
use enigo::{Direction, Enigo, Key, Keyboard, Settings};
use std::time::Duration;

enum Saved {
    Text(String),
    Image(arboard::ImageData<'static>),
    Nothing,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Pasted,
    /// Text is on the clipboard but we could not send the keystroke
    /// (macOS Accessibility not granted).
    CopiedOnly,
}

#[cfg(target_os = "macos")]
mod mac {
    use std::ffi::c_void;

    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
        fn AXIsProcessTrustedWithOptions(options: *const c_void) -> bool;
    }
    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        static kCFBooleanTrue: *const c_void;
        static kCFTypeDictionaryKeyCallBacks: c_void;
        static kCFTypeDictionaryValueCallBacks: c_void;
        fn CFDictionaryCreate(
            allocator: *const c_void,
            keys: *const *const c_void,
            values: *const *const c_void,
            num: isize,
            key_cb: *const c_void,
            value_cb: *const c_void,
        ) -> *const c_void;
        fn CFRelease(cf: *const c_void);
    }
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        static kAXTrustedCheckOptionPrompt: *const c_void;
    }

    pub fn trusted() -> bool {
        unsafe { AXIsProcessTrusted() }
    }

    /// Shows the system "allow Accessibility" prompt once.
    pub fn prompt() -> bool {
        unsafe {
            let keys = [kAXTrustedCheckOptionPrompt];
            let values = [kCFBooleanTrue];
            let dict = CFDictionaryCreate(
                std::ptr::null(),
                keys.as_ptr(),
                values.as_ptr(),
                1,
                &kCFTypeDictionaryKeyCallBacks as *const _ as *const c_void,
                &kCFTypeDictionaryValueCallBacks as *const _ as *const c_void,
            );
            let ok = AXIsProcessTrustedWithOptions(dict);
            CFRelease(dict);
            ok
        }
    }
}

pub fn accessibility_granted() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::trusted()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

pub fn request_accessibility() -> bool {
    #[cfg(target_os = "macos")]
    {
        mac::prompt()
    }
    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

/// Windows: an Alt (or Win) hotkey whose letter is swallowed by
/// `RegisterHotKey` leaves the focused app seeing a lone Alt press, which
/// opens its menu bar on release and eats our paste. Tapping an unassigned
/// virtual key while the modifier is still down cancels that (same trick
/// AutoHotkey uses with vkE8).
pub fn mask_menu_activation() {
    #[cfg(target_os = "windows")]
    if let Ok(mut enigo) = Enigo::new(&Settings::default()) {
        let _ = enigo.key(Key::Other(0xE8), Direction::Click);
    }
}

pub fn insert(text: &str, restore: bool) -> Result<Outcome, String> {
    let mut clipboard = Clipboard::new().map_err(|_| "ClipboardUnavailable".to_string())?;
    let saved = if restore {
        match clipboard.get_text() {
            Ok(t) => Saved::Text(t),
            Err(_) => clipboard
                .get_image()
                .map(Saved::Image)
                .unwrap_or(Saved::Nothing),
        }
    } else {
        Saved::Nothing
    };

    clipboard
        .set_text(text.to_string())
        .map_err(|_| "ClipboardUnavailable".to_string())?;

    if !accessibility_granted() {
        return Ok(Outcome::CopiedOnly);
    }

    std::thread::sleep(Duration::from_millis(60));
    let modifier = if cfg!(target_os = "macos") { Key::Meta } else { Key::Control };
    let mut enigo = Enigo::new(&Settings::default()).map_err(|_| "KeyboardUnavailable".to_string())?;
    // Let go of any modifier still held from the shortcut so we send a clean paste.
    for held in [Key::Alt, Key::Shift] {
        let _ = enigo.key(held, Direction::Release);
    }
    enigo
        .key(modifier, Direction::Press)
        .and_then(|_| enigo.key(Key::Unicode('v'), Direction::Click))
        .and_then(|_| enigo.key(modifier, Direction::Release))
        .map_err(|_| "KeyboardUnavailable".to_string())?;

    if restore {
        // The target app reads the clipboard asynchronously after the keystroke.
        std::thread::sleep(Duration::from_millis(400));
        match saved {
            Saved::Text(t) => {
                let _ = clipboard.set_text(t);
            }
            Saved::Image(img) => {
                let _ = clipboard.set_image(img);
            }
            Saved::Nothing => {}
        }
    }
    Ok(Outcome::Pasted)
}
