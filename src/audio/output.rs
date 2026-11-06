use std::sync::atomic::Ordering;
use std::sync::Arc;

use anyhow::{anyhow, Result};
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, FromSample, SampleFormat, SampleRate, SizedSample, StreamConfig};

use super::Shared;

pub struct Output {
    device: Device,
    stream: Option<cpal::Stream>,
    pub rate: u32,
    pub channels: u16,
}

impl Output {
    pub fn new() -> Result<Self> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| anyhow!("no audio output device"))?;
        Ok(Self {
            device,
            stream: None,
            rate: 0,
            channels: 2,
        })
    }

    fn pick(&self, want: u32) -> (StreamConfig, SampleFormat) {
        if let Ok(ranges) = self.device.supported_output_configs() {
            let mut best: Option<(u32, Result<(StreamConfig, SampleFormat)>)> = None;
            for range in ranges {
                if range.channels() < 2 {
                    continue;
                }
                let min = range.min_sample_rate().0;
                let max = range.max_sample_rate().0;
                let dist = if want < min {
                    min - want
                } else if want > max {
                    want - max
                } else {
                    0
                };
                if dist == 0 {
                    let cfg = range.with_sample_rate(SampleRate(want));
                    return (cfg.config(), cfg.sample_format());
                }
                if best.as_ref().map(|(d, _)| dist < *d).unwrap_or(true) {
                    let rate = want.clamp(min, max);
                    let cfg = range.with_sample_rate(SampleRate(rate));
                    best = Some((dist, Ok((cfg.config(), cfg.sample_format()))));
                }
            }
            if let Some((_, Ok(cfg))) = best {
                return cfg;
            }
        }
        if let Ok(def) = self.device.default_output_config() {
            return (def.config(), def.sample_format());
        }
        (
            StreamConfig {
                channels: 2,
                sample_rate: SampleRate(44_100),
                buffer_size: cpal::BufferSize::Default,
            },
            SampleFormat::F32,
        )
    }

    /// Make sure a stream is running, at `want` Hz when possible.
    /// Returns the sample rate actually in use.
    pub fn ensure(&mut self, want: u32, shared: Arc<Shared>) -> Result<u32> {
        if self.stream.is_some() && self.rate == want {
            return Ok(self.rate);
        }
        self.stream = None;
        let (config, format) = self.pick(want);
        self.rate = config.sample_rate.0;
        self.channels = config.channels;

        let stream = match format {
            SampleFormat::F32 => build::<f32>(&self.device, &config, shared),
            SampleFormat::I16 => build::<i16>(&self.device, &config, shared),
            SampleFormat::U16 => build::<u16>(&self.device, &config, shared),
            SampleFormat::I32 => build::<i32>(&self.device, &config, shared),
            SampleFormat::F64 => build::<f64>(&self.device, &config, shared),
            other => Err(anyhow!("unsupported sample format: {other:?}")),
        }?;

        stream.play()?;
        self.stream = Some(stream);
        Ok(self.rate)
    }
}

fn build<T>(device: &Device, config: &StreamConfig, shared: Arc<Shared>) -> Result<cpal::Stream>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let err_fn = move |e| eprintln!("ymp: audio stream error: {e}");
    let mut gain: f32 = 1.0;
    let stream = device.build_output_stream(
        config,
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            fill(data, &shared, channels, &mut gain);
        },
        err_fn,
        None,
    )?;
    Ok(stream)
}

fn fill<T>(data: &mut [T], shared: &Arc<Shared>, channels: usize, gain: &mut f32)
where
    T: SizedSample + FromSample<f32>,
{
    let target = f32::from_bits(shared.volume.load(Ordering::Relaxed));
    let paused = shared.paused.load(Ordering::Relaxed);

    if paused {
        for d in data.iter_mut() {
            *d = T::from_sample(0.0f32);
        }
        return;
    }

    let mut ring = shared
        .ring
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    let mut tap = shared.tap.try_lock().ok();
    let tap_len = super::TAP_LEN;

    for (i, d) in data.iter_mut().enumerate() {
        if (*gain - target).abs() > 0.0005 {
            *gain += (target - *gain) * 0.0025;
        } else {
            *gain = target;
        }
        let s = ring.pop_front().unwrap_or(0.0);
        let v = s * *gain;
        if let Some(t) = tap.as_mut() {
            if i % channels.max(1) == 0 {
                let pos = t.pos;
                t.data[pos] = v;
                t.pos = (pos + 1) % tap_len;
            }
        }
        *d = T::from_sample(v);
    }

    let drained = ring.is_empty();
    drop(ring);
    drop(tap);

    let frames = data.len() / channels.max(1);
    shared
        .pos_frames
        .fetch_add(frames as u64, Ordering::Relaxed);

    if drained && shared.eof.load(Ordering::Relaxed) {
        shared.ended.store(true, Ordering::Relaxed);
    }

}
