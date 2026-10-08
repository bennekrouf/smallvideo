//! ScreenCaptureKit session: the display the pointer is on, written straight to a file by
//! `SCRecordingOutput` (hardware encoded, macOS 15+).

use crate::pointer::{self, Logger};
use crate::{Codec, Event, Options, Take};
use anyhow::{anyhow, bail, Context, Result};
use crossbeam_channel::Sender;
use screencapturekit::prelude::*;
use screencapturekit::recording_output::{
    RecordingCallbacks, SCRecordingOutput, SCRecordingOutputCodec, SCRecordingOutputConfiguration,
    SCRecordingOutputFileType,
};
use small_video_core::take;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Shared {
    /// When the first frame was written: time zero for the pointer log.
    started: Mutex<Option<Instant>>,
    failed: Mutex<Option<String>>,
    /// Set when the file is complete or the recording failed.
    done: Mutex<bool>,
    done_cv: Condvar,
}

impl Shared {
    fn set_done(&self) {
        *self.done.lock().unwrap() = true;
        self.done_cv.notify_all();
    }

    /// Whether the recording ended within `timeout`.
    fn wait_done(&self, timeout: Duration) -> bool {
        let done = self.done.lock().unwrap();
        *self.done_cv.wait_timeout_while(done, timeout, |d| !*d).unwrap().0
    }
}

pub struct Active {
    stream: SCStream,
    output: SCRecordingOutput,
    pointer: Logger,
    shared: Arc<Shared>,
    dir: PathBuf,
    /// The display's area in global points, to normalize pointer positions.
    frame: CGRect,
    width: u32,
    height: u32,
}

fn sc<E: std::fmt::Debug>(what: &'static str) -> impl FnOnce(E) -> anyhow::Error {
    move |e| anyhow!("{what}: {e:?}")
}

impl Active {
    pub fn start(dir: PathBuf, options: &Options, events: Sender<Event>) -> Result<Self> {
        pointer::prepare_thread();
        let content = SCShareableContent::get().map_err(sc(
            "Can't list the screens. Allow Small Video in System Settings › Privacy & Security › Screen Recording",
        ))?;
        let displays = content.displays();
        let at = pointer::location();
        let contains = |d: &SCDisplay| {
            let f = d.frame();
            at.is_some_and(|(x, y)| {
                x >= f.origin.x && x < f.origin.x + f.size.width && y >= f.origin.y && y < f.origin.y + f.size.height
            })
        };
        let display = displays.iter().find(|d| contains(d)).or(displays.first()).context("No display to record")?;

        // Leave Small Video's own windows out of the recording.
        let me = std::process::id() as i32;
        let own: Vec<_> = content.applications().into_iter().filter(|a| a.process_id() == me).collect();
        let own: Vec<&_> = own.iter().collect();
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_applications(&own, &[])
            .build()
            .map_err(sc("Content filter"))?;

        let scale = filter.point_pixel_scale().max(1.0);
        let frame = display.frame();
        let full = frame.size.width * scale as f64;
        let px = options.max_width.map_or(full, |m| full.min(m as f64)) / frame.size.width;
        let width = (frame.size.width * px).round() as u32 / 2 * 2;
        let height = (frame.size.height * px).round() as u32 / 2 * 2;
        let config = SCStreamConfiguration::new()
            .with_width(width)
            .with_height(height)
            .with_fps(options.fps)
            .with_shows_cursor(false)
            .with_captures_audio(options.system_audio)
            .with_captures_microphone(options.microphone)
            .map_err(sc("Microphone"))?;

        let shared = Arc::new(Shared::default());
        let callbacks = {
            let (on_start, on_fail, on_finish) = (shared.clone(), shared.clone(), shared.clone());
            RecordingCallbacks::new()
                .on_start(move || {
                    *on_start.started.lock().unwrap() = Some(Instant::now());
                    let _ = events.send(Event::Started);
                })
                .on_fail(move |e| {
                    *on_fail.failed.lock().unwrap() = Some(e);
                    on_fail.set_done();
                })
                .on_finish(move || on_finish.set_done())
        };
        let out_config = SCRecordingOutputConfiguration::new()
            .map_err(sc("Recording output"))?
            .with_output_url(&dir.join(take::SCREEN))?
            .with_video_codec(match options.codec {
                Codec::H264 => SCRecordingOutputCodec::H264,
                Codec::Hevc => SCRecordingOutputCodec::HEVC,
            })
            .with_output_file_type(SCRecordingOutputFileType::MOV);
        let output = SCRecordingOutput::new_with_delegate(&out_config, callbacks)
            .context("Recording to a file needs macOS 15 or later")?;

        let stream = SCStream::new(&filter, &config).map_err(sc("Stream"))?;
        stream.add_recording_output(&output).map_err(sc("Recording output"))?;
        let pointer = Logger::start();
        stream.start_capture().map_err(sc("Starting the capture"))?;
        Ok(Self { stream, output, pointer, shared, dir, frame, width, height })
    }

    pub fn finish(self) -> Result<Take> {
        // Removing the only output stops the capture and waits until the file is complete.
        let removed = self.stream.remove_recording_output(&self.output).map_err(sc("Stopping"));
        let _ = self.stream.stop_capture();
        let raw = self.pointer.finish();
        removed?;
        // The file is finalized after the stream stops; its delegate says when.
        let ended = self.shared.wait_done(Duration::from_secs(10));
        if let Some(e) = self.shared.failed.lock().unwrap().take() {
            bail!("Recording failed: {e}");
        }
        let file = self.dir.join(take::SCREEN);
        if !file.is_file() {
            bail!(
                "The recording wasn't saved ({}).",
                if ended { "ScreenCaptureKit wrote no file" } else { "it never finished" }
            );
        }
        let started = self.shared.started.lock().unwrap().context("The recording never started")?;
        let d = self.output.recorded_duration();
        let duration = if d.timescale > 0 { d.value as f64 / d.timescale as f64 } else { 0.0 };

        let f = self.frame;
        let log = pointer::event_log(&raw, started, (f.origin.x, f.origin.y), (f.size.width, f.size.height));
        std::fs::write(self.dir.join(take::EVENTS), serde_json::to_vec(&log)?)?;
        // ScreenCaptureKit's file output has no bitrate setting: worth shrinking afterwards.
        Ok(Take { dir: self.dir, duration, width: self.width, height: self.height, compact: true })
    }
}
