use eframe::egui::{Key, Modifiers};
use sequoia::playback::input::{ensure_config, has_binding, key_name};
use serde_json::json;

#[test]
fn config_is_created_once_and_preserves_custom_and_empty_files() {
    let dir = tempfile::tempdir().unwrap();
    ensure_config(dir.path()).unwrap();
    let path = dir.path().join("input.conf");
    assert!(
        std::fs::read_to_string(&path)
            .unwrap()
            .contains("MBTN_RIGHT cycle pause")
    );
    for content in ["# custom\r\nMBTN_RIGHT ignore\r\n", ""] {
        std::fs::write(&path, content).unwrap();
        ensure_config(dir.path()).unwrap();
        assert_eq!(std::fs::read_to_string(&path).unwrap(), content);
    }
}

#[test]
fn egui_keys_use_mpv_names_and_modifiers() {
    for (key, expected) in [
        (Key::Space, "SPACE"),
        (Key::ArrowLeft, "LEFT"),
        (Key::PageDown, "PGDWN"),
        (Key::P, "p"),
        (Key::Period, "."),
        (Key::Comma, ","),
        (Key::OpenBracket, "["),
        (Key::Minus, "-"),
    ] {
        assert_eq!(key_name(key, Modifiers::NONE), expected);
    }
    assert_eq!(key_name(Key::J, Modifiers::SHIFT), "J");
    assert_eq!(key_name(Key::ArrowRight, Modifiers::SHIFT), "Shift+RIGHT");
    assert_eq!(key_name(Key::O, Modifiers::CTRL), "Ctrl+o");
    let bindings = json!([
        {"key": "SPACE", "priority": -1},
        {"key": "p", "priority": 0},
        {"key": "MBTN_RIGHT", "priority": 1, "cmd": "ignore"}
    ]);
    assert!(!has_binding(&bindings, "SPACE"));
    assert!(has_binding(&bindings, "p"));
    assert!(has_binding(&bindings, "MBTN_RIGHT"));
}

#[test]
#[ignore = "requires installed mpv"]
fn real_mpv_input_conf_and_overrides() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let executable = sequoia::mpv::resolve(std::path::Path::new("")).unwrap();
    let engine = sequoia::mpv::Mpv::start(&executable, None, dir.path(), true)?;
    let bindings = engine.request(json!(["get_property", "input-bindings"]))?;
    assert!(has_binding(&bindings, "MBTN_RIGHT"));
    assert!(has_binding(&bindings, "."));
    engine.request(json!(["set_property", "pause", false]))?;
    engine.request(json!(["keypress", "MBTN_RIGHT"]))?;
    wait_property(&engine, "pause", json!(true))?;
    engine.request(json!(["keypress", "MBTN_RIGHT"]))?;
    wait_property(&engine, "pause", json!(false))?;
    engine.request(json!(["keypress", "p"]))?;
    wait_property(&engine, "pause", json!(true))?;
    engine.request(json!(["set_property", "volume", 50]))?;
    engine.request(json!(["keypress", "WHEEL_UP"]))?;
    wait_property(&engine, "volume", json!(52.0))?;
    drop(engine);

    std::fs::write(
        dir.path().join("input.conf"),
        "MBTN_RIGHT set volume 31\nSPACE ignore\nCtrl+x set speed 1.5\n",
    )?;
    let engine = sequoia::mpv::Mpv::start(&executable, None, dir.path(), true)?;
    engine.request(json!(["set_property", "pause", true]))?;
    engine.request(json!(["keypress", "MBTN_RIGHT"]))?;
    wait_property(&engine, "volume", json!(31.0))?;
    engine.request(json!(["keypress", "Ctrl+x"]))?;
    wait_property(&engine, "speed", json!(1.5))?;
    assert_eq!(engine.request(json!(["get_property", "pause"]))?, true);
    assert!(has_binding(
        &engine.request(json!(["get_property", "input-bindings"]))?,
        "SPACE"
    ));
    drop(engine);
    let imported = "# Existing config without mouse bindings\nWHEEL_UP add volume 1\n";
    std::fs::write(dir.path().join("input.conf"), imported)?;
    let engine = sequoia::mpv::Mpv::start(&executable, None, dir.path(), true)?;
    engine.request(json!(["set_property", "pause", false]))?;
    engine.request(json!(["keypress", "MBTN_RIGHT"]))?;
    wait_property(&engine, "pause", json!(true))?;
    assert_eq!(std::fs::read_to_string(dir.path().join("input.conf"))?, imported);
    Ok(())
}

fn wait_property(
    engine: &sequoia::mpv::Mpv,
    name: &str,
    expected: serde_json::Value,
) -> anyhow::Result<()> {
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
    loop {
        if engine.request(json!(["get_property", name]))? == expected {
            return Ok(());
        }
        anyhow::ensure!(
            std::time::Instant::now() < deadline,
            "{name} did not become {expected}"
        );
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}
