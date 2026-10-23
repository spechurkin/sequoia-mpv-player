use serde::Deserialize;
use std::{
    collections::BTreeMap,
    path::Path,
    sync::{LazyLock, RwLock},
};

#[derive(Clone, Debug, Deserialize)]
pub struct Locale {
    pub format: String,
    pub version: u32,
    pub id: String,
    pub name: String,
    pub strings: BTreeMap<String, String>,
}
impl Locale {
    pub fn parse(bytes: &[u8]) -> anyhow::Result<Self> {
        anyhow::ensure!(bytes.len() <= 1024 * 1024, "Locale exceeds 1 MiB");
        let locale: Self =
            serde_json::from_slice(bytes.strip_prefix(&[0xef, 0xbb, 0xbf]).unwrap_or(bytes))?;
        anyhow::ensure!(
            locale.format == "sequoia/locale" && locale.version == 1,
            "Unsupported locale format"
        );
        anyhow::ensure!(
            !locale.id.is_empty()
                && locale.id.len() <= 32
                && locale
                    .id
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_'),
            "Invalid locale ID"
        );
        anyhow::ensure!(!locale.name.trim().is_empty(), "Empty locale name");
        Ok(locale)
    }
}
#[derive(Default)]
pub struct Catalogs {
    pub locales: BTreeMap<String, Locale>,
    pub warnings: Vec<String>,
}
impl Catalogs {
    pub fn scan(&mut self, directory: &Path) {
        let Ok(entries) = std::fs::read_dir(directory) else {
            return;
        };
        let mut paths: Vec<_> = entries.flatten().map(|entry| entry.path()).collect();
        paths.sort();
        for path in paths {
            if !path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("json"))
            {
                continue;
            }
            let result = (|| -> anyhow::Result<Locale> {
                anyhow::ensure!(
                    std::fs::metadata(&path)?.len() <= 1024 * 1024,
                    "Locale exceeds 1 MiB"
                );
                Locale::parse(&std::fs::read(&path)?)
            })();
            match result {
                Ok(locale) => {
                    self.locales.insert(locale.id.clone(), locale);
                }
                Err(error) => self.warnings.push(format!("{}: {error}", path.display())),
            }
        }
    }
    pub fn text<'a>(&'a self, language: &str, key: &'a str) -> &'a str {
        self.locales
            .get(language)
            .and_then(|locale| locale.strings.get(key))
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                self.locales
                    .get("en")
                    .and_then(|locale| locale.strings.get(key))
            })
            .map(String::as_str)
            .unwrap_or(key)
    }
}
static CATALOGS: LazyLock<Catalogs> = LazyLock::new(|| {
    let mut catalogs = Catalogs::default();
    // The build script discovers every shipped JSON; additional files are read at startup.
    for bytes in include!(concat!(env!("OUT_DIR"), "/locales.rs")) {
        match Locale::parse(bytes) {
            Ok(locale) => {
                catalogs.locales.insert(locale.id.clone(), locale);
            }
            Err(error) => catalogs.warnings.push(format!("Bundled locale: {error}")),
        }
    }
    if cfg!(debug_assertions) {
        catalogs.scan(&Path::new(env!("CARGO_MANIFEST_DIR")).join("locales"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            catalogs.scan(&dir.join("locales"));
        }
    }
    catalogs.scan(&crate::library::data_dir().join("locales"));
    catalogs
});
static LANGUAGE: RwLock<String> = RwLock::new(String::new());
pub fn catalogs() -> &'static Catalogs {
    &CATALOGS
}
pub fn default_language() -> String {
    "ru".into()
}
pub fn set_language(language: &str) {
    *LANGUAGE.write().unwrap() = if CATALOGS.locales.contains_key(language) {
        language.to_string()
    } else {
        default_language()
    };
}
pub fn language() -> String {
    let language = LANGUAGE.read().unwrap();
    if language.is_empty() {
        default_language()
    } else {
        language.clone()
    }
}
pub fn text<'a>(language: &str, key: &'a str) -> &'a str {
    CATALOGS.text(language, key)
}
pub fn t(key: &str) -> &str {
    text(&language(), key)
}
pub fn format(key: &str, arguments: &[(&str, String)]) -> String {
    interpolate(t(key), arguments)
}

// Scan the template once: braces in file names or metadata must stay literal.
pub fn interpolate(template: &str, arguments: &[(&str, String)]) -> String {
    let mut output = String::new();
    let mut rest = template;
    while let Some(start) = rest.find('{') {
        output.push_str(&rest[..start]);
        let Some(end) = rest[start..].find('}').map(|end| start + end) else {
            output.push_str(&rest[start..]);
            return output;
        };
        let name = &rest[start + 1..end];
        if let Some((_, value)) = arguments.iter().find(|(key, _)| *key == name) {
            output.push_str(value);
        } else {
            output.push_str(&rest[start..=end]);
        }
        rest = &rest[end + 1..];
    }
    output.push_str(rest);
    output
}
