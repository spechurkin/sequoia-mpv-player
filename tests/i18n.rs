use sequoia::i18n::{Catalogs, Locale, interpolate};

fn locale(bytes: &[u8]) -> Locale {
    Locale::parse(bytes).unwrap()
}

#[test]
fn switches_active_language_and_falls_back_when_unavailable() {
    sequoia::i18n::set_language("en");
    assert_eq!(sequoia::i18n::t("settings.title"), "Settings");
    sequoia::i18n::set_language("ru");
    assert_eq!(sequoia::i18n::t("settings.title"), "Настройки");
    sequoia::i18n::set_language("unavailable");
    assert_eq!(sequoia::i18n::language(), "ru");
}

#[test]
fn shipped_catalogs_have_matching_keys_and_placeholders() {
    let en = locale(include_bytes!("../locales/en.json"));
    let ru = locale(include_bytes!("../locales/ru.json"));
    assert_eq!(
        en.strings.keys().collect::<Vec<_>>(),
        ru.strings.keys().collect::<Vec<_>>()
    );
    for (key, english) in &en.strings {
        assert!(key.contains('.'), "{key}");
        assert!(!english.trim().is_empty(), "{key}");
        assert!(!ru.strings[key].trim().is_empty(), "{key}");
        assert_eq!(
            placeholders(english),
            placeholders(&ru.strings[key]),
            "{key}"
        );
    }
    fn placeholders(text: &str) -> std::collections::BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|s| s.split_once('}').map(|(name, _)| name))
            .collect()
    }
}

#[test]
fn discovers_an_additional_language_and_ignores_invalid_files() {
    let directory = tempfile::tempdir().unwrap();
    std::fs::write(
        directory.path().join("de.json"),
        br#"{
        "format":"sequoia/locale", "version":1, "id":"de", "name":"Deutsch",
        "strings":{"settings.title":"Einstellungen", "settings.save":""}
    }"#,
    )
    .unwrap();
    std::fs::write(directory.path().join("broken.json"), b"{").unwrap();
    std::fs::write(directory.path().join("readme.txt"), b"not a catalog").unwrap();
    let en = locale(include_bytes!("../locales/en.json"));
    let mut catalogs = Catalogs::default();
    catalogs.locales.insert(en.id.clone(), en);
    catalogs.scan(directory.path());
    assert_eq!(catalogs.locales["de"].name, "Deutsch");
    assert_eq!(catalogs.text("de", "settings.title"), "Einstellungen");
    assert_eq!(catalogs.text("de", "settings.save"), "Save");
    assert_eq!(catalogs.text("missing", "settings.title"), "Settings");
    assert_eq!(catalogs.text("de", "missing.key"), "missing.key");
    assert_eq!(catalogs.warnings.len(), 1);
}

#[test]
fn interpolation_can_reorder_values_and_preserves_braces_in_user_data() {
    assert_eq!(
        interpolate(
            "{count}: {path}",
            &[("path", "{count}.mp3".into()), ("count", "2".into())]
        ),
        "2: {count}.mp3"
    );
}

#[test]
fn settings_keep_old_profiles_and_new_language_ids() {
    let old: sequoia::model::Settings = serde_json::from_str("{}").unwrap();
    assert_eq!(old.language, "ru");
    let mut settings = old;
    settings.language = "pt-BR".into();
    let directory = tempfile::tempdir().unwrap();
    sequoia::library::save(
        directory.path(),
        &sequoia::model::Data {
            settings,
            ..Default::default()
        },
    )
    .unwrap();
    assert_eq!(
        sequoia::library::load(directory.path())
            .unwrap()
            .settings
            .language,
        "pt-BR"
    );
}

#[test]
fn all_localization_keys_used_by_the_source_exist() {
    let en = locale(include_bytes!("../locales/en.json"));
    for entry in walkdir::WalkDir::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src"))
        .into_iter()
        .flatten()
    {
        if entry.path().extension().is_some_and(|ext| ext == "rs") {
            let source = std::fs::read_to_string(entry.path()).unwrap();
            for quoted in source.split('"').skip(1).step_by(2) {
                if quoted.starts_with("text.") || quoted.starts_with("settings.") {
                    assert!(
                        en.strings.contains_key(quoted),
                        "{}: {quoted}",
                        entry.path().display()
                    );
                }
            }
        }
    }
}
