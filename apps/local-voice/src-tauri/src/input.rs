use enigo::{Enigo, Key, Keyboard, Mouse, Settings};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ReplacementContext {
    pub(crate) foreground: isize,
    pub(crate) focus: isize,
    pub(crate) physical_generation: u64,
}

#[cfg(test)]
pub(crate) fn replacement_context_matches(
    captured: ReplacementContext,
    current: ReplacementContext,
) -> bool {
    captured == current
}

/// Wrapper for Enigo to store in Tauri's managed state.
/// Enigo is wrapped in a Mutex since it requires mutable access.
pub struct EnigoState(pub Mutex<Enigo>);

impl EnigoState {
    pub fn new() -> Result<Self, String> {
        let enigo = Enigo::new(&Settings::default())
            .map_err(|e| format!("Failed to initialize Enigo: {}", e))?;
        Ok(Self(Mutex::new(enigo)))
    }
}

/// Get the current mouse cursor position using the managed Enigo instance.
/// Returns None if the state is not available or if getting the location fails.
pub fn get_cursor_position(app_handle: &AppHandle) -> Option<(i32, i32)> {
    let enigo_state = app_handle.try_state::<EnigoState>()?;
    let enigo = enigo_state.0.lock().ok()?;
    enigo.location().ok()
}

/// Sends a Ctrl+V or Cmd+V paste command using platform-specific virtual key codes.
/// This ensures the paste works regardless of keyboard layout (e.g., Russian, AZERTY, DVORAK).
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9));
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press modifier + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Ctrl+Shift+V paste command.
/// This is commonly used in terminal applications on Linux to paste without formatting.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_ctrl_shift_v(enigo: &mut Enigo) -> Result<(), String> {
    // Platform-specific key definitions
    #[cfg(target_os = "macos")]
    let (modifier_key, v_key_code) = (Key::Meta, Key::Other(9)); // Cmd+Shift+V on macOS
    #[cfg(target_os = "windows")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Other(0x56)); // VK_V
    #[cfg(target_os = "linux")]
    let (modifier_key, v_key_code) = (Key::Control, Key::Unicode('v'));

    // Press Ctrl/Cmd + Shift + V
    enigo
        .key(modifier_key, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press modifier key: {}", e))?;
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(v_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click V key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;
    enigo
        .key(modifier_key, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release modifier key: {}", e))?;

    Ok(())
}

/// Sends a Shift+Insert paste command (Windows and Linux only).
/// This is more universal for terminal applications and legacy software.
/// Note: On Wayland, this may not work - callers should check for Wayland and use alternative methods.
pub fn send_paste_shift_insert(enigo: &mut Enigo) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    let insert_key_code = Key::Other(0x2D); // VK_INSERT
    #[cfg(not(target_os = "windows"))]
    let insert_key_code = Key::Other(0x76); // XK_Insert (keycode 118 / 0x76, also used as fallback)

    // Press Shift + Insert
    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|e| format!("Failed to press Shift key: {}", e))?;
    enigo
        .key(insert_key_code, enigo::Direction::Click)
        .map_err(|e| format!("Failed to click Insert key: {}", e))?;

    std::thread::sleep(std::time::Duration::from_millis(100));

    enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|e| format!("Failed to release Shift key: {}", e))?;

    Ok(())
}

/// Pastes text directly using the enigo text method.
/// This tries to use system input methods if possible, otherwise simulates keystrokes one by one.
pub fn paste_text_direct(enigo: &mut Enigo, text: &str) -> Result<(), String> {
    enigo
        .text(text)
        .map_err(|e| format!("Failed to send text directly: {}", e))?;

    Ok(())
}

pub(crate) fn send_select_left(enigo: &mut Enigo, count: usize) -> Result<(), String> {
    if count == 0 {
        return Err("Cannot select an empty replacement range".to_string());
    }

    enigo
        .key(Key::Shift, enigo::Direction::Press)
        .map_err(|error| format!("Failed to press Shift: {error}"))?;

    let mut selection_error = None;
    for _ in 0..count {
        if let Err(error) = enigo.key(Key::LeftArrow, enigo::Direction::Click) {
            selection_error = Some(format!("Failed to extend replacement selection: {error}"));
            break;
        }
    }
    let release_result = enigo
        .key(Key::Shift, enigo::Direction::Release)
        .map_err(|error| format!("Failed to release Shift: {error}"));

    if let Some(error) = selection_error {
        return Err(error);
    }
    release_result
}

