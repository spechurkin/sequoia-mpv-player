use serde_json::json;
use std::{io::Write, path::Path};

#[test]
#[ignore = "requires installed mpv"]
fn real_mpv_ipc_load_pause_seek_and_shutdown() -> anyhow::Result<()> {
    let directory = tempfile::tempdir()?;
    let wave = directory.path().join("silence.wav");
    let mut file = std::fs::File::create(&wave)?;
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
