use super::*;
use sequoia::music::{Drag, SortKey, Target};

impl Player {
    pub(super) fn playlists_preview(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|s| s == "--playlists-preview")
            || self.smoke_started.elapsed()
            < Duration::from_millis(700 * (self.smoke_step as u64 + 1))
        {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            match self.smoke_step {
                0 => {
                    self.mode = Mode::Music;
                    self.error = None;
                    self.playlist_browser = true;
                    self.data.playlists.clear();
                    let item = Media {
                        path: "test-playlist.flac".into(),
                        title: "Тестовая запись".into(),
                        artist: "Тестовый исполнитель".into(),
                        audio: true,
                        ..Default::default()
                    };
                    self.data
                        .playlists
                        .insert("Тестовый плейлист".into(), vec![item]);
                    self.data
                        .playlists
                        .insert("Другой сохранённый список".into(), vec![]);
                    self.data.playlists.insert("Пустой плейлист".into(), vec![]);
                    anyhow::ensure!(
                        self.open_playlists.is_empty(),
                        "saved playlists opened automatically"
                    );
                    self.save();
                }
                1 | 3 | 5 => ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                    egui::UserData::default(),
                )),
                2 => {
                    self.open_music_playlist("Тестовый плейлист");
                    self.open_music_playlist("Тестовый плейлист");
                    anyhow::ensure!(self.open_playlists.len() == 1, "duplicate tabs");
                    self.queue.replace(self.target_items(&self.music_target), 0);
                    self.queue_playlist = Some("Тестовый плейлист".into());
                }
                4 => {
                    let item = Media {
                        path: "second.flac".into(),
                        title: "Вторая тестовая запись".into(),
                        audio: true,
                        ..Default::default()
                    };
                    self.music_drop(
                        self.music_target.clone(),
                        1,
                        &Drag {
                            items: vec![item],
                            source: None,
                            indices: vec![],
                        },
                        true,
                    );
                    self.close_music_playlist("Тестовый плейлист");
                    anyhow::ensure!(
                        self.open_playlists.is_empty() && self.music_target == Target::Queue,
                        "tab not closed"
                    );
                    anyhow::ensure!(
                        self.queue.index == Some(0)
                            && self.queue.items.len() == 2
                            && self.queue_playlist.as_deref() == Some("Тестовый плейлист"),
                        "closing changed playback binding"
                    );
                    let loaded = library::load(&self.root)?;
                    anyhow::ensure!(
                        loaded.playlists.len() == 3
                            && loaded.playlists["Тестовый плейлист"].len() == 2,
                        "close lost saved playlist"
                    );
                    self.open_music_playlist("Тестовый плейлист");
                    anyhow::ensure!(
                        self.target_items(&self.music_target).len() == 2,
                        "reopen lost edits"
                    );
                    self.close_music_playlist("Тестовый плейлист");
                }
                6 => {
                    std::fs::write("artifacts/playlists-regression.txt", "PASS: saved playlists start closed; open deduplicates tabs; edits persist on close and reload; reopen preserves contents; close preserves queue index, items and playlist binding. Programmatic application actions; audio playback and OS pointer interaction were not exercised.\n")?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(e) = result {
            let _ = std::fs::write("artifacts/playlists-regression.txt", format!("FAIL: {e:#}"));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
    }
    pub(super) fn grouping_preview(&mut self, ctx: &egui::Context) {
        if !std::env::args().any(|s| s == "--grouping-preview")
            || self.smoke_started.elapsed()
            < Duration::from_millis(700 * (self.smoke_step as u64 + 1))
        {
            return;
        }
        match self.smoke_step {
            0 => {
                self.mode = Mode::Music;
                self.error = None;
                self.data.library = (0..4)
                    .map(|i| Media {
                        path: format!("test-{i}.flac").into(),
                        title: format!("Тестовая запись {}", i + 1),
                        artist: "Исполнитель; Гость".into(),
                        album_artist: "Исполнитель альбома".into(),
                        album: "Тестовый альбом".into(),
                        disc: i / 2 + 1,
                        track: i % 2 + 1,
                        genre: "Тестовый жанр".into(),
                        year: 2026,
                        grouping: "Тестовая группа".into(),
                        audio: true,
                        ..Default::default()
                    })
                    .collect();
                self.data.settings.music_grouping = Default::default();
                self.data.settings.music_browser_width = 300.;
            }
            1 | 3 | 5 => {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()))
            }
            2 => self.grouping_draft = Some(self.data.settings.music_grouping.clone()),
            4 => {
                self.grouping_draft = None;
                self.editor(self.data.library[0].clone());
            }
            6 => ctx.send_viewport_cmd(egui::ViewportCommand::Close),
            _ => {}
        }
        self.smoke_step += 1;
    }
    pub(super) fn music_regression(&mut self, ctx: &egui::Context) {
        if !self.music_test
            || self.smoke_started.elapsed()
            < Duration::from_millis(350 * (1 + self.smoke_step as u64))
        {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            use std::io::Write;
            std::fs::create_dir_all("artifacts")?;
            let mut trace = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open("artifacts/music-drag-trace.txt")?;
            writeln!(
                trace,
                "step={} input={:?} payload={} source={:?} target={:?} rows={}",
                self.smoke_step,
                ctx.input(|i| (
                    i.pointer.hover_pos(),
                    i.pointer.any_down(),
                    i.pointer.any_released()
                )),
                egui::DragAndDrop::payload::<Drag>(ctx).is_some(),
                self.test_library_rect,
                self.test_playlist_rect,
                self.queue.items.len()
            )?;
            let source = self.test_library_rect.map(|r| r.center());
            let target = self.test_playlist_rect.map(|r| r.center());
            let tab = self.test_tab_rect.map(|r| r.center());
            let step = self.smoke_step;
            let mut inputs = Vec::new();
            let mut mouse = |message, pos: Option<egui::Pos2>| -> anyhow::Result<()> {
                let pos =
                    pos.ok_or_else(|| anyhow::anyhow!("missing UI hit area at step {step}"))?;
                inputs.push(match message {
                    0x0200 => egui::Event::PointerMoved(pos),
                    0x0201 | 0x0202 => egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed: message == 0x0201,
                        modifiers: egui::Modifiers::NONE,
                    },
                    _ => anyhow::bail!("Unsupported test input"),
                });
                Ok(())
            };
            match self.smoke_step {
                0 => {
                    self.mode = Mode::Music;
                    self.data.settings.music_grouping.levels = [sequoia::music::Level::None; 3];
                    self.filter.clear();
                    self.data.settings.music_browser_width = 280.;
                    let root = std::env::current_dir()?.join("artifacts/fixtures");
                    self.data.library = vec![
                        library::read_media(&root.join("test.mp3")),
                        library::read_media(&root.join("test.flac")),
                    ];
                    self.data.playlists.clear();
                    self.data.playlists.insert("Перетягивание".into(), vec![]);
                    self.open_playlists = vec!["Перетягивание".into()];
                    self.music_target = Target::Queue;
                    self.queue.replace(vec![], 0);
                    self.queue_playlist = None;
                }
                1 | 6 | 10 => {
                    mouse(0x0200, source)?;
                    mouse(0x0201, source)?;
                }
                2 => mouse(0x0200, source.map(|p| p + Vec2::new(40., 0.)))?,
                3 => mouse(0x0200, target)?,
                4 => mouse(0x0202, target)?,
                5 => {
                    anyhow::ensure!(
                        (self.data.settings.music_browser_width - 280.).abs() < 2.,
                        "sidebar width changed without dragging: {}",
                        self.data.settings.music_browser_width
                    );
                    anyhow::ensure!(
                        self.queue.items.len() == 1,
                        "library-to-queue drop failed: {} rows",
                        self.queue.items.len()
                    );
                }
                7 => mouse(0x0200, tab)?,
                8 => mouse(0x0202, tab)?,
                9 => {
                    anyhow::ensure!(
                        self.data.playlists["Перетягивание"].len() == 1,
                        "drop on inactive playlist tab failed"
                    );
                    self.music_target = Target::Playlist("Перетягивание".into());
                    self.play_music_target(0);
                    self.command(json!(["set_property", "loop-file", "inf"]));
                    self.command(json!(["set_property", "volume", 0]));
                }
                11 => mouse(0x0200, target)?,
                12 => mouse(0x0202, target)?,
                13 => {
                    anyhow::ensure!(
                        self.data.playlists["Перетягивание"].len() == 2
                            && self.queue.items.len() == 2,
                        "live playlist edit did not update playback queue"
                    );
                    self.playback_probe = self.position;
                }
                14 => {
                    let pos = self.test_row_rects.first().map(|r| r.center());
                    mouse(0x0200, pos)?;
                    mouse(0x0201, pos)?;
                }
                15 => mouse(
                    0x0200,
                    self.test_row_rects
                        .last()
                        .map(|r| egui::pos2(r.center().x, r.bottom() - 2.)),
                )?,
                16 => mouse(
                    0x0202,
                    self.test_row_rects
                        .last()
                        .map(|r| egui::pos2(r.center().x, r.bottom() - 2.)),
                )?,
                17 => {
                    anyhow::ensure!(
                        self.queue.index == Some(1),
                        "current index changed incorrectly after reorder: {:?}",
                        self.queue.index
                    );
                    anyhow::ensure!(
                        self.position > self.playback_probe + 0.05 && !self.paused,
                        "drag interrupted playback"
                    );
                    self.playback_probe = self.data.settings.music_browser_width as f64;
                }
                18 => {
                    let pos = Some(egui::pos2(
                        self.data.settings.music_browser_width - 2.,
                        260.,
                    ));
                    mouse(0x0200, pos)?;
                    mouse(0x0201, pos)?;
                }
                19 => mouse(
                    0x0200,
                    Some(egui::pos2(self.playback_probe as f32 + 65., 260.)),
                )?,
                20 => mouse(
                    0x0202,
                    Some(egui::pos2(self.playback_probe as f32 + 65., 260.)),
                )?,
                21 => {
                    anyhow::ensure!(
                        self.data.settings.music_browser_width as f64 > self.playback_probe + 25.,
                        "sidebar did not resize"
                    );
                    self.save();
                    anyhow::ensure!(
                        library::load(&self.root)?.playlists["Перетягивание"].len() == 2,
                        "playlist not persisted"
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                22 => {
                    std::fs::create_dir_all("artifacts")?;
                    std::fs::write("artifacts/music-regression.txt", "PASS: egui pointer drag from library to queue; drop on inactive saved playlist tab; drop into playing saved playlist; row reorder preserves playback identity and position; stable sidebar width; separator drag resizes sidebar; playlist persists\n")?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
            self.test_input.extend(inputs);
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::create_dir_all("artifacts");
            let _ = std::fs::write(
                "artifacts/music-regression.txt",
                format!("FAIL at step {}: {error:#}", self.smoke_step),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
    }
    pub(super) fn open_music_playlist(&mut self, name: &str) {
        if !self.data.playlists.contains_key(name) {
            return;
        }
        if !self.open_playlists.iter().any(|n| n == name) {
            self.open_playlists.push(name.into());
        }
        self.music_target = Target::Playlist(name.into());
        self.music_rows.clear();
    }
    fn close_music_playlist(&mut self, name: &str) {
        self.save();
        self.open_playlists.retain(|n| n != name);
        if self.music_target == Target::Playlist(name.into()) {
            self.music_target = Target::Queue;
            self.music_rows.clear();
        }
    }
    fn saved_playlists(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            if tool_label(ui, Icon::PlaylistAdd, sequoia::i18n::t("text.create"), false).clicked() {
                self.playlist_seed.clear();
                self.show_new_playlist = true;
            }
            ui.label(
                RichText::new(sequoia::i18n::format("text.saved", &[("arg0", format!("{}", self.data.playlists.len()))]))
                    .small()
                    .color(MUTED),
            );
        });
        ui.add(
            egui::TextEdit::singleline(&mut self.playlist_filter)
                .hint_text(sequoia::i18n::t("text.search_playlists"))
                .desired_width(f32::INFINITY),
        );
        let query = self.playlist_filter.to_lowercase();
        let lists: Vec<_> = self
            .data
            .playlists
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&query))
            .map(|(name, items)| (name.clone(), items.len()))
            .collect();
        egui::ScrollArea::vertical()
            .id_salt("saved-playlists")
            .show(ui, |ui| {
                if lists.is_empty() {
                    ui.label(if self.data.playlists.is_empty() {
                        sequoia::i18n::t("text.no_saved_playlists_yet_click_create")
                    } else {
                        sequoia::i18n::t("text.no_matches_found")
                    });
                }
                for (name, count) in lists {
                    let opened = self.open_playlists.contains(&name);
                    let response = ui
                        .add_sized(
                            [ui.available_width(), 26.],
                            egui::Button::new(&name).truncate().selected(opened),
                        )
                        .on_hover_text(sequoia::i18n::format("text.name_count_tracks_click_to_open", &[("name", format!("{}", name)), ("count", format!("{}", count))]));
                    if response.clicked() {
                        self.open_music_playlist(&name);
                    }
                    if let Some(payload) = response.dnd_release_payload::<Drag>() {
                        self.music_drop(Target::Playlist(name.clone()), count, &payload, true);
                    }
                    response.context_menu(|ui| {
                        if ui.button(sequoia::i18n::t("text.open_2")).clicked() {
                            self.open_music_playlist(&name);
                            ui.close();
                        }
                        if opened && ui.button(sequoia::i18n::t("text.close_tab")).clicked() {
                            self.close_music_playlist(&name);
                            ui.close();
                        }
                        if ui.button(sequoia::i18n::t("text.edit_track_tags")).clicked()
                        {
                            self.edit_tracks(self.target_items(&Target::Playlist(name.clone())));
                            ui.close();
                        }
                    });
                }
            });
    }
    fn sort_music_target(&mut self) {
        let items = self.target_items(&self.music_target);
        let mut order: Vec<_> = (0..items.len()).collect();
        order.sort_by(|&a, &b| {
            sequoia::music::compare(
                &items[a],
                &items[b],
                self.data.settings.music_sort,
                self.data.settings.music_descending,
            )
        });
        match &self.music_target {
            Target::Queue => {
                self.queue.reorder_to(&order);
                self.sync_music_queue();
            }
            Target::Playlist(name) => {
                if self.queue_playlist.as_ref() == Some(name) {
                    self.queue.reorder_to(&order);
                }
                self.data.playlists.insert(
                    name.clone(),
                    order.into_iter().map(|i| items[i].clone()).collect(),
                );
                self.save();
            }
        }
        self.music_rows.clear();
    }
    pub(super) fn music_drop_files(&mut self, paths: Vec<PathBuf>) {
        if self.scanning {
            self.notify(sequoia::i18n::t("text.wait_for_the_scan_to_finish"));
            return;
        }
        self.scanning = true;
        self.scanned = 0;
        self.cancel = Arc::new(AtomicBool::new(false));
        let cancel = self.cancel.clone();
        let tx = self.tx.clone();
        let target = self.music_target.clone();
        std::thread::spawn(move || {
            let result = (|| -> anyhow::Result<Vec<Media>> {
                let mut items = Vec::new();
                for path in paths {
                    if path.is_dir() {
                        let base = items.len();
                        items.extend(
                            library::scan(&path, true, true, &cancel, |n| {
                                let _ = tx.send(Job::Progress(base + n));
                            })?
                                .items,
                        );
                    } else if library::audio(&path) {
                        items.push(library::read_media(&path));
                    }
                }
                anyhow::ensure!(!items.is_empty(), sequoia::i18n::t("text.no_supported_audio_files_found"));
                Ok(items)
            })();
            let _ = tx.send(Job::MusicDrop(result, target));
        });
    }
    fn target_items(&self, target: &Target) -> Vec<Media> {
        match target {
            Target::Queue => self.queue.items.clone(),
            Target::Playlist(name) => self.data.playlists.get(name).cloned().unwrap_or_default(),
        }
    }
    pub(super) fn sync_music_queue(&mut self) {
        self.selected_queue.clear();
        if let Some(name) = &self.queue_playlist {
            self.data
                .playlists
                .insert(name.clone(), self.queue.items.clone());
            self.save();
        }
    }
    pub(super) fn music_drop(&mut self, target: Target, before: usize, drag: &Drag, copy: bool) {
        let same = drag.source.as_ref() == Some(&target) && !copy;
        let selection = match &target {
            Target::Queue => {
                let selection = if same {
                    self.queue.move_rows(&drag.indices, before)
                } else {
                    let at = before.min(self.queue.items.len());
                    self.queue.insert(at, drag.items.clone());
                    (at..at + drag.items.len()).collect()
                };
                self.sync_music_queue();
                selection
            }
            Target::Playlist(name) => {
                let items = self.data.playlists.entry(name.clone()).or_default();
                let selection = if same {
                    sequoia::music::move_rows(items, &drag.indices, before)
                } else {
                    let at = before.min(items.len());
                    items.splice(at..at, drag.items.clone());
                    (at..at + drag.items.len()).collect()
                };
                if self.queue_playlist.as_ref() == Some(name) {
                    if same {
                        self.queue.move_rows(&drag.indices, before);
                    } else {
                        self.queue.insert(before, drag.items.clone());
                    }
                }
                self.save();
                selection
            }
        };
        if self.music_target == target {
            self.music_rows = selection.into_iter().collect();
        }
    }
    fn play_music_target(&mut self, index: usize) {
        match self.music_target.clone() {
            Target::Queue => self.queue.select(index),
            Target::Playlist(name) => {
                let items = self.data.playlists.get(&name).cloned().unwrap_or_default();
                self.queue.replace(items, index);
                self.queue_playlist = Some(name);
            }
        }
        self.play_queue();
    }
    pub(super) fn remove_music_rows(&mut self) {
        let selected: Vec<_> = self.music_rows.iter().copied().rev().collect();
        let bound = matches!(&self.music_target, Target::Queue)
            || matches!(&self.music_target, Target::Playlist(name) if self.queue_playlist.as_ref() == Some(name));
        if bound {
            if self
                .queue
                .index
                .is_some_and(|i| self.music_rows.contains(&i))
            {
                self.stop();
            }
            for &i in &selected {
                self.queue.remove(i);
            }
        }
        match self.music_target.clone() {
            Target::Queue => self.sync_music_queue(),
            Target::Playlist(name) => {
                if let Some(items) = self.data.playlists.get_mut(&name) {
                    for &i in &selected {
                        if i < items.len() {
                            items.remove(i);
                        }
                    }
                }
                self.save();
            }
        }
        self.music_rows.clear();
    }
    pub(super) fn music_sidebar(&mut self, ctx: &egui::Context) {
        if self.mode != Mode::Music {
            return;
        }
        let panel = egui::SidePanel::left("music-browser-v2")
            .default_width(self.data.settings.music_browser_width.clamp(220., 480.))
            .width_range(220.0..=480.0)
            .resizable(true)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    if tool_label(ui, Icon::Album, sequoia::i18n::t("text.collection"), !self.playlist_browser).clicked()
                    {
                        self.playlist_browser = false;
                    }
                    if tool_label(ui, Icon::Playlist, sequoia::i18n::t("text.playlists"), self.playlist_browser).clicked()
                    {
                        self.playlist_browser = true;
                    }
                });
                ui.separator();
                if self.playlist_browser {
                    self.saved_playlists(ui);
                    return;
                }
                egui::TopBottomPanel::bottom("music-library-footer")
                    .resizable(false)
                    .frame(egui::Frame::new().fill(PANEL).inner_margin(egui::Margin {
                        top: 6,
                        ..Default::default()
                    }))
                    .show_inside(ui, |ui| self.music_library_footer(ui));
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.filter)
                            .hint_text(sequoia::i18n::t("text.search_library"))
                            .desired_width((ui.available_width() - 48.).max(80.)),
                    );
                    let grouping_button = icon(ui, Icon::Grouping, sequoia::i18n::t("text.library_grouping"));
                    egui::Popup::menu(&grouping_button).show(|ui| {
                        ui.label(sequoia::i18n::t("text.group_library"));
                        for preset in sequoia::music::Grouping::presets() {
                            if ui
                                .selectable_label(
                                    self.data.settings.music_grouping == preset,
                                    preset.label(),
                                )
                                .clicked()
                            {
                                self.data.settings.music_grouping = preset;
                                self.library_rows.clear();
                                self.save();
                                ui.close();
                            }
                        }
                        ui.separator();
                        if ui.button(sequoia::i18n::t("text.configure_grouping")).clicked() {
                            self.grouping_draft = Some(self.data.settings.music_grouping.clone());
                            ui.close();
                        }
                    });
                });
                ui.horizontal(|ui| {
                    let previous = (
                        self.data.settings.music_sort,
                        self.data.settings.music_descending,
                    );
                    egui::ComboBox::from_id_salt("music-library-sort")
                        .width((ui.available_width() - 40.).max(100.))
                        .selected_text(sequoia::i18n::format("text.by", &[("arg0", format!("{}", self.data.settings.music_sort.label()))]))
                        .show_ui(ui, |ui| {
                            for &(key, name) in SortKey::ALL {
                                ui.selectable_value(&mut self.data.settings.music_sort, key, sequoia::i18n::t(name));
                            }
                        });
                    if icon(
                        ui,
                        if self.data.settings.music_descending {
                            Icon::MoveDown
                        } else {
                            Icon::MoveUp
                        },
                        sequoia::i18n::t("text.change_sort_direction"),
                    )
                        .clicked()
                    {
                        self.data.settings.music_descending = !self.data.settings.music_descending;
                    }
                    if previous
                        != (
                        self.data.settings.music_sort,
                        self.data.settings.music_descending,
                    )
                    {
                        self.save();
                    }
                });
                let query = self.filter.to_lowercase();
                if tool_label(ui, Icon::MoveDown, sequoia::i18n::t("text.sort_playlist"), false)
                    .on_hover_text(
                        sequoia::i18n::t("text.apply_the_selected_field_and_direction_to_the_central_playlist"),
                    )
                    .clicked()
                {
                    self.sort_music_target();
                }
                let items = sequoia::music::sorted(
                    self.data
                        .library
                        .iter()
                        .filter(|m| {
                            format!(
                                "{} {} {} {} {} {} {}",
                                m.title,
                                m.artist,
                                m.album,
                                m.album_artist,
                                m.genre,
                                m.grouping,
                                m.path.display()
                            )
                                .to_lowercase()
                                .contains(&query)
                        })
                        .cloned()
                        .collect(),
                    self.data.settings.music_sort,
                    self.data.settings.music_descending,
                );
                let available = ui.available_height();
                egui::ScrollArea::vertical()
                    .id_salt("library-browser-list")
                    .max_height(available)
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        if items.is_empty() {
                            ui.label(
                                RichText::new(if self.data.library.is_empty() {
                                    sequoia::i18n::t("text.choose_a_music_folder_or_refresh_the_library")
                                } else {
                                    sequoia::i18n::t("text.no_matches_found")
                                })
                                    .color(MUTED),
                            );
                        }
                        self.library_tree(ui, &items, &items, 0);
                    });
            });
        self.data.settings.music_browser_width = panel.response.rect.width();
        self.grouping_dialog(ctx);
    }
    fn music_library_footer(&mut self, ui: &mut egui::Ui) {
        if let Some(current) = self.current.clone().filter(|m| m.audio) {
            ui.horizontal(|ui| {
                if let Some(cover) = &self.cover {
                    ui.add(egui::Image::new(cover).fit_to_exact_size(Vec2::splat(36.)));
                } else {
                    let (rect, _) = ui.allocate_exact_size(Vec2::splat(36.), Sense::hover());
                    icons::paint(ui, Icon::Record, rect, MUTED);
                }
                ui.add_sized(
                    [(ui.available_width() - 36.).max(20.), 36.],
                    egui::Label::new(RichText::new(&current.title).small()).truncate(),
                );
                if icon(ui, Icon::EditTags, sequoia::i18n::t("text.edit_tags")).clicked() {
                    self.editor(current);
                }
            });
        }
        ui.add(
            egui::Label::new(
                RichText::new(self.data.settings.music_root.display().to_string())
                    .small()
                    .color(MUTED),
            )
                .truncate(),
        )
            .on_hover_text(self.data.settings.music_root.display().to_string());
        ui.horizontal(|ui| {
            if icon(ui, Icon::Folder, sequoia::i18n::t("text.choose_music_folder")).clicked() {
                self.open_folder(true);
            }
            ui.label(
                RichText::new(sequoia::i18n::format("text.tracks_2", &[("arg0", format!("{}", self.data.library.len()))]))
                    .small()
                    .color(MUTED),
            );
        });
        ui.add_enabled_ui(!self.scanning, |ui| {
            if tool_label(ui, Icon::Refresh, sequoia::i18n::t("text.rescan_library"), false)
                .on_hover_text(sequoia::i18n::t("text.find_music_again_in_the_chosen_folder_and_all_subfolders"))
                .clicked()
            {
                self.start_scan(self.data.settings.music_root.clone(), true);
            }
        });
    }
    fn library_tree(&mut self, ui: &mut egui::Ui, group: &[Media], all: &[Media], depth: usize) {
        let grouping = self.data.settings.music_grouping.clone();
        if depth >= 3 || grouping.levels[depth] == sequoia::music::Level::None {
            let (range, step) = visible_rows(ui, group.len());
            if range.start > 0 {
                ui.add_space(range.start as f32 * step);
            }
            for item in &group[range.clone()] {
                ui.push_id(("library-track", &item.path), |ui| {
                    self.library_track(ui, item, all)
                });
            }
            if range.end < group.len() {
                ui.add_space((group.len() - range.end) as f32 * step);
            }
            return;
        }
        let mut groups = std::collections::BTreeMap::<(String, String), Vec<Media>>::new();
        for item in group {
            groups
                .entry(grouping.key(item, depth))
                .or_default()
                .push(item.clone());
        }
        let mut groups: Vec<_> = groups.into_iter().collect();
        groups.sort_by(|a, b| {
            a.0.0
                .to_lowercase()
                .cmp(&b.0.0.to_lowercase())
                .then(a.0.cmp(&b.0))
        });
        if self.data.settings.music_descending {
            groups.reverse();
        }
        for ((name, extra), tracks) in groups {
            let label = if extra.is_empty() {
                name.clone()
            } else {
                format!("{name} · {extra}")
            };
            let response = library_group_header(ui, format!("{label} ({})", tracks.len()))
                .id_salt((grouping.levels, depth, &name, &extra))
                .default_open(std::env::args().any(|s| s == "--grouping-preview"))
                .show(ui, |ui| self.library_tree(ui, &tracks, all, depth + 1));
            let header = response.header_response.interact(Sense::click_and_drag());
            header.context_menu(|ui| {
                if tool_label(ui, Icon::EditTags, sequoia::i18n::t("text.edit_group_tags"), false).clicked()
                {
                    self.edit_tracks(tracks.clone());
                    ui.close();
                }
            });
            if header.drag_started() {
                egui::DragAndDrop::set_payload(
                    ui.ctx(),
                    Drag {
                        items: tracks,
                        source: None,
                        indices: vec![],
                    },
                );
            }
            header.on_hover_text(sequoia::i18n::t("text.drag_this_group_to_a_playlist_click_to_expand"));
        }
    }
    fn grouping_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut draft) = self.grouping_draft.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        let mut cancel = false;
        egui::Window::new(sequoia::i18n::t("text.library_grouping"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(sequoia::i18n::t("text.choose_up_to_three_library_tree_levels"));
                egui::Grid::new("grouping-levels").show(ui, |ui| {
                    for (i, name) in [sequoia::i18n::t("text.first_level"), sequoia::i18n::t("text.second_level"), sequoia::i18n::t("text.third_level")]
                        .iter()
                        .enumerate()
                    {
                        ui.label(*name);
                        ui.add_enabled_ui(
                            i == 0 || draft.levels[i - 1] != sequoia::music::Level::None,
                            |ui| {
                                egui::ComboBox::from_id_salt(("group-level", i))
                                    .selected_text(draft.levels[i].label())
                                    .show_ui(ui, |ui| {
                                        for level in sequoia::music::Level::ALL {
                                            ui.selectable_value(
                                                &mut draft.levels[i],
                                                level,
                                                level.label(),
                                            );
                                        }
                                    });
                            },
                        );
                        ui.end_row();
                        if draft.levels[i] == sequoia::music::Level::None {
                            draft.levels[i..].fill(sequoia::music::Level::None);
                        }
                    }
                });
                ui.checkbox(
                    &mut draft.separate_albums,
                    sequoia::i18n::t("text.separate_albums_by_the_grouping_tag"),
                );
                ui.horizontal(|ui| {
                    if ui.button(sequoia::i18n::t("text.reset")).clicked() {
                        draft = Default::default();
                    }
                    apply = ui.button(sequoia::i18n::t("text.ok_2")).clicked();
                    cancel = ui.button(sequoia::i18n::t("text.cancel")).clicked();
                });
            });
        if apply {
            self.data.settings.music_grouping = draft;
            self.library_rows.clear();
            self.save();
        } else if open && !cancel {
            self.grouping_draft = Some(draft);
        }
    }
    fn library_track(&mut self, ui: &mut egui::Ui, item: &Media, all: &[Media]) {
        let response = browser_row(ui, item, self.library_rows.contains(&item.path));
        if self.music_test && all.first().is_some_and(|m| m.path == item.path) {
            self.test_library_rect = Some(response.rect);
        }
        if response.clicked() {
            let mods = ui.input(|i| i.modifiers);
            if mods.shift {
                let start = all.iter().position(|m| self.library_rows.contains(&m.path));
                let end = all.iter().position(|m| m.path == item.path);
                if let (Some(start), Some(end)) = (start, end) {
                    self.library_rows.extend(
                        all[start.min(end)..=start.max(end)]
                            .iter()
                            .map(|m| m.path.clone()),
                    );
                } else {
                    self.library_rows.insert(item.path.clone());
                }
            } else if mods.command || mods.ctrl {
                if !self.library_rows.remove(&item.path) {
                    self.library_rows.insert(item.path.clone());
                }
            } else {
                self.library_rows.clear();
                self.library_rows.insert(item.path.clone());
            }
        }
        if response.drag_started() {
            if !self.library_rows.contains(&item.path) {
                self.library_rows.clear();
                self.library_rows.insert(item.path.clone());
            }
            let items = all
                .iter()
                .filter(|m| self.library_rows.contains(&m.path))
                .cloned()
                .collect();
            egui::DragAndDrop::set_payload(
                ui.ctx(),
                Drag {
                    items,
                    source: None,
                    indices: vec![],
                },
            );
        }
        if response.double_clicked() {
            let target = self.music_target.clone();
            let at = self.target_items(&target).len();
            self.music_drop(
                target,
                at,
                &Drag {
                    items: vec![item.clone()],
                    source: None,
                    indices: vec![],
                },
                true,
            );
            self.play_music_target(at);
        }
        response.context_menu(|ui| {
            if tool_label(ui, Icon::PlaylistAdd, sequoia::i18n::t("text.add_to_playlist"), false).clicked()
            {
                let target = self.music_target.clone();
                let at = self.target_items(&target).len();
                self.music_drop(
                    target,
                    at,
                    &Drag {
                        items: vec![item.clone()],
                        source: None,
                        indices: vec![],
                    },
                    true,
                );
                ui.close();
            }
            if tool_label(ui, Icon::EditTags, sequoia::i18n::t("text.tags_and_cover"), false).clicked() {
                let items = if self.library_rows.contains(&item.path) {
                    all.iter()
                        .filter(|m| self.library_rows.contains(&m.path))
                        .cloned()
                        .collect()
                } else {
                    vec![item.clone()]
                };
                self.edit_tracks(items);
                ui.close();
            }
            if tool_label(ui, Icon::Folder, sequoia::i18n::t("text.show_in_folder"), false).clicked() {
                if let Err(error) = reveal(&item.path) {
                    self.error = Some(format!("{error:#}"));
                }
                ui.close();
            }
        });
    }
    pub(super) fn music(&mut self, ui: &mut egui::Ui) {
        egui::ScrollArea::horizontal()
            .id_salt("playlist-tabs")
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    let mut tabs = vec![(Target::Queue, sequoia::i18n::t("text.queue").to_string())];
                    tabs.extend(
                        self.open_playlists
                            .iter()
                            .map(|name| (Target::Playlist(name.clone()), name.clone())),
                    );
                    for (target, label) in tabs {
                        let response =
                            tool_label(ui, Icon::Playlist, &label, self.music_target == target);
                        if self.music_test && label == "Перетягивание" {
                            self.test_tab_rect = Some(response.rect);
                        }
                        if response.clicked() {
                            self.music_target = target.clone();
                            self.music_rows.clear();
                        }
                        if let Target::Playlist(name) = &target {
                            if icon(ui, Icon::Close, sequoia::i18n::t("text.close_tab_and_keep_playlist")).clicked()
                            {
                                self.close_music_playlist(name);
                            }
                        }
                        if let Some(payload) = response.dnd_release_payload::<Drag>() {
                            let at = self.target_items(&target).len();
                            self.music_drop(target, at, &payload, ui.input(|i| i.modifiers.ctrl));
                        }
                    }
                    if icon(ui, Icon::PlaylistAdd, sequoia::i18n::t("text.create_playlist")).clicked() {
                        self.playlist_seed.clear();
                        self.show_new_playlist = true;
                    }
                });
            });
        ui.separator();
        ui.horizontal(|ui| {
            if icon(ui, Icon::Play, sequoia::i18n::t("text.play_selected")).clicked() {
                self.play_music_target(self.music_rows.first().copied().unwrap_or(0));
            }
            if icon(ui, Icon::Close, sequoia::i18n::t("text.remove_selected_from_playlist_delete")).clicked()
            {
                self.remove_music_rows();
            }
            let selected_items: Vec<_> = self
                .target_items(&self.music_target)
                .into_iter()
                .enumerate()
                .filter(|(i, _)| self.music_rows.contains(i))
                .map(|(_, m)| m)
                .collect();
            if ui
                .add_enabled_ui(!selected_items.is_empty(), |ui| {
                    icon(
                        ui,
                        Icon::EditTags,
                        sequoia::i18n::t("text.edit_tags_of_selected_tracks"),
                    )
                })
                .inner
                .clicked()
            {
                self.edit_tracks(selected_items);
            }
            if icon(ui, Icon::Save, sequoia::i18n::t("text.save_playlist_in_the_application")).clicked()
            {
                if matches!(self.music_target, Target::Playlist(_)) {
                    self.save();
                    self.notify(sequoia::i18n::t("text.playlist_saved_in_the_application"));
                } else {
                    self.playlist_seed = self.target_items(&self.music_target);
                    self.show_new_playlist = true;
                }
            }
            if icon(ui, Icon::Export, sequoia::i18n::t("text.export_playlist_to_m3u")).clicked() {
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("playlist.m3u8")
                    .save_file()
                {
                    if let Err(e) =
                        library::export_m3u(&path, &self.target_items(&self.music_target))
                    {
                        self.error = Some(e.to_string());
                    }
                }
            }
            if let Target::Playlist(name) = self.music_target.clone() {
                if icon(ui, Icon::Trash, sequoia::i18n::t("text.delete_this_internal_playlist")).clicked()
                {
                    self.close_music_playlist(&name);
                    self.data.playlists.remove(&name);
                    if self.queue_playlist.as_ref() == Some(&name) {
                        self.queue_playlist = None;
                    }
                    self.music_target = Target::Queue;
                    self.music_rows.clear();
                    self.save();
                }
            }
            let items = self.target_items(&self.music_target);
            ui.label(
                RichText::new(sequoia::i18n::format("text.tracks_3", &[("arg0", format!("{}", items.len())), ("arg1", format!("{}", model::time(items.iter().map(|m| m.duration).sum())))]))
                    .small()
                    .color(MUTED),
            );
        });
        let items = self.target_items(&self.music_target);
        let target = self.music_target.clone();
        let header = ui
            .allocate_exact_size(Vec2::new(ui.available_width(), 22.), Sense::hover())
            .0;
        for (text, x) in [
            ("№", 0.),
            (sequoia::i18n::t("text.title"), 0.05),
            (sequoia::i18n::t("text.artist"), 0.43),
            (sequoia::i18n::t("text.album"), 0.67),
            (sequoia::i18n::t("text.time"), 0.91),
        ] {
            ui.painter().text(
                header.min + Vec2::new(header.width() * x + 4., 4.),
                egui::Align2::LEFT_TOP,
                text,
                FontId::proportional(11.),
                MUTED,
            );
        }
        ui.separator();
        let mut pending_drop = None;
        let mut play = None;
        self.test_row_rects.clear();
        let minimum_height = ui.available_height();
        let mut row_rects = Vec::new();
        let area = egui::ScrollArea::vertical()
            .id_salt("central-playlist")
            .scroll_source(egui::scroll_area::ScrollSource {
                drag: false,
                ..Default::default()
            })
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(ui.available_width(), minimum_height));
                let (range, step) = visible_rows(ui, items.len());
                let origin = ui.next_widget_position();
                for i in 0..items.len() {
                    row_rects.push((i, egui::Rect::from_min_size(
                        origin + Vec2::new(10., i as f32 * step),
                        Vec2::new((ui.available_width() - 10.).max(0.), 24.),
                    )));
                }
                if range.start > 0 {
                    ui.add_space(range.start as f32 * step);
                }
                for i in range.clone() {
                    let item = &items[i];
                    let response = ui
                        .push_id(("playlist-row", i), |ui| {
                            playlist_row(
                                ui,
                                item,
                                i,
                                self.music_rows.contains(&i),
                                self.current.is_some() && self.queue.index == Some(i) && (target == Target::Queue || matches!(&target, Target::Playlist(name) if self.queue_playlist.as_ref() == Some(name))),
                            )
                        })
                        .inner;
                    if self.music_test {
                        self.test_row_rects.push(response.rect);
                    }
                    response.context_menu(|ui| {
                        if tool_label(ui, Icon::EditTags, sequoia::i18n::t("text.edit_selected_tags"), false).clicked() {
                            let selected = if self.music_rows.contains(&i) {
                                items.iter().enumerate().filter(|(n, _)| self.music_rows.contains(n)).map(|(_, m)| m.clone()).collect()
                            } else { vec![item.clone()] };
                            self.edit_tracks(selected);
                            ui.close();
                        }
                    });
                    if response.clicked() {
                        let mods = ui.input(|i| i.modifiers);
                        if mods.shift {
                            let start = self.music_rows.first().copied().unwrap_or(i);
                            for row in start.min(i)..=start.max(i) {
                                self.music_rows.insert(row);
                            }
                        } else if mods.ctrl || mods.command {
                            if !self.music_rows.remove(&i) {
                                self.music_rows.insert(i);
                            }
                        } else {
                            self.music_rows.clear();
                            self.music_rows.insert(i);
                        }
                    }
                    if response.double_clicked() {
                        play = Some(i);
                    }
                    if let Some(movement) = playlist_drag(
                        ui, &response, i, &mut self.music_rows, &items, &target,
                    ) {
                        pending_drop = Some(movement);
                    }
                }
                if range.end < items.len() {
                    ui.add_space((items.len() - range.end) as f32 * step);
                }
                if items.is_empty() {
                    ui.add_space(18.);
                    ui.label(RichText::new(sequoia::i18n::t("text.drag_tracks_from_the_library_here")).color(MUTED));
                    ui.label(
                        RichText::new(sequoia::i18n::t("text.you_can_also_drag_files_from_your_file_manager"))
                            .small()
                            .color(MUTED),
                    );
                }
            });
        if self.music_test {
            self.test_playlist_rect = Some(area.inner_rect);
        }
        selection::marquee(
            ui, ("music-selection", format!("{target:?}")), area.inner_rect,
            &row_rects, &mut self.music_rows, true,
        );
        if let Some((before, payload)) = pending_drop {
            self.music_drop(target, before, &payload, ui.input(|i| i.modifiers.ctrl));
        } else {
            let response = ui.interact(
                area.inner_rect,
                ui.id().with("playlist-append-drop"),
                Sense::hover(),
            );
            if let Some(payload) = response.dnd_release_payload::<Drag>() {
                self.music_drop(
                    target,
                    items.len(),
                    &payload,
                    ui.input(|i| i.modifiers.ctrl),
                );
            }
        }
        if let Some(index) = play {
            self.play_music_target(index);
        }
        if let Some(payload) = egui::DragAndDrop::payload::<Drag>(ui.ctx()) {
            if let Some(pos) = ui.input(|i| i.pointer.hover_pos()) {
                egui::Area::new(egui::Id::new("music-drag-preview"))
                    .order(egui::Order::Tooltip)
                    .fixed_pos(pos + Vec2::new(14., 14.))
                    .interactable(false)
                    .show(ui.ctx(), |ui| {
                        egui::Frame::popup(ui.style()).show(ui, |ui| {
                            ui.label(sequoia::i18n::format("text.tracks_2", &[("arg0", format!("{}", payload.items.len()))]));
                        });
                    });
            }
        }
    }
}
fn library_group_header(ui: &egui::Ui, text: String) -> egui::CollapsingHeader {
    let width =
        (ui.available_width() - ui.spacing().indent - ui.spacing().button_padding.x).max(1.);
    // CollapsingHeader forces Extend for text; a pre-laid-out galley keeps truncation.
    let galley = egui::WidgetText::from(text).into_galley(
        ui,
        Some(egui::TextWrapMode::Truncate),
        width,
        egui::TextStyle::Button,
    );
    egui::CollapsingHeader::new(galley)
}

