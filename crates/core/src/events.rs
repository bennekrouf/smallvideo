//! Pointer activity logged next to the recording. The screen is captured without its cursor,
//! so the cursor can be redrawn later: smoothed, resized, and followed by the camera.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct CursorSample {
    pub t: f64,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Button {
    Left,
    Right,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Click {
    pub t: f64,
    pub x: f32,
    pub y: f32,
    pub button: Button,
}

/// Everything the pointer did during a take, in time order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct EventLog {
    pub cursor: Vec<CursorSample>,
    pub clicks: Vec<Click>,
}

impl EventLog {
    /// Pointer position at `t`, interpolated between samples. `None` before any was logged.
    pub fn cursor_at(&self, t: f64) -> Option<(f32, f32)> {
        position_at(&self.cursor, t)
    }
}

/// Position at `t` along `samples` (sorted by time), held at either end.
pub fn position_at(samples: &[CursorSample], t: f64) -> Option<(f32, f32)> {
    let i = samples.partition_point(|s| s.t <= t);
    match (i.checked_sub(1).and_then(|p| samples.get(p)), samples.get(i)) {
        (Some(a), Some(b)) => {
            let k = ((t - a.t) / (b.t - a.t)) as f32;
            Some((a.x + (b.x - a.x) * k, a.y + (b.y - a.y) * k))
        }
        (Some(a), None) => Some((a.x, a.y)),
        (None, Some(b)) => Some((b.x, b.y)),
        (None, None) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(t: f64, x: f32, y: f32) -> CursorSample {
        CursorSample { t, x, y }
    }

    #[test]
    fn interpolates_between_samples_and_holds_at_the_ends() {
        let log = EventLog { cursor: vec![s(1.0, 0.0, 0.0), s(2.0, 1.0, 0.5)], clicks: vec![] };
        assert_eq!(log.cursor_at(1.5), Some((0.5, 0.25)));
        assert_eq!(log.cursor_at(0.0), Some((0.0, 0.0)));
        assert_eq!(log.cursor_at(9.0), Some((1.0, 0.5)));
        assert_eq!(EventLog::default().cursor_at(1.0), None);
    }
}
