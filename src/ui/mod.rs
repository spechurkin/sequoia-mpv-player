use eframe::egui::{self, Align, Color32, FontId, Key, Layout, RichText, Sense, Stroke, Vec2};
mod batch_tags;
mod music;
mod selection;
mod settings;
mod web;
#[cfg(windows)]
mod tray;
#[cfg(windows)]
mod tray_regression;
mod window_frame;
use raw_window_handle::HasWindowHandle;
use sequoia::{
    icons::{self, Icon},
    library,
    model::{self, Data, Media, Queue, Repeat, Settings},
    mpv::{self, Event, Mpv},
    online::{self, Candidate},
};
use serde_json::{json, Value};
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc,
    },
    time::{Duration, Instant},
};

const BG: Color32 = Color32::from_rgb(22, 25, 26);
const PANEL: Color32 = Color32::from_rgb(29, 33, 35);
const RAISED: Color32 = Color32::from_rgb(40, 46, 48);
const INK: Color32 = Color32::from_rgb(241, 239, 231);
const MUTED: Color32 = Color32::from_rgb(171, 183, 184);
const ACCENT: Color32 = Color32::from_rgb(222, 177, 119);
#[derive(Clone, Copy, PartialEq)]
enum Mode {
    Video,
    Music,
}
enum Job {
    Web(u64, anyhow::Result<Vec<Media>>),
    Scan(anyhow::Result<library::ScanResult>, bool),
    Progress(usize),
    Cover(PathBuf, Option<Vec<u8>>),
    Search(anyhow::Result<Vec<Candidate>>),
    Artwork(anyhow::Result<Vec<u8>>),
    Saved(anyhow::Result<Media>),
    BatchProgress(usize),
    BatchSaved(sequoia::tags::BatchResult),
    MusicDrop(anyhow::Result<Vec<Media>>, sequoia::music::Target),
}
struct Editor {
    media: Media,
    cover: Option<Vec<u8>>,
    results: Vec<Candidate>,
    searching: bool,
    saving: bool,
    query_title: String,
    query_artist: String,
}
pub struct Player {
    instance: sequoia::instance::Instance,
    show_link: bool,
    link_input: String,
    link_loading: bool,
    link_generation: u64,
    link_cancel: Arc<AtomicBool>,
    #[cfg(windows)]
    link_drop: Option<sequoia::playback::web_drop::LinkDrop>,
    data: Data,
    root: PathBuf,
    queue: Queue,
    mode: Mode,
    grouping_draft: Option<sequoia::music::Grouping>,
    filter: String,
    engine: Option<Mpv>,
    #[cfg(windows)]
    host: Option<sequoia::native::VideoHost>,
    #[cfg(windows)]
    tray: Option<tray::Tray>,
    hidden_to_tray: bool,
    exiting: bool,
    paused: bool,
    position: f64,
    duration: f64,
    volume: f64,
    speed: f64,
    mute: bool,
    tracks: Vec<Value>,
    chapters: Vec<Value>,
    current: Option<Media>,
    cover: Option<egui::TextureHandle>,
    error: Option<String>,
    notice: String,
    osd_status: String,
    notice_until: Instant,
    show_settings: bool,
    draft: Settings,
    settings_category: settings::Category,
    show_console: bool,
    console_input: String,
    console_output: String,
    show_help: bool,
    editor: Option<Editor>,
    batch_editor: Option<batch_tags::BatchEditor>,
    show_new_playlist: bool,
    playlist_name: String,
    playlist_seed: Vec<Media>,
    tx: mpsc::Sender<Job>,
    rx: mpsc::Receiver<Job>,
    scanning: bool,
    cancel: Arc<AtomicBool>,
    scanned: usize,
    queue_open: bool,
    selected_queue: std::collections::BTreeSet<usize>,
    fullscreen: bool,
    initial_scan: bool,
    smoke: bool,
    smoke_step: usize,
    smoke_started: Instant,
    popup_test: bool,
    track_popups: [Option<egui::Id>; 3],
    playback_probe: f64,
    music_target: sequoia::music::Target,
    open_playlists: Vec<String>,
    playlist_browser: bool,
    playlist_filter: String,
    music_rows: std::collections::BTreeSet<usize>,
    library_rows: std::collections::BTreeSet<PathBuf>,
    queue_playlist: Option<String>,
    music_test: bool,
    test_library_rect: Option<egui::Rect>,
    test_playlist_rect: Option<egui::Rect>,
    test_tab_rect: Option<egui::Rect>,
    test_row_rects: Vec<egui::Rect>,
    test_input: Vec<egui::Event>,
}
impl Player {
    fn can_hide_to_tray(&self) -> bool {
        #[cfg(windows)]
        {
            self.data.settings.close_to_tray
                && self.tray.as_ref().is_some_and(|tray| tray.available())
                && !self.smoke
                && !self.popup_test
                && !self.music_test
                && !std::env::args().any(|arg| arg.ends_with("-preview"))
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    fn tray_events(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        {
            use tray::Action;
            let actions = self
                .tray
                .as_ref()
                .map(|tray| tray.actions())
                .unwrap_or_default();
            for action in actions {
                match action {
                    Action::Show => {
                        self.hidden_to_tray = false;
                        if let Some(tray) = &self.tray {
                            let _ = tray.background(false);
                        }
                        ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                        ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    }
                    Action::TogglePause => self.toggle_pause(),
                    Action::Previous => {
                        if let Some(item) = self.queue.previous() {
                            self.play(item);
                        }
                    }
                    Action::Next => self.next(false),
                    Action::Stop => self.stop(),
                    Action::Exit => {
                        self.exiting = true;
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
            if let Some(tray) = &mut self.tray {
                let title = self
                    .current
                    .as_ref()
                    .map(|media| {
                        if media.artist.is_empty() {
                            media.title.clone()
                        } else {
                            format!("{} · {}", media.artist, media.title)
                        }
                    })
                    .unwrap_or_default();
                tray.update(
                    &title,
                    self.paused,
                    self.current.is_some() || !self.queue.items.is_empty(),
                    self.current.is_some(),
                );
            }
        }
        if ctx.input(|i| i.viewport().close_requested()) && !self.exiting && self.can_hide_to_tray()
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            #[cfg(windows)]
            if let Some(tray) = &self.tray {
                if let Err(error) = tray.background(true) {
                    self.error = Some(error.to_string());
                    return;
                }
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            self.hidden_to_tray = true;
            self.save();
        }
    }

    pub fn new(cc: &eframe::CreationContext<'_>, instance: sequoia::instance::Instance) -> Self {
        let ctx = &cc.egui_ctx;
        ctx.set_fonts(sequoia::fonts::system_definitions());
        let mut visuals = egui::Visuals::dark();
        visuals.panel_fill = BG;
        visuals.window_fill = PANEL;
        visuals.extreme_bg_color = BG;
        visuals.override_text_color = Some(INK);
        visuals.selection.bg_fill = Color32::from_rgb(82, 67, 47);
        visuals.widgets.inactive.bg_fill = RAISED;
        visuals.widgets.inactive.weak_bg_fill = RAISED;
        visuals.widgets.hovered.bg_fill = Color32::from_rgb(61, 69, 71);
        visuals.widgets.active.bg_fill = Color32::from_rgb(89, 72, 49);
        visuals.window_corner_radius = egui::CornerRadius::same(10);
        ctx.set_visuals(visuals);
        let mut style = (*ctx.style()).clone();
        style.spacing.item_spacing = Vec2::new(5., 5.);
        style.spacing.button_padding = Vec2::new(6., 3.);
        style.spacing.interact_size = Vec2::new(24., 22.);
        style
            .text_styles
            .insert(egui::TextStyle::Heading, FontId::proportional(14.));
        style
            .text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.));
        style
            .text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(12.));
        style
            .text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(12.));
        ctx.set_style(style);
        let root = library::data_dir();
        let (data, mut error) = match library::load(&root) {
            Ok(data) => (data, None),
            Err(e) => (
                Data::default(),
                Some(sequoia::i18n::format("text.could_not_read_the_library_the_damaged_file_was_preserved_e", &[("e", format!("{}", e))])),
            ),
        };
        sequoia::i18n::set_language(&data.settings.language);
        #[cfg(windows)]
        let host = cc
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(handle) => {
                    match sequoia::native::VideoHost::new(handle.hwnd.get()) {
                        Ok(host) => Some(host),
                        Err(e) => {
                            error = Some(e.to_string());
                            None
                        }
                    }
                }
                _ => None,
            });
        let engine_path = mpv::resolve(&data.settings.mpv_path);
        #[cfg(windows)]
        let tray = match cc
            .window_handle()
            .ok()
            .and_then(|handle| match handle.as_raw() {
                raw_window_handle::RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!(sequoia::i18n::t("text.no_windows_window")))
            .and_then(|hwnd| tray::Tray::new(ctx, hwnd))
        {
            Ok(tray) => Some(tray),
            Err(e) => {
                error = Some(sequoia::i18n::format("text.tray_unavailable_closing_will_exit_the_application_e", &[("e", format!("{}", e))]));
                None
            }
        };
        let _ = library::save(&root, &data);
        #[cfg(windows)]
        let hwnd = host.as_ref().map(|h| h.id());
        #[cfg(not(windows))]
        let hwnd = None;
        let engine = match engine_path {
            Some(path) => match Mpv::start(&path, hwnd, &root.join("mpv"), false) {
                Ok(engine) => Some(engine),
                Err(e) => {
                    error = Some(e.to_string());
                    None
                }
            },
            None => {
                error = Some(sequoia::i18n::t("text.mpv_exe_not_found_open_settings_select_mpv_exe_and_click_save_and_connect").into());
                None
            }
        };
        let (tx, rx) = mpsc::channel();
        let mut draft = data.settings.clone();
        draft.language = sequoia::i18n::language();
        let volume = draft.volume;
        #[cfg(windows)]
        let link_drop = cc.window_handle().ok().and_then(|handle| match handle.as_raw() {
            raw_window_handle::RawWindowHandle::Win32(handle) =>
                match sequoia::playback::web_drop::LinkDrop::new(handle.hwnd.get(), ctx.clone()) {
                    Ok(drop) => Some(drop),
                    Err(e) => {
                        error = Some(sequoia::i18n::format("text.url_drag_and_drop_unavailable_e_use_open_url", &[("e", format!("{}", e))]));
                        None
                    }
                },
            _ => None,
        });
        let app = Self {
            show_link: false,
            link_input: String::new(),
            link_loading: false,
            link_generation: 0,
            link_cancel: Arc::new(AtomicBool::new(false)),
            #[cfg(windows)]
            link_drop,
            data,
            root,
            queue: Queue::default(),
            mode: Mode::Video,
            grouping_draft: None,
            filter: String::new(),
            engine,
            #[cfg(windows)]
            host,
            #[cfg(windows)]
            tray,
            instance,
            hidden_to_tray: false,
            exiting: false,
            paused: true,
            position: 0.,
            duration: 0.,
            volume,
            speed: 1.,
            mute: false,
            tracks: vec![],
            chapters: vec![],
            current: None,
            cover: None,
            error,
            notice: String::new(),
            osd_status: String::new(),
            notice_until: Instant::now(),
            show_settings: false,
            draft,
            settings_category: Default::default(),
            show_console: false,
            console_input: r#"["get_property", "video-params"]"#.into(),
            console_output: String::new(),
            show_help: false,
            editor: None,
            batch_editor: None,
            show_new_playlist: false,
            playlist_name: String::new(),
            playlist_seed: vec![],
            tx,
            rx,
            scanning: false,
            cancel: Arc::new(AtomicBool::new(false)),
            scanned: 0,
            queue_open: true,
            selected_queue: Default::default(),
            fullscreen: false,
            initial_scan: false,
            smoke: std::env::args().any(|s| s == "--smoke-test"),
            smoke_step: 0,
            smoke_started: Instant::now(),
            popup_test: std::env::args().any(|s| s == "--popup-test"),
            track_popups: [None; 3],
            playback_probe: 0.,
            music_target: sequoia::music::Target::Queue,
            open_playlists: Vec::new(),
            playlist_browser: false,
            playlist_filter: String::new(),
            music_rows: Default::default(),
            library_rows: Default::default(),
            queue_playlist: None,
            music_test: std::env::args().any(|s| s == "--music-test"),
            test_library_rect: None,
            test_playlist_rect: None,
            test_tab_rect: None,
            test_row_rects: vec![],
            test_input: vec![],
        };
        app.command(json!(["set_property", "volume", volume]));
        app
    }
    fn command(&self, cmd: Value) {
        if let Some(engine) = &self.engine {
            let _ = engine.command(cmd);
        }
    }
    fn request(&mut self, cmd: Value) -> Option<Value> {
        match self
            .engine
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!(sequoia::i18n::t("text.mpv_not_connected")))
            .and_then(|m| m.request(cmd))
        {
            Ok(v) => Some(v),
            Err(e) => {
                self.error = Some(e.to_string());
                None
            }
        }
    }
    fn open_settings(&mut self) {
        match library::read_mpv_config(&self.root, &self.data.settings.mpv_config) {
            Ok(content) => {
                self.data.settings.mpv_config = content;
                self.draft = self.data.settings.clone();
                self.draft.language = sequoia::i18n::language();
                self.show_settings = true;
            }
            Err(e) => self.error = Some(e.to_string()),
        }
    }
    fn save(&mut self) {
        if let Err(e) = library::save(&self.root, &self.data) {
            self.error = Some(sequoia::i18n::format("text.failed_to_save_settings_e", &[("e", format!("{}", e))]));
        }
    }
    fn notify(&mut self, message: impl Into<String>) {
        self.notice = message.into();
        self.notice_until = Instant::now() + Duration::from_secs(8);
    }
    fn play(&mut self, media: Media) {
        self.cancel_link();
        if !sequoia::playback::web::is_url(&media.path.to_string_lossy()) && !media.path.is_file() {
            self.error = Some(sequoia::i18n::format("text.file_unavailable", &[("arg0", format!("{}", media.path.display()))]));
            return;
        }
        let is_gif = media
            .path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("gif"));
        if self
            .request(json!([
                "set_property",
                "loop-file",
                if is_gif { "inf" } else { "no" }
            ]))
            .is_none()
        {
            return;
        }
        if self
            .request(json!(["loadfile", media.path, "replace"]))
            .is_none()
        {
            return;
        }
        self.command(json!(["set_property", "pause", false]));
        self.mode = if media.audio {
            Mode::Music
        } else {
            Mode::Video
        };
        self.position = 0.;
        self.duration = media.duration;
        self.paused = false;
        self.cover = None;
        let tx = self.tx.clone();
        let path = media.path.clone();
        std::thread::spawn(move || {
            let bytes = library::cover(&path);
            let _ = tx.send(Job::Cover(path, bytes));
        });
        self.current = Some(media);
    }
    fn play_queue(&mut self) {
        if self.queue.index.is_none() && !self.queue.items.is_empty() {
            self.queue.select(0);
        }
        if let Some(media) = self.queue.current().cloned() {
            self.play(media);
        }
    }
    fn toggle_pause(&mut self) {
        if self.current.is_none() {
            self.play_queue();
        } else {
            self.command(json!(["cycle", "pause"]));
        }
    }
    fn stop(&mut self) {
        self.cancel_link();
        self.command(json!(["stop"]));
        self.paused = true;
        self.position = 0.;
        self.current = None;
        self.cover = None;
        self.tracks.clear();
        self.chapters.clear();
    }
    fn next(&mut self, automatic: bool) {
        if let Some(item) = self.queue.advance(automatic) {
            self.play(item);
        } else if automatic {
            self.stop();
        }
    }
    fn open_files(&mut self) {
        if let Some(files) = rfd::FileDialog::new()
            .set_title(sequoia::i18n::t("text.open_video_gif_or_music"))
            .pick_files()
        {
            self.add_paths(files);
        }
    }
    fn add_paths(&mut self, files: Vec<PathBuf>) {
        let links: Vec<_> = files.iter().filter_map(|p| {
            let text = p.to_string_lossy();
            if sequoia::playback::web::is_url(&text) { Some(text.into_owned()) } else { sequoia::playback::web::shortcut(p) }
        }).collect();
        if !links.is_empty() {
            self.open_links(links);
            return;
        }
        let items: Vec<_> = files
            .iter()
            .filter(|p| p.is_file())
            .map(|p| library::read_media(p))
            .collect();
        if !items.is_empty() {
            self.queue_playlist = None;
            self.music_target = sequoia::music::Target::Queue;
            self.music_rows.clear();
            self.queue.replace(items, 0);
            self.play_queue();
        }
    }
    fn open_folder(&mut self, catalog: bool) {
        if let Some(root) = rfd::FileDialog::new()
            .set_title(if catalog {
                sequoia::i18n::t("text.music_library_folder")
            } else {
                sequoia::i18n::t("text.media_folder")
            })
            .pick_folder()
        {
            if catalog {
                self.data.settings.music_root = root.clone();
                self.save();
            }
            self.start_scan(root, catalog);
        }
    }
    fn start_scan(&mut self, root: PathBuf, catalog: bool) {
        if self.scanning {
            self.notify(sequoia::i18n::t("text.wait_for_the_scan_to_finish_or_cancel_it"));
            return;
        }
        self.scanning = true;
        self.scanned = 0;
        self.cancel = Arc::new(AtomicBool::new(false));
        let tx = self.tx.clone();
        let cancel = self.cancel.clone();
        let recursive = if catalog {
            true
        } else {
            self.data.settings.recursive
        };
        std::thread::spawn(move || {
            let result = library::scan(&root, recursive, catalog, &cancel, |n| {
                let _ = tx.send(Job::Progress(n));
            });
            let _ = tx.send(Job::Scan(result, catalog));
        });
    }
    fn screenshot(&mut self) {
        if self.current.as_ref().is_none_or(|m| m.audio) {
            self.notify(sequoia::i18n::t("text.screenshots_are_available_when_playing_video_or_gif"));
            return;
        }
        let folder = &self.data.settings.screenshot_folder;
        if let Err(e) = std::fs::create_dir_all(folder) {
            self.error = Some(e.to_string());
            return;
        }
        let folder = folder.clone();
        let Some(position) = self
            .request(json!(["get_property", "time-pos"]))
            .and_then(|value| value.as_f64())
        else {
            return;
        };
        let title = &self.current.as_ref().unwrap().title;
        let file = folder.join(sequoia::playback::screenshot::filename(
            title,
            chrono::Local::now().date_naive(),
            position,
        ));
        if self
            .request(json!(["screenshot-to-file", file, "subtitles"]))
            .is_some()
        {
            self.notify(sequoia::i18n::format("text.screenshot_saved", &[("arg0", format!("{}", file.display()))]));
        }
    }
    fn editor(&mut self, media: Media) {
        if self.batch_editor.is_some() || self.editor.is_some() {
            return;
        }
        let media = if media.path.is_file() {
            library::read_media(&media.path)
        } else {
            media
        };
        let cover = library::cover(&media.path);
        let auto_lookup = self.data.settings.auto_lookup && !media.title.trim().is_empty();
        if auto_lookup {
            let tx = self.tx.clone();
            let title = media.title.clone();
            let artist = media.artist.clone();
            std::thread::spawn(move || {
                let _ = tx.send(Job::Search(online::search(&title, &artist)));
            });
        }
        self.editor = Some(Editor {
            query_title: media.title.clone(),
            query_artist: media.artist.clone(),
            media,
            cover,
            results: vec![],
            searching: auto_lookup,
            saving: false,
        });
    }
    fn handle_events(&mut self, ctx: &egui::Context) {
        let events: Vec<_> = self
            .engine
            .as_ref()
            .map(|m| m.events.try_iter().collect())
            .unwrap_or_default();
        for event in events {
            match event {
                Event::Osd(message) => self.osd_status = message,
                Event::Property(name, data) => match name.as_str() {
                    "pause" => self.paused = data.as_bool().unwrap_or(true),
                    "time-pos" => self.position = data.as_f64().unwrap_or(0.),
                    "duration" => self.duration = data.as_f64().unwrap_or(0.),
                    "volume" => self.volume = data.as_f64().unwrap_or(self.volume),
                    "speed" => self.speed = data.as_f64().unwrap_or(1.),
                    "mute" => self.mute = data.as_bool().unwrap_or(false),
                    "track-list" => self.tracks = data.as_array().cloned().unwrap_or_default(),
                    "chapter-list" => self.chapters = data.as_array().cloned().unwrap_or_default(),
                    _ => {}
                },
                Event::Eof => self.next(true),
                Event::Error(e) => self.error = Some(e),
                Event::Disconnected => {
                    self.osd_status.clear();
                    self.engine = None;
                    self.paused = true;
                    self.error = Some(sequoia::i18n::t("text.the_mpv_connection_was_closed_open_settings_and_reconnect_the_engine").into());
                }
            }
        }
        while let Ok(job) = self.rx.try_recv() {
            match job {
                Job::Web(generation, result) => self.link_result(generation, result),
                Job::Progress(n) => self.scanned = n,
                Job::MusicDrop(result, target) => {
                    self.scanning = false;
                    match result {
                        Ok(items) => {
                            if matches!(&target, sequoia::music::Target::Playlist(name) if !self.data.playlists.contains_key(name))
                            {
                                self.notify(sequoia::i18n::t("text.playlist_already_deleted"));
                                continue;
                            }
                            let count = items.len();
                            let at = match &target {
                                sequoia::music::Target::Queue => self.queue.items.len(),
                                sequoia::music::Target::Playlist(name) => {
                                    self.data.playlists.get(name).map_or(0, Vec::len)
                                }
                            };
                            self.music_drop(
                                target,
                                at,
                                &sequoia::music::Drag {
                                    items,
                                    source: None,
                                    indices: vec![],
                                },
                                true,
                            );
                            self.notify(sequoia::i18n::format("text.tracks_added_count", &[("count", format!("{}", count))]));
                        }
                        Err(error) => self.error = Some(error.to_string()),
                    }
                }
                Job::Scan(result, catalog) => {
                    self.scanning = false;
                    match result {
                        Ok(result) => {
                            let count = result.items.len();
                            if catalog {
                                self.data.library = result.items;
                                self.save();
                            } else {
                                self.queue_playlist = None;
                                self.queue.replace(result.items, 0);
                                self.play_queue();
                            }
                            self.notify(sequoia::i18n::format("text.files_found_count_folders_skipped", &[("count", format!("{}", count)), ("arg0", format!("{}", result.warnings.len()))]));
                        }
                        Err(e) => self.error = Some(e.to_string()),
                    }
                }
                Job::Cover(path, bytes) => {
                    if self.current.as_ref().is_some_and(|m| m.path == path) {
                        self.cover = bytes.and_then(|b| texture(ctx, &b));
                    }
                }
                Job::Search(result) => {
                    if let Some(editor) = &mut self.editor {
                        editor.searching = false;
                        match result {
                            Ok(items) => {
                                if items.is_empty() {
                                    self.notify(sequoia::i18n::t("text.musicbrainz_no_matches_found_change_the_title_or_artist"));
                                } else {
                                    editor.results = items;
                                }
                            }
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                }
                Job::Artwork(result) => {
                    if let Some(editor) = &mut self.editor {
                        editor.searching = false;
                        match result {
                            Ok(bytes) => editor.cover = Some(bytes),
                            Err(e) => self.error = Some(e.to_string()),
                        }
                    }
                }
                Job::BatchProgress(n) => {
                    if let Some(editor) = &mut self.batch_editor {
                        editor.done = n;
                    }
                }
                Job::BatchSaved(result) => {
                    for media in &result.saved {
                        self.update_track_metadata(media);
                    }
                    self.save();
                    if let Some(editor) = &mut self.batch_editor {
                        editor.saving = false;
                        editor.result = Some(result);
                    }
                }
                Job::Saved(result) => match result {
                    Ok(media) => {
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
                        self.editor = None;
                        self.save();
                        self.notify(sequoia::i18n::t("text.tags_written_the_original_was_saved_in_tag_backups"));
                    }
                    Err(e) => {
                        if let Some(editor) = &mut self.editor {
                            editor.saving = false;
                        }
                        self.error = Some(e.to_string());
                    }
                },
            }
        }
    }
    fn hotkeys(&mut self, ctx: &egui::Context) {
        let editing = ctx.wants_keyboard_input()
            || egui::Popup::is_any_open(ctx)
            || self.show_help
            || self.error.is_some()
            || self.show_settings
            || self.editor.is_some()
            || self.batch_editor.is_some()
            || self.show_console
            || self.show_new_playlist
            || self.show_link;
        if ctx.input(|i| i.key_pressed(Key::Escape)) {
            if self.fullscreen {
                self.fullscreen = false;
                ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(false));
            }
            self.show_settings = false;
            self.show_console = false;
            self.show_help = false;
            if self
                .editor
                .as_ref()
                .is_none_or(|e| !e.saving && !e.searching)
            {
                self.editor = None;
            }
            self.show_new_playlist = false;
            self.show_link = false;
        }
        if editing {
            return;
        }
        let has_selection = if self.mode == Mode::Music {
            !self.music_rows.is_empty()
        } else {
            self.queue_open && !self.selected_queue.is_empty()
        };
        if has_selection && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, Key::Delete)) {
            if self.mode == Mode::Music {
                self.remove_music_rows();
            } else {
                self.remove_queue_rows();
            }
        }
        // Ask mpv for active bindings so Lua sections and user overrides keep their priority.
        let keys = ctx.input(|i| {
            i.events.iter().filter_map(|event| match event {
                egui::Event::Key { key, modifiers, pressed: true, .. } if *key != Key::Escape =>
                    Some((*key, *modifiers)),
                _ => None,
            }).collect::<Vec<_>>()
        });
        if self.engine.is_some() && !keys.is_empty() {
            let bindings = self.engine.as_ref()
                .and_then(|engine| engine.request(json!(["get_property", "input-bindings"])).ok())
                .unwrap_or(Value::Null);
            for (key, modifiers) in keys {
                let name = sequoia::playback::input::key_name(key, modifiers);
                if sequoia::playback::input::has_binding(&bindings, &name)
                    || !matches!(key, Key::Space | Key::ArrowLeft | Key::ArrowRight
                        | Key::ArrowUp | Key::ArrowDown | Key::O | Key::S | Key::F | Key::M)
                {
                    self.command(json!(["keypress", name]));
                    ctx.input_mut(|i| {
                        i.consume_key(modifiers, key);
                    });
                }
            }
        }
        let input = ctx.input(|i| {
            (
                i.key_pressed(Key::Space),
                i.key_pressed(Key::ArrowLeft),
                i.key_pressed(Key::ArrowRight),
                i.key_pressed(Key::ArrowUp),
                i.key_pressed(Key::ArrowDown),
                i.modifiers.shift,
                i.modifiers.ctrl,
                i.key_pressed(Key::O),
                i.key_pressed(Key::S),
                i.key_pressed(Key::F),
                i.key_pressed(Key::M),
            )
        });
        if input.0 {
            self.toggle_pause();
        }
        if input.1 || input.2 || input.3 || input.4 {
            if input.5 {
                if self.mode == Mode::Video && self.current.as_ref().is_some_and(|m| !m.audio) {
                    self.command(json!([if input.1 || input.4 {
                        "frame-back-step"
                    } else {
                        "frame-step"
                    }]));
                }
            } else {
                let sign = if input.1 || input.4 { -1. } else { 1. };
                self.command(json!([
                    "seek",
                    sign * self.data.settings.seek_seconds,
                    "relative+exact"
                ]));
            }
        }
        if input.6 && input.7 {
            if input.5 {
                self.open_folder(false);
            } else {
                self.open_files();
            }
        } else if input.8 {
            self.screenshot();
        }
        if input.9 {
            self.fullscreen = !self.fullscreen;
            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(self.fullscreen));
        }
        if input.10 {
            self.command(json!(["cycle", "mute"]));
        }
    }
    fn header(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("title")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin {
                        left: 8,
                        ..Default::default()
                    }),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(0, 3))
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing.x = 8.;
                            ui.spacing_mut().button_padding.x = 10.;
                            ui.horizontal(|ui| {
                                if tool_label(ui, Icon::Video, sequoia::i18n::t("text.video"), self.mode == Mode::Video)
                                    .clicked()
                                {
                                    self.mode = Mode::Video;
                                }
                                if tool_label(ui, Icon::Music, sequoia::i18n::t("text.music"), self.mode == Mode::Music)
                                    .clicked()
                                {
                                    self.mode = Mode::Music;
                                }
                            });
                        });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        window_frame::caption_buttons(
                            ui,
                            if self.can_hide_to_tray() {
                                sequoia::i18n::t("text.hide_to_tray_audio_continues")
                            } else {
                                sequoia::i18n::t("text.close")
                            },
                        );
                        if icon(ui, Icon::Settings, sequoia::i18n::t("settings.title")).clicked() {
                            self.open_settings();
                        }
                        if icon(ui, Icon::Keyboard, sequoia::i18n::t("text.keyboard_shortcuts")).clicked() {
                            self.show_help = !self.show_help;
                        }
                        let blank = ui.allocate_response(
                            Vec2::new(ui.available_width(), 32.),
                            Sense::click_and_drag(),
                        );
                        if blank.drag_started() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::StartDrag);
                        }
                        if blank.double_clicked() {
                            ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(
                                !ctx.input(|i| i.viewport().maximized.unwrap_or(false)),
                            ));
                        }
                    });
                });
            });
        egui::TopBottomPanel::top("toolbar")
            .frame(
                egui::Frame::new()
                    .fill(BG)
                    .inner_margin(egui::Margin::symmetric(8, 4)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let open = tool_label(ui, Icon::Folder, sequoia::i18n::t("text.open"), false)
                        .on_hover_text(sequoia::i18n::t("text.open_a_file_folder_or_url"));
                    egui::Popup::menu(&open).show(|ui| {
                        if tool_label(ui, Icon::Import, sequoia::i18n::t("text.open_url"), false).clicked() {
                            self.show_link = true;
                            ui.close();
                        }
                        if tool_label(ui, Icon::FileOpen, sequoia::i18n::t("text.open_file"), false)
                            .on_hover_text("Ctrl+O")
                            .clicked()
                        {
                            ui.close();
                            self.open_files();
                        }
                        if tool_label(ui, Icon::Folder, sequoia::i18n::t("text.open_folder"), false)
                            .on_hover_text("Ctrl+Shift+O")
                            .clicked()
                        {
                            ui.close();
                            self.open_folder(false);
                        }
                    });
                    if self.mode == Mode::Music {
                        ui.checkbox(&mut self.data.settings.recursive, sequoia::i18n::t("text.include_subfolders"))
                            .on_hover_text(
                                sequoia::i18n::t("text.for_ordinary_folders_the_music_library_always_includes_all_subfolders"),
                            );
                        if tool_label(ui, Icon::Import, "M3U", false)
                            .on_hover_text(sequoia::i18n::t("text.import_playlist"))
                            .clicked()
                        {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter(sequoia::i18n::t("text.playlist"), &["m3u", "m3u8"])
                                .pick_file()
                            {
                                match library::import_m3u(&path) {
                                    Ok(items) => {
                                        let target = self.music_target.clone();
                                        let at = match &target {
                                            sequoia::music::Target::Queue => self.queue.items.len(),
                                            sequoia::music::Target::Playlist(name) => {
                                                self.data.playlists.get(name).map_or(0, Vec::len)
                                            }
                                        };
                                        self.music_drop(
                                            target,
                                            at,
                                            &sequoia::music::Drag {
                                                items,
                                                source: None,
                                                indices: vec![],
                                            },
                                            true,
                                        );
                                    }
                                    Err(e) => self.error = Some(e.to_string()),
                                }
                            }
                        }
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.mode != Mode::Music
                            && tool_label(ui, Icon::Queue, sequoia::i18n::t("text.queue"), self.queue_open)
                            .clicked()
                        {
                            self.queue_open = !self.queue_open;
                        }
                        if icon(ui, Icon::Terminal, sequoia::i18n::t("text.mpv_console")).clicked() {
                            self.show_console = true;
                        }
                        if !self.osd_status.is_empty() {
                            let width = (ui.available_width() - if self.scanning { 210. } else { 0. }).clamp(0., 400.);
                            ui.add_sized([width, 22.], egui::Label::new(
                                RichText::new(self.osd_status.split_whitespace().collect::<Vec<_>>().join(" "))
                                    .small().color(ACCENT),
                            ).truncate()).on_hover_text(&self.osd_status);
                        }
                        if self.scanning {
                            if icon(ui, Icon::Close, sequoia::i18n::t("text.cancel_scan")).clicked()
                            {
                                self.cancel.store(true, Ordering::Relaxed);
                            }
                            ui.spinner();
                            ui.label(
                                RichText::new(sequoia::i18n::format("text.scanning_files", &[("arg0", format!("{}", self.scanned))]))
                                    .small()
                                    .color(MUTED),
                            );
                        } else if self.osd_status.is_empty() && Instant::now() < self.notice_until {
                            ui.add(
                                egui::Label::new(RichText::new(&self.notice).small().color(ACCENT))
                                    .truncate(),
                            )
                                .on_hover_text(&self.notice);
                        }
                    });
                });
            });
    }
    fn transport(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("transport")
            .frame(
                egui::Frame::new()
                    .fill(PANEL)
                    .inner_margin(egui::Margin::symmetric(10, 5)),
            )
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(model::time(self.position))
                            .monospace()
                            .color(ACCENT),
                    );
                    let mut position = self.position;
                    let slider = ui
                        .scope(|ui| {
                            ui.spacing_mut().slider_width = (ui.available_width() - 62.).max(100.);
                            ui.add(
                                egui::Slider::new(&mut position, 0.0..=self.duration.max(0.01))
                                    .show_value(false),
                            )
                        })
                        .inner;
                    if slider.drag_stopped() || (slider.changed() && !slider.dragged()) {
                        self.command(json!(["seek", position, "absolute+exact"]));
                    }
                    ui.label(
                        RichText::new(model::time(self.duration))
                            .monospace()
                            .color(MUTED),
                    );
                });
                ui.horizontal(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new((ui.available_width() * 0.27).min(300.), 36.),
                        Layout::top_down(Align::Min),
                        |ui| {
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        self.current
                                            .as_ref()
                                            .map(|m| m.title.as_str())
                                            .unwrap_or(sequoia::i18n::t("text.nothing_playing")),
                                    )
                                        .strong(),
                                )
                                    .truncate(),
                            );
                            ui.add(
                                egui::Label::new(
                                    RichText::new(
                                        self.current
                                            .as_ref()
                                            .map(|m| m.subtitle())
                                            .filter(|s| !s.is_empty())
                                            .unwrap_or_else(|| {
                                                sequoia::i18n::t("text.video_gif_and_music_in_one_place").into()
                                            }),
                                    )
                                        .small()
                                        .color(MUTED),
                                )
                                    .truncate(),
                            );
                        },
                    );
                    if icon_button(
                        ui,
                        Icon::Shuffle,
                        if self.queue.shuffle {
                            sequoia::i18n::t("text.shuffle_enabled")
                        } else {
                            sequoia::i18n::t("text.enable_shuffle")
                        },
                        false,
                        if self.queue.shuffle { ACCENT } else { INK },
                    )
                        .clicked()
                    {
                        self.queue.shuffle = !self.queue.shuffle;
                    }
                    if icon(ui, Icon::Previous, sequoia::i18n::t("text.previous_file")).clicked() {
                        if let Some(m) = self.queue.previous() {
                            self.play(m);
                        }
                    }
                    if icon_button(
                        ui,
                        if self.paused || self.current.is_none() {
                            Icon::Play
                        } else {
                            Icon::Pause
                        },
                        sequoia::i18n::t("text.play_pause_space"),
                        true,
                        BG,
                    )
                        .clicked()
                    {
                        self.toggle_pause();
                    }
                    if icon(ui, Icon::Next, sequoia::i18n::t("text.next_file")).clicked() {
                        self.next(false);
                    }
                    if icon(ui, Icon::Stop, sequoia::i18n::t("text.stop")).clicked() {
                        self.stop();
                    }
                    if self.mode == Mode::Video {
                        ui.add_enabled_ui(self.current.as_ref().is_some_and(|m| !m.audio), |ui| {
                            if icon(ui, Icon::FrameBackward, sequoia::i18n::t("text.previous_frame_shift_left")).clicked()
                            {
                                self.command(json!(["frame-back-step"]));
                            }
                            if icon(ui, Icon::FrameForward, sequoia::i18n::t("text.next_frame_shift_right")).clicked()
                            {
                                self.command(json!(["frame-step"]));
                            }
                        });
                    }
                    if icon_button(
                        ui,
                        match self.queue.repeat {
                            Repeat::None => Icon::RepeatOff,
                            Repeat::One => Icon::RepeatOne,
                            Repeat::All => Icon::RepeatAll,
                        },
                        sequoia::i18n::t("text.repeat_off_one_all"),
                        false,
                        if self.queue.repeat != Repeat::None {
                            ACCENT
                        } else {
                            INK
                        },
                    )
                        .clicked()
                    {
                        self.queue.repeat = match self.queue.repeat {
                            Repeat::None => Repeat::One,
                            Repeat::One => Repeat::All,
                            Repeat::All => Repeat::None,
                        };
                    }
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if icon(
                            ui,
                            if self.fullscreen {
                                Icon::Collapse
                            } else {
                                Icon::Expand
                            },
                            sequoia::i18n::t("text.fullscreen_f"),
                        )
                            .clicked()
                        {
                            self.fullscreen = !self.fullscreen;
                            ctx.send_viewport_cmd(egui::ViewportCommand::Fullscreen(
                                self.fullscreen,
                            ));
                        }
                        let old = self.volume;
                        ui.add_sized(
                            [100., 24.],
                            egui::Slider::new(&mut self.volume, 0.0..=150.).show_value(false),
                        );
                        if old != self.volume {
                            self.command(json!(["set_property", "volume", self.volume]));
                            self.data.settings.volume = self.volume;
                        }
                        ui.label(
                            RichText::new(format!("{}%", self.volume as i32))
                                .small()
                                .color(MUTED),
                        );
                        if icon(
                            ui,
                            if self.mute || self.volume == 0. {
                                Icon::VolumeMuted
                            } else if self.volume < 50. {
                                Icon::VolumeLow
                            } else {
                                Icon::VolumeHigh
                            },
                            sequoia::i18n::t("text.mute_unmute_m"),
                        )
                            .clicked()
                        {
                            self.command(json!(["cycle", "mute"]));
                        }
                    });
                });
            });
    }
    fn queue_panel(&mut self, ctx: &egui::Context) {
        if !self.queue_open || self.mode == Mode::Music {
            return;
        }
        egui::SidePanel::right("queue")
            .resizable(true)
            .default_width(240.)
            .width_range(210.0..=420.0)
            .frame(egui::Frame::new().fill(PANEL).inner_margin(8))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(sequoia::i18n::t("text.queue")).strong());
                    ui.label(RichText::new(self.queue.items.len().to_string()).color(MUTED));
                });
                ui.add_space(3.);
                ui.horizontal_wrapped(|ui| {
                    if icon(ui, Icon::Save, sequoia::i18n::t("text.save_queue_as_playlist")).clicked()
                    {
                        self.playlist_seed = self.queue.items.clone();
                        self.show_new_playlist = true;
                    }
                    if icon(ui, Icon::Export, sequoia::i18n::t("text.export_m3u")).clicked() {
                        if let Some(p) = rfd::FileDialog::new()
                            .set_file_name("Sequoia.m3u8")
                            .save_file()
                        {
                            if let Err(e) = library::export_m3u(&p, &self.queue.items) {
                                self.error = Some(e.to_string());
                            }
                        }
                    }
                    if icon(ui, Icon::Trash, sequoia::i18n::t("text.clear_queue")).clicked() {
                        self.stop();
                        self.queue.replace(vec![], 0);
                        self.selected_queue.clear();
                    }
                });
                ui.separator();
                let available = ui.available_height() - 43.;
                let mut play = None;
                let mut pending_move = None;
                let mut row_rects = Vec::new();
                let area = egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .scroll_source(egui::scroll_area::ScrollSource {
                        drag: false,
                        ..Default::default()
                    })
                    .max_height(available)
                    .show(ui, |ui| {
                        if self.queue.items.is_empty() {
                            ui.add_space(24.);
                            ui.label(RichText::new(sequoia::i18n::t("text.the_queue_is_empty")).color(MUTED));
                            ui.label(
                                RichText::new(
                                    sequoia::i18n::t("text.open_a_file_or_folder_or_add_music_from_the_library"),
                                )
                                    .small()
                                    .color(MUTED),
                            );
                        }
                        for (i, item) in self.queue.items.iter().enumerate() {
                            let active = self.queue.index == Some(i);
                            let response = media_row(
                                ui,
                                &format!("{:02}   {}", i + 1, item.title),
                                &if item.artist.is_empty() {
                                    model::time(item.duration)
                                } else {
                                    item.artist.clone()
                                },
                                ui.available_width(),
                                active || self.selected_queue.contains(&i),
                            ).interact(Sense::click_and_drag());
                            row_rects.push((i, response.rect));
                            if let Some(movement) = selection::queue_drag(
                                ui, &response, i, &mut self.selected_queue,
                            ) {
                                pending_move = Some(movement);
                            }
                            if response.clicked() {
                                selection::click(
                                    &mut self.selected_queue, i, ui.input(|i| i.modifiers),
                                );
                            }
                            if response.double_clicked() {
                                play = Some(i);
                            }
                            response.on_hover_text(item.path.display().to_string());
                        }
                    });
                selection::marquee(
                    ui, "video-queue-selection", area.inner_rect, &row_rects,
                    &mut self.selected_queue, true,
                );
                if pending_move.is_none() {
                    let response = ui.interact(
                        area.inner_rect, ui.id().with("video-queue-append-drop"), Sense::hover(),
                    );
                    if let Some(payload) = response.dnd_release_payload::<selection::QueueDrag>() {
                        pending_move = Some((self.queue.items.len(), payload.rows.clone()));
                    }
                }
                if let Some((before, rows)) = pending_move {
                    let selected = self.queue.move_rows(&rows, before);
                    self.sync_music_queue();
                    self.selected_queue = selected.iter().copied().collect();
                    if self.music_target == sequoia::music::Target::Queue {
                        self.music_rows = selected.into_iter().collect();
                    }
                }
                ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        if icon(ui, Icon::MoveUp, sequoia::i18n::t("text.move_up_in_queue")).clicked() {
                            if let Some(i) = self.selected_queue.first().copied() {
                                if i > 0 {
                                    let rows: Vec<_> = self.selected_queue.iter().copied().collect();
                                    self.selected_queue = self.queue.move_rows(&rows, i - 1).into_iter().collect();
                                }
                            }
                        }
                        if icon(ui, Icon::MoveDown, sequoia::i18n::t("text.move_down_in_queue")).clicked() {
                            if let Some(i) = self.selected_queue.last().copied() {
                                if i + 1 < self.queue.items.len() {
                                    let rows: Vec<_> = self.selected_queue.iter().copied().collect();
                                    self.selected_queue = self.queue.move_rows(&rows, i + 2).into_iter().collect();
                                }
                            }
                        }
                        if icon(ui, Icon::Close, sequoia::i18n::t("text.remove_selected_from_queue_delete")).clicked() {
                            self.remove_queue_rows();
                        }
                        if icon(ui, Icon::Play, sequoia::i18n::t("text.play_selected_file")).clicked()
                        {
                            play = self.selected_queue.first().copied();
                        }
                    });
                });
                if let Some(i) = play {
                    self.queue.select(i);
                    self.play_queue();
                }
            });
    }
    fn video(&mut self, ui: &mut egui::Ui) -> egui::Rect {
        ui.horizontal(|ui| {
            ui.label(RichText::new(sequoia::i18n::t("text.video")).size(14.).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if icon(ui, Icon::Camera, sequoia::i18n::t("text.save_screenshot_s")).clicked() {
                    self.screenshot();
                }
            });
        });
        ui.add_space(4.);
        let height = (ui.available_height() - 44.).max(100.);
        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(ui.available_width(), height), Sense::click());
        ui.painter()
            .rect_filled(rect, 8., Color32::from_rgb(9, 12, 13));
        if self.current.as_ref().is_some_and(|m| !m.audio)
            && !self.show_settings && !self.show_console && !self.show_help
            && self.error.is_none() && self.editor.is_none() && self.batch_editor.is_none()
            && !self.show_link && !self.show_new_playlist && !egui::Popup::is_any_open(ui.ctx())
        {
            let modifiers = ui.input(|i| i.modifiers);
            for (button, name) in [
                (egui::PointerButton::Primary, "MBTN_LEFT"),
                (egui::PointerButton::Secondary, "MBTN_RIGHT"),
                (egui::PointerButton::Middle, "MBTN_MID"),
                (egui::PointerButton::Extra1, "MBTN_BACK"),
                (egui::PointerButton::Extra2, "MBTN_FORWARD"),
            ] {
                if response.clicked_by(button) {
                    self.command(json!(["keypress", sequoia::playback::input::modified(name, modifiers, true)]));
                }
            }
            if response.hovered() {
                for event in ui.input(|i| i.events.clone()) {
                    if let egui::Event::MouseWheel { delta, modifiers, .. } = event {
                        if delta.y != 0. {
                            let key = if delta.y > 0. { "WHEEL_UP" } else { "WHEEL_DOWN" };
                            self.command(json!(["keypress", sequoia::playback::input::modified(key, modifiers, true)]));
                        }
                    }
                }
            }
        }
        if self.current.as_ref().is_none_or(|m| m.audio) {
            let center = rect.center();
            ui.painter().text(
                center - Vec2::new(0., 36.),
                egui::Align2::CENTER_CENTER,
                sequoia::i18n::t("text.open_a_video_or_gif"),
                FontId::proportional(22.),
                INK,
            );
            ui.painter().text(
                center + Vec2::new(0., 10.),
                egui::Align2::CENTER_CENTER,
                sequoia::i18n::t("text.open_a_file_or_url_or_drag_it_into_the_window"),
                FontId::proportional(13.),
                MUTED,
            );
            ui.painter().text(
                center + Vec2::new(0., 44.),
                egui::Align2::CENTER_CENTER,
                "Ctrl+O",
                FontId::proportional(13.),
                MUTED,
            );
            if response.clicked() {
                self.open_files();
            }
        }
        ui.add_space(5.);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 26.),
            Layout::left_to_right(Align::Center).with_main_wrap(true),
            |ui| {
                ui.spacing_mut().interact_size.y = 26.;
                let chapters_width = if self.chapters.is_empty() { 0. } else { 88. };
                let track_width =
                    ((ui.available_width() - 270. - chapters_width) / 3.).clamp(70., 150.);
                for (index, (kind, label)) in
                    [("video", sequoia::i18n::t("text.video")), ("audio", sequoia::i18n::t("text.audio")), ("sub", sequoia::i18n::t("text.subtitles"))]
                        .into_iter()
                        .enumerate()
                {
                    let tracks: Vec<_> = self
                        .tracks
                        .iter()
                        .filter(|t| t["type"] == kind)
                        .cloned()
                        .collect();
                    let selected = tracks
                        .iter()
                        .find(|t| t["selected"] == true)
                        .map(track_name)
                        .unwrap_or_else(|| sequoia::i18n::t("text.off").into());
                    let combo = egui::ComboBox::from_id_salt(kind)
                        .selected_text(format!("{label}: {selected}"))
                        .width(track_width)
                        .truncate()
                        .show_ui(ui, |ui| {
                            if ui.selectable_label(selected == sequoia::i18n::t("text.off"), sequoia::i18n::t("text.off")).clicked() {
                                self.command(json!([
                                    "set_property",
                                    match kind {
                                        "video" => "vid",
                                        "audio" => "aid",
                                        _ => "sid",
                                    },
                                    "no"
                                ]));
                            }
                            for track in &tracks {
                                if ui
                                    .selectable_label(track["selected"] == true, track_name(track))
                                    .clicked()
                                {
                                    self.command(json!([
                                        "set_property",
                                        match kind {
                                            "video" => "vid",
                                            "audio" => "aid",
                                            _ => "sid",
                                        },
                                        track["id"]
                                    ]));
                                }
                            }
                        });
                    self.track_popups[index] = Some(combo.response.id.with("popup"));
                }
                if icon(ui, Icon::SubtitleAdd, sequoia::i18n::t("text.add_external_subtitles")).clicked()
                {
                    if let Some(path) = rfd::FileDialog::new()
                        .add_filter(sequoia::i18n::t("text.subtitles"), &["srt", "ass", "ssa", "vtt", "sub", "idx"])
                        .pick_file()
                    {
                        self.request(json!(["sub-add", path, "select"]));
                    }
                }
                egui::ComboBox::from_id_salt("speed")
                    .selected_text(sequoia::i18n::format("text.speed", &[("arg0", format!("{}", self.speed))]))
                    .width(110.)
                    .show_ui(ui, |ui| {
                        for speed in [0.25, 0.5, 0.75, 1., 1.25, 1.5, 2., 3.] {
                            if ui
                                .selectable_label(self.speed == speed, format!("{speed}×"))
                                .clicked()
                            {
                                self.command(json!(["set_property", "speed", speed]));
                            }
                        }
                    });
                if !self.chapters.is_empty() {
                    egui::ComboBox::from_id_salt("chapters")
                        .selected_text(sequoia::i18n::t("text.chapters"))
                        .width(80.)
                        .show_ui(ui, |ui| {
                            for chapter in &self.chapters {
                                if ui
                                    .button(chapter["title"].as_str().unwrap_or(sequoia::i18n::t("text.chapter")))
                                    .clicked()
                                {
                                    self.command(json!([
                                        "seek",
                                        chapter["time"],
                                        "absolute+exact"
                                    ]));
                                }
                            }
                        });
                }
            },
        );
        rect
    }
    fn dialogs(&mut self, ctx: &egui::Context) {
        self.link_dialog(ctx);
        self.settings_dialog(ctx);
        if self.show_console {
            let mut open = true;
            egui::Window::new(sequoia::i18n::t("text.mpv_console"))
                .open(&mut open)
                .default_width(650.)
                .show(ctx, |ui| {
                    ui.label(sequoia::i18n::t("text.any_json_ipc_command_from_the_mpv_documentation_for_example"));
                    ui.label(
                        RichText::new(r#"["set_property", "sub-delay", 0.5]"#)
                            .monospace()
                            .color(ACCENT),
                    );
                    ui.add(
                        egui::TextEdit::multiline(&mut self.console_input)
                            .code_editor()
                            .desired_width(f32::INFINITY)
                            .desired_rows(3),
                    );
                    if ui.button(sequoia::i18n::t("text.run")).clicked() {
                        match serde_json::from_str::<Value>(&self.console_input) {
                            Ok(command) if command.is_array() => {
                                if let Some(value) = self.request(command) {
                                    self.console_output =
                                        serde_json::to_string_pretty(&value).unwrap_or_default();
                                }
                            }
                            _ => {
                                self.console_output =
                                    sequoia::i18n::t("text.enter_a_valid_json_array_command_name_arguments")
                                        .into()
                            }
                        }
                    }
                    egui::ScrollArea::vertical()
                        .max_height(300.)
                        .show(ui, |ui| {
                            ui.add(
                                egui::TextEdit::multiline(&mut self.console_output)
                                    .code_editor()
                                    .desired_width(f32::INFINITY)
                                    .desired_rows(9),
                            );
                        });
                });
            self.show_console = open;
        }
        if self.show_help {
            egui::Window::new(sequoia::i18n::t("text.keyboard_and_controls"))
                .open(&mut self.show_help)
                .default_width(420.)
                .show(ctx, |ui| {
                    for text in [
                        sequoia::i18n::t("text.space_play_pause"),
                        sequoia::i18n::t("text.right_click_video_pause_resume"),
                        sequoia::i18n::t("text.mouse_wheel_over_video_volume_p_pause_speed"),
                        sequoia::i18n::t("text.bindings_mpv_input_conf_in_the_data_folder_after_reconnecting"),
                        sequoia::i18n::t("text.arrow_keys_seek_by_the_configured_number_of_seconds"),
                        sequoia::i18n::t("text.shift_previous_next_frame_in_video_mode"),
                        sequoia::i18n::t("text.s_screenshot_with_subtitles_in_the_chosen_folder"),
                        sequoia::i18n::t("text.f_fullscreen_escape_leave_fullscreen"),
                        sequoia::i18n::t("text.m_mute_unmute"),
                        sequoia::i18n::t("text.ctrl_o_open_files"),
                        sequoia::i18n::t("text.ctrl_shift_o_open_folder"),
                        sequoia::i18n::t("text.double_click_play_a_queue_or_library_item"),
                        sequoia::i18n::t("text.delete_remove_selected_items_from_the_queue_or_playlist"),
                        sequoia::i18n::t("text.box_selection_drag_from_empty_queue_space"),
                        sequoia::i18n::t("text.in_music_start_a_selection_box_from_the_strip_to_the_left_of_rows"),
                        sequoia::i18n::t("text.ctrl_add_to_selection_shift_select_a_range"),
                        sequoia::i18n::t("text.drag_files_from_your_file_manager_into_the_window"),
                    ] {
                        ui.label(text);
                    }
                });
        }
        if self.show_new_playlist {
            let mut open = true;
            egui::Window::new(sequoia::i18n::t("text.save_internal_playlist"))
                .open(&mut open)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(sequoia::i18n::format("text.tracks", &[("arg0", format!("{}", self.playlist_seed.len()))]));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.playlist_name)
                            .hint_text(sequoia::i18n::t("text.playlist_name")),
                    );
                    let name = self.playlist_name.trim().to_string();
                    if self.data.playlists.contains_key(&name) {
                        ui.label(
                            RichText::new(sequoia::i18n::t("text.this_name_is_already_in_use_choose_another")).color(ACCENT),
                        );
                    }
                    if ui
                        .add_enabled(
                            !name.is_empty() && !self.data.playlists.contains_key(&name),
                            egui::Button::new(sequoia::i18n::t("text.create_playlist")),
                        )
                        .clicked()
                    {
                        self.data
                            .playlists
                            .insert(name.clone(), self.playlist_seed.clone());
                        self.open_music_playlist(&name);
                        self.music_rows.clear();
                        self.save();
                        self.show_new_playlist = false;
                        self.playlist_name.clear();
                    }
                });
            if !open {
                self.show_new_playlist = false;
            }
        }
        self.metadata_dialog(ctx);
        self.batch_tags_dialog(ctx);
        if let Some(message) = self.error.clone() {
            let mut open = true;
            egui::Window::new(sequoia::i18n::t("text.message"))
                .id(egui::Id::new("error"))
                .open(&mut open)
                .default_width(530.)
                .collapsible(false)
                .show(ctx, |ui| {
                    ui.label(&message);
                    if ui.button(sequoia::i18n::t("text.ok")).clicked() {
                        self.error = None;
                    }
                    if self.engine.is_none() && ui.button(sequoia::i18n::t("text.open_mpv_settings")).clicked()
                    {
                        self.error = None;
                        self.open_settings();
                    }
                });
            if !open {
                self.error = None;
            }
        }
    }
    fn metadata_dialog(&mut self, ctx: &egui::Context) {
        let Some(mut editor) = self.editor.take() else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new(sequoia::i18n::t("text.tags_and_cover")).open(&mut open).default_width(700.).collapsible(false).show(ctx, |ui| {
            ui.label(RichText::new(editor.media.path.display().to_string()).small().color(MUTED));
            egui::ScrollArea::vertical().max_height(580.).show(ui, |ui| {
                ui.add_enabled_ui(!editor.saving, |ui| {
                    egui::Grid::new("tag-fields").num_columns(2).spacing([14., 10.]).show(ui, |ui| {
                        for (label, field) in [(sequoia::i18n::t("text.title"), &mut editor.media.title), (sequoia::i18n::t("text.artist"), &mut editor.media.artist), (sequoia::i18n::t("text.album"), &mut editor.media.album), (sequoia::i18n::t("text.album_artist"), &mut editor.media.album_artist), (sequoia::i18n::t("text.grouping"), &mut editor.media.grouping), (sequoia::i18n::t("text.genre"), &mut editor.media.genre)] {
                            ui.label(label);
                            ui.add(egui::TextEdit::singleline(field).desired_width(440.));
                            ui.end_row();
                        }
                        ui.label(sequoia::i18n::t("text.year"));
                        ui.add(egui::DragValue::new(&mut editor.media.year).range(0..=9999));
                        ui.end_row();
                        ui.label(sequoia::i18n::t("text.disc_number"));
                        ui.add(egui::DragValue::new(&mut editor.media.disc).range(0..=9999));
                        ui.end_row();
                        ui.label(sequoia::i18n::t("text.format"));
                        ui.label(library::extension(&editor.media.path).to_uppercase());
                        ui.end_row();
                        ui.label(sequoia::i18n::t("text.track_number"));
                        ui.add(egui::DragValue::new(&mut editor.media.track).range(0..=9999));
                        ui.end_row();
                    });
                    ui.horizontal(|ui| {
                        if let Some(bytes) = &editor.cover { if let Some(texture) = texture(ctx, bytes) { ui.add(egui::Image::new(&texture).fit_to_exact_size(Vec2::splat(90.))); } }
                        ui.vertical(|ui| {
                            if tool_label(ui, Icon::Cover, sequoia::i18n::t("text.cover_from_file"), false).clicked() {
                                if let Some(path) = rfd::FileDialog::new().add_filter(sequoia::i18n::t("text.cover"), &["jpg", "jpeg", "png"]).pick_file() {
                                    match std::fs::read(path) {
                                        Ok(bytes) => editor.cover = Some(bytes),
                                        Err(e) => self.error = Some(e.to_string())
                                    }
                                }
                            }
                            ui.label(RichText::new(sequoia::i18n::t("text.the_cover_will_be_embedded_in_the_file_tags")).small().color(MUTED));
                        });
                    });
                    ui.separator();
                    ui.heading(sequoia::i18n::t("text.search_musicbrainz"));
                    ui.label(RichText::new(sequoia::i18n::t("text.the_query_sends_the_title_and_artist_select_a_match_and_check_it_before_sav")).small().color(MUTED));
                    ui.horizontal(|ui| {
                        ui.add(egui::TextEdit::singleline(&mut editor.query_title).hint_text(sequoia::i18n::t("text.title")).desired_width(250.));
                        ui.add(egui::TextEdit::singleline(&mut editor.query_artist).hint_text(sequoia::i18n::t("text.artist")).desired_width(220.));
                        if ui.add_enabled_ui(!editor.searching && !editor.query_title.trim().is_empty(), |ui| tool_label(ui, Icon::Search, sequoia::i18n::t("text.search"), false)).inner.clicked() {
                            editor.searching = true;
                            let tx = self.tx.clone();
                            let title = editor.query_title.clone();
                            let artist = editor.query_artist.clone();
                            std::thread::spawn(move || { let _ = tx.send(Job::Search(online::search(&title, &artist))); });
                        }
                    });
                    if editor.searching {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label(sequoia::i18n::t("text.fetching_data"));
                        });
                    }
                    let mut chosen = None;
                    let mut artwork = None;
                    for (i, result) in editor.results.iter().enumerate() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label(format!("{} · {} · {} ({})", result.title, result.artist, result.album, result.year));
                            if tool_label(ui, Icon::Check, sequoia::i18n::t("text.use_tags"), false).clicked() { chosen = Some(i); }
                            if ui.add_enabled_ui(result.release_id.is_some() && !editor.searching, |ui| tool_label(ui, Icon::Download, sequoia::i18n::t("text.cover"), false)).inner.clicked() { artwork = result.release_id.clone(); }
                        });
                        ui.separator();
                    }
                    if let Some(i) = chosen {
                        let result = &editor.results[i];
                        editor.media.title = result.title.clone();
                        editor.media.artist = result.artist.clone();
                        editor.media.album = result.album.clone();
                        editor.media.year = result.year;
                    }
                    if let Some(id) = artwork {
                        editor.searching = true;
                        let tx = self.tx.clone();
                        std::thread::spawn(move || { let _ = tx.send(Job::Artwork(online::cover(&id))); });
                    }
                });
            });
            ui.separator();
            ui.label(RichText::new(sequoia::i18n::t("text.writing_tags_does_not_interrupt_playback_the_original_will_be_saved_in_tag")).small().color(MUTED));
            if ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_enabled(!editor.saving && !editor.searching, egui::Button::new(RichText::new(sequoia::i18n::t("text.write_tags_to_file")).color(ACCENT)))
            }).inner.clicked() { save = true; }
            if editor.saving { ui.spinner(); }
        });
        if save {
            self.save_editor(&mut editor);
        }
        if open || editor.saving || editor.searching {
            self.editor = Some(editor);
        }
    }
    fn save_editor(&self, editor: &mut Editor) {
        editor.saving = true;
        let tx = self.tx.clone();
        let media = editor.media.clone();
        let bytes = editor.cover.clone();
        let root = self.root.clone();
        std::thread::spawn(move || {
            let _ = tx.send(Job::Saved(library::write_tags(
                &media,
                bytes.as_deref(),
                &root,
            )));
        });
    }
    fn popup_regression(&mut self, ctx: &egui::Context) {
        if !self.popup_test
            || self.smoke_started.elapsed()
            < Duration::from_millis(700 * (1 + self.smoke_step as u64))
        {
            return;
        }
        let result = (|| -> anyhow::Result<()> {
            if self.smoke_step == 0 {
                self.add_paths(vec![
                    std::env::current_dir()?.join("artifacts/fixtures/multi.mkv")
                ]);
                anyhow::ensure!(self.error.is_none(), "startup error: {:?}", self.error);
                self.request(json!(["set_property", "pause", false]));
            } else {
                let engine = self
                    .engine
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("mpv disconnected"))?;
                anyhow::ensure!(
                    engine.request(json!(["get_property", "pause"]))? == false,
                    "playback paused"
                );
                anyhow::ensure!(
                    engine.request(json!(["get_property", "idle-active"]))? == false,
                    "playback stopped"
                );
                anyhow::ensure!(
                    engine.request(json!(["get_property", "eof-reached"]))? == false,
                    "unexpected EOF"
                );
                let position = engine
                    .request(json!(["get_property", "time-pos"]))?
                    .as_f64()
                    .ok_or_else(|| anyhow::anyhow!("no position"))?;
                anyhow::ensure!(
                    position > self.playback_probe + 0.05,
                    "playback time stalled"
                );
                self.playback_probe = position;
                #[cfg(windows)]
                if let Some(host) = &self.host {
                    anyhow::ensure!(
                        unsafe {
                            windows::Win32::UI::WindowsAndMessaging::IsWindowVisible(host.hwnd)
                        }
                        .as_bool(),
                        "popup hid video HWND"
                    );
                }
            }
            match self.smoke_step {
                1..=3 => {
                    let index = self.smoke_step - 1;
                    if index > 0 {
                        let previous = self.track_popups[index - 1].unwrap();
                        anyhow::ensure!(
                            egui::Popup::is_id_open(ctx, previous),
                            "dropdown did not remain open"
                        );
                        let property = ["vid", "aid"][index - 1];
                        self.request(json!(["set_property", property, 2]));
                        egui::Popup::close_id(ctx, previous);
                    }
                    egui::Popup::open_id(
                        ctx,
                        self.track_popups[index]
                            .ok_or_else(|| anyhow::anyhow!("missing combo box"))?,
                    );
                }
                4 => {
                    let id = self.track_popups[2].unwrap();
                    anyhow::ensure!(
                        egui::Popup::is_id_open(ctx, id),
                        "subtitle dropdown not open"
                    );
                    self.request(json!(["set_property", "sid", 1]));
                    egui::Popup::close_id(ctx, id);
                }
                5 => {
                    let engine = self.engine.as_ref().unwrap();
                    for (property, expected) in [("vid", 2), ("aid", 2), ("sid", 1)] {
                        anyhow::ensure!(
                            engine.request(json!(["get_property", property]))? == expected,
                            "track selection failed: {property}"
                        );
                    }
                    std::fs::create_dir_all("artifacts")?;
                    std::fs::write("artifacts/popup-regression.txt", "PASS: real video/audio/subtitle dropdowns stay open; native video HWND stays visible; mpv remains loaded and unpaused; playback time advances across all dropdowns and track changes; selected vid=2, aid=2, sid=1\n")?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::create_dir_all("artifacts");
            let _ = std::fs::write("artifacts/popup-regression.txt", format!("FAIL: {error:#}"));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
    }
    fn smoke_test(&mut self, ctx: &egui::Context) {
        if !self.smoke
            || self.smoke_started.elapsed() < Duration::from_secs(1 + self.smoke_step as u64)
        {
            return;
        }
        let result: anyhow::Result<()> = (|| {
            match self.smoke_step {
                0 => {
                    anyhow::ensure!(self.engine.is_some(), "mpv not connected");
                    self.mode = Mode::Music;
                }
                1 => {
                    self.open_settings();
                }
                2 => {
                    self.show_settings = false;
                    self.show_console = true;
                }
                3 => {
                    self.console_output = self
                        .request(json!(["get_property", "mpv-version"]))
                        .unwrap_or(Value::Null)
                        .to_string();
                    anyhow::ensure!(self.console_output != "null", "mpv version unavailable");
                }
                4 => {
                    self.show_console = false;
                    self.show_help = true;
                }
                5 => {
                    self.show_help = false;
                    self.mode = Mode::Video;
                    self.queue_open = false;
                }
                6 => {
                    self.queue_open = true;
                    self.show_new_playlist = true;
                }
                7 => {
                    self.show_new_playlist = false;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                8 => {
                    let file = std::env::current_dir()?.join("artifacts/fixtures/test.mp4");
                    self.add_paths(vec![file]);
                }
                9 => {
                    self.request(json!(["set_property", "pause", true]));
                    self.request(json!(["frame-step"]));
                    anyhow::ensure!(self.duration > 0., "video duration not reported");
                }
                10 => {
                    self.data.settings.screenshot_folder =
                        std::env::current_dir()?.join("artifacts/screenshots");
                    self.screenshot();
                    anyhow::ensure!(
                        std::fs::read_dir(&self.data.settings.screenshot_folder)?
                            .next()
                            .is_some(),
                        "screenshot missing"
                    );
                }
                11 => {
                    let file = std::env::current_dir()?.join("artifacts/fixtures/test.gif");
                    self.add_paths(vec![file]);
                }
                12 => {
                    anyhow::ensure!(self.duration > 0., "GIF duration missing");
                    self.stop();
                    self.data.settings.music_root =
                        std::env::current_dir()?.join("artifacts/fixtures");
                    self.start_scan(self.data.settings.music_root.clone(), true);
                    self.mode = Mode::Music;
                }
                13 => {
                    anyhow::ensure!(self.data.library.len() >= 2, "music scan missing");
                    let mut items = self.data.library.clone();
                    for item in &mut items {
                        let copy = self.root.join(item.path.file_name().unwrap());
                        std::fs::copy(&item.path, &copy)?;
                        item.path = copy;
                    }
                    self.data.library = items.clone();
                    self.data
                        .playlists
                        .insert("tag regression".into(), items.clone());
                    self.queue.replace(items, 0);
                    self.queue.repeat = Repeat::All;
                    self.queue.shuffle = true;
                    self.play_queue();
                }
                14 => {
                    let media = self
                        .current
                        .clone()
                        .ok_or_else(|| anyhow::anyhow!("audio not loaded"))?;
                    self.editor(media);
                    self.request(json!(["seek", 2.0, "absolute+exact"]));
                    self.playback_probe = 2.0;
                    let mut editor = self.editor.take().unwrap();
                    editor.media.title = "Теги без прерывания".into();
                    editor.cover = Some(std::fs::read("artifacts/fixtures/cover.png")?);
                    self.save_editor(&mut editor);
                    self.editor = Some(editor);
                }
                15 => {
                    anyhow::ensure!(
                        self.editor.is_none(),
                        "tag write did not finish: {:?}",
                        self.error
                    );
                    let current = self
                        .current
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("tag write cleared current track"))?;
                    anyhow::ensure!(
                        current.title == "Теги без прерывания",
                        "current metadata stale"
                    );
                    anyhow::ensure!(
                        self.queue.index == Some(0)
                            && self.queue.shuffle
                            && self.queue.repeat == Repeat::All,
                        "queue state changed"
                    );
                    anyhow::ensure!(
                        self.queue.current() == Some(current),
                        "queue metadata stale"
                    );
                    anyhow::ensure!(
                        self.data.library[0] == *current
                            && self.data.playlists["tag regression"][0] == *current,
                        "catalog or playlist metadata stale"
                    );
                    anyhow::ensure!(self.cover.is_some(), "saved artwork missing");
                    anyhow::ensure!(
                        !self.paused
                            && self.position > self.playback_probe
                            && self.position < self.playback_probe + 2.5,
                        "tag write stopped or restarted playback"
                    );
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                16 => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(Vec2::new(1000., 700.)));
                }
                17 => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(
                        egui::UserData::default(),
                    ));
                }
                18 => {
                    self.save();
                    std::fs::create_dir_all("artifacts")?;
                    std::fs::write("artifacts/desktop-smoke.txt", "PASS: native window, mpv IPC, music view, settings, console version query, help, video view, queue toggle, playlist dialog, MP4 embedded playback, frame-step, PNG screenshot, GIF playback, music scan, MP3/FLAC playback, editor tag save preserves playback position and queue state, current/catalog/playlist metadata and artwork updated, resize, persistence\n")?;
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::create_dir_all("artifacts");
            let _ = std::fs::write("artifacts/desktop-smoke.txt", format!("FAIL: {error:#}"));
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        self.smoke_step += 1;
    }
}
impl eframe::App for Player {
    fn raw_input_hook(&mut self, _: &egui::Context, input: &mut egui::RawInput) {
        if self.music_test {
            input.events.retain(|event| {
                !matches!(
                    event,
                    egui::Event::PointerMoved(_)
                        | egui::Event::PointerGone
                        | egui::Event::PointerButton { .. }
                )
            });
            input.events.append(&mut self.test_input);
        }
    }
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.handle_events(ctx);
        if !self.initial_scan {
            self.initial_scan = true;
            let paths = sequoia::instance::launch_paths();
            if !paths.is_empty() {
                self.add_paths(paths);
            }
        }
        self.tray_events(ctx);
        match self.instance.requests() {
            Ok(requests) => {
                for paths in requests {
                    self.hidden_to_tray = false;
                    #[cfg(windows)]
                    if let Some(tray) = &self.tray {
                        let _ = tray.background(false);
                    }
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                    if !paths.is_empty() {
                        self.add_paths(paths);
                    }
                }
            }
            Err(error) => self.error = Some(error.to_string()),
        }
        #[cfg(windows)]
        self.tray_regression(ctx);
        if self.hidden_to_tray {
            ctx.request_repaint_after(Duration::from_millis(200));
            return;
        }
        self.hotkeys(ctx);
        let drops = ctx.input(|i| i.raw.dropped_files.clone());
        self.poll_link_drop();
        if !drops.is_empty() {
            let paths: Vec<_> = drops.into_iter().filter_map(|f| f.path).collect();
            if paths.iter().any(|p| sequoia::playback::web::is_url(&p.to_string_lossy()) || sequoia::playback::web::shortcut(p).is_some()) {
                self.add_paths(paths);
            } else if self.mode == Mode::Music {
                self.music_drop_files(paths);
            } else if paths.len() == 1 && paths[0].is_dir() {
                self.start_scan(paths[0].clone(), false);
            } else {
                self.add_paths(paths);
            }
        }
        self.header(ctx);
        self.transport(ctx);
        self.queue_panel(ctx);
        self.music_sidebar(ctx);
        let mut video_rect = egui::Rect::NOTHING;
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(BG).inner_margin(10))
            .show(ctx, |ui| {
                if self.mode == Mode::Video {
                    video_rect = self.video(ui);
                } else {
                    self.music(ui);
                }
            });
        self.dialogs(ctx);
        window_frame::resize_handles(ctx);
        let overlay_layers = ctx.memory(|memory| memory.areas().visible_layer_ids());
        let overlays: Vec<_> = overlay_layers
            .into_iter()
            .filter(|layer| {
                matches!(
                    layer.order,
                    egui::Order::Middle | egui::Order::Foreground | egui::Order::Tooltip
                )
            })
            .filter_map(|layer| egui::AreaState::load(ctx, layer.id).map(|area| area.rect()))
            .collect();
        #[cfg(windows)]
        if let Some(host) = &self.host {
            host.bounds(
                video_rect,
                ctx.pixels_per_point(),
                self.mode == Mode::Video && self.current.as_ref().is_some_and(|m| !m.audio),
                &overlays,
            );
        }
        for event in ctx.input(|i| i.raw.events.clone()) {
            if let egui::Event::Screenshot { image, .. } = event {
                let _ = std::fs::create_dir_all("artifacts");
                let bytes: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_array()).collect();
                let _ = image::save_buffer(
                    format!("artifacts/sequoia-{}.png", self.smoke_step),
                    &bytes,
                    image.size[0] as u32,
                    image.size[1] as u32,
                    image::ColorType::Rgba8,
                );
            }
        }
        self.smoke_test(ctx);
        self.popup_regression(ctx);
        self.music_regression(ctx);
        self.batch_tags_preview(ctx);
        self.grouping_preview(ctx);
        self.playlists_preview(ctx);
        self.web_preview(ctx);
        ctx.request_repaint_after(Duration::from_millis(80));
    }
    fn on_exit(&mut self, _: Option<&eframe::glow::Context>) {
        self.cancel_link();
        self.cancel.store(true, Ordering::Relaxed);
        self.save();
        self.engine = None;
        #[cfg(windows)]
        {
            self.tray = None;
        }
    }
}
fn path_field(ui: &mut egui::Ui, label: &str, path: &mut PathBuf, directory: bool) {
    ui.label(label);
    ui.horizontal(|ui| {
        let mut value = path.to_string_lossy().to_string();
        if ui
            .add(egui::TextEdit::singleline(&mut value).desired_width(510.))
            .changed()
        {
            *path = value.into();
        }
        if ui.button(sequoia::i18n::t("text.browse")).clicked() {
            let choice = if directory {
                rfd::FileDialog::new().pick_folder()
            } else {
                rfd::FileDialog::new()
                    .add_filter("mpv", &["exe"])
                    .pick_file()
            };
            if let Some(choice) = choice {
                *path = choice;
            }
        }
    });
}
fn track_name(track: &Value) -> String {
    let title = track["title"]
        .as_str()
        .or(track["lang"].as_str())
        .or(track["codec"].as_str())
        .unwrap_or(sequoia::i18n::t("text.track"));
    format!("{} · {title}", track["id"])
}
fn texture(ctx: &egui::Context, bytes: &[u8]) -> Option<egui::TextureHandle> {
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let size = [image.width() as usize, image.height() as usize];
    Some(ctx.load_texture(
        "cover",
        egui::ColorImage::from_rgba_unmultiplied(size, image.as_raw()),
        egui::TextureOptions::LINEAR,
    ))
}
fn reveal(path: &std::path::Path) -> anyhow::Result<()> {
    #[cfg(windows)]
    {
        sequoia::native::reveal_file(path)
    }
    #[cfg(not(windows))]
    anyhow::bail!(
        "{}", sequoia::i18n::format("text.show_in_folder_is_only_available_on_windows", &[("arg0", format!("{}", path.display()))]))
}
fn open_directory(path: &std::path::Path) {
    let _ = std::fs::create_dir_all(path);
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("explorer.exe").arg(path).spawn();
    }
}
fn media_row(
    ui: &mut egui::Ui,
    title: &str,
    subtitle: &str,
    width: f32,
    selected: bool,
) -> egui::Response {
    let (rect, response) = ui.allocate_exact_size(Vec2::new(width, 24.), Sense::click());
    response.widget_info(|| {
        egui::WidgetInfo::labeled(
            egui::WidgetType::SelectableLabel,
            ui.is_enabled(),
            format!("{title}, {subtitle}"),
        )
    });
    ui.painter().rect_filled(
        rect,
        4.,
        if selected {
            Color32::from_rgb(78, 65, 46)
        } else if response.hovered() {
            Color32::from_rgb(52, 59, 61)
        } else {
            RAISED
        },
    );
    if response.has_focus() {
        ui.painter()
            .rect_stroke(rect, 4., Stroke::new(1., ACCENT), egui::StrokeKind::Inside);
    }
    let split = if subtitle.is_empty() {
        width - 12.
    } else {
        width * 0.57
    };
    for (text, x, space, color) in [
        (title, 6., split - 12., if selected { ACCENT } else { INK }),
        (subtitle, split, width - split - 6., MUTED),
    ] {
        let mut job = egui::text::LayoutJob::simple(
            text.to_owned(),
            FontId::proportional(12.),
            color,
            space.max(5.),
        );
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        ui.painter().galley(
            rect.min + Vec2::new(x, 4.),
            ui.painter().layout_job(job),
            color,
        );
    }
    response
}
fn icon(ui: &mut egui::Ui, icon: Icon, tooltip: &str) -> egui::Response {
    icon_button(ui, icon, tooltip, false, INK)
}
fn icon_button(
    ui: &mut egui::Ui,
    icon: Icon,
    tooltip: &str,
    accented: bool,
    tint: Color32,
) -> egui::Response {
    let pixels = (18. * ui.ctx().pixels_per_point()).ceil() as u32;
    let texture = icons::texture(ui.ctx(), icon, pixels);
    let image = egui::Image::new(&texture)
        .fit_to_exact_size(Vec2::splat(18.))
        .tint(tint);
    let response = ui
        .scope(|ui| {
            ui.spacing_mut().button_padding = Vec2::splat(4.);
            if accented {
                let widgets = &mut ui.visuals_mut().widgets;
                for (visuals, fill) in [
                    (&mut widgets.inactive, ACCENT),
                    (&mut widgets.hovered, Color32::from_rgb(238, 196, 140)),
                    (&mut widgets.active, Color32::from_rgb(198, 151, 94)),
                ] {
                    visuals.bg_fill = fill;
                    visuals.weak_bg_fill = fill;
                    visuals.bg_stroke = Stroke::NONE;
                    visuals.corner_radius = egui::CornerRadius::same(4);
                    visuals.expansion = 0.;
                }
            }
            ui.add(
                egui::Button::new(image)
                    .frame(true)
                    .frame_when_inactive(accented)
                    .min_size(Vec2::splat(26.)),
            )
        })
        .inner;
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::WidgetType::Button, ui.is_enabled(), tooltip)
    });
    response.on_hover_text(tooltip)
}
fn tool_label(ui: &mut egui::Ui, icon: Icon, label: &str, selected: bool) -> egui::Response {
    let pixels = (16. * ui.ctx().pixels_per_point()).ceil() as u32;
    let texture = icons::texture(ui.ctx(), icon, pixels);
    let image = egui::Image::new(&texture)
        .fit_to_exact_size(Vec2::splat(16.))
        .tint(if selected { ACCENT } else { INK });
    ui.add(
        egui::Button::new((image, label))
            .selected(selected)
            .min_size(Vec2::new(0., 26.)),
    )
}
