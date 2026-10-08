//! The recording's sound: its loudness over time (for blank cutting), and the ffmpeg filter
//! that leaves the cut stretches out of it at export.

use crate::tools;
use anyhow::{bail, Context, Result};
use small_video_core::{Loudness, Timeline};
use std::io::Read;
use std::path::Path;
use std::process::Stdio;

/// Splitter's loudness window: 50 ms.
const WINDOW_SECS: f64 = 0.05;
const RATE: u32 = 16_000;

/// The sound, timed from the start of the recording: the microphone may start a moment after
/// the picture, and the gap is filled with silence so times line up with the video.
const ALIGN: &str = "aresample=async=1:first_pts=0";

/// Whether `file` has a sound track.
pub fn has_audio(ffmpeg: &Path, file: &Path) -> Result<bool> {
    let out = tools::command(ffmpeg).args(["-hide_banner", "-nostdin", "-i"]).arg(file).output()?;
    // ffmpeg describes the input on stderr (and complains that no output was given).
    Ok(String::from_utf8_lossy(&out.stderr).lines().any(|l| l.contains("Stream #") && l.contains("Audio:")))
}

/// The loudness of `file`'s sound in 50 ms windows; `None` if it has no sound.
pub fn loudness(ffmpeg: &Path, file: &Path) -> Result<Option<Loudness>> {
    if !has_audio(ffmpeg, file)? {
        return Ok(None);
    }
    let mut child = tools::command(ffmpeg)
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(file)
        .args(["-map", "0:a:0", "-af", ALIGN, "-ac", "1", "-ar", &RATE.to_string(), "-f", "f32le", "-"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("Starting ffmpeg")?;
    let mut bytes = Vec::new();
    child.stdout.take().context("ffmpeg output")?.read_to_end(&mut bytes)?;
    let mut errors = String::new();
    if let Some(mut e) = child.stderr.take() {
        let _ = e.read_to_string(&mut errors);
    }
    if !child.wait()?.success() {
        bail!("Reading the sound failed: {}", errors.trim());
    }
    let per_window = (RATE as f64 * WINDOW_SECS) as usize;
    let samples: Vec<f32> = bytes.as_chunks::<4>().0.iter().map(|b| f32::from_le_bytes(*b)).collect();
    let db = samples
        .chunks(per_window)
        .map(|w| {
            let rms = (w.iter().map(|s| (*s as f64).powi(2)).sum::<f64>() / w.len() as f64).sqrt();
            (20.0 * rms.max(1e-9).log10()) as f32
        })
        .collect();
    Ok(Some(Loudness { window: WINDOW_SECS, db }))
}

/// Seconds faded out and in at each cut, so a cut never clicks.
const FADE_SECS: f64 = 0.01;

/// An ffmpeg filter graph taking input 1's sound and producing `[aout]`: only the timeline's
/// parts, joined, each faded in and out over a few milliseconds.
pub fn cut_filter(timeline: &Timeline) -> String {
    let n = timeline.parts.len();
    let mut graph = format!("[1:a]{ALIGN},asplit={n}");
    for i in 0..n {
        graph += &format!("[s{i}]");
    }
    for (i, &(start, end)) in timeline.parts.iter().enumerate() {
        let fade = FADE_SECS.min((end - start) / 2.0);
        graph += &format!(
            ";[s{i}]atrim=start={start:.6}:end={end:.6},asetpts=PTS-STARTPTS,\
             afade=t=in:d={fade:.6},afade=t=out:st={:.6}:d={fade:.6}[a{i}]",
            (end - start - fade).max(0.0)
        );
    }
    graph += ";";
    for i in 0..n {
        graph += &format!("[a{i}]");
    }
    graph + &format!("concat=n={n}:v=0:a=1[aout]")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_filter_keeps_each_part() {
        let tl = Timeline { parts: vec![(1.0, 4.0), (6.0, 10.0)] };
        let f = cut_filter(&tl);
        assert!(f.starts_with("[1:a]aresample=async=1:first_pts=0,asplit=2[s0][s1];"));
        assert!(f.contains("[s0]atrim=start=1.000000:end=4.000000,asetpts=PTS-STARTPTS"));
        assert!(f.contains("afade=t=out:st=2.990000:d=0.010000[a0]"));
        assert!(f.ends_with("[a0][a1]concat=n=2:v=0:a=1[aout]"));
    }
}
