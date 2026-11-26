//! Microphone level and the dot-matrix voice band.

use eframe::egui::{self, Color32, Pos2, Rect, Vec2};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread::JoinHandle;

// ---------------------------------------------------------------------------
// Part 1: microphone level
// ---------------------------------------------------------------------------

/// Live input level. Only the RMS is kept, audio samples are never stored or logged.
#[cfg_attr(not(windows), allow(dead_code))]
pub(super) struct MicLevel {
    level: Arc<AtomicU32>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl MicLevel {
    #[cfg(windows)]
    pub(super) fn start() -> Result<MicLevel, String> {
        let level = Arc::new(AtomicU32::new(0f32.to_bits()));
        let stop = Arc::new(AtomicBool::new(false));
        // The device handle is not Send, so it is opened inside the thread
        // and the outcome is reported back before start() returns.
        let (tx, rx) = std::sync::mpsc::channel();
        let (l, s) = (level.clone(), stop.clone());
        let thread = std::thread::Builder::new()
            .name("covalos-mic".into())
            .spawn(move || win::capture(&l, &s, &tx))
            .map_err(|_| "Could not open the microphone".to_string())?;
        match rx.recv() {
            Ok(Ok(())) => Ok(MicLevel {
                level,
                stop,
                thread: Some(thread),
            }),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("Could not open the microphone".to_string()),
        }
    }

    #[cfg(not(windows))]
    pub(super) fn start() -> Result<MicLevel, String> {
        Err("Microphone input is only available on Windows for now".to_string())
    }

    /// Latest raw RMS of the most recent buffer, 0..1.
    pub(super) fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed)).clamp(0.0, 1.0)
    }
}

impl Drop for MicLevel {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(windows)]
mod win {
    use std::mem::size_of;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::mpsc::Sender;
    use std::time::Duration;
    use windows_sys::Win32::Media::Audio::{
        CALLBACK_NULL, HWAVEIN, WAVE_FORMAT_PCM, WAVE_MAPPER, WAVEFORMATEX, WAVEHDR, WHDR_DONE,
        waveInAddBuffer, waveInClose, waveInGetNumDevs, waveInOpen, waveInPrepareHeader,
        waveInReset, waveInStart, waveInUnprepareHeader,
    };

    const RATE: u32 = 16_000;
    const SAMPLES_PER_BUF: usize = 800; // 50 ms
    const BUFFERS: usize = 4;
    const HDR_SIZE: u32 = size_of::<WAVEHDR>() as u32;

    /// An open input device plus the buffers it writes into. The buffers are
    /// boxed so their addresses stay fixed, and they are owned here so they
    /// outlive the device: Drop closes the device before the fields drop.
    struct Capture {
        hwi: HWAVEIN,
        #[allow(clippy::vec_box)] // stable addresses are required while the driver owns them
        headers: Vec<Box<WAVEHDR>>,
        // Never read directly, the driver writes through the header pointers.
        _data: Vec<Box<[i16]>>,
        prepared: usize,
    }

    impl Drop for Capture {
        fn drop(&mut self) {
            // SAFETY: hwi is a valid open handle; reset returns all queued
            // buffers to us, after which they can be unprepared and the
            // device closed. Headers are valid boxed WAVEHDRs.
            unsafe {
                waveInReset(self.hwi);
                for h in self.headers.iter_mut().take(self.prepared) {
                    waveInUnprepareHeader(self.hwi, &mut **h, HDR_SIZE);
                }
                waveInClose(self.hwi);
            }
        }
    }

