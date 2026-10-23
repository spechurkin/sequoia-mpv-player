use anyhow::{Context, Result};
use eframe::egui;
use std::{cell::Cell, sync::mpsc};
use windows::{
    core::{w, PCWSTR},
    Win32::{
        Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM},
        UI::{Shell::*, WindowsAndMessaging::*},
    },
};

const CALLBACK: u32 = WM_APP + 71;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Show,
    TogglePause,
    Previous,
    Next,
    Stop,
    Exit,
}

struct State {
    player: HWND,
    ctx: egui::Context,
    tx: mpsc::Sender<Action>,
    icon: NOTIFYICONDATAW,
    taskbar_created: u32,
    paused: bool,
    playable: bool,
    active: bool,
    installed: Cell<bool>,
    title: String,
}

// The hidden shell window belongs to eframe's UI thread and shares its message loop.
pub struct Tray {
    hwnd: HWND,
    state: Box<State>,
    rx: mpsc::Receiver<Action>,
}

impl Tray {
    pub fn new(ctx: &egui::Context, player: isize) -> Result<Self> {
        unsafe {
            let class = w!("Sequoia.Tray");
            let wc = WNDCLASSW {
                lpfnWndProc: Some(window_proc),
                lpszClassName: class,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                class,
                w!("Sequoia tray"),
                WS_OVERLAPPED,
                0,
                0,
                0,
                0,
                None,
                None,
                None,
                None,
            )?;
            // Reuse the executable's authored icon, including its alpha channel.
            let ico = include_bytes!(concat!(env!("OUT_DIR"), "/sequoia.ico"));
            let offset = u32::from_le_bytes(ico[18..22].try_into().unwrap()) as usize;
            let icon = match CreateIconFromResourceEx(
                &ico[offset..],
                true,
                0x30000,
                32,
                32,
                LR_DEFAULTCOLOR,
            ) {
                Ok(icon) => icon,
                Err(e) => {
                    let _ = DestroyWindow(hwnd);
                    return Err(e.into());
                }
            };
            let (tx, rx) = mpsc::channel();
            let mut state = Box::new(State {
                player: HWND(player as _),
                ctx: ctx.clone(),
                tx,
                icon: NOTIFYICONDATAW {
                    cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                    hWnd: hwnd,
                    uID: 1,
                    uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
                    uCallbackMessage: CALLBACK,
                    hIcon: icon,
                    ..Default::default()
                },
                taskbar_created: RegisterWindowMessageW(w!("TaskbarCreated")),
                paused: true,
                playable: false,
                active: false,
                installed: Cell::new(false),
                title: String::new(),
            });
            copy_tip(&mut state.icon.szTip, "Sequoia");
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, (&mut *state as *mut State) as isize);
            let tray = Self { hwnd, state, rx };
            tray.state
                .installed
                .set(Shell_NotifyIconW(NIM_ADD, &tray.state.icon).as_bool());
            anyhow::ensure!(
                tray.state.installed.get(),
                sequoia::i18n::t("text.windows_could_not_add_the_tray_icon")
            );
            Ok(tray)
        }
    }

    pub fn available(&self) -> bool {
        self.state.installed.get()
    }

    pub fn background(&self, enabled: bool) -> Result<()> {
        unsafe {
            if enabled {
                anyhow::ensure!(
                    SetTimer(Some(self.hwnd), 1, 200, None) != 0,
                    sequoia::i18n::t("text.failed_to_start_background_playback_processing")
                );
            } else {
                let _ = KillTimer(Some(self.hwnd), 1);
            }
        }
        Ok(())
    }

    pub fn actions(&self) -> Vec<Action> {
        self.rx.try_iter().collect()
    }

    pub(crate) fn test_command(&self, id: usize) {
        unsafe {
            SendMessageW(self.hwnd, WM_COMMAND, Some(WPARAM(id)), Some(LPARAM(0)));
        }
    }

    pub(crate) fn test_shell_restart(&self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.state.icon);
            SendMessageW(
                self.hwnd,
                self.state.taskbar_created,
                Some(WPARAM(0)),
                Some(LPARAM(0)),
            );
        }
    }

    pub fn update(&mut self, title: &str, paused: bool, playable: bool, active: bool) {
        self.state.paused = paused;
        self.state.playable = playable;
        self.state.active = active;
        let tip = if title.is_empty() {
            "Sequoia".into()
        } else {
            format!(
                "Sequoia · {} · {title}",
                if paused { sequoia::i18n::t("text.paused") } else { sequoia::i18n::t("text.playing") }
            )
        };
        if self.state.title != tip {
            self.state.title = tip;
            copy_tip(&mut self.state.icon.szTip, &self.state.title);
            unsafe {
                let _ = Shell_NotifyIconW(NIM_MODIFY, &self.state.icon);
            }
        }
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        unsafe {
            let _ = Shell_NotifyIconW(NIM_DELETE, &self.state.icon);
            let _ = KillTimer(Some(self.hwnd), 1);
            SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(self.hwnd);
            let _ = DestroyIcon(self.state.icon.hIcon);
        }
    }
}

