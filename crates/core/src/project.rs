//! A take's edits and look, saved as `project.json` next to the recording.

use crate::events::EventLog;
use crate::zoom::{auto_zoom, AutoZoom, Zoom};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// Length of the recording in seconds.
    pub duration: f64,
    /// Size of the recording in pixels.
    pub width: u32,
    pub height: u32,
    pub zooms: Vec<Zoom>,
    #[serde(default)]
    pub style: Style,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Background {
    Color(String),
    /// Top to bottom, CSS colors.
    Gradient(String, String),
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Aspect {
    /// The recording's own shape.
    Source,
    Landscape16x9,
    Portrait9x16,
    Square,
}

impl Aspect {
    /// Width / height of the output, given the recording's.
    pub fn ratio(self, source: f32) -> f32 {
        match self {
            Aspect::Source => source,
            Aspect::Landscape16x9 => 16.0 / 9.0,
            Aspect::Portrait9x16 => 9.0 / 16.0,
            Aspect::Square => 1.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub background: Background,
    /// Space around the recording, as a fraction of the output's shorter side.
    pub padding: f32,
    /// Corner radius, as a fraction of the output's shorter side.
    pub radius: f32,
    pub shadow: bool,
    pub aspect: Aspect,
    /// Cursor size relative to the system's.
    pub cursor_scale: f32,
    /// Seconds the cursor path is averaged over (0 = drawn as recorded).
    pub cursor_smoothing: f64,
}

impl Default for Style {
    fn default() -> Self {
        Self {
            background: Background::Gradient("#4f46e5".into(), "#db2777".into()),
            padding: 0.06,
            radius: 0.015,
            shadow: true,
            aspect: Aspect::Source,
            cursor_scale: 1.5,
            cursor_smoothing: 0.08,
        }
    }
}

impl Project {
    /// A fresh project for a new take, with zooms suggested from its clicks.
    pub fn new(duration: f64, width: u32, height: u32, events: &EventLog) -> Self {
        Self {
            duration,
            width,
            height,
            zooms: auto_zoom(&events.clicks, duration, &AutoZoom::default()),
            style: Style::default(),
        }
    }

    pub fn load(path: &Path) -> std::io::Result<Self> {
        Ok(serde_json::from_slice(&std::fs::read(path)?)?)
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, serde_json::to_vec_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_json() {
        let p = Project::new(12.5, 2880, 1800, &EventLog::default());
        let json = serde_json::to_string(&p).unwrap();
        assert_eq!(serde_json::from_str::<Project>(&json).unwrap(), p);
    }
}
