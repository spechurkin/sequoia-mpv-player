use eframe::egui::{self, Color32, Rect, TextureHandle};

macro_rules! catalog {
    ($($variant:ident => $name:literal),+ $(,)?) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum Icon { $($variant),+ }
        impl Icon {
            pub const ALL: &'static [Self] = &[$(Self::$variant),+];
            pub fn name(self) -> &'static str { match self { $(Self::$variant => $name),+ } }
            pub fn svg(self) -> &'static str { match self { $(Self::$variant => include_str!(concat!("../../assets/icons/", $name, ".svg"))),+ } }
        }
    }
}
catalog! {
    Play => "play", Pause => "pause", Stop => "stop", Previous => "previous", Next => "next",
    SeekBackward => "seek-backward", SeekForward => "seek-forward", FrameBackward => "frame-backward", FrameForward => "frame-forward",
    Shuffle => "shuffle", RepeatOff => "repeat-off", RepeatOne => "repeat-one", RepeatAll => "repeat-all",
    VolumeHigh => "volume-high", VolumeLow => "volume-low", VolumeMuted => "volume-muted",
    FileOpen => "file-open", Folder => "folder-open", FolderTree => "folder-tree",
    Video => "video", Music => "music", AudioTrack => "audio-track", VideoTrack => "video-track",
    Subtitles => "subtitles", SubtitleAdd => "subtitle-add", Chapters => "chapters", Speed => "speed",
    Camera => "camera", Expand => "fullscreen", Collapse => "fullscreen-exit", Queue => "queue",
    Playlist => "playlist", PlaylistAdd => "playlist-add", Import => "import", Export => "export", Save => "save",
    Trash => "trash", MoveUp => "move-up", MoveDown => "move-down", Search => "search", EditTags => "edit-tags",
    Cover => "cover", Download => "download", Refresh => "refresh", Settings => "settings", Grouping => "grouping", Terminal => "terminal",
    Keyboard => "keyboard", Minimize => "minimize", Maximize => "maximize", Restore => "restore", Close => "close",
    More => "more", Back => "back", Check => "check", Record => "record", Album => "album", Artist => "artist", Error => "error", Help => "help",
}
pub const APP_SVG: &str = include_str!("../../assets/app/sequoia.svg");

pub fn raster(svg: &str, size: u32) -> anyhow::Result<resvg::tiny_skia::Pixmap> {
    let options = resvg::usvg::Options::default();
    let tree = resvg::usvg::Tree::from_str(svg, &options)?;
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size)
        .ok_or_else(|| anyhow::anyhow!("Invalid SVG raster size"))?;
    let scale = size as f32 / tree.size().width().max(tree.size().height());
    resvg::render(
        &tree,
        resvg::tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    Ok(pixmap)
}
pub fn app_icon() -> egui::IconData {
    let pixmap = raster(APP_SVG, 256).expect("embedded application SVG must render");
    egui::IconData {
        rgba: unpremultiply(&pixmap),
        width: 256,
        height: 256,
    }
}
pub fn unpremultiply(pixmap: &resvg::tiny_skia::Pixmap) -> Vec<u8> {
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let p = p.demultiply();
            [p.red(), p.green(), p.blue(), p.alpha()]
        })
        .collect()
}
pub fn texture(ctx: &egui::Context, icon: Icon, pixels: u32) -> TextureHandle {
    let id = egui::Id::new(("svg-icon", icon, pixels));
    if let Some(handle) = ctx.data(|data| data.get_temp::<TextureHandle>(id)) {
        return handle;
    }
    let svg = icon.svg().replace("currentColor", "#ffffff");
    let pixmap = raster(&svg, pixels).expect("embedded tool SVG must render");
    let rgba = unpremultiply(&pixmap);
    let image = egui::ColorImage::from_rgba_unmultiplied([pixels as usize, pixels as usize], &rgba);
    let handle = ctx.load_texture(
        format!("svg:{}:{pixels}", icon.name()),
        image,
        egui::TextureOptions::LINEAR,
    );
    ctx.data_mut(|data| data.insert_temp(id, handle.clone()));
    handle
}
pub fn paint(ui: &egui::Ui, icon: Icon, rect: Rect, tint: Color32) {
    let pixels = (rect.width() * ui.ctx().pixels_per_point())
        .ceil()
        .clamp(16., 512.) as u32;
    let handle = texture(ui.ctx(), icon, pixels);
    ui.painter().image(
        handle.id(),
        rect,
        Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
        if ui.is_enabled() {
            tint
        } else {
            tint.gamma_multiply(0.4)
        },
    );
}
