//! End to end: a generated take through ffmpeg, the compositor and back. Skipped (passes) when
//! ffmpeg isn't installed.

use small_video_core::{take, Button, Click, CursorSample, EventLog, Project};
use small_video_export::{export, ffmpeg, Progress, Settings};
use std::path::Path;
use std::process::Command;
use std::sync::atomic::AtomicBool;

fn make_take(dir: &Path, ffmpeg: &Path) {
    let status = Command::new(ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=320x200:rate=30:duration=2"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=2"])
        .args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(dir.join(take::SCREEN))
        .status()
        .unwrap();
    assert!(status.success());
    let events = EventLog {
        cursor: (0..60).map(|i| CursorSample { t: i as f64 / 30.0, x: 0.2 + i as f32 / 100.0, y: 0.5 }).collect(),
        clicks: vec![Click { t: 1.0, x: 0.5, y: 0.5, button: Button::Left }],
    };
    std::fs::write(dir.join(take::EVENTS), serde_json::to_vec(&events).unwrap()).unwrap();
    Project::new(2.0, 320, 200, &events).save(&dir.join(take::PROJECT)).unwrap();
}

fn probe(ffprobe: &Path, file: &Path, entries: &str) -> String {
    let out = Command::new(ffprobe)
        .args(["-v", "error", "-count_frames", "-show_entries", entries, "-of", "csv=p=0"])
        .arg(file)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

#[test]
fn exports_video_with_sound() {
    let Some(ffmpeg) = ffmpeg() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let ffprobe = ffmpeg.with_file_name("ffprobe");
    let dir = std::env::temp_dir().join(format!("small-video-export-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    make_take(&dir, &ffmpeg);

    let out = dir.join("out.mp4");
    let progress = Progress::default();
    export(&dir, &out, Settings { long_side: 640, fps: 30 }, &progress, &AtomicBool::new(false)).unwrap();
    assert_eq!(progress.fraction(), 1.0);

    if ffprobe.is_file() {
        let video = probe(&ffprobe, &out, "stream=codec_name,width,height,nb_read_frames");
        assert!(video.starts_with("h264,640,400,"), "{video}");
        let frames: u32 = video.lines().next().unwrap().rsplit(',').next().unwrap().parse().unwrap();
        assert!((59..=61).contains(&frames), "{frames} frames");
        assert!(video.contains("aac"), "{video}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn cancelling_leaves_no_file() {
    let Some(ffmpeg) = ffmpeg() else {
        return;
    };
    let dir = std::env::temp_dir().join(format!("small-video-cancel-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    make_take(&dir, &ffmpeg);
    let out = dir.join("out.mp4");
    let err = export(&dir, &out, Settings::default(), &Progress::default(), &AtomicBool::new(true)).unwrap_err();
    assert!(err.is::<small_video_export::Cancelled>());
    assert!(!out.exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn compacting_shrinks_and_keeps_sound_and_length() {
    let Some(ffmpeg) = ffmpeg() else {
        return;
    };
    let ffprobe = ffmpeg.with_file_name("ffprobe");
    let dir = std::env::temp_dir().join(format!("small-video-compact-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    // A wasteful recording, like ScreenCaptureKit's: a near-still picture at a high bitrate.
    let file = dir.join(take::SCREEN);
    let status = Command::new(&ffmpeg)
        .args(["-v", "error", "-y", "-f", "lavfi", "-i", "testsrc2=size=640x400:rate=30:duration=3"])
        .args(["-f", "lavfi", "-i", "sine=frequency=440:duration=3"])
        .args(["-c:v", "libx264", "-b:v", "20M", "-pix_fmt", "yuv420p", "-c:a", "aac", "-shortest"])
        .arg(&file)
        .status()
        .unwrap();
    assert!(status.success());
    let before = std::fs::metadata(&file).unwrap().len();

    let after = small_video_export::compact::compact(&file).unwrap();
    assert!(after < before / 2, "{before} → {after}");
    assert_eq!(std::fs::metadata(&file).unwrap().len(), after);
    if ffprobe.is_file() {
        let streams = probe(&ffprobe, &file, "stream=codec_name");
        assert_eq!(streams.lines().collect::<Vec<_>>(), ["hevc", "aac"]);
        let secs: f64 = probe(&ffprobe, &file, "format=duration").parse().unwrap();
        assert!((secs - 3.0).abs() < 0.1, "{secs}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
