//! 原生图像后端的窗口事实与编译入口；UI 保留自己的时钟、取消和资源归属。
use gpui::Window;
use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

/// 当前经过产品接线验证的流式图像后端；编译功能不等于授权开启效果。
pub(crate) const SUPPORTED: bool = cfg!(windows);

pub(crate) fn window_token(window: &Window) -> Option<isize> {
    match HasWindowHandle::window_handle(window).ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

pub(crate) fn is_visible(native: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::{IsIconic, IsWindowVisible};
        let hwnd = native as *mut std::ffi::c_void;
        native != 0 && unsafe { IsWindowVisible(hwnd) != 0 && IsIconic(hwnd) == 0 }
    }
    #[cfg(not(windows))]
    {
        let _ = native;
        false
    }
}

pub(crate) fn is_foreground(native: isize) -> bool {
    #[cfg(windows)]
    {
        use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
        native != 0 && unsafe { GetForegroundWindow() == native as *mut std::ffi::c_void }
    }
    #[cfg(not(windows))]
    {
        let _ = native;
        false
    }
}

#[cfg(all(windows, feature = "shader-background"))]
mod hlsl;
#[cfg(all(windows, feature = "shader-background"))]
pub(crate) use hlsl::compile as compile_hlsl;

#[cfg(all(not(windows), feature = "shader-background"))]
pub(crate) fn compile_hlsl(_source: &str, _entry: &str) -> anyhow::Result<std::sync::Arc<[u8]>> {
    anyhow::bail!("native background shader compilation is unavailable on this platform")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_native_window_never_allows_animation() {
        assert!(!is_visible(0));
        assert!(!is_foreground(0));
    }

    #[cfg(all(not(windows), feature = "shader-background"))]
    #[test]
    fn unsupported_backend_reports_no_native_bytecode() {
        assert!(!SUPPORTED);
        assert!(compile_hlsl("not read or compiled", "main").is_err());
    }
}
