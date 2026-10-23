#![cfg(windows)]
use eframe::egui::{pos2, vec2, Rect};
use sequoia::native::VideoHost;
use windows::{
    core::w,
    Win32::{Foundation::HINSTANCE, Graphics::Gdi::*, UI::WindowsAndMessaging::*},
};

#[test]
fn popup_masks_only_overlap_and_never_hides_video_host() -> anyhow::Result<()> {
    unsafe {
        let parent = CreateWindowExW(
            WINDOW_EX_STYLE(0),
            w!("STATIC"),
            w!("Sequoia regression test"),
            WS_OVERLAPPED,
            0,
            0,
            800,
            600,
            None,
            None,
            Some(HINSTANCE::default()),
            None,
        )?;
        let result = (|| -> anyhow::Result<()> {
            let host = VideoHost::new(parent.0 as isize)?;
            let video = Rect::from_min_size(pos2(20., 40.), vec2(640., 360.));
            let popup = Rect::from_min_size(pos2(500., 320.), vec2(200., 160.));
            for scale in [1., 1.25, 1.5, 2.] {
                host.bounds(video, scale, true, &[]);
                host.bounds(video, scale, true, &[popup]);
                assert_ne!(
                    GetWindowLongPtrW(host.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0,
                    0,
                    "popup hid video HWND"
                );
                let region = CreateRectRgn(0, 0, 0, 0);
                assert_eq!(GetWindowRgn(host.hwnd, region), COMPLEXREGION);
                assert!(
                    PtInRegion(region, (100. * scale) as i32, (100. * scale) as i32).as_bool(),
                    "uncovered video clipped"
                );
                assert!(
                    !PtInRegion(region, (500. * scale) as i32, (300. * scale) as i32).as_bool(),
                    "popup occluded by native video"
                );
                let _ = DeleteObject(HGDIOBJ(region.0));
                host.bounds(video, scale, true, &[]);
                let region = CreateRectRgn(0, 0, 0, 0);
                assert_eq!(
                    GetWindowRgn(host.hwnd, region),
                    GDI_REGION_TYPE(0),
                    "popup mask not cleared"
                );
                let _ = DeleteObject(HGDIOBJ(region.0));
                assert_ne!(
                    GetWindowLongPtrW(host.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0,
                    0
                );
            }
            host.bounds(video, 1., false, &[]);
            assert_eq!(
                GetWindowLongPtrW(host.hwnd, GWL_STYLE) as u32 & WS_VISIBLE.0,
                0
            );
            Ok(())
        })();
        let _ = DestroyWindow(parent);
        result
    }
}
