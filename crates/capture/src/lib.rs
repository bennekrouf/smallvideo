//! Screen recording.
//!
//! A dedicated thread owns the capture session, like splitter's playback engine: the UI sends
//! it commands and polls for events, and never blocks on ScreenCaptureKit (asking for content
//! can wait on a permission prompt; stopping waits for the file to be finalized).
//!
//! A take is a folder holding the screen recording (cursor hidden, microphone included) and a
//! log of the pointer's movements and clicks, so the cursor can be redrawn later.
//!
//! Backends: ScreenCaptureKit on macOS (`mac`), Windows.Graphics.Capture on Windows (`win`).
//! Elsewhere starting a recording fails with a message.

use crossbeam_channel::{Receiver, Sender};
use std::path::PathBuf;

#[cfg(target_os = "macos")]
mod mac;
#[cfg(target_os = "macos")]
use mac::Active;
#[cfg_attr(not(windows), allow(dead_code))]
mod pcm;
#[cfg(any(target_os = "macos", windows))]
mod pointer;
#[cfg(windows)]
mod win;
#[cfg(windows)]
use win::Active;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Codec {
    H264,
    /// About half the size of H.264 for screen content, hardware encoded on Apple silicon.
    Hevc,
}

#[derive(Clone, Debug)]
pub struct Options {
    pub fps: u32,
    pub codec: Codec,
    /// Record at most this many pixels wide (the display's full resolution if smaller).
    pub max_width: Option<u32>,
    pub microphone: bool,
    /// Sound played by apps on the Mac.
    pub system_audio: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self { fps: 60, codec: Codec::H264, max_width: None, microphone: true, system_audio: false }
    }
}

/// A finished recording.
#[derive(Clone, Debug)]
pub struct Take {
    pub dir: PathBuf,
    pub duration: f64,
    pub width: u32,
    pub height: u32,
    /// The file was written at a fixed, high bitrate and is worth re-encoding smaller.
    pub compact: bool,
}

#[derive(Debug)]
pub enum Event {
    /// Frames are being written.
    Started,
    Finished(Take),
    Failed(String),
}

enum Cmd {
    Start { dir: PathBuf, options: Options },
    Stop,
}

#[derive(Clone)]
pub struct Recorder {
    tx: Sender<Cmd>,
    events: Receiver<Event>,
}

impl Recorder {
    pub fn spawn() -> Self {
        let (tx, rx) = crossbeam_channel::unbounded();
        let (etx, events) = crossbeam_channel::unbounded();
        std::thread::Builder::new()
            .name("small-video-capture".into())
            .spawn(move || run(rx, etx))
            .expect("spawn capture thread");
        Self { tx, events }
    }

    /// Records into `dir`, which must exist. Ignored while a recording is running.
    pub fn start(&self, dir: PathBuf, options: Options) {
        let _ = self.tx.send(Cmd::Start { dir, options });
    }

    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }

    pub fn try_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }
}

#[cfg(any(target_os = "macos", windows))]
fn run(rx: Receiver<Cmd>, events: Sender<Event>) {
    let mut active = None;
    for cmd in rx {
        match cmd {
            Cmd::Start { dir, options } if active.is_none() => match Active::start(dir, &options, events.clone()) {
                Ok(a) => active = Some(a),
                Err(e) => {
                    let _ = events.send(Event::Failed(format!("{e:#}")));
                }
            },
            Cmd::Start { .. } => {}
            Cmd::Stop => {
                if let Some(a) = active.take() {
                    let _ = events.send(match a.finish() {
                        Ok(take) => Event::Finished(take),
                        Err(e) => Event::Failed(format!("{e:#}")),
                    });
                }
            }
        }
    }
}

#[cfg(not(any(target_os = "macos", windows)))]
fn run(rx: Receiver<Cmd>, events: Sender<Event>) {
    for cmd in rx {
        if let Cmd::Start { .. } = cmd {
            let _ = events.send(Event::Failed("Recording is only available on macOS and Windows for now.".into()));
        }
    }
}
