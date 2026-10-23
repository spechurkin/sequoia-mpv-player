use anyhow::{Context, Result};
use std::{
    ffi::OsString,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

// Keep the lock file in place: deleting it can let two processes lock different files.
pub struct Instance {
    _lock: File,
    inbox: PathBuf,
}

#[cfg(windows)]
fn lock(path: &Path) -> io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .share_mode(1) // FILE_SHARE_READ permits reading the owner PID only.
        .open(path)
}

#[cfg(unix)]
fn lock(path: &Path) -> io::Result<File> {
    use std::os::fd::AsRawFd;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)?;
    if unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(file)
}

fn contended(error: &io::Error) -> bool {
    #[cfg(windows)]
    {
        error.raw_os_error() == Some(32)
    }
    #[cfg(unix)]
    {
        error.kind() == io::ErrorKind::WouldBlock
    }
}

// Native code units preserve filenames that are not valid Unicode.
#[cfg(windows)]
type NativeString = Vec<u16>;
#[cfg(unix)]
type NativeString = Vec<u8>;

fn encode(path: &Path) -> NativeString {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str().encode_wide().collect()
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
}

fn decode(value: NativeString) -> PathBuf {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&value).into()
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(value).into()
    }
}

pub fn launch_paths() -> Vec<PathBuf> {
    std::env::args_os()
        .skip(1)
        .filter(|arg| !arg.to_string_lossy().starts_with("--"))
        .map(PathBuf::from)
        .collect()
}

impl Instance {
    /// None means that the launch request was delivered to the running instance.
    pub fn start(root: &Path, paths: Vec<PathBuf>) -> Result<Option<Self>> {
        fs::create_dir_all(root)?;
        let lock_path = root.join("instance.lock");
        let inbox = root.join("launch-requests");
        let mut owner = match lock(&lock_path) {
            Ok(owner) => owner,
            Err(error) if contended(&error) => {
                fs::create_dir_all(&inbox)?;
                #[cfg(windows)]
                if let Ok(pid) = fs::read_to_string(&lock_path)
                    .unwrap_or_default()
                    .parse::<u32>()
                {
                    unsafe {
                        let _ =
                            windows::Win32::UI::WindowsAndMessaging::AllowSetForegroundWindow(pid);
                    }
                }
                let cwd = std::env::current_dir()?;
                let paths: Vec<_> = paths
                    .iter()
                    .map(|path| {
                        if path.is_absolute()
                            || crate::playback::web::is_url(&path.to_string_lossy())
                        {
                            encode(path)
                        } else {
                            encode(&cwd.join(path))
                        }
                    })
                    .collect();
                let name = format!("{}-{:016x}", std::process::id(), rand::random::<u64>());
                let temporary = inbox.join(format!("{name}.tmp"));
                let mut request = OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&temporary)?;
                request.write_all(&serde_json::to_vec(&paths)?)?;
                request.sync_all()?;
                drop(request);
                fs::rename(&temporary, inbox.join(format!("{name}.json")))
                    .context(crate::i18n::t("text.failed_to_forward_launch_to_the_running_sequoia"))?;
                return Ok(None);
            }
            Err(error) => return Err(error).context(crate::i18n::t("text.failed_to_lock_the_sequoia_profile")),
        };
        owner.set_len(0)?;
        write!(owner, "{}", std::process::id())?;
        owner.flush()?;
        fs::create_dir_all(&inbox)?;
        Ok(Some(Self {
            _lock: owner,
            inbox,
        }))
    }

    pub fn requests(&self) -> Result<Vec<Vec<PathBuf>>> {
        let mut requests = Vec::new();
        for entry in fs::read_dir(&self.inbox)? {
            let path = entry?.path();
            if path.extension().is_none_or(|extension| extension != "json") {
                continue;
            }
            let result = fs::read(&path)
                .map_err(anyhow::Error::from)
                .and_then(|data| Ok(serde_json::from_slice::<Vec<NativeString>>(&data)?));
            fs::remove_file(&path)?;
            requests.push(result?.into_iter().map(decode).collect());
        }
        Ok(requests)
    }
}
