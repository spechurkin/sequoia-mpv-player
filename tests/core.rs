use sequoia::{
    library,
    model::{Data, Media, Queue, Repeat},
};
use std::{fs, path::PathBuf, sync::atomic::AtomicBool};
fn tracks() -> Vec<Media> {
    (0..5)
        .map(|i| Media {
            path: PathBuf::from(format!("{i}.mp3")),
            title: format!("Track {i}"),
            ..Default::default()
        })
        .collect()
}
#[test]
fn tray_setting_defaults_for_old_data_and_preserves_explicit_opt_out() {
    let old: Data = serde_json::from_str(r#"{"settings":{"volume":42}}"#).unwrap();
    assert!(old.settings.close_to_tray);
    let disabled: Data = serde_json::from_str(r#"{"settings":{"close_to_tray":false}}"#).unwrap();
    let restored: Data = serde_json::from_str(&serde_json::to_string(&disabled).unwrap()).unwrap();
    assert!(!restored.settings.close_to_tray);
}
#[test]
fn sequential_queue_ends() {
    let mut q = Queue::default();
    q.replace(tracks(), 0);
    for _ in 0..4 {
        assert!(q.advance(true).is_some());
    }
    assert!(q.advance(true).is_none());
    assert_eq!(q.index, Some(4));
}
#[test]
fn repeat_one_only_automatic() {
    let mut q = Queue::default();
    q.replace(tracks(), 2);
    q.repeat = Repeat::One;
    assert_eq!(q.advance(true).unwrap().title, "Track 2");
    assert_eq!(q.advance(false).unwrap().title, "Track 3");
}
#[test]
fn repeat_all_wraps() {
    let mut q = Queue::default();
    q.replace(tracks(), 4);
    q.repeat = Repeat::All;
    assert_eq!(q.advance(true).unwrap().title, "Track 0");
}
#[test]
fn shuffle_visits_once_and_previous_retraces() {
    let mut q = Queue::default();
    q.replace(tracks(), 0);
    q.shuffle = true;
    let mut seen = std::collections::HashSet::new();
    seen.insert(q.current().unwrap().path.clone());
    let mut previous = q.current().unwrap().clone();
    for _ in 0..4 {
        previous = q.current().unwrap().clone();
        assert!(seen.insert(q.advance(true).unwrap().path));
    }
    assert!(q.advance(true).is_none());
    assert_eq!(q.previous().unwrap(), previous);
}
#[test]
fn shuffle_repeat_has_no_boundary_duplicate() {
    let mut q = Queue::default();
    q.replace(tracks(), 0);
    q.shuffle = true;
    q.repeat = Repeat::All;
    for _ in 0..50 {
        let previous = q.current().unwrap().path.clone();
        assert_ne!(q.advance(true).unwrap().path, previous);
    }
}
#[test]
fn singleton_repeat_and_empty_queue() {
    let mut q = Queue::default();
    assert!(q.advance(true).is_none());
    q.replace(vec![tracks().remove(0)], 0);
    q.shuffle = true;
    assert!(q.advance(true).is_none());
    q.repeat = Repeat::All;
    assert!(q.advance(true).is_some());
}
#[test]
fn reorder_preserves_current_identity() {
    let mut q = Queue::default();
    q.replace(tracks(), 2);
    let current = q.current().unwrap().clone();
    q.move_item(2, 0);
    assert_eq!(q.current(), Some(&current));
    q.remove(4);
    assert_eq!(q.current(), Some(&current));
    q.remove(0);
    assert!(q.current().is_none());
}
#[test]
fn scan_recurses_and_filters() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir(dir.path().join("album")).unwrap();
    fs::write(dir.path().join("one.MP3"), []).unwrap();
    fs::write(dir.path().join("album/two.FLAC"), []).unwrap();
    fs::write(dir.path().join("clip.gif"), []).unwrap();
    fs::write(dir.path().join("readme.txt"), []).unwrap();
    let cancel = AtomicBool::new(false);
    assert_eq!(
        library::scan(dir.path(), false, false, &cancel, |_| {})
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        library::scan(dir.path(), true, true, &cancel, |_| {})
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        library::scan(dir.path(), true, false, &cancel, |_| {})
            .unwrap()
            .items
            .len(),
        3
    );
}
#[test]
fn scan_cancel_does_not_return_partial_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = AtomicBool::new(true);
    assert!(library::scan(dir.path(), true, false, &cancel, |_| {}).is_err());
}
#[test]
fn persistence_unicode_and_replacement() {
    let root = tempfile::tempdir().unwrap();
    let mut data = Data::default();
    data.playlists.insert("Вечер".into(), tracks());
    data.settings.seek_seconds = 12.;
    library::save(root.path(), &data).unwrap();
    data.settings.seek_seconds = 3.;
    library::save(root.path(), &data).unwrap();
    let loaded = library::load(root.path()).unwrap();
    assert_eq!(loaded.settings.seek_seconds, 3.);
    assert_eq!(loaded.playlists["Вечер"].len(), 5);
}
#[test]
fn manual_mpv_files_survive_startup_and_unrelated_saves() {
    let root = tempfile::tempdir().unwrap();
    let folder = root.path().join("mpv");
    fs::create_dir(&folder).unwrap();
    let config = b"# My settings\r\nvolume=63\r\n[custom]\r\nspeed=1.25\r\n";
    let bindings = b"# My bindings\r\nSPACE cycle pause\r\n";
    fs::write(folder.join("mpv.conf"), config).unwrap();
    fs::write(folder.join("input.conf"), bindings).unwrap();
    let mut data = library::load(root.path()).unwrap();
    library::save(root.path(), &data).unwrap();
    data.settings.volume = 25.;
    data.settings.mpv_config = "stale=setting".into();
    library::save(root.path(), &data).unwrap();
    assert_eq!(fs::read(folder.join("mpv.conf")).unwrap(), config);
    assert_eq!(fs::read(folder.join("input.conf")).unwrap(), bindings);
    assert_eq!(
        library::read_mpv_config(root.path(), "")
            .unwrap()
            .as_bytes(),
        config
    );

    fs::write(folder.join("mpv.conf"), b"# replaced while running\n").unwrap();
    library::save(root.path(), &data).unwrap();
    assert_eq!(
        fs::read(folder.join("mpv.conf")).unwrap(),
        b"# replaced while running\n"
    );
}
#[test]
fn mpv_editor_saves_only_explicit_edits_and_detects_external_changes() {
    let root = tempfile::tempdir().unwrap();
    let mut data = Data::default();
    data.settings.mpv_config = "volume=42\n".into();
    library::save(root.path(), &data).unwrap();
    let path = root.path().join("mpv/mpv.conf");
    let previous = library::read_mpv_config(root.path(), "").unwrap();
    assert_eq!(previous, "volume=42\n");
    library::update_mpv_config(root.path(), &previous, "speed=1.5\n").unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "speed=1.5\n");
    library::update_mpv_config(root.path(), &previous, &previous).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), "speed=1.5\n");
    assert!(library::update_mpv_config(root.path(), &previous, "").is_err());
    assert_eq!(fs::read_to_string(&path).unwrap(), "speed=1.5\n");
    library::update_mpv_config(root.path(), "speed=1.5\n", "").unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
    library::save(root.path(), &data).unwrap();
    assert_eq!(fs::read(&path).unwrap(), b"");
}
#[test]
fn corrupt_data_is_backed_up() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("library.json"), "bad json").unwrap();
    assert!(library::load(root.path()).is_err());
    assert!(fs::read_dir(root.path())
        .unwrap()
        .filter_map(Result::ok)
        .any(|e| e.file_name().to_string_lossy().contains("corrupt")));
}
#[test]
fn m3u_relative_and_unicode_roundtrip() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("Музыка.mp3");
    fs::write(&file, []).unwrap();
    let list = root.path().join("list.m3u8");
    fs::write(&list, "\u{feff}#EXTM3U\nМузыка.mp3\n").unwrap();
    let tracks = library::import_m3u(&list).unwrap();
    assert_eq!(tracks[0].path, file);
    library::export_m3u(&list, &tracks).unwrap();
    assert_eq!(library::import_m3u(&list).unwrap(), tracks);
}
#[test]
fn known_music_folder_exists_as_absolute_path() {
    assert!(library::music_folder().is_absolute());
}
