use std::path::PathBuf;

/// Tauri copies external binaries next to the app executable with the
/// target-triple suffix stripped, in both dev (target/debug/) and the
/// installed app, so ffmpeg always sits beside the running binary.
pub fn ffmpeg_path() -> Result<PathBuf, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let dir = exe.parent().ok_or("executable has no parent directory")?;
    let name = if cfg!(windows) { "ffmpeg.exe" } else { "ffmpeg" };
    let path = dir.join(name);
    if !path.exists() {
        return Err(format!("bundled ffmpeg not found at {}", path.display()));
    }
    Ok(path)
}
