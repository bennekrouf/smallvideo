//! Zoom regions: stretches of the take where the camera moves in on what's happening.
//! Suggested from clicks, then edited by hand like any other part of the project.

use crate::events::Click;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Zoom {
    pub start: f64,
    pub end: f64,
    /// 2.0 shows half the width and half the height of the screen.
    pub scale: f32,
    /// The point the camera centers on, as far as the screen's edges allow.
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Copy, Debug)]
pub struct AutoZoom {
    /// The camera starts moving in this long before a click.
    pub lead: f64,
    /// And stays in this long after the last one.
    pub hold: f64,
    /// Clicks closer than this share one zoom instead of zooming out in between.
    pub merge_gap: f64,
    pub scale: f32,
}

impl Default for AutoZoom {
    fn default() -> Self {
        Self { lead: 0.8, hold: 1.5, merge_gap: 1.0, scale: 1.8 }
    }
}

/// One zoom per burst of clicks, centered on the clicks' average position, inside `[0, duration]`.
pub fn auto_zoom(clicks: &[Click], duration: f64, cfg: &AutoZoom) -> Vec<Zoom> {
    let mut zooms: Vec<(Zoom, u32)> = Vec::new();
    for c in clicks {
        let (start, end) = ((c.t - cfg.lead).max(0.0), (c.t + cfg.hold).min(duration));
        match zooms.last_mut() {
            Some((z, n)) if start <= z.end + cfg.merge_gap => {
                z.end = z.end.max(end);
                // Running mean of the clicks' positions.
                *n += 1;
                z.x += (c.x - z.x) / *n as f32;
                z.y += (c.y - z.y) / *n as f32;
            }
            _ => zooms.push((Zoom { start, end, scale: cfg.scale, x: c.x, y: c.y }, 1)),
        }
    }
    zooms.into_iter().map(|(z, _)| z).filter(|z| z.end > z.start).collect()
}

/// Shortest zoom worth keeping, in seconds.
pub const MIN_SECS: f64 = 0.3;

/// Adds `z`, trimmed to the gap it starts in so zooms never overlap. Returns its index, or
/// `None` if it starts inside another zoom or the gap is too short.
pub fn insert(zooms: &mut Vec<Zoom>, mut z: Zoom) -> Option<usize> {
    let i = zooms.partition_point(|o| o.start <= z.start);
    if i > 0 && zooms[i - 1].end > z.start {
        return None;
    }
    if let Some(next) = zooms.get(i) {
        z.end = z.end.min(next.start);
    }
    if z.end - z.start < MIN_SECS {
        return None;
    }
    zooms.insert(i, z);
    Some(i)
}

/// Moves zoom `i`'s edges, kept between its neighbours and at least `MIN_SECS` long.
pub fn set_bounds(zooms: &mut [Zoom], i: usize, start: f64, end: f64, duration: f64) {
    let lo = if i > 0 { zooms[i - 1].end } else { 0.0 };
    let hi = zooms.get(i + 1).map_or(duration, |n| n.start);
    let start = start.clamp(lo, (hi - MIN_SECS).max(lo));
    zooms[i].start = start;
    zooms[i].end = end.clamp((start + MIN_SECS).min(hi), hi);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::Button;

    fn zoom(start: f64, end: f64) -> Zoom {
        Zoom { start, end, scale: 2.0, x: 0.5, y: 0.5 }
    }

    #[test]
    fn inserted_zooms_never_overlap() {
        let mut z = vec![zoom(2.0, 4.0), zoom(6.0, 8.0)];
        assert_eq!(insert(&mut z, zoom(5.0, 7.0)), Some(1));
        assert_eq!(z[1].end, 6.0, "trimmed to the next zoom");
        assert_eq!(insert(&mut z, zoom(3.0, 3.5)), None, "starts inside a zoom");
        assert_eq!(insert(&mut z, zoom(5.9, 9.0)), None, "inside the one just added");
        assert_eq!(insert(&mut z, zoom(0.0, 1.0)), Some(0));
        assert_eq!(z.len(), 4);
    }

    #[test]
    fn bounds_stay_between_neighbours() {
        let mut z = vec![zoom(2.0, 4.0), zoom(6.0, 8.0), zoom(9.0, 10.0)];
        set_bounds(&mut z, 1, 3.0, 9.5, 12.0);
        assert_eq!((z[1].start, z[1].end), (4.0, 9.0));
        set_bounds(&mut z, 2, 9.0, 20.0, 12.0);
        assert_eq!(z[2].end, 12.0);
        set_bounds(&mut z, 0, 1.0, 1.0, 12.0);
        assert!((z[0].end - z[0].start - MIN_SECS).abs() < 1e-9);
    }

    fn click(t: f64, x: f32, y: f32) -> Click {
        Click { t, x, y, button: Button::Left }
    }

    #[test]
    fn nearby_clicks_share_one_zoom() {
        let z = auto_zoom(&[click(5.0, 0.2, 0.2), click(6.0, 0.4, 0.4)], 60.0, &AutoZoom::default());
        assert_eq!(z.len(), 1);
        assert!((z[0].start - 4.2).abs() < 1e-9 && (z[0].end - 7.5).abs() < 1e-9);
        assert!((z[0].x - 0.3).abs() < 1e-6);
    }

    #[test]
    fn distant_clicks_get_their_own_zoom() {
        let z = auto_zoom(&[click(5.0, 0.2, 0.2), click(20.0, 0.8, 0.8)], 60.0, &AutoZoom::default());
        assert_eq!(z.len(), 2);
    }

    #[test]
    fn zooms_stay_inside_the_take() {
        let z = auto_zoom(&[click(0.1, 0.5, 0.5), click(9.9, 0.5, 0.5)], 10.0, &AutoZoom::default());
        assert_eq!(z[0].start, 0.0);
        assert_eq!(z.last().unwrap().end, 10.0);
    }
}
