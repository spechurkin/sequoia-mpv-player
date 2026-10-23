use std::io::Read;

pub const FRAME_END: &str = "__SEQUOIA_OSD_END__";

// A terminal status suffix frames updates even when the message has no trailing newline.
pub fn read_updates(mut reader: impl Read, mut update: impl FnMut(String)) {
    let mut pending = Vec::new();
    let mut bytes = [0; 4096];
    loop {
        let count = match reader.read(&mut bytes) {
            Ok(0) => break,
            Ok(count) => count,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        };
        pending.extend_from_slice(&bytes[..count]);
        while let Some(end) = pending
            .windows(FRAME_END.len())
            .position(|s| s == FRAME_END.as_bytes())
        {
            let message = String::from_utf8_lossy(&pending[..end]).trim().to_string();
            pending.drain(..end + FRAME_END.len());
            update(message);
        }
        if pending.len() > 256 * 1024 {
            pending.clear();
        }
    }
}
