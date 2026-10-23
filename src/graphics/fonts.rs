use eframe::egui::{FontData, FontDefinitions, FontFamily};
use std::path::Path;

/// Keep egui's typography; consult installed fonts only for missing characters.
pub fn system_definitions() -> FontDefinitions {
    let mut definitions = FontDefinitions::default();
    #[cfg(windows)]
    if let Some(windows) = std::env::var_os("WINDIR") {
        append_windows_fallbacks(&mut definitions, &Path::new(&windows).join("Fonts"));
    }
    definitions
}

fn append_windows_fallbacks(definitions: &mut FontDefinitions, directory: &Path) {
    // Alternatives in each group cover different Windows language installations.
    for alternatives in [
        &["seguisym.ttf"][..],
        &["segoeui.ttf"][..],
        &["YuGothR.ttc", "meiryo.ttc", "msgothic.ttc"][..],
        &["msyh.ttc", "simsun.ttc"][..],
        &["msjh.ttc", "mingliu.ttc"][..],
        &["malgun.ttf", "gulim.ttc"][..],
        &["simsunb.ttf", "mingliub.ttc"][..],
        &["SimsunExtG.ttf"][..],
        &["seguiemj.ttf"][..],
        &["ebrima.ttf"][..],
        &["Nirmala.ttc"][..],
        &["LeelawUI.ttf", "leelawad.ttf"][..],
        &["gadugi.ttf"][..],
        &["javatext.ttf"][..],
        &["mmrtext.ttf"][..],
        &["himalaya.ttf"][..],
        &["monbaiti.ttf"][..],
        &["sylfaen.ttf"][..],
        &["seguihis.ttf"][..],
    ] {
        for filename in alternatives {
            let Ok(bytes) = std::fs::read(directory.join(filename)) else {
                continue;
            };
            // Validate with the renderer's parser so a damaged font cannot break startup.
            if ab_glyph::FontRef::try_from_slice(&bytes).is_err() {
                continue;
            }
            let name = format!("system/{filename}");
            definitions
                .font_data
                .insert(name.clone(), FontData::from_owned(bytes).into());
            for family in [FontFamily::Proportional, FontFamily::Monospace] {
                definitions
                    .families
                    .entry(family)
                    .or_default()
                    .push(name.clone());
            }
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_or_invalid_system_fonts_preserve_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let mut definitions = FontDefinitions::default();
        let defaults = definitions.clone();
        append_windows_fallbacks(&mut definitions, directory.path());
        assert_eq!(definitions, defaults);
        std::fs::write(directory.path().join("seguisym.ttf"), b"invalid font").unwrap();
        append_windows_fallbacks(&mut definitions, directory.path());
        assert_eq!(definitions, defaults);
    }

    #[cfg(windows)]
    #[test]
    fn installed_fallbacks_render_unicode_in_both_families() {
        let definitions = system_definitions();
        let defaults = FontDefinitions::default();
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            assert!(definitions.families[&family].starts_with(&defaults.families[&family]));
        }
        let ctx = eframe::egui::Context::default();
        ctx.set_fonts(definitions);
        let _ = ctx.run(Default::default(), |ctx| {
            ctx.fonts(|fonts| {
                for family in [FontFamily::Proportional, FontFamily::Monospace] {
                    let id = eframe::egui::FontId::new(13., family);
                    for c in "←→↑↓↔↕⇐⇒⟵⟶日本語ひらがなカタカナ中文简体繁體한국어𠮷♫♯✓Привет".chars()
                    {
                        assert!(
                            fonts.has_glyph(&id, c),
                            "missing {c} (U+{:04X}) in {}",
                            c as u32,
                            id.family
                        );
                    }
                    let galley = fonts.layout_no_wrap(
                        "Shift + ← / → · 日本語 · 中文 · 한국어".into(),
                        id,
                        eframe::egui::Color32::WHITE,
                    );
                    assert!(galley.size().x > 100.);
                    assert!(galley.size().y < 26., "fallback inflated row height");
                }
            });
        });
    }
}
