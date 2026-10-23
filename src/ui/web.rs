use super::*;
use sequoia::playback::web;

impl Player {
    pub(super) fn web_preview(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|s| s == "--web-preview")
            || self.smoke_started.elapsed()
            < Duration::from_millis(650 * (self.smoke_step as u64 + 1))
        {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            match self.smoke_step {
                0 => {
                    self.error = None;
                    #[cfg(windows)]
                    anyhow::ensure!(self.link_drop.is_some(), "OLE drop target not registered");
                    self.show_link = true;
                    self.link_input =
                        "https://www.youtube.com/watch?v=BaW_jenozKc&list=PL123".into();
                }
                1 => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                )),
                2 => {
                    let generation = self.link_generation;
                    self.cancel_link();
                    self.link_result(generation, Err(anyhow::anyhow!("stale result")));
                    anyhow::ensure!(self.error.is_none(), "cancelled result changed UI");
                    self.show_link = false;
                    self.link_loading = true;
                }
                3 => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                )),
                _ => {
                    self.cancel_link();
                    std::fs::write(
                        "artifacts/web-regression.txt",
                        "PASS: native URL dialog and loading state rendered; OLE target registered; cancellation ignores stale responses. Programmatic actions; browser pointer drag and network playback were not exercised.\n",
                    )?;
                    self.exiting = true;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
            self.smoke_step += 1;
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::write("artifacts/web-regression.txt", format!("FAIL: {error:#}"));
            self.exiting = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(super) fn cancel_link(&mut self) {
        self.link_cancel.store(true, Ordering::Relaxed);
        self.link_generation = self.link_generation.wrapping_add(1);
        self.link_loading = false;
    }

    pub(super) fn open_links(&mut self, links: Vec<String>) {
        self.cancel_link();
        let generation = self.link_generation;
        self.link_loading = true;
        self.link_cancel = Arc::new(AtomicBool::new(false));
        let cancelled = self.link_cancel.clone();
        self.notify(sequoia::i18n::t("text.yt_dlp_fetching_video_list"));
        let tx = self.tx.clone();
        std::thread::spawn(move || {
            let result = links.iter().try_fold(Vec::new(), |mut items, url| {
                if cancelled.load(Ordering::Relaxed) {
                    anyhow::bail!(sequoia::i18n::t("text.cancelled"));
                }
                items.extend(web::extract_cancellable(url, &cancelled)?);
                Ok(items)
            });
            let _ = tx.send(Job::Web(generation, result));
        });
    }

    pub(super) fn link_result(&mut self, generation: u64, result: anyhow::Result<Vec<Media>>) {
        if generation != self.link_generation {
            return;
        }
        self.link_loading = false;
        match result {
            Ok(items) => {
                self.notify(sequoia::i18n::format("text.items_received", &[("arg0", format!("{}", items.len()))]));
                self.queue_playlist = None;
                self.music_target = sequoia::music::Target::Queue;
                self.music_rows.clear();
                self.queue.replace(items, 0);
                self.play_queue();
            }
            Err(error) => self.error = Some(format!("{error:#}")),
        }
    }

    pub(super) fn link_dialog(&mut self, ctx: &egui::Context) {
        if self.link_loading {
            egui::Window::new(sequoia::i18n::t("text.opening_url"))
                .collapsible(false)
                .resizable(false)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(sequoia::i18n::t("text.yt_dlp_fetching_video_list"));
                    });
                    if ui.button(sequoia::i18n::t("text.cancel_2")).clicked() {
                        self.cancel_link();
                    }
                });
        }
        if !self.show_link {
            return;
        }
        let mut open = true;
        egui::Window::new(sequoia::i18n::t("text.open_url"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(sequoia::i18n::t("text.video_audio_or_playlist_url_http_https"));
                let input =
                    ui.add(egui::TextEdit::singleline(&mut self.link_input).desired_width(420.));
                if ctx.memory(|m| m.focused().is_none()) && self.link_input.is_empty() {
                    input.request_focus();
                }
                let valid = web::is_url(self.link_input.trim());
                if !self.link_input.is_empty() && !valid {
                    ui.colored_label(
                        MUTED,
                        sequoia::i18n::t("text.enter_the_full_address_starting_with_https_or_http"),
                    );
                }
                ui.horizontal(|ui| {
                    if ui
                        .add_enabled(valid, egui::Button::new(sequoia::i18n::t("text.open_2")))
                        .clicked()
                        || valid && input.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter))
                    {
                        self.open_links(vec![self.link_input.trim().to_owned()]);
                        self.show_link = false;
                    }
                    if ui.button(sequoia::i18n::t("text.cancel")).clicked() {
                        self.show_link = false;
                    }
                });
            });
        if !open {
            self.show_link = false;
        }
    }

    pub(super) fn poll_link_drop(&mut self) {
        #[cfg(windows)]
        if let Some(drop) = &self.link_drop {
            let inputs: Vec<_> = drop.events.try_iter().collect();
            for input in inputs {
                match input {
                    sequoia::playback::web_drop::Input::Links(links) => self.open_links(links),
                    sequoia::playback::web_drop::Input::Files(paths) => {
                        if paths.iter().any(|p| web::shortcut(p).is_some()) {
                            self.add_paths(paths);
                        } else if self.mode == Mode::Music {
                            self.music_drop_files(paths);
                        } else if paths.len() == 1 && paths[0].is_dir() {
                            self.start_scan(paths[0].clone(), false);
                        } else {
                            self.add_paths(paths);
                        }
                    }
                }
            }
        }
    }
}
