use std::sync::Arc;

use rustfft::num_complex::Complex;
use rustfft::{Fft, FftPlanner};

pub const BANDS: usize = 16;
const WINDOW: usize = 1024;

pub struct Analyzer {
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,
    buf: Vec<Complex<f32>>,
    edges: Vec<usize>,
    rate: f32,
    smooth: Vec<f32>,
    peak: Vec<f32>,
}

impl Analyzer {
    pub fn new(rate: f32) -> Self {
        let mut planner = FftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(WINDOW);
        let window = (0..WINDOW)
            .map(|i| {
                let x = std::f32::consts::PI * 2.0 * i as f32 / (WINDOW as f32 - 1.0);
                0.5 - 0.5 * x.cos()
            })
            .collect();
        let mut me = Self {
            fft,
            window,
            buf: vec![Complex::new(0.0, 0.0); WINDOW],
            edges: Vec::new(),
            rate: 0.0,
            smooth: vec![0.0; BANDS],
            peak: vec![0.0; BANDS],
        };
        me.set_rate(rate);
        me
    }

    fn set_rate(&mut self, rate: f32) {
        if (self.rate - rate).abs() < 1.0 && !self.edges.is_empty() {
            self.rate = rate;
            return;
        }
        self.rate = rate;
        let nyquist = rate / 2.0;
        let lo = 45.0f32;
        let hi = nyquist.min(16_000.0);
        self.edges = (0..=BANDS)
            .map(|b| {
                let t = b as f32 / BANDS as f32;
                let f = lo * (hi / lo).powf(t);
                let bin = (f / nyquist * (WINDOW / 2) as f32).round();
                bin.clamp(1.0, (WINDOW / 2 - 1) as f32) as usize
            })
            .collect();
        self.edges.dedup();
        while self.edges.len() < BANDS + 1 {
            let last = *self.edges.last().unwrap_or(&1);
            self.edges.push((last + 1).min(WINDOW / 2 - 1));
        }
    }

    /// Feed the most recent mono samples; returns the smoothed band levels.
    pub fn update(&mut self, samples: &[f32], rate: f32) -> &[f32] {
        self.set_rate(rate);
        let n = samples.len().min(WINDOW);
        if n == 0 {
            for s in self.smooth.iter_mut() {
                *s *= 0.85;
            }
            return &self.smooth;
        }
        let start = samples.len() - n;
        let offset = WINDOW - n;
        for i in 0..WINDOW {
            self.buf[i] = Complex::new(0.0, 0.0);
        }
        for i in 0..n {
            self.buf[offset + i] = Complex::new(samples[start + i] * self.window[i], 0.0);
        }
        self.fft.process(&mut self.buf);

        for b in 0..BANDS {
            let lo = self.edges[b];
            let hi = self.edges[b + 1].max(lo + 1);
            let mut mag = 0.0f32;
            for k in lo..hi.min(WINDOW / 2) {
                let m = self.buf[k].norm();
                if m > mag {
                    mag = m;
                }
            }
            // -70..-6 dBFS mapped to 0..1
            let db = 20.0 * (mag / (WINDOW as f32 * 0.25)).max(1e-6).log10();
            let v = ((db + 70.0) / 64.0).clamp(0.0, 1.0);
            let cur = self.smooth[b];
            self.smooth[b] = if v > cur {
                cur + (v - cur) * 0.65
            } else {
                cur + (v - cur) * 0.16
            };
            self.peak[b] = (self.peak[b] - 0.03).max(self.smooth[b]);
        }
        &self.smooth
    }
}
