use std::fs::File;
use std::path::Path;

use anyhow::{anyhow, Result};
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder as SymphoniaDecoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader, SeekMode, SeekTo};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

pub struct AudioDecoder {
    reader: Box<dyn FormatReader>,
    decoder: Box<dyn SymphoniaDecoder>,
    track_id: u32,
    pub rate: u32,
    pub channels: usize,
    pub duration: Option<f64>,
    scratch: Vec<f32>,
}

impl AudioDecoder {
    pub fn open(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(&ext.to_ascii_lowercase());
        }

        let probed = symphonia::default::get_probe().format(
            &hint,
            mss,
            &FormatOptions {
                enable_gapless: true,
                ..Default::default()
            },
            &MetadataOptions::default(),
        )?;

        let reader = probed.format;
        let track = reader
            .tracks()
            .iter()
            .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
            .ok_or_else(|| anyhow!("no decodable audio track"))?;

        let track_id = track.id;
        let rate = track.codec_params.sample_rate.unwrap_or(44_100);
        let channels = track
            .codec_params
            .channels
            .map(|c| c.count())
            .unwrap_or(2);

        let duration = track
            .codec_params
            .n_frames
            .map(|n| n as f64 / rate as f64)
            .filter(|d| d.is_finite() && *d > 0.0);

        let decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &DecoderOptions::default())?;

        Ok(Self {
            reader,
            decoder,
            track_id,
            rate,
            channels,
            duration,
            scratch: Vec::new(),
        })
    }

    /// Decode the next packet into interleaved stereo f32.
    pub fn next_chunk(&mut self) -> Result<Option<&[f32]>> {
        loop {
            let packet = match self.reader.next_packet() {
                Ok(p) => p,
                Err(SymError::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                Err(SymError::ResetRequired) => return Ok(None),
                Err(e) => return Err(e.into()),
            };

            if packet.track_id() != self.track_id {
                continue;
            }

            let decoded = match self.decoder.decode(&packet) {
                Ok(d) => d,
                Err(SymError::DecodeError(_)) => continue,
                Err(SymError::IoError(ref e))
                    if e.kind() == std::io::ErrorKind::UnexpectedEof =>
                {
                    return Ok(None)
                }
                Err(e) => return Err(e.into()),
            };

            let frames = decoded.frames();
            if frames == 0 {
                continue;
            }
            let spec = *decoded.spec();
            let ch = spec.channels.count();

            let mut sbuf = SampleBuffer::<f32>::new(frames as u64, spec);
            sbuf.copy_interleaved_ref(decoded);
            let samples = sbuf.samples();

            self.scratch.clear();
            self.scratch.reserve(frames * 2);
            match ch {
                0 => continue,
                1 => {
                    for i in 0..frames {
                        let s = samples[i];
                        self.scratch.push(s);
                        self.scratch.push(s);
                    }
                }
                2 => self.scratch.extend_from_slice(&samples[..frames * 2]),
                _ => {
                    for i in 0..frames {
                        self.scratch.push(samples[i * ch]);
                        self.scratch.push(samples[i * ch + 1]);
                    }
                }
            }
            return Ok(Some(&self.scratch));
        }
    }

    pub fn seek(&mut self, secs: f64) -> Result<()> {
        let secs = secs.max(0.0);
        self.reader.seek(
            SeekMode::Accurate,
            SeekTo::Time {
                time: Time::from(secs),
                track_id: Some(self.track_id),
            },
        )?;
        self.decoder.reset();
        Ok(())
    }
}

/// Linear resampler used only when the output device refuses the file's rate.
pub struct Resampler {
    step: f64,
    pos: f64,
    prev: Option<[f32; 2]>,
}

impl Resampler {
    pub fn new(step: f64) -> Self {
        Self {
            step,
            pos: 0.0,
            prev: None,
        }
    }

    pub fn is_passthrough(&self) -> bool {
        (self.step - 1.0).abs() < 1e-9
    }

    pub fn process(&mut self, input: &[f32], out: &mut Vec<f32>) {
        let frames = input.len() / 2;
        if frames == 0 {
            return;
        }
        if self.is_passthrough() {
            out.extend_from_slice(input);
            return;
        }
        let prev = self.prev.unwrap_or([input[0], input[1]]);
        let total = frames + 1;
        let at = |k: usize| -> [f32; 2] {
            if k == 0 {
                prev
            } else {
                [input[(k - 1) * 2], input[(k - 1) * 2 + 1]]
            }
        };
        let mut pos = self.pos;
        while pos + 1.0 < total as f64 {
            let i = pos.floor() as usize;
            let f = (pos - i as f64) as f32;
            let a = at(i);
            let b = at(i + 1);
            out.push(a[0] * (1.0 - f) + b[0] * f);
            out.push(a[1] * (1.0 - f) + b[1] * f);
            pos += self.step;
        }
        self.pos = pos - (total as f64 - 1.0);
        self.prev = Some([input[(frames - 1) * 2], input[(frames - 1) * 2 + 1]]);
    }
}
