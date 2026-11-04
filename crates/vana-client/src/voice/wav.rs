//! Test runs' WAV files: FFXI_VOICE_REC records what is heard, FFXI_VOICE_MIC reads a microphone stand-in.

use super::RATE;
use std::io::{Seek, SeekFrom, Write};
use std::path::Path;

/// 48 kHz mono 16-bit; the sizes in the header are put right on drop.
pub struct Writer {
    f: std::io::BufWriter<std::fs::File>,
    n: u32,
}

impl Writer {
    pub fn create(path: &Path) -> std::io::Result<Self> {
        let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
        f.write_all(&header(0))?;
        Ok(Self { f, n: 0 })
    }

    pub fn write(&mut self, s: &[f32]) {
        for x in s {
            let _ = self.f.write_all(&((x.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        let before = self.n;
        self.n += s.len() as u32;
        // The header is put right every second: a run may end without the writer dropped.
        if before / RATE != self.n / RATE {
            self.settle();
        }
    }

    fn settle(&mut self) {
        let h = header(self.n);
        let _ = self.f.seek(SeekFrom::Start(0)).and_then(|_| self.f.write_all(&h)).and_then(|_| self.f.seek(SeekFrom::End(0))).and_then(|_| self.f.flush());
    }
}

impl Drop for Writer {
    fn drop(&mut self) {
        self.settle();
    }
}

fn header(samples: u32) -> [u8; 44] {
    let data = samples * 2;
    let mut h = [0u8; 44];
    h[..4].copy_from_slice(b"RIFF");
    h[4..8].copy_from_slice(&(36 + data).to_le_bytes());
    h[8..16].copy_from_slice(b"WAVEfmt ");
    h[16..20].copy_from_slice(&16u32.to_le_bytes());
    h[20..22].copy_from_slice(&1u16.to_le_bytes());
    h[22..24].copy_from_slice(&1u16.to_le_bytes());
    h[24..28].copy_from_slice(&RATE.to_le_bytes());
    h[28..32].copy_from_slice(&(RATE * 2).to_le_bytes());
    h[32..34].copy_from_slice(&2u16.to_le_bytes());
    h[34..36].copy_from_slice(&16u16.to_le_bytes());
    h[36..40].copy_from_slice(b"data");
    h[40..44].copy_from_slice(&data.to_le_bytes());
    h
}

/// A 16-bit PCM WAV as mono f32 and its rate.
pub fn read(path: &Path) -> anyhow::Result<(Vec<f32>, u32)> {
    let b = std::fs::read(path)?;
    anyhow::ensure!(b.len() > 12 && &b[..4] == b"RIFF" && &b[8..12] == b"WAVE", "not a WAV file");
    let (mut at, mut fmt) = (12, None);
    while at + 8 <= b.len() {
        let id = &b[at..at + 4];
        let len = u32::from_le_bytes(b[at + 4..at + 8].try_into()?) as usize;
        let body = b.get(at + 8..(at + 8 + len).min(b.len())).unwrap_or_default();
        if id == b"fmt " && body.len() >= 16 {
            let u16_at = |o: usize| u16::from_le_bytes([body[o], body[o + 1]]);
            fmt = Some((u16_at(0), u16_at(2).max(1) as usize, u32::from_le_bytes(body[4..8].try_into()?), u16_at(14)));
        }
        if id == b"data" {
            let Some((1, ch, rate, 16)) = fmt else { anyhow::bail!("only 16-bit PCM WAV files") };
            let s: Vec<f32> = body.chunks_exact(2 * ch).map(|f| f.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).sum::<f32>() / ch as f32).collect();
            return Ok((s, rate));
        }
        at += 8 + len + (len & 1);
    }
    anyhow::bail!("no data in the WAV file")
}