#[cfg(target_os = "windows")]
pub(crate) fn capture_replacement_context() -> Option<ReplacementContext> {
    windows_input_monitor::capture()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn capture_replacement_context() -> Option<ReplacementContext> {
    None
}

#[cfg(target_os = "windows")]
mod windows_input_monitor {
    use super::ReplacementContext;
    use std::mem::size_of;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{mpsc, OnceLock};
    use std::time::Duration;
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::{HINSTANCE, LPARAM, LRESULT, WPARAM};
    use windows::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetForegroundWindow, GetGUIThreadInfo, GetMessageW,
        SetWindowsHookExW, TranslateMessage, UnhookWindowsHookEx, GUITHREADINFO, HC_ACTION,
        KBDLLHOOKSTRUCT, LLKHF_INJECTED, LLMHF_INJECTED, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
        WH_MOUSE_LL,
    };

    static PHYSICAL_INPUT_GENERATION: AtomicU64 = AtomicU64::new(0);
    static MONITOR_AVAILABLE: OnceLock<bool> = OnceLock::new();

    pub(super) fn capture() -> Option<ReplacementContext> {
        if !monitor_available() {
            return None;
        }

        let foreground = unsafe { GetForegroundWindow() };
        if foreground.0.is_null() {
            return None;
        }

        let mut gui = GUITHREADINFO {
            cbSize: size_of::<GUITHREADINFO>() as u32,
            ..Default::default()
        };
        if unsafe { GetGUIThreadInfo(0, &mut gui) }.is_err() || gui.hwndFocus.0.is_null() {
            return None;
        }

        Some(ReplacementContext {
            foreground: foreground.0 as isize,
            focus: gui.hwndFocus.0 as isize,
            physical_generation: PHYSICAL_INPUT_GENERATION.load(Ordering::Acquire),
        })
    }

    fn monitor_available() -> bool {
        *MONITOR_AVAILABLE.get_or_init(|| {
            let (ready_tx, ready_rx) = mpsc::sync_channel(1);
            std::thread::spawn(move || run_monitor(ready_tx));
            ready_rx
                .recv_timeout(Duration::from_secs(1))
                .unwrap_or(false)
        })
    }

    fn run_monitor(ready: mpsc::SyncSender<bool>) {
        let module = match unsafe { GetModuleHandleW(PCWSTR::null()) } {
            Ok(module) => HINSTANCE(module.0),
            Err(_) => {
                let _ = ready.send(false);
                return;
            }
        };
        let keyboard = match unsafe {
            SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_hook), Some(module), 0)
        } {
            Ok(hook) => hook,
            Err(_) => {
                let _ = ready.send(false);
                return;
            }
        };
        let mouse =
            match unsafe { SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), Some(module), 0) } {
                Ok(hook) => hook,
                Err(_) => {
                    let _ = unsafe { UnhookWindowsHookEx(keyboard) };
                    let _ = ready.send(false);
                    return;
                }
            };

        let _ = ready.send(true);
        let mut message = MSG::default();
        while unsafe { GetMessageW(&mut message, None, 0, 0) }.0 > 0 {
            let _ = unsafe { TranslateMessage(&message) };
            unsafe { DispatchMessageW(&message) };
        }
        let _ = unsafe { UnhookWindowsHookEx(mouse) };
        let _ = unsafe { UnhookWindowsHookEx(keyboard) };
    }

    unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
            if !event.flags.contains(LLKHF_INJECTED) {
                PHYSICAL_INPUT_GENERATION.fetch_add(1, Ordering::AcqRel);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    unsafe extern "system" fn mouse_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HC_ACTION as i32 {
            let event = unsafe { &*(lparam.0 as *const MSLLHOOKSTRUCT) };
            if event.flags & LLMHF_INJECTED == 0 {
                PHYSICAL_INPUT_GENERATION.fetch_add(1, Ordering::AcqRel);
            }
        }
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }
}

#[cfg(test)]
mod tests {
    use super::{replacement_context_matches, ReplacementContext};

    #[test]
    fn replacement_context_requires_same_window_focus_and_input_generation() {
        let captured = ReplacementContext {
            foreground: 11,
            focus: 22,
            physical_generation: 7,
        };

        assert!(replacement_context_matches(captured, captured));
        assert!(!replacement_context_matches(
            captured,
            ReplacementContext {
                foreground: 12,
                ..captured
            }
        ));
        assert!(!replacement_context_matches(
            captured,
            ReplacementContext {
                focus: 23,
                ..captured
            }
        ));
        assert!(!replacement_context_matches(
            captured,
            ReplacementContext {
                physical_generation: 8,
                ..captured
            }
        ));
    }
}
