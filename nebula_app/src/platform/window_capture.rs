//! A bounded, transient client-area snapshot for native theme transitions.

use gpui::Window;
use std::io;

pub(crate) struct ClientFrame {
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) bgra: Vec<u8>,
}

pub(crate) fn supported() -> bool {
    cfg!(windows)
}

#[cfg(not(windows))]
pub(crate) fn capture(_window: &Window) -> io::Result<ClientFrame> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "Client-area theme snapshots are not supported on this platform",
    ))
}

#[cfg(windows)]
pub(crate) fn capture(window: &Window) -> io::Result<ClientFrame> {
    use std::ptr;
    use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
    use windows_sys::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BitBlt, ClientToScreen, CreateCompatibleDC,
        CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC,
        HGDIOBJ, RGBQUAD, ReleaseDC, SRCCOPY, SelectObject,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClientRect, GetForegroundWindow, GetSystemMetrics, IsIconic, SM_CXVIRTUALSCREEN,
        SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };
    use winit::raw_window_handle::{HasWindowHandle, RawWindowHandle};

    struct Capture {
        window: HWND,
        source: HDC,
        memory: HDC,
        bitmap: HBITMAP,
        previous: HGDIOBJ,
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            // SAFETY: every non-null handle is owned here; restore before deletion.
            unsafe {
                if !self.previous.is_null() {
                    SelectObject(self.memory, self.previous);
                }
                if !self.bitmap.is_null() {
                    DeleteObject(self.bitmap);
                }
                if !self.memory.is_null() {
                    DeleteDC(self.memory);
                }
                if !self.source.is_null() {
                    ReleaseDC(self.window, self.source);
                }
            }
        }
    }

    let handle = HasWindowHandle::window_handle(window).map_err(|error| {
        io::Error::other(format!("Could not access the client window: {error}"))
    })?;
    let RawWindowHandle::Win32(handle) = handle.as_raw() else {
        return Err(io::Error::other("Expected a Win32 client window"));
    };
    let hwnd = handle.hwnd.get() as HWND;
    let mut rect = RECT { left: 0, top: 0, right: 0, bottom: 0 };
    // SAFETY: hwnd is borrowed from the live GPUI window; rect is writable.
    if unsafe {
        GetForegroundWindow() != hwnd || IsIconic(hwnd) != 0 || GetClientRect(hwnd, &mut rect) == 0
    } {
        return Err(io::Error::other("The client window is not drawable"));
    }
    let width = u32::try_from(rect.right - rect.left)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let height = u32::try_from(rect.bottom - rect.top)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    let bytes = u64::from(width) * u64::from(height) * 4;
    if width == 0 || height == 0 || width > 4096 || height > 4096 || bytes > 32 * 1024 * 1024 {
        return Err(io::Error::other("Client snapshot exceeds the 32 MiB transition budget"));
    }
    let mut origin = POINT { x: 0, y: 0 };
    // SAFETY: hwnd is live; metrics and coordinates are device pixels on the
    // application's DPI-aware UI thread. Capture only its visible client area.
    unsafe {
        if ClientToScreen(hwnd, &mut origin) == 0
            || !client_is_on_screen(
                (origin.x, origin.y, width, height),
                (
                    GetSystemMetrics(SM_XVIRTUALSCREEN),
                    GetSystemMetrics(SM_YVIRTUALSCREEN),
                    GetSystemMetrics(SM_CXVIRTUALSCREEN),
                    GetSystemMetrics(SM_CYVIRTUALSCREEN),
                ),
            )
        {
            return Err(io::Error::other("The client area is not fully on screen"));
        }
    }
    let mut capture = Capture {
        window: ptr::null_mut(),
        source: ptr::null_mut(),
        memory: ptr::null_mut(),
        bitmap: ptr::null_mut(),
        previous: ptr::null_mut(),
    };
    let mut pixels = ptr::null_mut();
    let bitmap_info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            biSizeImage: bytes as u32,
            biXPelsPerMeter: 0,
            biYPelsPerMeter: 0,
            biClrUsed: 0,
            biClrImportant: 0,
        },
        bmiColors: [RGBQUAD { rgbBlue: 0, rgbGreen: 0, rgbRed: 0, rgbReserved: 0 }],
    };
    // SAFETY: handles and the top-down 32-bit DIB are owned by Capture; the
    // checked size describes the accessible pixel buffer until bitmap deletion.
    unsafe {
        // DirectComposition windows have no GDI redirection bitmap. Read the
        // presented screen pixels rather than an empty window device context.
        capture.source = GetDC(ptr::null_mut());
        if capture.source.is_null() {
            return Err(io::Error::other("Could not obtain the client device context"));
        }
        capture.memory = CreateCompatibleDC(capture.source);
        if capture.memory.is_null() {
            return Err(io::Error::other("Could not create the snapshot device context"));
        }
        capture.bitmap = CreateDIBSection(
            capture.source,
            &bitmap_info,
            DIB_RGB_COLORS,
            &mut pixels,
            ptr::null_mut(),
            0,
        );
        if capture.bitmap.is_null() || pixels.is_null() {
            return Err(io::Error::other("Could not allocate the snapshot bitmap"));
        }
        capture.previous = SelectObject(capture.memory, capture.bitmap);
        if capture.previous.is_null() || capture.previous as isize == -1 {
            capture.previous = ptr::null_mut();
            return Err(io::Error::other("Could not select the snapshot bitmap"));
        }
        if BitBlt(
            capture.memory,
            0,
            0,
            width as i32,
            height as i32,
            capture.source,
            origin.x,
            origin.y,
            SRCCOPY,
        ) == 0
        {
            return Err(io::Error::other("Could not capture the client pixels"));
        }
        if GdiFlush() == 0 {
            return Err(io::Error::other("Could not finish the client snapshot"));
        }
        let mut bgra = std::slice::from_raw_parts(pixels.cast::<u8>(), bytes as usize).to_vec();
        if bgra.chunks_exact(4).all(|pixel| pixel[..3] == [0, 0, 0]) {
            return Err(io::Error::other("Client capture returned an empty frame"));
        }
        for pixel in bgra.chunks_exact_mut(4) {
            pixel[3] = 255;
        }
        Ok(ClientFrame { width, height, bgra })
    }
}

#[cfg(windows)]
fn client_is_on_screen(client: (i32, i32, u32, u32), screen: (i32, i32, i32, i32)) -> bool {
    let (x, y, width, height) = client;
    let (left, top, screen_width, screen_height) = screen;
    screen_width > 0
        && screen_height > 0
        && x >= left
        && y >= top
        && i64::from(x) + i64::from(width) <= i64::from(left) + i64::from(screen_width)
        && i64::from(y) + i64::from(height) <= i64::from(top) + i64::from(screen_height)
}

#[cfg(all(test, windows))]
mod tests {
    use super::client_is_on_screen;

    #[test]
    fn client_capture_stays_inside_the_virtual_screen_including_negative_origins() {
        let screen = (-1920, -1080, 3840, 2160);
        assert!(client_is_on_screen((-1920, -1080, 1920, 1080), screen));
        assert!(client_is_on_screen((0, 0, 1920, 1080), screen));
        for client in [(-1921, 0, 20, 20), (0, -1081, 20, 20), (1910, 0, 20, 20), (0, 1070, 20, 20)]
        {
            assert!(!client_is_on_screen(client, screen));
        }
        assert!(!client_is_on_screen((i32::MAX, 0, 4096, 100), (0, 0, i32::MAX, 1080)));
        assert!(!client_is_on_screen((0, 0, 20, 20), (0, 0, 0, 1080)));
    }
}
