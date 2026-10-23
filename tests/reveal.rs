#![cfg(windows)]

#[test]
fn missing_track_returns_its_path_without_opening_explorer() {
    let root = tempfile::tempdir().unwrap();
    let missing = root
        .path()
        .join("Музыка с пробелами, тест")
        .join("Трек.mp3");
    let error = sequoia::native::reveal_file(&missing).unwrap_err();
    assert!(error.to_string().contains(&missing.display().to_string()));
}

#[test]
#[ignore = "opens Windows Explorer on the desktop"]
fn reveal_track_with_spaces_unicode_and_comma() -> anyhow::Result<()> {
    let folder = std::env::current_dir()?.join("artifacts/reveal-test/Музыка с пробелами, тест");
    std::fs::create_dir_all(&folder)?;
    let track = folder.join("Песня, номер 1.mp3");
    std::fs::write(&track, [])?;
    sequoia::native::reveal_file(&track)
}