fn visible_rows(ui: &egui::Ui, count: usize) -> (std::ops::Range<usize>, f32) {
    let step = 24. + ui.spacing().item_spacing.y;
    let origin = ui.next_widget_position().y;
    let start = (((ui.clip_rect().top() - origin) / step).floor().max(0.) as usize).min(count);
    let end = (((ui.clip_rect().bottom() - origin) / step).ceil().max(0.) as usize + 1)
        .min(count)
        .max(start);
    (start..end, step)
}
fn browser_row(ui: &mut egui::Ui, item: &Media, selected: bool) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 24.),
        Sense::click_and_drag(),
    );
    if selected || response.hovered() {
        ui.painter().rect_filled(
            rect,
            2.,
            if selected {
                Color32::from_rgb(78, 65, 46)
            } else {
                RAISED
            },
        );
    }
    text_cell(
        ui,
        &item.title,
        rect.shrink2(Vec2::new(5., 3.)),
        if selected { ACCENT } else { INK },
    );
    response.on_hover_text(sequoia::i18n::format("text.n_ndrag_to_playlist", &[("arg0", format!("{}", item.title)), ("arg1", format!("{}", item.subtitle()))]))
}
pub(super) fn playlist_drag(
    ui: &egui::Ui,
    response: &egui::Response,
    index: usize,
    selected: &mut std::collections::BTreeSet<usize>,
    items: &[Media],
    target: &Target,
) -> Option<(usize, Arc<Drag>)> {
    if response.drag_started() {
        if !selected.contains(&index) {
            selected.clear();
            selected.insert(index);
        }
        let indices: Vec<_> = selected.iter().copied().filter(|&i| i < items.len()).collect();
        egui::DragAndDrop::set_payload(ui.ctx(), Drag {
            items: indices.iter().map(|&i| items[i].clone()).collect(),
            source: Some(target.clone()),
            indices,
        });
    }
    let after = ui.input(|i| i.pointer.hover_pos().is_some_and(|p| p.y > response.rect.center().y));
    let before = index + usize::from(after);
    if response.dnd_hover_payload::<Drag>().is_some() {
        let y = if after { response.rect.bottom() } else { response.rect.top() };
        ui.painter().hline(response.rect.x_range(), y, Stroke::new(2., ACCENT));
    }
    response.dnd_release_payload::<Drag>().map(|payload| (before, payload))
}

