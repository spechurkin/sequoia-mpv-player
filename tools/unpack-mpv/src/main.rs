use std::{error::Error, path::PathBuf, process::ExitCode};

fn unpack() -> Result<(), Box<dyn Error>> {
    let mut args = std::env::args_os().skip(1);
    let archive = args
        .next()
        .map(PathBuf::from)
        .ok_or("Missing MPV archive")?;
    let destination = args
        .next()
        .map(PathBuf::from)
        .ok_or("Missing output directory")?;
    if args.next().is_some() {
        return Err("Usage: sequoia-unpack-mpv <archive.7z> <empty-output-directory>".into());
    }
    if !archive.is_file() {
        return Err(format!("MPV archive does not exist: {}", archive.display()).into());
    }
    if destination.exists() && destination.read_dir()?.next().is_some() {
        return Err("MPV output directory must be empty".into());
    }
    sevenz_rust2::decompress_file(&archive, &destination)?;
    if !destination.join("mpv.exe").is_file() {
        return Err("Archive does not contain mpv.exe".into());
    }
    println!("MPV extracted with Rust: {}", destination.display());
    Ok(())
}

fn main() -> ExitCode {
    match unpack() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("MPV extraction failed: {error}");
            ExitCode::FAILURE
        }
    }
}
