//! Cursor smoothing: the logged path is jittery and steps at the sampling rate; the drawn
//! cursor should glide.

use crate::events::CursorSample;

/// The path averaged over `±window` seconds around each sample. Centered, so the smoothed
/// cursor doesn't lag behind the real one; the first and last samples stay where they are.
pub fn smooth(samples: &[CursorSample], window: f64) -> Vec<CursorSample> {
    let (mut lo, mut hi) = (0, 0);
    let (mut sx, mut sy) = (0.0f64, 0.0f64);
    samples
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i == 0 || i == samples.len() - 1 {
                return *s;
            }
            while hi < samples.len() && samples[hi].t <= s.t + window {
                sx += samples[hi].x as f64;
                sy += samples[hi].y as f64;
                hi += 1;
            }
            while samples[lo].t < s.t - window {
                sx -= samples[lo].x as f64;
                sy -= samples[lo].y as f64;
                lo += 1;
            }
            let n = (hi - lo) as f64;
            CursorSample { t: s.t, x: (sx / n) as f32, y: (sy / n) as f32 }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_still_cursor_stays_put() {
        let path: Vec<_> = (0..50).map(|i| CursorSample { t: i as f64 / 60.0, x: 0.3, y: 0.7 }).collect();
        for s in smooth(&path, 0.05) {
            assert!((s.x - 0.3).abs() < 1e-6 && (s.y - 0.7).abs() < 1e-6);
        }
    }

    #[test]
    fn jitter_is_reduced() {
        let path: Vec<_> = (0..120)
            .map(|i| CursorSample { t: i as f64 / 60.0, x: if i % 2 == 0 { 0.49 } else { 0.51 }, y: 0.5 })
            .collect();
        let out = smooth(&path, 0.05);
        for s in &out[10..110] {
            assert!((s.x - 0.5).abs() < 0.004, "{}", s.x);
        }
    }
}
