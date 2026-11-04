use anyhow::{Context, Result};
use serde_json::{json, Value};
#[cfg(windows)]
use std::fs::{File, OpenOptions};
use std::{
    collections::HashMap,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Mutex,
    },
    thread,
    time::{Duration, Instant},
};
#[cfg(windows)]
type IpcStream = File;
#[cfg(unix)]
type IpcStream = std::os::unix::net::UnixStream;

#[derive(Debug)]
pub enum Event {
    Property(String, Value),
    Osd(String),
    Eof,
    Error(String),
    Disconnected,
}

#[derive(Default)]
struct Completion {
    reported: bool,
}
impl Completion {
    fn command(&mut self, command: &Value) {
        let replaces = command[0] == "loadfile"
            && (command[2].is_null() || command[2] == "replace");
        if replaces || command[0] == "stop" || command[0] == "quit" {
            self.reported = true;
        }
    }

    fn observe(&mut self, message: &Value) -> bool {
        if message["event"] == "start-file" {
            self.reported = false;
        }
        // mpv 0.27/0.32 omit the reason from IPC end-file events. App-issued
        // stops and replacements suppress completion until the next start-file.
        let ended = (message["event"] == "end-file"
            && (message["reason"] == "eof" || message.get("reason").is_none()))
            || (message["event"] == "property-change"
                && message["name"] == "eof-reached"
                && message["data"] == true);
        if ended && !self.reported {
            self.reported = true;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod completion_tests {
    use super::*;

    #[test]
    fn held_open_eof_and_end_file_report_once_in_either_order() {
        let property = json!({"event":"property-change", "name":"eof-reached", "data":true});
        let end = json!({"event":"end-file", "reason":"eof"});
        for messages in [[&property, &end], [&end, &property]] {
            let mut completion = Completion::default();
            assert!(completion.observe(messages[0]));
            assert!(!completion.observe(messages[1]));
            assert!(!completion.observe(&property));
            assert!(!completion.observe(&json!({"event":"start-file"})));
            assert!(completion.observe(&property));
        }
    }

    #[test]
    fn pause_stop_and_errors_are_not_completion() {
        let mut completion = Completion::default();
        for message in [
            json!({"event":"property-change", "name":"pause", "data":true}),
            json!({"event":"property-change", "name":"eof-reached", "data":false}),
            json!({"event":"end-file", "reason":"stop"}),
            json!({"event":"end-file", "reason":"error"}),
        ] {
            assert!(!completion.observe(&message));
        }
    }

    #[test]
    fn legacy_end_file_advances_once_and_suppresses_requested_stops() {
        let end = json!({"event":"end-file"});
        let start = json!({"event":"start-file"});
        for command in [json!(["stop"]), json!(["quit"]),
            json!(["loadfile", "next.wav"]), json!(["loadfile", "next.wav", "replace"])] {
            let mut completion = Completion::default();
            completion.observe(&start);
            completion.command(&command);
            assert!(!completion.observe(&end));
            completion.observe(&start);
            assert!(completion.observe(&end));
            assert!(!completion.observe(&end));
        }
    }

    #[test]
    fn appending_and_seeking_do_not_suppress_legacy_completion() {
        for command in [json!(["loadfile", "next.wav", "append"]),
            json!(["loadfile", "next.wav", "append-play"]),
            json!(["seek", 10, "absolute+exact"]), json!(["set_property", "pause", true])] {
            let mut completion = Completion::default();
            completion.command(&command);
            assert!(completion.observe(&json!({"event":"end-file"})));
        }
    }
}
type Pending = Arc<Mutex<HashMap<u64, mpsc::Sender<std::result::Result<Value, String>>>>>;
pub struct Mpv {
    child: Child,
    writer: Arc<Mutex<IpcStream>>,
    pending: Pending,
    completion: Arc<Mutex<Completion>>,
    next_id: AtomicU64,
    pub events: mpsc::Receiver<Event>,
}
pub fn resolve(configured: &Path) -> Option<PathBuf> {
    let mut paths = vec![configured.to_path_buf()];
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let name = if cfg!(windows) { "mpv.exe" } else { "mpv" };
            paths.push(dir.join("mpv").join(name));
            paths.push(dir.join(name));
        }
    }
    #[cfg(windows)]
    paths.push(PathBuf::from("C:/Program Files/mpv/mpv.exe"));
    #[cfg(target_os = "macos")]
    paths.extend(["/opt/homebrew/bin/mpv", "/usr/local/bin/mpv"].map(PathBuf::from));
    if let Some(value) = std::env::var_os("PATH") {
        paths.extend(
            std::env::split_paths(&value)
                .map(|p| p.join(if cfg!(windows) { "mpv.exe" } else { "mpv" })),
        );
    }
    paths.into_iter().find(|p| p.is_file())
}
impl Mpv {
    pub fn start(
        executable: &Path,
        hwnd: Option<isize>,
        config: &Path,
        headless: bool,
    ) -> Result<Self> {
        std::fs::create_dir_all(config)?;
        super::input::ensure_config(config)?;
        #[cfg(windows)]
        let pipe = format!(
            r"\\.\pipe\sequoia-{}-{}",
            std::process::id(),
            crate::library::stamp()
        );
        #[cfg(unix)]
        let socket_dir = tempfile::Builder::new()
            .prefix("sequoia-")
            .tempdir_in("/tmp")?;
        #[cfg(unix)]
        let pipe = socket_dir
            .path()
            .join("mpv.sock")
            .to_string_lossy()
            .into_owned();
        let mut cmd = Command::new(executable);
        super::web::configure_tools(&mut cmd);
        if let Some(tool) = super::web::resolve_tool("yt-dlp") {
            let path = tool.to_string_lossy();
            // An absolute path prevents Windows from choosing an older yt-dlp
            // beside mpv.exe before consulting PATH. Append accepts one key/value.
            cmd.arg(format!("--script-opts-append=ytdl_hook-ytdl_path={path}"));
        }
        cmd.args([
            "--ytdl=yes",
            "--ytdl-raw-options=no-playlist=",
            "--idle=yes",
            "--keep-open=no",
            "--terminal=yes",
            "--input-terminal=no",
            "--video-osd=no",
            "--term-osd=force",
            "--term-osd-bar=no",
            "--term-playing-msg=",
            "--msg-color=no",
            "--msg-module=no",
            "--msg-time=no",
            "--msg-level=all=no,statusline=status",
            "--quiet=no",
            "--osc=no",
            "--input-default-bindings=no",
            "--input-vo-keyboard=no",
            "--input-cursor=no",
            "--hwdec=auto",
            "--volume-max=150",
        ])
            .arg(format!("--input-ipc-server={pipe}"))
            .arg(format!("--config-dir={}", config.display()))
            .arg(format!("--term-status-msg={}", super::osd::FRAME_END))
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(if headless {
                Stdio::inherit()
            } else {
                Stdio::null()
            });
        if headless {
            cmd.args(["--vo=null", "--ao=null", "--force-window=no"]);
        } else if let Some(hwnd) = hwnd {
            cmd.arg(format!("--wid={hwnd}")).arg("--force-window=yes");
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().context(crate::i18n::t("text.failed_to_start_mpv"))?;
        let (tx, events) = mpsc::channel();
        if let Some(stdout) = child.stdout.take() {
            let osd_tx = tx.clone();
            thread::spawn(move || super::osd::read_updates(stdout, |message| {
                let _ = osd_tx.send(Event::Osd(message));
            }));
        }
        let deadline = Instant::now() + Duration::from_secs(10);
        let file = loop {
            match connect_ipc(&pipe) {
                Ok(file) => break file,
                Err(e) => {
                    if Instant::now() > deadline || child.try_wait()?.is_some() {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(e)
                            .context(crate::i18n::t("text.mpv_did_not_create_an_ipc_channel_check_the_path_and_mpv_conf"));
                    }
                    thread::sleep(Duration::from_millis(60));
                }
            }
        };
        let mut reader = file.try_clone()?;
        #[cfg(unix)]
        reader.set_read_timeout(Some(Duration::from_millis(100)))?;
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let read_pending = pending.clone();
        let completion = Arc::new(Mutex::new(Completion::default()));
        let read_completion = completion.clone();
        thread::spawn(move || {
            #[cfg(unix)]
            let _socket_dir = socket_dir;
            let mut buffered = Vec::new();
            // Synchronous Windows pipe handles serialize I/O. Read only available bytes,
            // so the reader never blocks a concurrent command write on the duplicated handle.
            while let Ok(available) = pipe_available(&reader) {
                if available == 0 {
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                let mut bytes = vec![0u8; available.min(64 * 1024)];
                let count = match reader.read(&mut bytes) {
                    Ok(0) => break,
                    Err(e)
                    if matches!(
                            e.kind(),
                            std::io::ErrorKind::WouldBlock
                                | std::io::ErrorKind::TimedOut
                                | std::io::ErrorKind::Interrupted
                        ) =>
                        {
                            continue
                        }
                    Err(_) => break,
                    Ok(n) => n,
                };
                buffered.extend_from_slice(&bytes[..count]);
                while let Some(newline) = buffered.iter().position(|b| *b == b'\n') {
                    let line: Vec<_> = buffered.drain(..=newline).collect();
                    let line = String::from_utf8_lossy(&line);
                    let msg: Value = match serde_json::from_str(&line) {
                        Ok(msg) => msg,
                        Err(_) => continue,
                    };
                    if let Some(id) = msg["request_id"].as_u64() {
                        if let Some(reply) = read_pending.lock().unwrap().remove(&id) {
                            let _ = reply.send(if msg["error"] == "success" {
                                Ok(msg["data"].clone())
                            } else {
                                Err(format!("mpv: {}", msg["error"]))
                            });
                        } else if msg["error"].as_str().is_some_and(|e| e != "success") {
                            let _ = tx.send(Event::Error(crate::i18n::format("text.mpv_command_rejected", &[("arg0", format!("{}", msg["error"]))])));
                        }
                    }
                    if read_completion.lock().unwrap().observe(&msg) {
                        let _ = tx.send(Event::Eof);
                    }
                    match msg["event"].as_str() {
                        Some("property-change") => {
                            let _ = tx.send(Event::Property(
                                msg["name"].as_str().unwrap_or_default().into(),
                                msg["data"].clone(),
                            ));
                        }
                        Some("end-file") if msg["reason"] == "error" => {
                            let _ = tx.send(Event::Error(crate::i18n::format("text.failed_to_play_file", &[("arg0", format!("{}", msg["file_error"]))])));
                        }
                        _ => {}
                    }
                }
            }
            for (_, sender) in read_pending.lock().unwrap().drain() {
                let _ = sender.send(Err(crate::i18n::t("text.mpv_disconnected").into()));
            }
            let _ = tx.send(Event::Disconnected);
        });
        let engine = Self {
            child,
            writer: Arc::new(Mutex::new(file)),
            pending,
            completion,
            next_id: AtomicU64::new(1),
            events,
        };
        match engine.request(json!(["get_property", "input-bindings"])) {
            Ok(bindings) => {
                // Imported configs need the mouse default too, without overwriting an explicit binding.
                if !super::input::has_binding(&bindings, "MBTN_RIGHT") {
                    engine.request(json!(["keybind", "MBTN_RIGHT", "cycle pause"]))?;
                }
            }
            // mpv 0.27 reads input.conf but cannot report its bindings over IPC.
            Err(error) if error.to_string() == "mpv: \"property not found\"" => {}
            Err(error) => return Err(error),
        }
        for (id, property) in [
            "pause",
            "time-pos",
            "duration",
            "volume",
            "mute",
            "speed",
            "track-list",
            "chapter-list",
            "media-title",
            "eof-reached",
        ]
            .iter()
            .enumerate()
        {
            engine.command(json!(["observe_property", id + 1, property]))?;
        }
        Ok(engine)
    }
    pub fn command(&self, command: Value) -> Result<()> {
        anyhow::ensure!(command.is_array(), crate::i18n::t("text.the_command_must_be_a_json_array"));
        let msg =
            json!({"command": command, "request_id": self.next_id.fetch_add(1, Ordering::Relaxed)});
        let mut file = self.writer.lock().unwrap();
        self.completion.lock().unwrap().command(&msg["command"]);
        writeln!(file, "{msg}")?;
        file.flush()?;
        Ok(())
    }
    pub fn request(&self, command: Value) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = mpsc::channel();
        self.pending.lock().unwrap().insert(id, tx);
        let write = (|| -> Result<()> {
            let mut file = self.writer.lock().unwrap();
            self.completion.lock().unwrap().command(&command);
            writeln!(file, "{}", json!({"command":command,"request_id":id}))?;
            file.flush()?;
            Ok(())
        })();
        if let Err(e) = write {
            self.pending.lock().unwrap().remove(&id);
            return Err(e);
        }
        let result = rx.recv_timeout(Duration::from_secs(10));
        self.pending.lock().unwrap().remove(&id);
        result
            .context(crate::i18n::t("text.mpv_did_not_respond_within_10_seconds"))?
            .map_err(anyhow::Error::msg)
    }
}
#[cfg(windows)]
fn pipe_available(file: &File) -> std::io::Result<usize> {
    use std::os::windows::io::AsRawHandle;
    use windows::Win32::{Foundation::HANDLE, System::Pipes::PeekNamedPipe};
    let mut available = 0;
    unsafe {
        PeekNamedPipe(
            HANDLE(file.as_raw_handle()),
            None,
            0,
            None,
            Some(&mut available),
            None,
        )
    }
        .map_err(|_| std::io::Error::last_os_error())?;
    Ok(available as usize)
}
#[cfg(unix)]
fn pipe_available(_: &IpcStream) -> std::io::Result<usize> {
    Ok(64 * 1024)
}
#[cfg(windows)]
fn connect_ipc(path: &str) -> std::io::Result<IpcStream> {
    OpenOptions::new().read(true).write(true).open(path)
}
#[cfg(unix)]
fn connect_ipc(path: &str) -> std::io::Result<IpcStream> {
    IpcStream::connect(path)
}
impl Drop for Mpv {
    fn drop(&mut self) {
        let _ = self.command(json!(["quit"]));
        for _ in 0..20 {
            if self.child.try_wait().ok().flatten().is_some() {
                return;
            }
            thread::sleep(Duration::from_millis(25));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
