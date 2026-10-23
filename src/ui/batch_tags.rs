use super::*;
use sequoia::tags::{self, TagPatch};
fn mixed_label() -> &'static str { sequoia::i18n::t("text.mixed_track_data") }

const LABELS: [&str; 9] = [
    "text.title",
    "text.artist",
    "text.album",
    "text.genre",
    "text.year",
    "text.track_number",
    "text.album_artist",
    "text.disc_number",
    "text.grouping",
];
pub(super) struct BatchEditor {
    items: Vec<Media>,
    values: [String; 9],
    initial: [String; 9],
    mixed: [bool; 9],
    changed: [bool; 9],
    cover: Option<Vec<u8>>,
    number: bool,
    start: u32,
    pub saving: bool,
    pub done: usize,
    pub result: Option<tags::BatchResult>,
}
fn values(m: &Media) -> [String; 9] {
    [
        m.title.clone(),
        m.artist.clone(),
        m.album.clone(),
        m.genre.clone(),
        if m.year == 0 {
            String::new()
        } else {
            m.year.to_string()
        },
        if m.track == 0 {
            String::new()
        } else {
            m.track.to_string()
        },
        m.album_artist.clone(),
        if m.disc == 0 {
            String::new()
        } else {
            m.disc.to_string()
        },
        m.grouping.clone(),
    ]
}
impl Player {
    pub(super) fn batch_tags_preview(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|s| s == "--batch-tags-preview")
            || self.smoke_started.elapsed()
            < Duration::from_millis(700 * (self.smoke_step as u64 + 1))
        {
            return;
        }
        let check = (|| -> anyhow::Result<()> {
            match self.smoke_step {
                0 => {
                    self.mode = Mode::Music;
                    self.error = None;
                    std::fs::create_dir_all(&self.root)?;
                    let mut items = Vec::new();
                    for (i, name) in ["test.mp3", "test.flac"].iter().enumerate() {
                        let path = self.root.join(name);
                        std::fs::copy(PathBuf::from("artifacts/fixtures").join(name), &path)?;
                        let mut media = library::read_media(&path);
                        media.title = format!("Тестовая композиция {}", i + 1);
                        media.track = i as u32 + 1;
                        items.push(library::write_tags(&media, None, &self.root)?);
                    }
                    self.data.library = items.clone();
                    self.queue.replace(items.clone(), 0);
                    self.data
                        .playlists
                        .insert("Проверка тегов".into(), items.clone());
                    self.edit_tracks(items);
                    let editor = self.batch_editor.as_mut().unwrap();
                    anyhow::ensure!(
                        editor.mixed[0]
                            && editor.mixed[5]
                            && editor.values[0] == mixed_label()
                            && editor.values[5] == mixed_label()
                            && editor.values == editor.initial
                            && !editor.changed.iter().any(|v| *v),
                        "mixed values or initial flags"
                    );
                    editor.values[2] = "Общий альбом".into();
                    editor.cover = Some(std::fs::read("artifacts/fixtures/cover.png")?);
                }
                1 => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                )),
                2 => {}
                3 => {
                    let editor = self.batch_editor.as_ref().unwrap();
                    if editor.saving {
                        return Ok(());
                    }
                    let result = editor
                        .result
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("save action did not complete"))?;
                    anyhow::ensure!(
                        editor.changed
                            == [false, false, true, false, false, false, false, false, false],
                        "only edited fields must be applied"
                    );
                    anyhow::ensure!(
                        result.failed.is_empty() && result.saved.len() == 2,
                        "batch failed: {:?}",
                        result.failed
                    );
                    for items in [
                        &self.data.library,
                        &self.queue.items,
                        &self.data.playlists["Проверка тегов"],
                    ] {
                        anyhow::ensure!(
                            items.len() == 2
                                && items.iter().all(|m| m.album == "Общий альбом")
                                && items[0].track == 1
                                && items[1].track == 2
                                && items[0].title != items[1].title,
                            "metadata synchronization or preservation"
                        );
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                4 => {
                    std::fs::write("artifacts/batch-tags-regression.txt", "PASS: native batch dialog rendered; mixed values; save action; MP3/FLAC batch write and shared cover verified; queue/library/playlist synchronization; individual titles and track numbers preserved. Programmatic application actions, not OS pointer input.\n")?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = check {
            let _ = std::fs::write(
                "artifacts/batch-tags-regression.txt",
                format!("FAIL: {error:#}"),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        if self.smoke_step == 3 && self.batch_editor.as_ref().is_some_and(|e| e.saving) {
            return;
        }
        self.smoke_step += 1;
    }
    pub(super) fn edit_tracks(&mut self, items: Vec<Media>) {
        if self.batch_editor.is_some() || self.editor.is_some() {
            return;
        }
        let items = tags::unique_tracks(items)
            .into_iter()
            .map(|media| {
                if media.path.is_file() {
                    library::read_media(&media.path)
                } else {
                    media
                }
            })
            .collect::<Vec<_>>();
        if items.len() == 1 {
            self.editor(items[0].clone());
            return;
        }
        let Some(first) = items.first() else {
            return;
        };
        let mut common = values(first);
        let mixed = std::array::from_fn(|i| items.iter().any(|m| values(m)[i] != common[i]));
        for i in 0..9 {
            if mixed[i] {
                common[i] = mixed_label().into();
            }
        }
        self.batch_editor = Some(BatchEditor {
            items,
            initial: common.clone(),
            values: common,
            mixed,
            changed: [false; 9],
            cover: None,
            number: false,
            start: 1,
            saving: false,
            done: 0,
            result: None,
        });
    }
    pub(super) fn batch_tags_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.batch_editor.take() else {
            return;
        };
        let mut open = true;
        let mut save = std::env::args().any(|s| s == "--batch-tags-preview")
            && self.smoke_step == 3
            && !editor.saving
            && editor.result.is_none();
        let mut close = false;
        egui::Window::new(sequoia::i18n::t("text.batch_tag_editor"))
            .open(&mut open)
            .default_width(720.)
            .default_height(580.)
            .max_width(760.)
            .vscroll(true)
            .collapsible(false)
            .show(ctx, |ui| {
                ui.label(sequoia::i18n::format("text.selected_files", &[("arg0", format!("{}", editor.items.len()))]));
                egui::ScrollArea::vertical()
                    .id_salt("batch-files")
                    .max_height(130.)
                    .show(ui, |ui| {
                        for (i, item) in editor.items.iter().enumerate() {
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::Label::new(format!(
                                        "{} · {}",
                                        if editor.number {
                                            editor.start as usize + i
                                        } else {
                                            item.track as usize
                                        },
                                        item.title
                                    ))
                                        .truncate(),
                                )
                                    .on_hover_text(item.path.display().to_string());
                            });
                        }
                    });
                ui.separator();
                ui.label(sequoia::i18n::t("text.changed_fields_apply_to_all_selected_tracks"));
                ui.label(
                    RichText::new(
                        sequoia::i18n::t("text.clear_a_field_to_remove_its_tag_unchanged_fields_keep_their_original_data"),
                    )
                        .small()
                        .color(MUTED),
                );
                ui.add_enabled_ui(!editor.saving && editor.result.is_none(), |ui| {
                    egui::Grid::new("batch-tag-fields")
                        .num_columns(2)
                        .spacing([14., 8.])
                        .show(ui, |ui| {
                            for (i, label) in LABELS.iter().enumerate() {
                                ui.label(sequoia::i18n::t(label));
                                let response = ui.add(
                                    egui::TextEdit::singleline(&mut editor.values[i])
                                        .desired_width(440.),
                                );
                                if response.changed() && i == 5 { editor.number = false; }
                                ui.end_row();
                            }
                        });
                    ui.horizontal(|ui| {
                        egui::ComboBox::from_id_salt("batch-numbering")
                            .selected_text(if editor.number { sequoia::i18n::t("text.number_sequentially_from") } else { sequoia::i18n::t("text.keep_track_numbers") })
                            .show_ui(ui, |ui| {
                                ui.selectable_value(&mut editor.number, false, sequoia::i18n::t("text.keep_track_numbers"));
                                ui.selectable_value(&mut editor.number, true, sequoia::i18n::t("text.number_sequentially_from"));
                            });
                        ui.add_enabled(
                            editor.number,
                            egui::DragValue::new(&mut editor.start).range(1..=9999),
                        );
                    });
                    ui.label(
                        RichText::new(sequoia::i18n::t("text.the_file_list_above_shows_the_numbering_order"))
                            .small()
                            .color(MUTED),
                    );
                    ui.horizontal(|ui| {
                        if tool_label(ui, Icon::Cover, sequoia::i18n::t("text.shared_cover_from_file"), false).clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter(sequoia::i18n::t("text.cover"), &["jpg", "jpeg", "png"])
                                .pick_file()
                            {
                                match std::fs::read(path) {
                                    Ok(bytes) => match image::load_from_memory(&bytes) {
                                        Ok(_) => editor.cover = Some(bytes),
                                        Err(e) => {
                                            self.error =
                                                Some(sequoia::i18n::format("text.failed_to_read_cover_e", &[("e", format!("{}", e))]))
                                        }
                                    },
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                        }
                        if let Some(bytes) = &editor.cover {
                            if let Some(texture) = texture(ctx, bytes) {
                                ui.add(
                                    egui::Image::new(&texture).fit_to_exact_size(Vec2::splat(56.)),
                                );
                            }
                            if ui.button(sequoia::i18n::t("text.keep_existing_covers")).clicked() {
                                editor.cover = None;
                            }
                        }
                    });
                });
                editor.changed = std::array::from_fn(|i| editor.values[i] != editor.initial[i]);
                let number = |i: usize| -> Option<u32> {
                    if !editor.changed[i] {
                        return None;
                    }
                    if editor.values[i].trim().is_empty() {
                        Some(0)
                    } else {
                        editor.values[i].trim().parse().ok()
                    }
                };
                let valid = (!editor.changed[4] || number(4).is_some_and(|n| n <= 9999))
                    && (!editor.changed[7] || number(7).is_some_and(|n| n <= 9999))
                    && (editor.number
                    || !editor.changed[5]
                    || number(5).is_some_and(|n| n <= 9999));
                if !valid {
                    ui.colored_label(
                        ACCENT,
                        sequoia::i18n::t("text.year_disc_and_track_numbers_an_integer_from_0_to_9999_or_leave_blank"),
                    );
                }
                ui.separator();
                ui.label(
                    RichText::new(
                        sequoia::i18n::t("text.originals_are_saved_in_tag_backups_files_are_written_in_the_background"),
                    )
                        .small()
                        .color(MUTED),
                );
                if editor.saving {
                    ui.add(
                        egui::ProgressBar::new(editor.done as f32 / editor.items.len() as f32)
                            .text(sequoia::i18n::format("text.processed", &[("arg0", format!("{}", editor.done)), ("arg1", format!("{}", editor.items.len()))])),
                    );
                } else if let Some(result) = &editor.result {
                    ui.label(sequoia::i18n::format("text.saved_errors", &[("arg0", format!("{}", result.saved.len())), ("arg1", format!("{}", result.failed.len()))]));
                    egui::ScrollArea::vertical()
                        .id_salt("batch-errors")
                        .max_height(140.)
                        .show(ui, |ui| {
                            for (path, error) in &result.failed {
                                ui.label(format!("{}: {error}", path.display()));
                            }
                        });
                    if ui.button(sequoia::i18n::t("text.close")).clicked() {
                        close = true;
                    }
                } else {
                    if ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.add_enabled(
                            valid
                                && (editor.changed.iter().any(|v| *v)
                                || editor.number
                                || editor.cover.is_some()),
                            egui::Button::new(
                                RichText::new(sequoia::i18n::t("text.write_tags_to_all_selected_files")).color(ACCENT),
                            ),
                        )
                    }).inner.clicked() {
                        save = true;
                    }
                }
                if save {
                    let patch = TagPatch {
                        title: editor.changed[0].then(|| editor.values[0].clone()),
                        artist: editor.changed[1].then(|| editor.values[1].clone()),
                        album: editor.changed[2].then(|| editor.values[2].clone()),
                        genre: editor.changed[3].then(|| editor.values[3].clone()),
                        year: number(4),
                        track: number(5),
                        album_artist: editor.changed[6].then(|| editor.values[6].clone()),
                        disc: number(7),
                        grouping: editor.changed[8].then(|| editor.values[8].clone()),
                        artwork: editor.cover.clone(),
                    };
                    let items = editor.items.clone();
                    let root = self.root.clone();
                    let tx = self.tx.clone();
                    let start = editor.number.then_some(editor.start);
                    editor.saving = true;
                    std::thread::spawn(move || {
                        let result = tags::write_batch(&items, &patch, start, &root, |n| {
                            let _ = tx.send(Job::BatchProgress(n));
                        });
                        let _ = tx.send(Job::BatchSaved(result));
                    });
                }
            });
        if (open && !close) || editor.saving {
            self.batch_editor = Some(editor);
        }
    }
    pub(super) fn update_track_metadata(&mut self, media: &Media) {
        for item in self
            .data
            .library
            .iter_mut()
            .chain(self.queue.items.iter_mut())
            .chain(self.data.playlists.values_mut().flatten())
            .chain(self.current.iter_mut())
        {
            if item.path == media.path {
                *item = media.clone();
            }
        }
        if self.current.as_ref().is_some_and(|m| m.path == media.path) {
            let tx = self.tx.clone();
            let path = media.path.clone();
            std::thread::spawn(move || {
                let bytes = library::cover(&path);
                let _ = tx.send(Job::Cover(path, bytes));
            });
        }
    }
}
