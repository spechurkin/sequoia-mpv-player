use super::*;
use windows::Win32::UI::WindowsAndMessaging::{GetParent, IsWindowVisible};

impl Player {
    pub(super) fn tray_regression(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|s| s == "--tray-test")
            || self.smoke_started.elapsed()
            < Duration::from_millis(850 * (self.smoke_step as u64 + 1))
        {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            let parent = unsafe {
                GetParent(
                    self.host
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("no video host"))?
                        .hwnd,
                )?
            };
            let visible = unsafe { IsWindowVisible(parent).as_bool() };
            let command = |this: &Self, id| this.tray.as_ref().unwrap().test_command(id);
            let position = |this: &Self| -> anyhow::Result<f64> {
                Ok(this
                    .engine
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("no mpv"))?
                    .request(json!(["get_property", "time-pos"]))?
                    .as_f64()
                    .unwrap())
            };
            match self.smoke_step {
                0 => {
                    anyhow::ensure!(
                        self.tray.as_ref().is_some_and(|tray| tray.available()),
                        "tray unavailable: {:?}",
                        self.error
                    );
                    self.data.settings.close_to_tray = true;
                    self.mode = Mode::Music;
                    self.add_paths(vec![
                        std::env::current_dir()?.join("artifacts/fixtures/test.mp3")
                    ]);
                    self.command(json!(["set_property", "mute", true]));
                }
                1 | 7 => {
                    self.playback_probe = position(self)?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                2 | 8 => {
                    anyhow::ensure!(self.hidden_to_tray && !visible, "window was not hidden");
                    anyhow::ensure!(
                        position(self)? > self.playback_probe + 0.1,
                        "hidden playback stalled"
                    );
                    command(self, if self.smoke_step == 2 { 2 } else { 4 });
                }
                3 => {
                    anyhow::ensure!(self.paused, "tray pause failed");
                    self.playback_probe = position(self)?;
                }
                4 => {
                    anyhow::ensure!(
                        (position(self)? - self.playback_probe).abs() < 0.15,
                        "pause did not hold"
                    );
                    command(self, 2);
                }
                5 => {
                    anyhow::ensure!(
                        !self.paused && position(self)? > self.playback_probe + 0.1,
                        "resume failed"
                    );
                    command(self, 1);
                }
                6 => {
                    anyhow::ensure!(!self.hidden_to_tray && visible, "restore failed");
                    self.mode = Mode::Video;
                    self.add_paths(vec![
                        std::env::current_dir()?.join("artifacts/fixtures/test.mp4"),
                        std::env::current_dir()?.join("artifacts/fixtures/test.mp3"),
                    ]);
                }
                9 => {
                    anyhow::ensure!(
                        self.current.as_ref().is_some_and(|m| m.audio),
                        "next failed while hidden"
                    );
                    command(self, 3);
                }
                10 => {
                    anyhow::ensure!(
                        self.current.as_ref().is_some_and(|m| !m.audio),
                        "previous failed while hidden"
                    );
                    self.tray.as_ref().unwrap().test_shell_restart();
                    anyhow::ensure!(
                        self.tray.as_ref().unwrap().available(),
                        "shell recovery failed"
                    );
                    command(self, 1);
                }
                11 => {
                    anyhow::ensure!(visible && !self.hidden_to_tray, "video restore failed");
                    command(self, 5);
                }
                12 => {
                    anyhow::ensure!(self.current.is_none() && self.paused, "stop failed");
                    self.add_paths(vec![
                        std::env::current_dir()?.join("artifacts/fixtures/test.mp4"),
                        std::env::current_dir()?.join("artifacts/fixtures/test.mp3"),
                    ]);
                }
                13 => {
                    self.engine.as_ref().unwrap().request(json!([
                        "seek",
                        7.5,
                        "absolute+exact"
                    ]))?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                14 => {
                    anyhow::ensure!(
                        self.hidden_to_tray
                            && !visible
                            && self.current.as_ref().is_some_and(|m| m.audio),
                        "automatic queue advance failed while hidden"
                    );
                    std::fs::write("artifacts/tray-result.txt", "PASS: native tray registered; music/video kept playing while hidden; pause/resume, next/previous, restore, stop, simulated shell recreation, automatic EOF queue advance and exit command dispatched.\n")?;
                    command(self, 6);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::write(
                "artifacts/tray-result.txt",
                format!("FAIL step {}: {error:#}", self.smoke_step),
            );
            self.exiting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
    }
}
