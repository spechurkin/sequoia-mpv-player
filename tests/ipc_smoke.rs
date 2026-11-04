use serde_json::json;
use std::{io::Write, path::Path};

fn write_silence(wave: &Path) -> anyhow::Result<()> {
    let mut file = std::fs::File::create(wave)?;
    let samples = 8000u32 * 10;
    file.write_all(b"RIFF")?;
    file.write_all(&(36 + samples * 2).to_le_bytes())?;
    file.write_all(b"WAVEfmt \x10\0\0\0\x01\0\x01\0")?;
    file.write_all(&8000u32.to_le_bytes())?;
    file.write_all(&16000u32.to_le_bytes())?;
    file.write_all(b"\x02\0\x10\0data")?;
    file.write_all(&(samples * 2).to_le_bytes())?;
    file.write_all(&vec![0u8; samples as usize * 2])?;
    drop(file);
    Ok(())
}

#[test]
#[ignore = "requires installed mpv"]
fn real_mpv_ipc_load_pause_seek_and_shutdown() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let wave = directory.path().join("silence.wav");
    write_silence(&wave)?;
    let executable = sequoia::mpv::resolve(Path::new("")).expect("mpv installed");
    let engine = sequoia::mpv::Mpv::start(&executable, None, directory.path(), true)?;
    engine.request(json!(["set_property", "pause", true]))?;
    engine.request(json!(["loadfile", wave]))?;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        if engine
            .request(json!(["get_property", "duration"]))
            .ok()
            .and_then(|v| v.as_f64())
            .is_some_and(|v| v >= 9.9)
        {
            break;
        }
        anyhow::ensure!(std::time::Instant::now() < deadline, "media did not load");
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(engine.request(json!(["get_property", "pause"]))?, true);
    engine.request(json!(["seek", 2, "absolute+exact"]))?;
    engine.request(json!(["stop"]))?;
    drop(engine);
    Ok(())
}

#[test]
#[ignore = "requires installed mpv"]
fn stop_and_replace_do_not_complete_current_file() -> anyhow::Result<()> {
    use sequoia::mpv::Event;
    use std::time::{Duration, Instant};

    let directory = tempfile::tempdir()?;
    let wave = directory.path().join("silence.wav");
    write_silence(&wave)?;
    let executable = sequoia::mpv::resolve(Path::new("")).expect("mpv installed");
    let engine = sequoia::mpv::Mpv::start(&executable, None, directory.path(), true)?;
    for command in [json!(["stop"]), json!(["loadfile", wave, "replace"])] {
        engine.request(json!(["loadfile", wave, "replace"]))?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while engine.request(json!(["get_property", "time-pos"]))
            .ok().and_then(|v| v.as_f64()).is_none_or(|v| v < 0.1) {
            anyhow::ensure!(Instant::now() < deadline, "playback did not start");
            std::thread::sleep(Duration::from_millis(20));
        }
        engine.request(command)?;
        let deadline = Instant::now() + Duration::from_millis(300);
        while let Ok(event) = engine.events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
            match event {
                Event::Eof => anyhow::bail!("stop/replace must not advance the queue"),
                Event::Error(error) => anyhow::bail!("{error}"),
                Event::Disconnected => anyhow::bail!("engine disconnected"),
                _ => {}
            }
        }
    }
    Ok(())
}

#[test]
#[ignore = "requires installed mpv"]
fn manual_navigation_then_seek_to_end_continues_queue() -> anyhow::Result<()> {
    use sequoia::{model::{Media, Queue}, mpv::{Event, Mpv}};
    use std::time::{Duration, Instant};

    let directory = tempfile::tempdir()?;
    let wave = directory.path().join("silence.wav");
    write_silence(&wave)?;
    let executable = sequoia::mpv::resolve(Path::new("")).expect("mpv installed");
    let engine = Mpv::start(&executable, None, directory.path(), true)?;
    for (shuffle, keep_open) in [(false, false), (true, false), (false, true), (true, true)] {
        engine.request(json!(["set_property", "keep-open", keep_open]))?;
        let mut queue = Queue::default();
        queue.replace((0..3).map(|i| Media {
            path: wave.clone(), title: format!("Track {i}"), audio: true,
            ..Default::default()
        }).collect(), 2);
        queue.shuffle = shuffle;
        for _ in 0..8 {
            assert!(queue.advance(false).is_some());
        }
        assert!(queue.previous().is_some());
        let mut completed = std::collections::HashSet::new();
        loop {
            assert!(completed.insert(queue.index.unwrap()), "duplicate playback");
            engine.request(json!(["stop"]))?;
            engine.events.try_iter().for_each(drop);
            engine.request(json!(["loadfile", queue.current().unwrap().path, "replace"]))?;
            engine.request(json!(["set_property", "pause", false]))?;
            let deadline = Instant::now() + Duration::from_secs(5);
            // Duration is available before decoding starts. Seeking to EOF then can
            // produce "no audio or video data played" instead of normal completion.
            while engine.request(json!(["get_property", "time-pos"]))
                .ok().and_then(|v| v.as_f64()).is_none_or(|v| v < 0.1) {
                anyhow::ensure!(Instant::now() < deadline, "playback did not start");
                std::thread::sleep(Duration::from_millis(20));
            }
            engine.request(json!(["seek", 10, "absolute+exact"]))?;
            let deadline = Instant::now() + Duration::from_secs(5);
            loop {
                let remaining = deadline.saturating_duration_since(Instant::now());
                match engine.events.recv_timeout(remaining)? {
                    Event::Eof => break,
                    Event::Error(error) => anyhow::bail!("{error}"),
                    Event::Disconnected => anyhow::bail!("engine disconnected"),
                    _ => {}
                }
            }
            std::thread::sleep(Duration::from_millis(100));
            assert!(!engine.events.try_iter().any(|event| matches!(event, Event::Eof)),
                "one file must not advance the queue twice");
            if queue.advance(true).is_none() {
                break;
            }
        }
        assert_eq!(completed.len(), 3, "seek must continue through remaining tracks");
    }
    Ok(())
}