    fn open() -> Result<Capture, String> {
        // SAFETY: plain query with no arguments.
        if unsafe { waveInGetNumDevs() } == 0 {
            return Err("No microphone found".to_string());
        }
        let fmt = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_PCM as u16,
            nChannels: 1,
            nSamplesPerSec: RATE,
            nAvgBytesPerSec: RATE * 2,
            nBlockAlign: 2,
            wBitsPerSample: 16,
            cbSize: 0,
        };
        let mut hwi: HWAVEIN = std::ptr::null_mut();
        // SAFETY: hwi and fmt are valid for the call, CALLBACK_NULL means the
        // callback arguments are ignored.
        let rc = unsafe { waveInOpen(&mut hwi, WAVE_MAPPER, &fmt, 0, 0, CALLBACK_NULL) };
        if rc != 0 || hwi.is_null() {
            return Err("Could not open the microphone".to_string());
        }
        let mut cap = Capture {
            hwi,
            headers: Vec::new(),
            _data: Vec::new(),
            prepared: 0,
        };
        let fail = || "Could not open the microphone".to_string();
        for _ in 0..BUFFERS {
            let mut data = vec![0i16; SAMPLES_PER_BUF].into_boxed_slice();
            let mut hdr = Box::new(WAVEHDR::default());
            hdr.lpData = data.as_mut_ptr().cast();
            hdr.dwBufferLength = (SAMPLES_PER_BUF * 2) as u32;
            cap._data.push(data);
            cap.headers.push(hdr);
            let h = &mut **cap.headers.last_mut().ok_or_else(fail)?;
            // SAFETY: h points to a live boxed header whose data pointer
            // targets a live boxed slice; both are owned by `cap`.
            if unsafe { waveInPrepareHeader(hwi, h, HDR_SIZE) } != 0 {
                return Err(fail());
            }
            cap.prepared += 1;
            // SAFETY: header was prepared just above.
            if unsafe { waveInAddBuffer(hwi, h, HDR_SIZE) } != 0 {
                return Err(fail());
            }
        }
        // SAFETY: valid open handle with queued buffers.
        if unsafe { waveInStart(hwi) } != 0 {
            return Err(fail());
        }
        Ok(cap)
    }

    fn rms(samples: &[i16]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum: f64 = samples
            .iter()
            .map(|&s| {
                let v = f64::from(s) / 32768.0;
                v * v
            })
            .sum();
        (sum / samples.len() as f64).sqrt() as f32
    }

    pub(super) fn capture(
        level: &AtomicU32,
        stop: &AtomicBool,
        ready: &Sender<Result<(), String>>,
    ) {
        let mut cap = match open() {
            Ok(c) => c,
            Err(e) => {
                let _ = ready.send(Err(e));
                return;
            }
        };
        let _ = ready.send(Ok(()));
        let hwi = cap.hwi;
        while !stop.load(Ordering::Relaxed) {
            for hdr in cap.headers.iter_mut() {
                let p: *mut WAVEHDR = &mut **hdr;
                // SAFETY: p is a live header; the driver updates dwFlags from
                // another thread, so read it volatile.
                let flags = unsafe { std::ptr::addr_of!((*p).dwFlags).read_volatile() };
                if flags & WHDR_DONE == 0 {
                    continue;
                }
                let n = (hdr.dwBytesRecorded as usize / 2).min(SAMPLES_PER_BUF);
                // SAFETY: lpData points at SAMPLES_PER_BUF i16s and n is
                // clamped to that; DONE means the driver is finished with it.
                let samples = unsafe { std::slice::from_raw_parts(hdr.lpData.cast::<i16>(), n) };
                level.store(rms(samples).to_bits(), Ordering::Relaxed);
                // SAFETY: header is prepared; re-queueing a DONE buffer is allowed.
                unsafe { waveInAddBuffer(hwi, p, HDR_SIZE) };
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        // Capture::drop resets, unprepares and closes the device.
    }
}

// ---------------------------------------------------------------------------
// Part 2: visual state
// ---------------------------------------------------------------------------

const ATTACK_SECS: f32 = 0.06;
const RELEASE_SECS: f32 = 0.25;
const IDLE_LEVEL: f32 = 0.02;

/// Smoothed voice level and per-column heights, all pure so it is testable.
pub(super) struct VoiceBand {
    level: f32,
    columns: Vec<f32>,
    time: f32,
}

impl VoiceBand {
    pub(super) fn new(columns: usize) -> Self {
        Self {
            level: 0.0,
            columns: vec![0.0; columns.max(1)],
            time: 0.0,
        }
    }

    pub(super) fn update(&mut self, rms: f32, dt: f32) {
        let dt = dt.clamp(0.0, 0.25);
        self.time += dt;
        // Square root maps normal speech (0.02..0.1 RMS) to roughly 0.4..0.9,
        // and the small floor keeps room noise from lighting the band.
        let target = ((rms.max(0.0) - 0.004).max(0.0) * 8.0).sqrt().min(1.0);
        let tau = if target > self.level {
            ATTACK_SECS
        } else {
            RELEASE_SECS
        };
        self.level += (target - self.level) * (1.0 - (-dt / tau).exp());
        self.level = self.level.clamp(0.0, 1.0);

        let n = self.columns.len();
        for (i, c) in self.columns.iter_mut().enumerate() {
            let x = if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.5
            };
            let bell = (-(x - 0.5).powi(2) / (2.0 * 0.22 * 0.22)).exp();
            // Two slow travelling waves keep the mound alive without rand.
            let wob = 1.0
                + 0.18 * (x * 9.0 + self.time * 3.1).sin()
                + 0.10 * (x * 17.0 - self.time * 4.7).sin();
            *c = (self.level * (0.25 + 0.75 * bell) * wob).clamp(0.0, 1.0);
        }
    }

    pub(super) fn level(&self) -> f32 {
        self.level
    }

    pub(super) fn idle(&self) -> bool {
        self.level < IDLE_LEVEL
    }

    /// Height of the mound at horizontal position `x` in 0..1.
    fn sample(&self, x: f32) -> f32 {
        let n = self.columns.len();
        if n == 1 {
            return self.columns[0];
        }
        let f = x.clamp(0.0, 1.0) * (n - 1) as f32;
        let i = (f as usize).min(n - 2);
        let t = f - i as f32;
        self.columns[i] * (1.0 - t) + self.columns[i + 1] * t
    }
}

