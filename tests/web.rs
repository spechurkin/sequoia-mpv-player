use sequoia::playback::web;
use serde_json::json;

#[test]
fn accepts_only_web_links() {
    assert!(web::is_url(
        "https://www.youtube.com/watch?v=abc&list=PL123"
    ));
    for input in [
        "file:///tmp/video",
        "--exec=bad",
        "https://",
        "https:///bad",
        "https://example.com/\n--exec bad",
    ] {
        assert!(!web::is_url(input), "{input}");
    }
}

#[test]
fn playlist_keeps_source_urls_order_and_skips_unavailable_entries() {
    let items = web::parse(&json!({"entries": [
        {"url": "https://www.youtube.com/watch?v=one", "title": "Первое", "duration": 12},
        null,
        {"url": "https://example.com/private", "availability": "private"},
        {"entries": [{"webpage_url": "https://vimeo.com/42", "url": "https://cdn.example/expired", "title": "Второе"}]}
    ]}), "https://example.com/list").unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0].title, "Первое");
    assert_eq!(items[0].duration, 12.);
    assert_eq!(items[1].path.to_string_lossy(), "https://vimeo.com/42");
    let saved = serde_json::to_string(&items).unwrap();
    let restored: Vec<sequoia::model::Media> = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored, items);
}

#[test]
fn single_video_and_empty_playlist() {
    let items = web::parse(&json!({"title": "Live"}), "https://example.com/live").unwrap();
    assert_eq!(items[0].path.to_string_lossy(), "https://example.com/live");
    assert!(web::parse(&json!({"entries": []}), "https://example.com/list").is_err());
}

#[test]
fn internet_shortcuts_utf8_and_utf16() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("video.url");
    let text = "[InternetShortcut]\r\nURL=https://example.com/video?list=1\r\n";
    std::fs::write(&file, text).unwrap();
    assert_eq!(
        web::shortcut(&file).unwrap(),
        "https://example.com/video?list=1"
    );
    let bytes: Vec<_> = [0xff, 0xfe]
        .into_iter()
        .chain(text.encode_utf16().flat_map(u16::to_le_bytes))
        .collect();
    std::fs::write(&file, bytes).unwrap();
    assert_eq!(
        web::shortcut(&file).unwrap(),
        "https://example.com/video?list=1"
    );
}

#[test]
#[ignore = "requires live YouTube, mpv, yt-dlp and Deno in PATH"]
fn live_youtube_extraction_and_playback() -> anyhow::Result<()> {
    use std::time::{Duration, Instant};
    let url = "https://www.youtube.com/watch?v=jNQXAC9IVRw";
    let items = web::extract(url)?;
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].title, "Me at the zoo");
    let engine_path = sequoia::mpv::resolve(std::path::Path::new("")).unwrap();
    let config = tempfile::tempdir()?;
    let engine = sequoia::mpv::Mpv::start(&engine_path, None, config.path(), true)?;
    engine.request(json!(["loadfile", items[0].path, "replace"]))?;
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        for event in engine.events.try_iter() {
            if let sequoia::mpv::Event::Error(error) = event {
                anyhow::bail!("{error}");
            }
        }
        let position = engine
            .request(json!(["get_property", "time-pos"]))
            .ok()
            .and_then(|v| v.as_f64())
            .unwrap_or(0.);
        if position > 0.2 {
            break;
        }
        anyhow::ensure!(
            Instant::now() < deadline,
            "YouTube playback did not advance"
        );
        std::thread::sleep(Duration::from_millis(200));
    }
    Ok(())
}
