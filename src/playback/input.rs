use eframe::egui::{Key, Modifiers};

pub fn ensure_config(directory: &std::path::Path) -> std::io::Result<()> {
    use std::io::Write;
    match std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("input.conf"))
    {
        Ok(mut file) => file.write_all(DEFAULT_CONFIG.as_bytes()),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error),
    }
}

pub const DEFAULT_CONFIG: &str = "# Sequoia input.conf - mpv key names and commands\n\
MBTN_RIGHT cycle pause\n\
p cycle pause\n\
. frame-step\n\
, frame-back-step\n\
WHEEL_UP add volume 2\n\
WHEEL_DOWN add volume -2\n\
9 add volume -2\n\
0 add volume 2\n\
[ multiply speed 0.9090909\n\
] multiply speed 1.1\n\
BS set speed 1\n\
j cycle sub\n\
J cycle sub down\n\
v cycle sub-visibility\n\
# Unbound Sequoia shortcuts (F, S, arrows, Ctrl+O) keep their app actions.\n";

pub fn key_name(key: Key, modifiers: Modifiers) -> String {
    let name = match key {
        Key::Space => "SPACE".into(),
        Key::ArrowLeft => "LEFT".into(),
        Key::ArrowRight => "RIGHT".into(),
        Key::ArrowUp => "UP".into(),
        Key::ArrowDown => "DOWN".into(),
        Key::Escape => "ESC".into(),
        Key::Enter => "ENTER".into(),
        Key::Backspace => "BS".into(),
        Key::Delete => "DEL".into(),
        Key::Insert => "INS".into(),
        Key::PageUp => "PGUP".into(),
        Key::PageDown => "PGDWN".into(),
        Key::Tab => "TAB".into(),
        Key::Home => "HOME".into(),
        Key::End => "END".into(),
        Key::Minus => "-".into(),
        Key::Quote => "'".into(),
        _ => key.symbol_or_name().to_string(),
    };
    let printable = name.chars().count() == 1;
    let name = if printable && !modifiers.shift {
        name.to_lowercase()
    } else {
        name
    };
    modified(&name, modifiers, !printable)
}

pub fn modified(name: &str, modifiers: Modifiers, include_shift: bool) -> String {
    format!(
        "{}{}{}{}{}",
        if modifiers.ctrl { "Ctrl+" } else { "" },
        if modifiers.alt { "Alt+" } else { "" },
        if modifiers.mac_cmd { "Meta+" } else { "" },
        if modifiers.shift && include_shift {
            "Shift+"
        } else {
            ""
        },
        name
    )
}

pub fn has_binding(bindings: &serde_json::Value, key: &str) -> bool {
    bindings.as_array().is_some_and(|bindings| {
        bindings
            .iter()
            .any(|binding| binding["key"] == key && binding["priority"].as_i64().unwrap_or(-1) >= 0)
    })
}
