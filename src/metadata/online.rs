use anyhow::{Context, Result};
use serde_json::Value;
use std::{
    sync::Mutex,
    time::{Duration, Instant},
};
static LAST_REQUEST: Mutex<Option<Instant>> = Mutex::new(None);
#[derive(Clone, Debug)]
pub struct Candidate {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub year: u32,
    pub release_id: Option<String>,
    pub score: u64,
}
fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(20)))
        .build()
        .into()
}
fn encode(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub fn search(title: &str, artist: &str) -> Result<Vec<Candidate>> {
    let mut last = LAST_REQUEST.lock().unwrap();
    if let Some(time) = *last {
        let wait = Duration::from_millis(1100).saturating_sub(time.elapsed());
        std::thread::sleep(wait);
    }
    *last = Some(Instant::now());
    let escape = |s: &str| s.replace('\\', "\\\\").replace('"', "\\\"");
    let query = if artist.trim().is_empty() {
        format!("recording:\"{}\"", escape(title))
    } else {
        format!(
            "recording:\"{}\" AND artist:\"{}\"",
            escape(title),
            escape(artist)
        )
    };
    let url = format!(
        "https://musicbrainz.org/ws/2/recording/?query={}&fmt=json&limit=12",
        encode(&query)
    );
    let body: Value = agent()
        .get(&url)
        .header("User-Agent", "Sequoia/0.1.0 (native desktop music library)")
        .call()
        .context(crate::i18n::t("text.musicbrainz_unavailable"))?
        .body_mut()
        .read_json()?;
    drop(last);
    Ok(body["recordings"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| {
            let release = r["releases"].as_array().and_then(|a| a.first());
            Candidate {
                title: r["title"].as_str().unwrap_or_default().into(),
                artist: r["artist-credit"]
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .map(|v| {
                                format!(
                                    "{}{}",
                                    v["name"].as_str().unwrap_or_default(),
                                    v["joinphrase"].as_str().unwrap_or_default()
                                )
                            })
                            .collect::<String>()
                    })
                    .unwrap_or_default(),
                album: release
                    .and_then(|v| v["title"].as_str())
                    .unwrap_or_default()
                    .into(),
                year: release
                    .and_then(|v| v["date"].as_str())
                    .and_then(|s| s.get(..4))
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(0),
                release_id: release.and_then(|v| v["id"].as_str()).map(str::to_owned),
                score: r["score"]
                    .as_u64()
                    .or_else(|| r["score"].as_str().and_then(|s| s.parse().ok()))
                    .unwrap_or(0),
            }
        })
        .collect())
}
pub fn cover(release_id: &str) -> Result<Vec<u8>> {
    anyhow::ensure!(
        release_id.len() == 36
            && release_id
                .bytes()
                .all(|c| c.is_ascii_hexdigit() || c == b'-'),
        crate::i18n::t("text.invalid_musicbrainz_id")
    );
    let mut response = agent()
        .get(format!(
            "https://coverartarchive.org/release/{release_id}/front-500"
        ))
        .call()
        .context(crate::i18n::t("text.no_cover_found_for_this_release"))?;
    let bytes = response
        .body_mut()
        .with_config()
        .limit(10 * 1024 * 1024)
        .read_to_vec()?;
    image::load_from_memory(&bytes).context(crate::i18n::t("text.the_server_returned_an_unsupported_cover_image"))?;
    Ok(bytes)
}