fn copy_tip(target: &mut [u16; 128], text: &str) {
    target.fill(0);
    let mut len = 0;
    for ch in text.chars() {
        let mut units = [0; 2];
        let encoded = ch.encode_utf16(&mut units);
        if len + encoded.len() >= target.len() {
            break;
        }
        target[len..len + encoded.len()].copy_from_slice(encoded);
        len += encoded.len();
    }
}

fn action_for_id(id: usize) -> Option<Action> {
    match id {
        1 => Some(Action::Show),
        2 => Some(Action::TogglePause),
        3 => Some(Action::Previous),
        4 => Some(Action::Next),
        5 => Some(Action::Stop),
        6 => Some(Action::Exit),
        _ => None,
    }
}

unsafe fn menu(hwnd: HWND, paused: bool, playable: bool, active: bool) -> Result<Option<Action>> {
    let menu = CreatePopupMenu()?;
    let result = (|| -> Result<Option<Action>> {
        for (id, label, enabled) in [
            (1, sequoia::i18n::t("text.show_sequoia"), true),
            (
                2,
                if paused {
                    sequoia::i18n::t("text.play")
                } else {
                    sequoia::i18n::t("text.paused")
                },
                playable,
            ),
            (3, sequoia::i18n::t("text.previous"), playable),
            (4, sequoia::i18n::t("text.next"), playable),
            (5, sequoia::i18n::t("text.stop_2"), active),
            (6, sequoia::i18n::t("text.exit"), true),
        ] {
            if id == 6 {
                AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null())?;
            }
            let label: Vec<u16> = label.encode_utf16().chain(Some(0)).collect();
            AppendMenuW(
                menu,
                MF_STRING | if enabled { MF_ENABLED } else { MF_GRAYED },
                id,
                PCWSTR(label.as_ptr()),
            )?;
        }
        SetMenuDefaultItem(menu, 1, 0)?;
        let mut point = POINT::default();
        GetCursorPos(&mut point)?;
        let _ = SetForegroundWindow(hwnd);
        let selected = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            point.x,
            point.y,
            None,
            hwnd,
            None,
        )
            .0;
        let _ = PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0));
        Ok(action_for_id(selected as usize))
    })();
    let _ = DestroyMenu(menu);
    result.context(sequoia::i18n::t("text.failed_to_open_the_tray_menu"))
}

unsafe extern "system" fn window_proc(hwnd: HWND, msg: u32, wp: WPARAM, lp: LPARAM) -> LRESULT {
    let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut State;
    if !ptr.is_null() {
        if msg == WM_TIMER && wp.0 == 1 {
            // Hidden windows don't receive WM_PAINT from request_redraw on Windows.
            // Post it explicitly so the UI thread can consume mpv/queue events at 5 Hz.
            let _ = PostMessageW(Some((*ptr).player), WM_PAINT, WPARAM(0), LPARAM(0));
            return LRESULT(0);
        }
        if msg == (*ptr).taskbar_created {
            let state = &*ptr;
            state
                .installed
                .set(Shell_NotifyIconW(NIM_ADD, &state.icon).as_bool());
            if !state.installed.get() {
                let _ = state.tx.send(Action::Show);
                state.ctx.request_repaint();
            }
            return LRESULT(0);
        }
        if msg == CALLBACK || msg == WM_COMMAND {
            // TrackPopupMenu runs a nested message loop; don't retain a State borrow across it.
            let (paused, playable, active) = ((*ptr).paused, (*ptr).playable, (*ptr).active);
            let action = if msg == WM_COMMAND {
                action_for_id(wp.0 & 0xffff)
            } else {
                match lp.0 as u32 {
                    WM_LBUTTONUP => Some(Action::Show),
                    WM_RBUTTONUP | WM_CONTEXTMENU => {
                        menu(hwnd, paused, playable, active).ok().flatten()
                    }
                    _ => None,
                }
            };
            if let Some(action) = action {
                let state = &*ptr;
                let _ = state.tx.send(action);
                state.ctx.request_repaint();
            }
            return LRESULT(0);
        }
    }
    DefWindowProcW(hwnd, msg, wp, lp)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn tooltip_truncates_without_splitting_unicode_or_losing_terminator() {
        let mut tip = [9; 128];
        copy_tip(&mut tip, &format!("{}🌲", "я".repeat(126)));
        assert_eq!(tip[126], 0);
        assert!(String::from_utf16(&tip[..126]).is_ok());
        copy_tip(&mut tip, "Sequoia");
        assert!(tip[7..].iter().all(|&v| v == 0));
    }
}
