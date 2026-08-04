/// 为 Overlay 注入置顶 / 工具窗 / DWM 毛玻璃（增强 Win32，非 DirectX 独占）。
pub fn configure_overlay_window_style(hwnd_raw: isize) {
    #[cfg(windows)]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::Graphics::Dwm::{
            DwmExtendFrameIntoClientArea, DwmSetWindowAttribute, DWMWA_SYSTEMBACKDROP_TYPE,
            DWM_SYSTEMBACKDROP_TYPE,
        };
        use windows::Win32::UI::Controls::MARGINS;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowLongW, SetWindowLongW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST,
            SWP_NOMOVE, SWP_NOSIZE, SWP_SHOWWINDOW, WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST,
        };

        let hwnd = HWND(hwnd_raw as *mut core::ffi::c_void);
        unsafe {
            let mut ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            ex_style |= (WS_EX_TOPMOST.0 | WS_EX_TOOLWINDOW.0 | WS_EX_LAYERED.0) as i32;
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style);

            // DWMSBT_TRANSIENTWINDOW = 3 (Acrylic-ish)
            let backdrop = DWM_SYSTEMBACKDROP_TYPE(3);
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const _ as *const _,
                std::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            );

            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_SHOWWINDOW,
            );
        }
        tracing::debug!("overlay style applied hwnd={hwnd_raw}");
    }
    #[cfg(not(windows))]
    {
        let _ = hwnd_raw;
    }
}
