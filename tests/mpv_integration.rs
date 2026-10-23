use sequoia::{library, mpv::Mpv};
use serde_json::json;
use std::{
    path::Path,
    time::{Duration, Instant},
};
#[test]
#[ignore = "requires installed mpv and generated media fixtures"]
fn tag_writes_preserve_loaded_stream_and_transport() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let exe = sequoia::mpv::resolve(Path::new("")).expect("mpv must be installed");
    let engine = Mpv::start(&exe, None, root.path(), true)?;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts/fixtures");
    let cover = std::fs::read(fixtures.join("cover.png"))?;
    for filename in ["test.mp3", "test.flac"] {
        let file = root.path().join(filename);
        std::fs::copy(fixtures.join(filename), &file)?;
        engine.request(json!(["loadfile", file, "replace"]))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.request(json!(["get_property", "duration"])).is_err() {
            anyhow::ensure!(Instant::now() < deadline, "file did not load");
            std::thread::sleep(Duration::from_millis(50));
        }
        engine.request(json!(["set_property", "volume", 42]))?;
        engine.request(json!(["set_property", "speed", 1.25]))?;
        engine.request(json!(["set_property", "mute", true]))?;
        for paused in [true, false] {
            engine.request(json!(["set_property", "pause", paused]))?;
            engine.request(json!(["seek", 2.0, "absolute+exact"]))?;
            std::thread::sleep(Duration::from_millis(150));
            let before = engine
                .request(json!(["get_property", "time-pos"]))?
                .as_f64()
                .unwrap();
            let mut media = library::read_media(&file);
            media.title = format!("Обновлённый трек {paused}");
            let saved = library::write_tags(&media, Some(&cover), root.path())?;
            assert_eq!(saved.title, media.title);
            assert_eq!(library::cover(&file).unwrap(), cover);
            #[cfg(windows)]
            {
                use std::os::windows::fs::OpenOptionsExt;
                let original = std::fs::read(&file)?;
                let _lock = std::fs::OpenOptions::new()
                    .read(true)
                    .share_mode(3)
                    .open(&file)?;
                media.title = "Заблокированная запись".into();
                assert!(library::write_tags(&media, None, root.path()).is_err());
                assert_eq!(std::fs::read(&file)?, original);
            }
            std::thread::sleep(Duration::from_millis(200));
            assert_eq!(engine.request(json!(["get_property", "pause"]))?, paused);
            assert_eq!(engine.request(json!(["get_property", "volume"]))?, 42.0);
            assert_eq!(engine.request(json!(["get_property", "speed"]))?, 1.25);
            assert_eq!(engine.request(json!(["get_property", "mute"]))?, true);
            let after = engine
                .request(json!(["get_property", "time-pos"]))?
                .as_f64()
                .unwrap();
            if paused {
                assert!((after - before).abs() < 0.05, "paused position changed");
            } else {
                assert!(after > before + 0.1, "playback stalled or restarted");
            }
            assert!(
                !engine.events.try_iter().any(|event| matches!(
                    event,
                    sequoia::mpv::Event::Eof
                        | sequoia::mpv::Event::Error(_)
                        | sequoia::mpv::Event::Disconnected
                )),
                "tag write interrupted playback"
            );
        }
        engine.request(json!(["stop"]))?;
    }
    Ok(())
}

#[test]
#[ignore = "requires installed mpv and generated media fixtures"]
fn real_engine_playback_and_tag_roundtrip() -> anyhow::Result<()> {
    let root = tempfile::tempdir()?;
    let exe = sequoia::mpv::resolve(Path::new("")).expect("mpv must be installed");
    let engine = Mpv::start(&exe, None, root.path(), true)?;
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("artifacts/fixtures");
    for filename in ["test.mp4", "test.gif", "test.mp3", "test.flac", "multi.mkv"] {
        let file = fixtures.join(filename);
        assert!(file.exists(), "missing {}", file.display());
        engine.request(json!(["loadfile", file, "replace"]))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if engine
                .request(json!(["get_property", "duration"]))
                .ok()
                .and_then(|v| v.as_f64())
                .is_some_and(|n| n > 0.)
            {
                break;
            }
            anyhow::ensure!(Instant::now() < deadline, "duration missing for {filename}");
            std::thread::sleep(Duration::from_millis(50));
        }
        engine.request(json!(["set_property", "pause", true]))?;
        assert_eq!(engine.request(json!(["get_property", "pause"]))?, true);
        engine.request(json!(["seek", 0.2, "absolute+exact"]))?;
        engine.request(json!(["set_property", "volume", 42]))?;
        assert_eq!(engine.request(json!(["get_property", "volume"]))?, 42.0);
        engine.request(json!(["set_property", "volume", 150]))?;
        assert_eq!(engine.request(json!(["get_property", "volume"]))?, 150.0);
        engine.request(json!(["set_property", "speed", 1.25]))?;
        assert_eq!(engine.request(json!(["get_property", "speed"]))?, 1.25);
        let tracks = engine.request(json!(["get_property", "track-list"]))?;
        assert!(!tracks.as_array().unwrap().is_empty());
        if filename == "multi.mkv" {
            engine.request(json!(["set_property", "aid", 2]))?;
            engine.request(json!(["set_property", "vid", 2]))?;
            engine.request(json!(["set_property", "sid", 1]))?;
            assert_eq!(engine.request(json!(["get_property", "aid"]))?, 2);
            assert_eq!(engine.request(json!(["get_property", "vid"]))?, 2);
            assert_eq!(engine.request(json!(["get_property", "sid"]))?, 1);
            engine.request(json!(["sub-add", fixtures.join("test.srt"), "select"]))?;
            let tracks = engine.request(json!(["get_property", "track-list"]))?;
            assert_eq!(
                tracks
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|t| t["type"] == "sub")
                    .count(),
                2
            );
        }
        engine.request(json!(["stop"]))?;
    }
    for filename in ["test.mp3", "test.flac"] {
        let file = root.path().join(filename);
        std::fs::copy(fixtures.join(filename), &file)?;
        let mut media = library::read_media(&file);
        media.title = "Проверка тегов".into();
        media.artist = "Исполнитель".into();
        media.album = "Альбом".into();
        media.year = 2026;
        media.track = 3;
        let cover = std::fs::read(fixtures.join("cover.png"))?;
        let saved = library::write_tags(&media, Some(&cover), root.path())?;
        assert_eq!(saved.title, media.title);
        assert_eq!(saved.artist, media.artist);
        assert_eq!(saved.year, 2026);
        assert_eq!(saved.track, 3);
        assert_eq!(library::cover(&file).unwrap(), cover);
    }
    assert!(engine
        .request(json!(["nonexistent-sequoia-command"]))
        .is_err());
    Ok(())
}
