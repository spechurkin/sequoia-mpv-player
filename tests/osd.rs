use sequoia::{
    mpv::{Event, Mpv},
    playback::osd::{read_updates, FRAME_END},
};
use serde_json::json;
use std::{
    io::{Read, Write},
    time::{Duration, Instant},
};

#[test]
fn fragmented_utf8_multiline_and_empty_status_frames() {
    struct Bytewise(std::io::Cursor<Vec<u8>>);
    impl Read for Bytewise {
        fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
            self.0.read(&mut buf[..1])
        }
    }
    let stream = format!(
        "\nГромкость: 52%\n{FRAME_END}\nПервая строка\nВторая строка\n{FRAME_END}\n{FRAME_END}"
    );
    let mut updates = Vec::new();
    read_updates(Bytewise(std::io::Cursor::new(stream.into_bytes())), |s| {
        updates.push(s)
    });
    assert_eq!(
        updates,
        ["Громкость: 52%", "Первая строка\nВторая строка", ""]
    );
}

#[test]
#[ignore = "requires installed mpv"]
fn real_hotkey_osd_is_forwarded_and_expires_without_video_overlay() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("input.conf"),
        "x show-text \"GLSL shaders cleared\" 200\ny show-text \"Громкость изменена\" 200\nz change-list glsl-shaders clr \"\"\n",
    )?;
    let wave = dir.path().join("silence.wav");
    let mut file = std::fs::File::create(&wave)?;
    file.write_all(b"RIFF")?;
    file.write_all(&(36u32 + 16000).to_le_bytes())?;
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0")?;
    file.write_all(&8000u32.to_le_bytes())?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(b"\x02\0\x10\0data")?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(&vec![0; 16000])?;
    drop(file);
    let executable = sequoia::mpv::resolve(std::path::Path::new("")).unwrap();
    let engine = Mpv::start(&executable, None, dir.path(), true)?;
    assert_eq!(engine.request(json!(["get_property", "video-osd"]))?, false);
    assert_eq!(
        engine.request(json!(["get_property", "sub-visibility"]))?,
        true
    );
    engine.request(json!(["set_property", "loop-file", "inf"]))?;
    engine.request(json!(["loadfile", wave]))?;
    let deadline = Instant::now() + Duration::from_secs(5);
    while engine.request(json!(["get_property", "duration"])).is_err() {
        anyhow::ensure!(Instant::now() < deadline, "wave did not load");
        std::thread::sleep(Duration::from_millis(20));
    }
    for (key, text) in [
        ("x", "GLSL shaders cleared"),
        ("y", "Громкость изменена"),
        ("z", "glsl-shaders:"),
    ] {
        engine.events.try_iter().for_each(drop);
        engine.request(json!(["keypress", key]))?;
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut shown = false;
        let mut received = Vec::new();
        loop {
            let event = engine
                .events
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|error| anyhow::anyhow!("{key}: {error}; received: {received:?}"))?;
            if let Event::Osd(message) = event {
                received.push(message.clone());
                if message == text {
                    shown = true;
                }
                if shown && message.is_empty() {
                    break;
                }
            }
            anyhow::ensure!(
                Instant::now() < deadline,
                "missing OSD text or expiry for {key}"
            );
        }
    }
    Ok(())
}
