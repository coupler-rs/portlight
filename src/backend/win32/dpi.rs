use std::mem;

use windows::core::{HRESULT, PCSTR};
use windows::Win32::Foundation::{BOOL, FALSE, HWND, POINT, RECT, S_OK};
use windows::Win32::Graphics::Gdi::{
    GetDC, GetDeviceCaps, MonitorFromPoint, MonitorFromWindow, ReleaseDC, HDC, HMONITOR,
    LOGPIXELSX, MONITOR_DEFAULTTONEAREST, MONITOR_DEFAULTTOPRIMARY,
};
use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryA};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, MDT_EFFECTIVE_DPI, MONITOR_DPI_TYPE,
    PROCESS_DPI_AWARENESS, PROCESS_PER_MONITOR_DPI_AWARE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    USER_DEFAULT_SCREEN_DPI, WINDOW_EX_STYLE, WINDOW_STYLE,
};

macro_rules! c_str {
    ($str:literal) => {
        concat!($str, "\0").as_ptr()
    };
}

#[allow(non_snake_case)]
pub struct DpiFns {
    pub SetProcessDPIAware: Option<unsafe extern "system" fn() -> BOOL>,
    pub _IsProcessDPIAware: Option<unsafe extern "system" fn() -> BOOL>,
    pub SetProcessDpiAwareness:
        Option<unsafe extern "system" fn(value: PROCESS_DPI_AWARENESS) -> HRESULT>,
    pub SetProcessDpiAwarenessContext:
        Option<unsafe extern "system" fn(value: DPI_AWARENESS_CONTEXT) -> BOOL>,
    pub GetDpiForMonitor: Option<
        unsafe extern "system" fn(
            hmonitor: HMONITOR,
            dpitype: MONITOR_DPI_TYPE,
            dpix: *mut u32,
            dpiy: *mut u32,
        ) -> HRESULT,
    >,
    pub GetDpiForWindow: Option<unsafe extern "system" fn(hwnd: HWND) -> u32>,
    pub EnableNonClientDpiScaling: Option<unsafe extern "system" fn(hwnd: HWND) -> BOOL>,
    pub _AdjustWindowRectExForDpi: Option<
        unsafe extern "system" fn(
            lprect: *mut RECT,
            dwstyle: WINDOW_STYLE,
            bmenu: BOOL,
            dwexstyle: WINDOW_EX_STYLE,
            dpi: u32,
        ) -> BOOL,
    >,
}

impl DpiFns {
    pub fn load() -> DpiFns {
        macro_rules! load {
            ($lib:expr, $symbol:literal) => {
                if let Ok(lib) = $lib {
                    mem::transmute(GetProcAddress(lib, PCSTR(c_str!($symbol))))
                } else {
                    None
                }
            };
        }

        unsafe {
            let user32 = LoadLibraryA(PCSTR(c_str!("user32.dll")));
            let shcore = LoadLibraryA(PCSTR(c_str!("shcore.dll")));

            DpiFns {
                SetProcessDPIAware: load!(user32, "SetProcessDPIAware"),
                _IsProcessDPIAware: load!(user32, "IsProcessDPIAware"),
                SetProcessDpiAwareness: load!(shcore, "SetProcessDpiAwareness"),
                SetProcessDpiAwarenessContext: load!(user32, "SetProcessDpiAwarenessContext"),
                GetDpiForMonitor: load!(shcore, "GetDpiForMonitor"),
                GetDpiForWindow: load!(user32, "GetDpiForWindow"),
                EnableNonClientDpiScaling: load!(user32, "EnableNonClientDpiScaling"),
                _AdjustWindowRectExForDpi: load!(user32, "AdjustWindowRectExForDpi"),
            }
        }
    }

    pub fn set_dpi_aware(&self) {
        #[allow(non_snake_case)]
        unsafe {
            if let Some(SetProcessDpiAwarenessContext) = self.SetProcessDpiAwarenessContext {
                let res = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

                if res == FALSE {
                    SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE);
                }
            } else if let Some(SetProcessDpiAwareness) = self.SetProcessDpiAwareness {
                let _ = SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
            } else if let Some(SetProcessDPIAware) = self.SetProcessDPIAware {
                SetProcessDPIAware();
            }
        }
    }

    pub unsafe fn dpi_for_window(&self, hwnd: HWND) -> u32 {
        #![allow(non_snake_case)]

        if let Some(GetDpiForWindow) = self.GetDpiForWindow {
            let dpi = GetDpiForWindow(hwnd);
            if dpi != 0 {
                return dpi;
            }
        }

        if let Some(GetDpiForMonitor) = self.GetDpiForMonitor {
            let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
            if monitor != HMONITOR(0) {
                let mut dpi_x = 0;
                let mut dpi_y = 0;
                let res = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                if res == S_OK {
                    return dpi_x;
                }
            }
        }

        let hdc = GetDC(HWND(0));
        if hdc != HDC(0) {
            let dpi = GetDeviceCaps(hdc, LOGPIXELSX) as u32;
            ReleaseDC(HWND(0), hdc);

            return dpi;
        }

        USER_DEFAULT_SCREEN_DPI
    }

    pub unsafe fn dpi_for_primary_monitor(&self) -> u32 {
        #![allow(non_snake_case)]

        if let Some(GetDpiForMonitor) = self.GetDpiForMonitor {
            // The primary monitor is located at (0,0).
            let monitor = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTOPRIMARY);
            if monitor != HMONITOR(0) {
                let mut dpi_x = 0;
                let mut dpi_y = 0;
                let res = GetDpiForMonitor(monitor, MDT_EFFECTIVE_DPI, &mut dpi_x, &mut dpi_y);
                if res == S_OK {
                    return dpi_x;
                }
            }
        }

        let hdc = GetDC(HWND(0));
        if hdc != HDC(0) {
            let dpi = GetDeviceCaps(hdc, LOGPIXELSX) as u32;
            ReleaseDC(HWND(0), hdc);

            return dpi;
        }

        USER_DEFAULT_SCREEN_DPI
    }
}
