use crate::model::{Data, Media};
use anyhow::{Context, Result};
use lofty::{
    config::WriteOptions,
    file::{AudioFile, TaggedFileExt},
    prelude::{Accessor, ItemKey},
    tag::Tag,
};
use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
};

#[cfg(windows)]
fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::{
        System::Com::CoTaskMemFree,
        UI::Shell::{SHGetKnownFolderPath, KF_FLAG_DEFAULT},
    };
    unsafe {
        let ptr = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
        let value = ptr.to_string().ok().map(PathBuf::from);
        CoTaskMemFree(Some(ptr.0.cast()));
        value
    }
}
pub fn music_folder() -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = known_folder(&windows::Win32::UI::Shell::FOLDERID_Music) {
        return path;
    }
    home().join("Music")
}
pub fn pictures_folder() -> PathBuf {
    #[cfg(windows)]
    if let Some(path) = known_folder(&windows::Win32::UI::Shell::FOLDERID_Pictures) {
        return path;
    }
    home().join("Pictures")
}
fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(PathBuf::from)
        .unwrap_or_default()
}
pub fn data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("SEQUOIA_DATA_DIR") {
        return path.into();
    }
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join(".config"))
        .join("Sequoia")
}
pub fn extension(path: &Path) -> String {
    path.extension()
        .unwrap_or_default()
        .to_string_lossy()
        .to_ascii_lowercase()
}
pub fn audio(path: &Path) -> bool {
    matches!(
        extension(path).as_str(),
        "mp3"
            | "flac"
            | "ogg"
            | "opus"
            | "m4a"
            | "aac"
            | "wav"
            | "wma"
            | "aiff"
            | "ape"
            | "alac"
            | "aif"
            | "mka"
            | "dsf"
            | "dff"
            | "wv"
            | "mpc"
    )
}
pub fn supported(path: &Path) -> bool {
    audio(path)
        || matches!(
            extension(path).as_str(),
            "mp4"
                | "mkv"
                | "avi"
                | "mov"
                | "webm"
                | "wmv"
                | "m4v"
                | "mpg"
                | "mpeg"
                | "ts"
                | "mts"
                | "m2ts"
                | "ogv"
                | "gif"
                | "vob"
                | "flv"
        )
}
pub fn read_media(path: &Path) -> Media {
    let mut media = Media {
        path: path.into(),
        title: path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into(),
        audio: audio(path),
        ..Default::default()
    };
    if let Ok(file) = lofty::read_from_path(path) {
        media.duration = file.properties().duration().as_secs_f64();
        if let Some(tag) = file.primary_tag().or_else(|| file.first_tag()) {
            media.title = tag
                .title()
                .map(|s| s.into_owned())
                .filter(|s| !s.is_empty())
                .unwrap_or(media.title);
            media.artist = tag
                .get_strings(&ItemKey::TrackArtist)
                .collect::<Vec<_>>()
                .join("; ");
            media.album_artist = tag
                .get_strings(&ItemKey::AlbumArtist)
                .collect::<Vec<_>>()
                .join("; ");
            media.disc = tag.disk().unwrap_or(0);
            media.grouping = tag
                .get_string(&ItemKey::ContentGroup)
                .unwrap_or_default()
                .into();
            media.album = tag.album().map(|s| s.into_owned()).unwrap_or_default();
            media.genre = tag.genre().map(|s| s.into_owned()).unwrap_or_default();
            media.year = tag.year().unwrap_or(0);
            media.track = tag.track().unwrap_or(0);
        }
    }
    media
}
pub struct ScanResult {
    pub items: Vec<Media>,
    pub warnings: Vec<String>,
}
pub fn scan(
    root: &Path,
    recursive: bool,
    audio_only: bool,
    cancel: &AtomicBool,
    mut progress: impl FnMut(usize),
) -> Result<ScanResult> {
    anyhow::ensure!(root.is_dir(), "{}", crate::i18n::format("text.folder_unavailable", &[("arg0", format!("{}", root.display()))]));
    let mut result = ScanResult {
        items: Vec::new(),
        warnings: Vec::new(),
    };
    let walker = walkdir::WalkDir::new(root)
        .follow_links(false)
        .max_depth(if recursive { usize::MAX } else { 1 });
    for entry in walker {
        anyhow::ensure!(!cancel.load(Ordering::Relaxed), crate::i18n::t("text.scan_cancelled"));
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                result.warnings.push(error.to_string());
                continue;
            }
        };
        if entry.file_type().is_file()
            && if audio_only {
            audio(entry.path())
        } else {
            supported(entry.path())
        }
        {
            result.items.push(read_media(entry.path()));
            if result.items.len() % 20 == 0 {
                progress(result.items.len());
            }
        }
    }
    result.items.sort_by(|a, b| {
        a.primary_artist()
            .to_lowercase()
            .cmp(&b.primary_artist().to_lowercase())
            .then(a.album.cmp(&b.album))
            .then(a.track.cmp(&b.track))
            .then(a.path.cmp(&b.path))
    });
    Ok(result)
}
pub fn cover(path: &Path) -> Option<Vec<u8>> {
    if let Ok(file) = lofty::read_from_path(path) {
        if let Some(pic) = file.tags().iter().flat_map(|t| t.pictures()).next() {
            return Some(pic.data().to_vec());
        }
    }
    for name in ["cover.jpg", "folder.jpg", "cover.png", "folder.png"] {
        if let Ok(data) = fs::read(path.parent()?.join(name)) {
            return Some(data);
        }
    }
    None
}
pub fn load(root: &Path) -> Result<Data> {
    let path = root.join("library.json");
    if !path.exists() {
        return Ok(Data::default());
    }
    let content = fs::read(&path)?;
    match serde_json::from_slice(&content) {
        Ok(data) => Ok(data),
        Err(e) => {
            fs::copy(
                &path,
                root.join(format!("library.corrupt-{}.json", stamp())),
            )?;
            Err(e.into())
        }
    }
}
pub fn save(root: &Path, data: &Data) -> Result<()> {
    fs::create_dir_all(root)?;
    let temporary = root.join("library.json.tmp");
    fs::write(&temporary, serde_json::to_vec_pretty(data)?)?;
    fs::rename(&temporary, root.join("library.json"))?;
    fs::create_dir_all(root.join("mpv"))?;
    match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join("mpv/mpv.conf"))
    {
        Ok(mut file) => {
            use std::io::Write;
            file.write_all(data.settings.mpv_config.as_bytes())?;
        }
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(e.into()),
    }
    Ok(())
}
pub fn read_mpv_config(root: &Path, fallback: &str) -> Result<String> {
    match fs::read_to_string(root.join("mpv/mpv.conf")) {
        Ok(content) => Ok(content),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(fallback.into()),
        Err(e) => Err(e).context(crate::i18n::t("text.failed_to_read_mpv_conf")),
    }
}
pub fn update_mpv_config(root: &Path, previous: &str, content: &str) -> Result<()> {
    if previous == content {
        return Ok(());
    }
    let current = read_mpv_config(root, previous)?;
    anyhow::ensure!(
        current == previous,
        crate::i18n::t("text.mpv_conf_was_changed_outside_sequoia_reopen_settings_before_editing")
    );
    fs::create_dir_all(root.join("mpv"))?;
    fs::write(root.join("mpv/mpv.conf"), content).context(crate::i18n::t("text.failed_to_save_mpv_conf"))
}
pub fn stamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos()
}
pub fn write_tags(media: &Media, artwork: Option<&[u8]>, root: &Path) -> Result<Media> {
    patch_tags(
        &media.path,
        &crate::tags::TagPatch {
            title: Some(media.title.clone()),
            artist: Some(media.artist.clone()),
            album: Some(media.album.clone()),
            album_artist: Some(media.album_artist.clone()),
            disc: Some(media.disc),
            grouping: Some(media.grouping.clone()),
            genre: Some(media.genre.clone()),
            year: Some(media.year),
            track: Some(media.track),
            artwork: artwork.map(|b| b.to_vec()),
        },
        root,
    )
}
pub fn patch_tags(path: &Path, patch: &crate::tags::TagPatch, root: &Path) -> Result<Media> {
    if patch.is_empty() {
        return Ok(read_media(path));
    }
    let backup_dir = root.join("tag-backups");
    fs::create_dir_all(&backup_dir)?;
    let filename = path
        .file_name()
        .context(crate::i18n::t("text.file_name_unavailable"))?
        .to_string_lossy();
    let backup = backup_dir.join(format!("{}-{filename}", stamp()));
    fs::copy(path, &backup).context(crate::i18n::t("text.failed_to_create_a_backup"))?;
    let temp = path.with_file_name(format!(".sequoia-{}-{filename}", stamp()));
    let operation = || -> Result<Media> {
        fs::copy(path, &temp)?;
        let mut file = lofty::read_from_path(&temp).context(crate::i18n::t("text.unsupported_tag_format"))?;
        if file.primary_tag().is_none() {
            file.insert_tag(Tag::new(file.primary_tag_type()));
        }
        let tag = file
            .primary_tag_mut()
            .context(crate::i18n::t("text.this_file_does_not_support_writing_tags"))?;
        if let Some(value) = &patch.title {
            if value.is_empty() {
                tag.remove_title();
            } else {
                tag.set_title(value.clone());
            }
        }
        if let Some(value) = &patch.artist {
            if value.is_empty() {
                tag.remove_artist();
            } else {
                tag.set_artist(value.clone());
            }
        }
        for (key, value) in [
            (ItemKey::AlbumArtist, &patch.album_artist),
            (ItemKey::ContentGroup, &patch.grouping),
        ] {
            if let Some(value) = value {
                tag.remove_key(&key);
                if !value.is_empty() {
                    tag.insert_text(key, value.clone());
                }
            }
        }
        if let Some(disc) = patch.disc {
            if disc == 0 {
                tag.remove_disk();
            } else {
                tag.set_disk(disc);
            }
        }
        if let Some(value) = &patch.album {
            if value.is_empty() {
                tag.remove_album();
            } else {
                tag.set_album(value.clone());
            }
        }
        if let Some(value) = &patch.genre {
            if value.is_empty() {
                tag.remove_genre();
            } else {
                tag.set_genre(value.clone());
            }
        }
        if let Some(year) = patch.year {
            if year == 0 {
                tag.remove_year();
            } else {
                tag.set_year(year);
            }
        }
        if let Some(track) = patch.track {
            if track == 0 {
                tag.remove_track();
            } else {
                tag.set_track(track);
            }
        }
        if let Some(bytes) = &patch.artwork {
            let mut picture =
                lofty::picture::Picture::from_reader(&mut std::io::Cursor::new(bytes))?;
            picture.set_pic_type(lofty::picture::PictureType::CoverFront);
            tag.remove_picture_type(lofty::picture::PictureType::CoverFront);
            tag.push_picture(picture);
        }
        file.save_to_path(&temp, WriteOptions::default())?;
        let check_file = lofty::read_from_path(&temp)?;
        let check = check_file.primary_tag().context(crate::i18n::t("text.tags_were_not_written"))?;
        anyhow::ensure!(
            patch
                .title
                .as_ref()
                .is_none_or(|v| check.title().unwrap_or_default().as_ref() == v)
                && patch.artist.as_ref().is_none_or(|v| check
                    .artist()
                    .unwrap_or_default()
                    .as_ref()
                    == v)
                && patch
                    .album
                    .as_ref()
                    .is_none_or(|v| check.album().unwrap_or_default().as_ref() == v)
                && patch
                    .genre
                    .as_ref()
                    .is_none_or(|v| check.genre().unwrap_or_default().as_ref() == v)
                && patch
                    .album_artist
                    .as_ref()
                    .is_none_or(
                        |v| check.get_string(&ItemKey::AlbumArtist).unwrap_or_default() == v
                    )
                && patch
                    .grouping
                    .as_ref()
                    .is_none_or(
                        |v| check.get_string(&ItemKey::ContentGroup).unwrap_or_default() == v
                    )
                && patch.disc.is_none_or(|v| check.disk().unwrap_or(0) == v)
                && patch.year.is_none_or(|v| check.year().unwrap_or(0) == v)
                && patch.track.is_none_or(|v| check.track().unwrap_or(0) == v)
                && patch.artwork.as_ref().is_none_or(|v| check
                    .pictures()
                    .iter()
                    .any(|p| p.pic_type() == lofty::picture::PictureType::CoverFront
                        && p.data() == v)),
            crate::i18n::t("text.written_tags_failed_verification")
        );
        fs::rename(&temp, path).context(
            crate::i18n::t("text.failed_to_replace_the_file_check_access_permissions_and_whether_another_app"),
        )?;
        Ok(read_media(path))
    };
    let result = operation();
    if temp.exists() {
        let _ = fs::remove_file(temp);
    }
    result
}
pub fn import_m3u(path: &Path) -> Result<Vec<Media>> {
    let content = fs::read_to_string(path)?;
    Ok(content
        .trim_start_matches('\u{feff}')
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty() && !s.starts_with('#'))
        .filter_map(|entry| {
            let p = PathBuf::from(entry);
            let p = if p.is_absolute() {
                p
            } else {
                path.parent().unwrap_or(Path::new(".")).join(p)
            };
            if supported(&p) {
                Some(read_media(&p))
            } else {
                None
            }
        })
        .collect())
}
pub fn export_m3u(path: &Path, items: &[Media]) -> Result<()> {
    let mut content = String::from("#EXTM3U\n");
    for item in items {
        content.push_str(&format!(
            "#EXTINF:{},{}\n{}\n",
            item.duration as i64,
            item.title.replace(['\r', '\n'], " "),
            item.path.display()
        ));
    }
    fs::write(path, content)?;
    Ok(())
}
