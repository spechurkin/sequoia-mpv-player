use crate::model::Media;
use anyhow::{bail, Context, Result};
use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub fn is_url(value: &str) -> bool {
    let value = value.trim();
    (value.starts_with("https://") || value.starts_with("http://"))
        && value
        .split_once("://")
        .is_some_and(|(_, rest)| !rest.is_empty() && !rest.starts_with('/'))
        && !value.chars().any(char::is_whitespace)
}

pub fn resolve_tool(name: &str) -> Option<PathBuf> {
    let filename = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.into()
    };
    let mut paths = Vec::new();
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            paths.extend([dir.join("yt-dlp").join(&filename), dir.join(&filename)]);
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path).map(|dir| dir.join(&filename)));
    }
    paths.into_iter().find(|p| p.is_file())
}

pub fn configure_tools(command: &mut Command) {
    let mut paths = Vec::new();
    for name in ["yt-dlp", "deno"] {
        if let Some(tool) = resolve_tool(name) {
            if let Some(parent) = tool.parent() {
                paths.push(parent.to_path_buf());
            }
        }
    }
    if let Some(path) = std::env::var_os("PATH") {
        paths.extend(std::env::split_paths(&path));
    }
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
}

pub fn extract(url: &str) -> Result<Vec<Media>> {
    extract_cancellable(url, &std::sync::atomic::AtomicBool::new(false))
}

pub fn extract_cancellable(
    url: &str,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Vec<Media>> {
    if !is_url(url) {
        bail!(crate::i18n::t("text.enter_an_http_or_https_url"));
    }
    let tool = resolve_tool("yt-dlp")
        .context(crate::i18n::t("text.yt_dlp_not_found_place_it_in_the_yt_dlp_folder_next_to_the_player_or_in_pat"))?;
    extract_with_cancel(&tool, url, cancelled)
}

pub fn extract_with(tool: &Path, url: &str) -> Result<Vec<Media>> {
    extract_with_cancel(tool, url, &std::sync::atomic::AtomicBool::new(false))
}

fn extract_with_cancel(
    tool: &Path,
    url: &str,
    cancelled: &std::sync::atomic::AtomicBool,
) -> Result<Vec<Media>> {
    let mut command = Command::new(tool);
    configure_tools(&mut command);
    command
        .args([
            "--ignore-config",
            "--no-update",
            "--flat-playlist",
            "--dump-single-json",
            "--yes-playlist",
            "--ignore-errors",
            "--skip-download",
            "--socket-timeout",
            "15",
            "--",
            url,
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let mut child = command.spawn().context(crate::i18n::t("text.failed_to_start_yt_dlp"))?;
    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let read = |mut stream: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).map(|_| bytes)
        })
    };
    let out = read(Box::new(stdout));
    let err = read(Box::new(stderr));
    let deadline = Instant::now() + Duration::from_secs(120);
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if cancelled.load(std::sync::atomic::Ordering::Relaxed) || Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(crate::i18n::t("text.yt_dlp_cancelled_or_timed_out"));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let output = out
        .join()
        .map_err(|_| anyhow::anyhow!(crate::i18n::t("text.failed_to_read_yt_dlp_output")))??;
    let error = err
        .join()
        .map_err(|_| anyhow::anyhow!(crate::i18n::t("text.failed_to_read_yt_dlp_output")))??;
    if output.is_empty() || !status.success() && output.first() != Some(&b'{') {
        bail!(
            "yt-dlp: {}",
            String::from_utf8_lossy(&error)
                .chars()
                .take(2000)
                .collect::<String>()
        );
    }
    parse(
        &serde_json::from_slice(&output).context(crate::i18n::t("text.invalid_yt_dlp_response"))?,
        url,
    )
}

pub fn parse(value: &Value, fallback: &str) -> Result<Vec<Media>> {
    fn visit(value: &Value, fallback: &str, items: &mut Vec<Media>) {
        if let Some(entries) = value["entries"].as_array() {
            for entry in entries {
                visit(entry, "", items);
            }
            return;
        }
        if value.is_null()
            || matches!(
                value["availability"].as_str(),
                Some("private" | "premium_only" | "subscriber_only")
            )
        {
            return;
        }
        let url = [
            value["webpage_url"].as_str(),
            value["original_url"].as_str(),
            value["url"].as_str(),
            Some(fallback),
        ]
            .into_iter()
            .flatten()
            .find(|s| is_url(s));
        if let Some(url) = url {
            items.push(Media {
                path: url.into(),
                title: value["title"].as_str().unwrap_or(url).into(),
                artist: value["uploader"].as_str().unwrap_or_default().into(),
                duration: value["duration"].as_f64().unwrap_or_default(),
                ..Default::default()
            });
        }
    }
    let mut items = Vec::new();
    visit(value, fallback, &mut items);
    if items.is_empty() {
        bail!(crate::i18n::t("text.no_available_video_or_audio_at_this_url"));
    }
    Ok(items)
}

pub fn shortcut(path: &Path) -> Option<String> {
    if !path.extension()?.eq_ignore_ascii_case("url") {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let text = if bytes.starts_with(&[0xff, 0xfe]) {
        String::from_utf16_lossy(
            &bytes[2..]
                .chunks_exact(2)
                .map(|c| u16::from_le_bytes([c[0], c[1]]))
                .collect::<Vec<_>>(),
        )
    } else {
        String::from_utf8_lossy(&bytes).into_owned()
    };
    text.lines().find_map(|line| {
        line.trim()
            .strip_prefix("URL=")
            .filter(|url| is_url(url))
            .map(str::to_owned)
    })
}
