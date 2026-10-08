//! Shrinks a fresh recording. ScreenCaptureKit writes its file at a fixed, high bitrate (about
//! 100–500 MB a minute on a Retina display); re-encoding it with the hardware HEVC encoder at
//! constant quality makes it about 5× smaller without visible loss on text, and a still screen
//! costs next to nothing.

use crate::{ensure_ffmpeg, tools};
use anyhow::{bail, Context, Result};
use std::path::Path;

/// VideoToolbox constant quality (1–100). 50 keeps small text crisp; 40 already smears it.
const QUALITY: &str = "50";

/// Re-encodes `file` in place, keeping its sound and timestamps. Leaves it untouched if the
/// result isn't smaller or anything fails. Returns the new size in bytes.
pub fn compact(file: &Path) -> Result<u64> {
    let ffmpeg = ensure_ffmpeg(&mut |_| {}, &|| false)?;
    let before = std::fs::metadata(file)?.len();
    let tmp = file.with_file_name(".compacting.mov");
    let video: &[&str] = if cfg!(target_os = "macos") {
        &["-c:v", "hevc_videotoolbox", "-q:v", QUALITY, "-tag:v", "hvc1"]
    } else {
        &["-c:v", "libx265", "-crf", "24", "-tag:v", "hvc1"]
    };
    let out = tools::command(&ffmpeg)
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(file)
        // Keep every stream and the variable frame rate (the screen only sends changed frames).
        .args(["-map", "0", "-fps_mode", "passthrough"])
        .args(video)
        .args(["-c:a", "copy"])
        .arg(&tmp)
        .output()
        .context("Starting ffmpeg")?;
    if !out.status.success() {
        let _ = std::fs::remove_file(&tmp);
        bail!("Compacting failed: {}", String::from_utf8_lossy(&out.stderr).trim());
    }
    let after = std::fs::metadata(&tmp)?.len();
    if after == 0 || after >= before {
        let _ = std::fs::remove_file(&tmp);
        return Ok(before);
    }
    std::fs::rename(&tmp, file)?;
    Ok(after)
}
