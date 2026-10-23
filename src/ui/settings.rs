use super::*;
use sequoia::i18n::{self, t};

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Category {
    #[default]
    General,
    Playback,
    Folders,
    Metadata,
    Mpv,
}

impl Player {
    pub(super) fn settings_dialog(&mut self, ctx: &egui::Context) {
        if !self.show_settings {
            return;
        }
        let mut open = true;
        egui::Window::new(t("settings.title"))
            .id(egui::Id::new("settings-window"))
            .open(&mut open).default_width(720.).collapsible(false)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (category, key) in [
                        (Category::General, "settings.general"),
                        (Category::Playback, "settings.playback"),
                        (Category::Folders, "settings.folders"),
                        (Category::Metadata, "settings.metadata"),
                        (Category::Mpv, "settings.mpv"),
                    ] {
                        ui.selectable_value(&mut self.settings_category, category, t(key));
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().id_salt("settings-content").max_height(460.).show(ui, |ui| {
                    ui.set_min_height(220.);
                    match self.settings_category {
                        Category::General => {
                            ui.label(t("settings.language"));
                            egui::ComboBox::from_id_salt("interface-language")
                                .selected_text(i18n::catalogs().locales.get(&self.draft.language).map(|locale| locale.name.as_str()).unwrap_or(&self.draft.language))
                                .show_ui(ui, |ui| {
                                    for locale in i18n::catalogs().locales.values() {
                                        ui.selectable_value(&mut self.draft.language, locale.id.clone(), &locale.name);
                                    }
                                });
                            #[cfg(windows)]
                            ui.checkbox(&mut self.draft.close_to_tray, t("text.hide_to_tray_on_close_and_keep_playing"))
                                .on_hover_text(t("text.click_the_sequoia_icon_to_return_to_exit_choose_exit_from_the_tray_menu"));
                            for warning in &i18n::catalogs().warnings { ui.colored_label(ACCENT, warning); }
                        }
                        Category::Playback => {
                            ui.label(t("text.arrow_key_seek_step_seconds"));
                            ui.add(egui::DragValue::new(&mut self.draft.seek_seconds).range(0.1..=600.).speed(0.5));
                            hint(ui, "text.shift_steps_backwards_forwards_by_one_frame_in_video_mode");
                        }
                        Category::Folders => {
                            path_field(ui, t("text.png_screenshots"), &mut self.draft.screenshot_folder, true);
                            path_field(ui, t("text.music_library_folder"), &mut self.draft.music_root, true);
                            if ui.button(t("text.restore_the_system_music_folder")).clicked() { self.draft.music_root = library::music_folder(); }
                            ui.checkbox(&mut self.draft.recursive, t("text.include_subfolders_when_opening_ordinary_folders"));
                            hint(ui, "settings.folders_hint");
                        }
                        Category::Metadata => {
                            ui.checkbox(&mut self.draft.auto_lookup, t("text.automatically_search_for_tags_when_opening_the_editor"));
                            hint(ui, "text.automatic_lookup_sends_the_title_and_artist_to_musicbrainz_writing_to_the_f");
                        }
                        Category::Mpv => {
                            path_field(ui, t("text.mpv_executable"), &mut self.draft.mpv_path, false);
                            hint(ui, "text.leave_blank_if_mpv_is_installed_in_program_files_or_next_to_the_application");
                            hint(ui, "text.mpv_options_filters_profiles_quality_and_audio_output_applied_after_reconne");
                            ui.add(egui::TextEdit::multiline(&mut self.draft.mpv_config).font(egui::TextStyle::Monospace).desired_rows(6).desired_width(f32::INFINITY).hint_text("sub-auto=fuzzy\nscreenshot-format=png\naudio-pitch-correction=yes"));
                            ui.label(i18n::format("text.configuration_and_lua_scripts", &[("arg0", self.root.join("mpv").display().to_string())]));
                            if ui.button(t("text.open_data_folder")).clicked() { open_directory(&self.root); }
                        }
                    }
                });
                ui.separator();
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new(t("settings.save")).color(ACCENT)).clicked() { self.apply_settings(false); }
                    if ui.button(t("text.save_and_connect")).clicked() { self.apply_settings(true); }
                    if ui.button(t("settings.cancel")).clicked() { self.show_settings = false; }
                });
            });
        if !open {
            self.show_settings = false;
        }
    }

    fn apply_settings(&mut self, reconnect: bool) {
        if let Err(error) = library::update_mpv_config(
            &self.root,
            &self.data.settings.mpv_config,
            &self.draft.mpv_config,
        ) {
            self.error = Some(error.to_string());
            return;
        }
        self.draft.seek_seconds = self.draft.seek_seconds.clamp(0.1, 600.);
        let previous = self.data.settings.clone();
        self.data.settings = self.draft.clone();
        if let Err(error) = library::save(&self.root, &self.data) {
            self.data.settings = previous;
            self.error = Some(error.to_string());
            return;
        }
        i18n::set_language(&self.data.settings.language);
        if reconnect {
            self.stop();
            self.engine = None;
            #[cfg(windows)]
            let hwnd = self.host.as_ref().map(|host| host.id());
            #[cfg(not(windows))]
            let hwnd = None;
            match mpv::resolve(&self.data.settings.mpv_path) {
                Some(path) => match Mpv::start(&path, hwnd, &self.root.join("mpv"), false) {
                    Ok(engine) => {
                        self.engine = Some(engine);
                        self.command(json!(["set_property", "volume", self.volume]));
                    }
                    Err(error) => {
                        self.error = Some(error.to_string());
                        return;
                    }
                },
                None => {
                    self.error = Some(t("text.select_an_existing_mpv_exe").into());
                    return;
                }
            }
        }
        self.show_settings = false;
        self.notify(t("settings.saved"));
    }
}

fn hint(ui: &mut egui::Ui, key: &str) {
    ui.label(RichText::new(t(key)).small().color(MUTED));
}
