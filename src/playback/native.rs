use anyhow::{Context, Result};
use std::cell::RefCell;
use windows::{
    core::w,
    Win32::{
        Foundation::{HINSTANCE, HWND},
        Graphics::Gdi::{CombineRgn, CreateRectRgn, DeleteObject, SetWindowRgn, HGDIOBJ, RGN_DIFF},
        UI::WindowsAndMessaging::*,
    },
};
pub fn reveal_file(path: &std::path::Path) -> Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows::{
        core::PCWSTR,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::Com::{CoInitializeEx, CoUninitialize, COINIT_APARTMENTTHREADED},
            UI::Shell::{ILFree, SHOpenFolderAndSelectItems, SHParseDisplayName},
        },
    };

    let path = std::path::absolute(path).context(crate::i18n::t("text.failed_to_resolve_the_file_path"))?;
    std::fs::metadata(&path).with_context(|| crate::i18n::format("text.file_unavailable", &[("arg0", format!("{}", path.display()))]))?;
    let wide: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
    unsafe {
        let initialized = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if initialized != RPC_E_CHANGED_MODE {
            initialized
                .ok()
                .context(crate::i18n::t("text.failed_to_connect_to_the_windows_shell"))?;
        }
        let result = (|| -> Result<()> {
            let mut item = std::ptr::null_mut();
            SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut item, 0, None)
                .context(crate::i18n::t("text.failed_to_find_the_file_in_the_windows_shell"))?;
            // With no child list, the shell opens the parent and selects this item.
            let result = SHOpenFolderAndSelectItems(item, None, 0);
            ILFree(Some(item));
            result.context(crate::i18n::t("text.failed_to_show_the_file_in_its_folder"))
        })();
        if initialized.is_ok() {
            CoUninitialize();
        }
        result
    }
}

pub struct VideoHost {
    pub hwnd: HWND,
    geometry: RefCell<Option<[i32; 4]>>,
    clipping: RefCell<Option<ClipRegion>>,
    visible: std::cell::Cell<bool>,
}
#[derive(Clone, PartialEq)]
struct ClipRegion {
    width: i32,
    height: i32,
    holes: Vec<[i32; 4]>,
}
impl VideoHost {
    pub fn new(parent: isize) -> Result<Self> {
        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                w!("STATIC"),
                w!("Sequoia mpv"),
                WS_CHILD | WS_DISABLED | WS_CLIPCHILDREN,
                0,
                0,
                1,
                1,
                Some(HWND(parent as _)),
                None,
                Some(HINSTANCE::default()),
                None,
            )
        }
            .context(crate::i18n::t("text.failed_to_create_the_video_window"))?;
        Ok(Self {
            hwnd,
            geometry: RefCell::new(None),
            clipping: RefCell::new(None),
            visible: std::cell::Cell::new(false),
        })
    }
    pub fn bounds(
        &self,
        rect: eframe::egui::Rect,
        scale: f32,
        visible: bool,
        overlays: &[eframe::egui::Rect],
    ) {
        unsafe {
            if !visible || !rect.is_positive() {
                if self.visible.replace(false) {
                    let _ = ShowWindow(self.hwnd, SW_HIDE);
                }
                return;
            }
            let geometry = [
                (rect.left() * scale).round() as i32,
                (rect.top() * scale).round() as i32,
                (rect.width() * scale).round().max(1.) as i32,
                (rect.height() * scale).round().max(1.) as i32,
            ];
            let geometry_changed = self.geometry.borrow().as_ref() != Some(&geometry);
            if geometry_changed
                && SetWindowPos(
                self.hwnd,
                None,
                geometry[0],
                geometry[1],
                geometry[2],
                geometry[3],
                SWP_NOACTIVATE | SWP_NOZORDER,
            )
                .is_ok()
            {
                *self.geometry.borrow_mut() = Some(geometry);
            }
            let mut holes: Vec<_> = overlays
                .iter()
                .map(|overlay| rect.intersect(*overlay))
                .filter(|r| r.is_positive())
                .map(|r| {
                    [
                        ((r.left() - rect.left()) * scale).floor() as i32,
                        ((r.top() - rect.top()) * scale).floor() as i32,
                        ((r.right() - rect.left()) * scale).ceil() as i32,
                        ((r.bottom() - rect.top()) * scale).ceil() as i32,
                    ]
                })
                .collect();
            holes.sort_unstable();
            let clipping = ClipRegion {
                width: geometry[2],
                height: geometry[3],
                holes,
            };
            if self.clipping.borrow().as_ref() != Some(&clipping) {
                if clipping.holes.is_empty() {
                    if SetWindowRgn(self.hwnd, None, true) != 0 {
                        *self.clipping.borrow_mut() = Some(clipping);
                    }
                } else {
                    let region = CreateRectRgn(0, 0, clipping.width, clipping.height);
                    if !region.0.is_null() {
                        for h in &clipping.holes {
                            let hole = CreateRectRgn(h[0], h[1], h[2], h[3]);
                            if !hole.0.is_null() {
                                CombineRgn(Some(region), Some(region), Some(hole), RGN_DIFF);
                                let _ = DeleteObject(HGDIOBJ(hole.0));
                            }
                        }
                        // Windows owns the region after a successful SetWindowRgn.
                        if SetWindowRgn(self.hwnd, Some(region), true) == 0 {
                            let _ = DeleteObject(HGDIOBJ(region.0));
                        } else {
                            *self.clipping.borrow_mut() = Some(clipping);
                        }
                    }
                }
            }
            if !self.visible.replace(true) {
                let _ = ShowWindow(self.hwnd, SW_SHOWNA);
            }
        }
    }
    pub fn id(&self) -> isize {
        self.hwnd.0 as isize
    }
}
impl Drop for VideoHost {
    fn drop(&mut self) {
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}