// ---------------------------------------------------------------------------
// Part 3: painting
// ---------------------------------------------------------------------------

const PALETTE: [Color32; 5] = [
    Color32::from_rgb(255, 150, 50), // orange
    Color32::from_rgb(240, 70, 170), // magenta
    Color32::from_rgb(50, 215, 240), // cyan
    Color32::from_rgb(70, 115, 255), // blue
    Color32::from_rgb(155, 95, 255), // violet
];

/// Spectrum color for `t` in 0..1 across the band.
fn spectrum_color(t: f32) -> Color32 {
    let f = t.clamp(0.0, 1.0) * (PALETTE.len() - 1) as f32;
    let i = (f as usize).min(PALETTE.len() - 2);
    PALETTE[i].lerp_to_gamma(PALETTE[i + 1], f - i as f32)
}

/// Cheap deterministic hash to 0..1 for the scattered pixels.
fn hash01(a: u32, b: u32) -> f32 {
    let mut h = a.wrapping_mul(0x9E37_79B1) ^ b.wrapping_mul(0x85EB_CA6B);
    h ^= h >> 15;
    h = h.wrapping_mul(0x2C1B_3C6D);
    h ^= h >> 12;
    (h & 0xFFFF) as f32 / 65535.0
}

pub(super) fn paint_voice_band(painter: &egui::Painter, rect: Rect, band: &VoiceBand, alpha: f32) {
    const PIXEL: f32 = 3.5;
    const GAP: f32 = 1.0;
    const PITCH: f32 = PIXEL + GAP;

    let alpha = alpha.clamp(0.0, 1.0);
    let cols = (rect.width() / PITCH).floor() as usize;
    let rows = (rect.height() / PITCH).floor() as usize;
    if alpha <= 0.0 || cols == 0 || rows == 0 {
        return;
    }
    let x0 = rect.center().x - cols as f32 * PITCH * 0.5;
    let bottom = rect.bottom();
    let idle = band.idle();
    let tick = (band.time * 7.0) as u32;

    let pixel = |ix: usize, row: usize, a: f32, color: Color32| {
        let min = Pos2::new(
            x0 + ix as f32 * PITCH,
            bottom - (row + 1) as f32 * PITCH + GAP,
        );
        painter.rect_filled(
            Rect::from_min_size(min, Vec2::splat(PIXEL)),
            0.5,
            color.gamma_multiply((a * alpha).clamp(0.0, 1.0)),
        );
    };

    for ix in 0..cols {
        let x = (ix as f32 + 0.5) / cols as f32;
        // Fade out toward both ends so the band melts into the composer.
        let edge = (x.min(1.0 - x) / 0.12).clamp(0.0, 1.0);
        let color = spectrum_color(x);

        if idle {
            if ix % 2 == 0 {
                pixel(ix, 0, 0.18 * edge, color);
            }
            continue;
        }

        let h = band.sample(x) * rows as f32;
        let full = h.floor() as usize;
        // Soft glow under the column, drawn first so pixels sit on top.
        if h > 1.0 {
            let glow = Rect::from_min_max(
                Pos2::new(x0 + ix as f32 * PITCH - 1.5, bottom - h * PITCH),
                Pos2::new(x0 + ix as f32 * PITCH + PIXEL + 1.5, bottom),
            );
            painter.rect_filled(
                glow,
                2.0,
                color.gamma_multiply(0.07 * edge * alpha * band.level()),
            );
        }
        for row in 0..=full.min(rows - 1) {
            // Partial top row fades in, base is brightest.
            let top = if row == full { h - full as f32 } else { 1.0 };
            let depth = 1.0 - 0.55 * (row as f32 / (h.max(1.0)));
            pixel(ix, row, top * depth * edge, color);
        }
        // Sparse stray pixels above the mound, reshuffled a few times a second.
        let r = hash01(ix as u32, tick);
        if r < 0.10 * band.level() && full + 1 < rows {
            let span = (rows - full - 1).min(rows / 2 + 1).max(1);
            let off = 1 + (hash01(tick, ix as u32 + 77) * span as f32) as usize;
            let row = (full + off).min(rows - 1);
            pixel(ix, row, 0.55 * edge, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn silence_stays_idle() {
        let mut b = VoiceBand::new(24);
        for _ in 0..30 {
            b.update(0.0, DT);
        }
        assert!(b.idle());
        assert!(b.level() < IDLE_LEVEL);
    }

    #[test]
    fn loud_raises_level_and_stays_bounded() {
        let mut b = VoiceBand::new(24);
        for _ in 0..6 {
            b.update(0.5, DT);
            assert!(b.level() <= 1.0);
        }
        assert!(!b.idle());
        assert!(b.level() > 0.4);
        for _ in 0..200 {
            b.update(1.0, DT);
        }
        assert!(b.level() <= 1.0);
    }

    #[test]
    fn release_is_slower_than_attack() {
        let mut b = VoiceBand::new(24);
        for _ in 0..60 {
            b.update(0.3, DT);
        }
        let peak = b.level();
        b.update(0.0, DT);
        assert!(b.level() > 0.0);
        assert!(b.level() > peak * 0.8, "released too quickly");
    }

    #[test]
    fn columns_in_range_and_centered() {
        let mut b = VoiceBand::new(33);
        for _ in 0..90 {
            b.update(0.2, DT);
        }
        assert!(b.columns.iter().all(|c| (0.0..=1.0).contains(c)));
        let mid = b.columns[16];
        assert!(mid >= b.columns[0]);
        assert!(mid >= b.columns[32]);
    }

    #[test]
    fn spectrum_endpoints() {
        assert_eq!(spectrum_color(0.0), PALETTE[0]);
        assert_eq!(spectrum_color(1.0), PALETTE[4]);
        assert_eq!(spectrum_color(-3.0), PALETTE[0]);
    }

    #[test]
    fn paint_smoke() {
        let ctx = egui::Context::default();
        let mut loud = VoiceBand::new(32);
        for _ in 0..30 {
            loud.update(0.2, DT);
        }
        let quiet = VoiceBand::new(32);
        let _ = ctx.run(egui::RawInput::default(), |ctx| {
            egui::CentralPanel::default().show(ctx, |ui| {
                let rect = Rect::from_min_size(Pos2::new(10.0, 10.0), Vec2::new(300.0, 60.0));
                paint_voice_band(ui.painter(), rect, &loud, 1.0);
                paint_voice_band(ui.painter(), rect, &quiet, 0.5);
                paint_voice_band(ui.painter(), Rect::NOTHING, &loud, 1.0);
            });
        });
    }
}
