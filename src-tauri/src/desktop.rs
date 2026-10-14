//! Keep the widget above Explorer's desktop without making it always-on-top.
use windows_sys::Win32::{Foundation::HWND, UI::WindowsAndMessaging::*};

fn desktop_class(name: &str) -> bool {
    matches!(name, "Progman" | "WorkerW")
}

/// Called on the UI thread. Explicit user hiding is checked by the caller.
/// Never activates the widget or changes its normal (non-topmost) band.
pub fn preserve_on_desktop(hwnd: HWND) {
    unsafe {
        if IsWindow(hwnd) == 0 {
            return;
        }
        let foreground = GetForegroundWindow();
        if foreground.is_null() || foreground == hwnd {
            return;
        }
        let root = GetAncestor(foreground, GA_ROOT);
        let mut class = [0u16; 128];
        let count = GetClassNameW(root, class.as_mut_ptr(), class.len() as i32);
        if count <= 0 || !desktop_class(&String::from_utf16_lossy(&class[..count as usize])) {
            return;
        }
        // Show Desktop may minimize a normal top-level window. Restore it only
        // while Explorer's desktop is foreground and without stealing focus.
        if IsIconic(hwnd) != 0 {
            ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        }
        SetWindowPos(
            hwnd,
            HWND_TOP,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_NOOWNERZORDER | SWP_SHOWWINDOW,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_explorer_desktop_triggers_recovery() {
        assert!(desktop_class("Progman"));
        assert!(desktop_class("WorkerW"));
        assert!(!desktop_class("CabinetWClass")); // File Explorer window
        assert!(!desktop_class("Shell_TrayWnd")); // Taskbar
        assert!(!desktop_class("Chrome_WidgetWin_1"));
    }
}