pub(super) fn playlist_row(
    ui: &mut egui::Ui,
    item: &Media,
    index: usize,
    selected: bool,
    playing: bool,
) -> egui::Response {
    let (allocated, _) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), 24.),
        Sense::hover(),
    );
    let rect = egui::Rect::from_min_max(allocated.min + Vec2::new(10., 0.), allocated.max);
    let response = ui.interact(rect, ui.id().with("row-hit"), Sense::click_and_drag());
    if selected || response.hovered() || index % 2 == 0 {
        ui.painter().rect_filled(
            rect,
            1.,
            if selected {
                Color32::from_rgb(78, 65, 46)
            } else {
                PANEL
            },
        );
    }
    let number = (index + 1).to_string();
    let duration = model::time(item.duration);
    for (text, left, right) in [
        (number.as_str(), 0., 0.05),
        (item.title.as_str(), 0.05, 0.43),
        (item.artist.as_str(), 0.43, 0.67),
        (item.album.as_str(), 0.67, 0.91),
        (duration.as_str(), 0.91, 1.),
    ] {
        let cell = egui::Rect::from_min_max(
            rect.min + Vec2::new(rect.width() * left + 4., 4.),
            egui::pos2(rect.left() + rect.width() * right - 4., rect.bottom()),
        );
        text_cell(ui, text, cell, if playing { ACCENT } else { INK });
    }
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 1., Stroke::new(1., ACCENT), egui::StrokeKind::Inside);
    }
    response.on_hover_text(item.path.display().to_string())
}
fn text_cell(ui: &egui::Ui, text: &str, rect: egui::Rect, color: Color32) {
    let mut job = egui::text::LayoutJob::simple(
        text.into(),
        FontId::proportional(12.),
        color,
        rect.width().max(1.),
    );
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    ui.painter()
        .galley(rect.min, ui.painter().layout_job(job), color);
}

