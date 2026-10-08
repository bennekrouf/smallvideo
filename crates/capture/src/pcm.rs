//! Microphone samples, as the Windows encoder takes them: 16-bit stereo at 44.1 or 48 kHz.
//! Microphones deliver anything from mono 16 kHz headsets to multichannel arrays, as floats
//! or integers.

/// Converts a running stream of interleaved samples to 16-bit little-endian stereo. Mono is
/// doubled, extra channels are dropped, and rates the AAC encoder doesn't take are resampled
/// (linearly) to 48 kHz. Keeps one frame between calls so buffers join without a click.
pub struct ToStereo16 {
    channels: usize,
    /// Input frames per output frame.
    step: f64,
    /// Position of the next output frame, in input frames from the start of the next buffer
    /// (-1 is the last frame of the previous one).
    pos: f64,
    prev: (f32, f32),
    pub out_rate: u32,
}

impl ToStereo16 {
    pub fn new(in_rate: u32, channels: u16) -> Self {
        let out_rate = if matches!(in_rate, 44_100 | 48_000) { in_rate } else { 48_000 };
        Self {
            channels: channels.max(1) as usize,
            step: in_rate as f64 / out_rate as f64,
            pos: 0.0,
            prev: (0.0, 0.0),
            out_rate,
        }
    }

    pub fn convert(&mut self, samples: impl IntoIterator<Item = f32>) -> Vec<u8> {
        let samples: Vec<f32> = samples.into_iter().collect();
        let frames: Vec<(f32, f32)> =
            samples.chunks_exact(self.channels).map(|f| (f[0], if self.channels > 1 { f[1] } else { f[0] })).collect();
        let Some(&last) = frames.last() else {
            return Vec::new();
        };
        let get = |i: isize| if i < 0 { self.prev } else { frames[i as usize] };
        let mut out = Vec::with_capacity((frames.len() as f64 / self.step) as usize * 4 + 8);
        let end = frames.len() as f64 - 1.0;
        while self.pos < end {
            let i = self.pos.floor();
            let k = (self.pos - i) as f32;
            let (a, b) = (get(i as isize), get(i as isize + 1));
            for v in [a.0 + (b.0 - a.0) * k, a.1 + (b.1 - a.1) * k] {
                out.extend_from_slice(&((v.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
            }
            self.pos += self.step;
        }
        self.pos -= frames.len() as f64;
        self.prev = last;
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frames(bytes: &[u8]) -> usize {
        bytes.len() / 4
    }

    #[test]
    fn stereo_at_a_supported_rate_passes_through() {
        let mut c = ToStereo16::new(48_000, 2);
        let a = c.convert([0.5, -0.5, 0.25, -0.25, 1.0, -1.0]);
        let b = c.convert([0.0, 0.0, 0.0, 0.0]);
        // One frame is held back to join the next buffer: 3 + 2 frames in, 1 still pending.
        assert_eq!(frames(&a) + frames(&b), 4);
        assert_eq!(&a[..4], &[(16383i16).to_le_bytes(), (-16383i16).to_le_bytes()].concat()[..]);
    }

    #[test]
    fn mono_is_doubled() {
        let mut c = ToStereo16::new(44_100, 1);
        let out = c.convert([0.5, 0.5, 0.5]);
        let l = i16::from_le_bytes([out[0], out[1]]);
        let r = i16::from_le_bytes([out[2], out[3]]);
        assert_eq!((l, r), (16383, 16383));
    }

    #[test]
    fn other_rates_become_48k() {
        let mut c = ToStereo16::new(16_000, 1);
        assert_eq!(c.out_rate, 48_000);
        let mut total = 0;
        for _ in 0..100 {
            total += frames(&c.convert(vec![0.1; 160]));
        }
        // 16 000 frames in → about 48 000 out (three per input frame).
        assert!((47_990..=48_000).contains(&total), "{total}");
    }
}
