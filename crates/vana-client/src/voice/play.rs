//! The voices heard: a jitter buffer per sender, mixed a frame at a time and resampled to the output's rate.

use super::{FRAME, RATE, Shared};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

/// Frames held before a voice starts (30 ms), or the wait for them; past `HOLD` the oldest go (clock drift).
const START: usize = 3;
const START_WAIT: Duration = Duration::from_millis(60);
const HOLD: usize = 8;

struct Stream {
    q: BTreeMap<u32, Box<[i16]>>,
    next: u32,
    buffering: bool,
    since: Instant,
    last: Instant,
}

#[derive(Default)]
pub struct Play {
    streams: HashMap<u64, Stream>,
}

impl Play {
    pub fn push(&mut self, who: u64, seq: u32, pcm: Vec<i16>) {
        let now = Instant::now();
        let s = self.streams.entry(who).or_insert_with(|| Stream { q: BTreeMap::new(), next: seq, buffering: true, since: now, last: now });
        if !s.buffering && seq < s.next {
            return;
        }
        if s.buffering && s.q.is_empty() {
            s.since = now;
        }
        s.q.insert(seq, pcm.into_boxed_slice());
        s.last = now;
        while s.q.len() > HOLD {
            s.q.pop_first();
            if let Some((&k, _)) = s.q.first_key_value()
                && !s.buffering
            {
                s.next = k;
            }
        }
    }

    pub fn forget(&mut self, who: u64) {
        self.streams.remove(&who);
    }

    /// One frame of every voice, summed into `out`.
    pub fn mix(&mut self, out: &mut [f32; FRAME]) {
        out.fill(0.0);
        let now = Instant::now();
        self.streams.retain(|_, s| !s.q.is_empty() || now.duration_since(s.last) < Duration::from_secs(2));
        for s in self.streams.values_mut() {
            if s.buffering {
                if s.q.len() >= START || (!s.q.is_empty() && now.duration_since(s.since) >= START_WAIT) {
                    s.buffering = false;
                    s.next = *s.q.keys().next().unwrap();
                } else {
                    continue;
                }
            }
            let frame = match s.q.remove(&s.next) {
                Some(f) => Some(f),
                None if s.q.is_empty() => {
                    s.buffering = true;
                    None
                }
                // A lost frame: skip ahead when more wait or the next is far, else one frame of silence.
                None => {
                    let first = *s.q.keys().next().unwrap();
                    if s.q.len() >= 2 || first.wrapping_sub(s.next) >= 3 {
                        s.next = first;
                        s.q.remove(&first)
                    } else {
                        None
                    }
                }
            };
            if !s.buffering {
                s.next = s.next.wrapping_add(1);
            }
            if let Some(f) = frame {
                for (o, &x) in out.iter_mut().zip(f.iter()) {
                    *o += x as f32 / 32768.0;
                }
            }
        }
    }
}

/// The voice output: Play's frames at the device's rate (linear), at the voice volume.
pub struct Out {
    shared: Arc<Shared>,
    step: f64,
    frac: f64,
    a: f32,
    b: f32,
    buf: [f32; FRAME],
    at: usize,
    rec: Option<super::wav::Writer>,
}

impl Out {
    pub fn new(shared: Arc<Shared>, rate: u32, rec: Option<std::ffi::OsString>) -> Self {
        let rec = rec.and_then(|p| super::wav::Writer::create(std::path::Path::new(&p)).map_err(|e| log::warn!("voice: recording: {e}")).ok());
        Self { shared, step: RATE as f64 / rate.max(1) as f64, frac: 1.0, a: 0.0, b: 0.0, buf: [0.0; FRAME], at: FRAME, rec }
    }

    fn sample(&mut self) -> f32 {
        if self.at == FRAME {
            if let Ok(mut p) = self.shared.play.lock() {
                p.mix(&mut self.buf);
            }
            if let Some(r) = self.rec.as_mut() {
                r.write(&self.buf);
            }
            self.at = 0;
        }
        self.at += 1;
        self.buf[self.at - 1]
    }
}

impl crate::audio::output::Render for Out {
    fn render(&mut self, out: &mut [f32]) {
        let vol = if self.shared.deaf.load(Ordering::Relaxed) { 0.0 } else { self.shared.volume.load(Ordering::Relaxed) as f32 / 100.0 };
        for f in out.chunks_exact_mut(2) {
            while self.frac >= 1.0 {
                self.a = self.b;
                self.b = self.sample();
                self.frac -= 1.0;
            }
            let s = ((self.a + (self.b - self.a) * self.frac as f32) * vol).clamp(-1.0, 1.0);
            self.frac += self.step;
            f[0] = s;
            f[1] = s;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(v: i16) -> Vec<i16> {
        vec![v; FRAME]
    }

    fn level(p: &mut Play) -> i32 {
        let mut out = [0.0; FRAME];
        p.mix(&mut out);
        (out[0] * 32768.0).round() as i32
    }

    #[test]
    fn buffers_then_plays_in_order() {
        let mut p = Play::default();
        p.push(1, 10, frame(100));
        p.push(1, 12, frame(300));
        // Two frames held: not yet.
        assert_eq!(level(&mut p), 0);
        p.push(1, 11, frame(200));
        assert_eq!([level(&mut p), level(&mut p), level(&mut p)], [100, 200, 300]);
        // Run dry: silence, then buffering again.
        assert_eq!(level(&mut p), 0);
        p.push(1, 13, frame(400));
        p.push(1, 14, frame(500));
        p.push(1, 15, frame(600));
        assert_eq!(level(&mut p), 400);
        // A late frame is dropped; a lost one is silence while one waits, skipped when more do.
        p.push(1, 13, frame(999));
        p.push(1, 17, frame(800));
        assert_eq!([level(&mut p), level(&mut p), level(&mut p), level(&mut p)], [500, 600, 0, 800]);
        p.push(1, 20, frame(1));
        p.push(1, 21, frame(2));
        assert_eq!(level(&mut p), 1);
    }

    #[test]
    fn two_voices_add() {
        let mut p = Play::default();
        for seq in 0..3 {
            p.push(1, seq, frame(1000));
            p.push(2, 50 + seq, frame(-250));
        }
        assert_eq!(level(&mut p), 750);
    }

    #[test]
    fn drift_drops_the_oldest() {
        let mut p = Play::default();
        for seq in 0..20 {
            p.push(1, seq, frame(seq as i16));
        }
        assert_eq!(level(&mut p), 12);
    }
}