#[cfg(test)]
mod layout_tests {
    use super::*;

    #[test]
    fn long_library_groups_leave_queue_visible() {
        for window_width in [960., 1100., 1380.] {
            for browser_width in [220., 280., 480.] {
                for expanded in [false, true] {
                    let ctx = egui::Context::default();
                    for _ in 0..3 {
                        let _ = ctx.run(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    Vec2::new(window_width, 720.),
                                )),
                                ..Default::default()
                            },
                            |ctx| {
                                let sidebar = egui::SidePanel::left("library")
                                    .default_width(browser_width)
                                    .width_range(220.0..=480.0)
                                    .frame(egui::Frame::new().fill(PANEL).inner_margin(8))
                                    .show(ctx, |ui| {
                                        egui::ScrollArea::vertical()
                                            .auto_shrink([false, false])
                                            .show(ui, |ui| {
                                                library_group_header(
                                                    ui,
                                                    "Long artist / album / folder name ".repeat(30),
                                                )
                                                    .id_salt("artist")
                                                    .default_open(expanded)
                                                    .show(ui, |ui| {
                                                        library_group_header(
                                                            ui,
                                                            "Nested album name ".repeat(30),
                                                        )
                                                            .id_salt("album")
                                                            .default_open(expanded)
                                                            .show(ui, |ui| {
                                                                browser_row(
                                                                    ui,
                                                                    &Media {
                                                                        title: "Long track name "
                                                                            .repeat(30),
                                                                        ..Default::default()
                                                                    },
                                                                    false,
                                                                );
                                                            });
                                                    });
                                            });
                                    });
                                assert!(
                                    sidebar.response.rect.width() <= browser_width + 1.,
                                    "library expanded to {} at requested width {browser_width}",
                                    sidebar.response.rect.width()
                                );
                                egui::CentralPanel::default().show(ctx, |ui| {
                                    let queue = ui.label(sequoia::i18n::t("text.queue"));
                                    assert!(queue.rect.right() < window_width);
                                    assert!(
                                        ui.available_width() >= window_width - browser_width - 30.
                                    );
                                });
                            },
                        );
                    }
                }
            }
        }
    }
}
