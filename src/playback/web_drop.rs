use std::{cell::Cell, path::PathBuf, sync::mpsc};
use windows::{
    core::{implement, w, Ref, Result},
    Win32::{
        Foundation::{HWND, POINTL},
        System::{
            Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, TYMED_HGLOBAL},
            DataExchange::RegisterClipboardFormatW,
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                IDropTarget, IDropTarget_Impl, RegisterDragDrop, ReleaseStgMedium, RevokeDragDrop,
                DROPEFFECT, DROPEFFECT_COPY, DROPEFFECT_NONE,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{DragQueryFileW, HDROP},
    },
};

pub enum Input {
    Links(Vec<String>),
    Files(Vec<PathBuf>),
}
pub struct LinkDrop {
    hwnd: HWND,
    pub events: mpsc::Receiver<Input>,
}
impl LinkDrop {
    pub fn new(hwnd: isize, ctx: eframe::egui::Context) -> Result<Self> {
        let hwnd = HWND(hwnd as _);
        let (tx, events) = mpsc::channel();
        let target: IDropTarget = Target {
            tx,
            ctx,
            accepted: Cell::new(false),
        }
            .into();
        unsafe {
            // Winit registers a file-only target on this HWND. Replace it with one
            // accepting both browser URLs and the same CF_HDROP file payloads.
            let _ = RevokeDragDrop(hwnd);
            RegisterDragDrop(hwnd, &target)?;
        }
        Ok(Self { hwnd, events })
    }
}
impl Drop for LinkDrop {
    fn drop(&mut self) {
        unsafe {
            let _ = RevokeDragDrop(self.hwnd);
        }
    }
}

#[implement(IDropTarget)]
struct Target {
    tx: mpsc::Sender<Input>,
    ctx: eframe::egui::Context,
    accepted: Cell<bool>,
}
fn formats() -> [u16; 4] {
    unsafe {
        [
            RegisterClipboardFormatW(w!("UniformResourceLocatorW")) as u16,
            13,
            15,
            RegisterClipboardFormatW(w!("UniformResourceLocator")) as u16,
        ]
    }
}
fn format(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    }
}
fn payload(data: &IDataObject) -> Option<Input> {
    for (index, id) in formats().into_iter().enumerate() {
        unsafe {
            let Ok(mut medium) = data.GetData(&format(id)) else {
                continue;
            };
            let global = medium.u.hGlobal;
            let result = if index == 2 {
                let drop = HDROP(global.0);
                let count = DragQueryFileW(drop, u32::MAX, None);
                let mut files = Vec::new();
                for i in 0..count {
                    let mut wide = vec![0; DragQueryFileW(drop, i, None) as usize + 1];
                    let len = DragQueryFileW(drop, i, Some(&mut wide));
                    use std::os::windows::ffi::OsStringExt;
                    files.push(PathBuf::from(std::ffi::OsString::from_wide(
                        &wide[..len as usize],
                    )));
                }
                (!files.is_empty()).then_some(Input::Files(files))
            } else {
                let ptr = GlobalLock(global);
                let size = GlobalSize(global);
                let text = if ptr.is_null() {
                    String::new()
                } else {
                    let text = if index == 3 {
                        String::from_utf8_lossy(std::slice::from_raw_parts(ptr as *const u8, size))
                            .into_owned()
                    } else {
                        String::from_utf16_lossy(std::slice::from_raw_parts(
                            ptr as *const u16,
                            size / 2,
                        ))
                    };
                    let _ = GlobalUnlock(global);
                    text
                };
                let links: Vec<_> = text
                    .trim_end_matches('\0')
                    .lines()
                    .map(str::trim)
                    .filter(|s| super::web::is_url(s))
                    .map(str::to_owned)
                    .collect();
                (!links.is_empty()).then_some(Input::Links(links))
            };
            ReleaseStgMedium(&mut medium);
            if result.is_some() {
                return result;
            }
        }
    }
    None
}
#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<'_, IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.accepted.set(data.as_ref().is_some_and(|data| {
            formats()
                .into_iter()
                .any(|id| unsafe { data.QueryGetData(&format(id)).is_ok() })
        }));
        unsafe {
            *effect = if self.accepted.get() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        Ok(())
    }
    fn DragOver(&self, _: MODIFIERKEYS_FLAGS, _: &POINTL, effect: *mut DROPEFFECT) -> Result<()> {
        unsafe {
            *effect = if self.accepted.get() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        Ok(())
    }
    fn DragLeave(&self) -> Result<()> {
        self.accepted.set(false);
        Ok(())
    }
    fn Drop(
        &self,
        data: Ref<'_, IDataObject>,
        _: MODIFIERKEYS_FLAGS,
        _: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let input = data.as_ref().and_then(payload);
        unsafe {
            *effect = if input.is_some() {
                DROPEFFECT_COPY
            } else {
                DROPEFFECT_NONE
            };
        }
        if let Some(input) = input {
            let _ = self.tx.send(input);
            self.ctx.request_repaint();
        }
        Ok(())
    }
}
