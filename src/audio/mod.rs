pub mod decoder;
pub mod output;

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use anyhow::Result;

use decoder::{AudioDecoder, Resampler};
use output::Output;

pub const TAP_LEN: usize = 4096;

pub struct Tap {
    pub data: Vec<f32>,
    pub pos: usize,
}

pub struct Shared {
    pub ring: Mutex<VecDeque<f32>>,
    pub tap: Mutex<Tap>,
    pub pos_frames: AtomicU64,
    pub rate: AtomicU32,
    pub base_ms: AtomicU64,
    pub duration_ms: AtomicU64,
    pub paused: AtomicBool,
    pub volume: AtomicU32,
    pub ended: AtomicBool,
    pub eof: AtomicBool,
    pub generation: AtomicU64,
    pub error: Mutex<Option<String>>,
    pub alive: AtomicBool,
}

impl Shared {
    fn new(volume: f32) -> Self {
        Self {
            ring: Mutex::new(VecDeque::with_capacity(1 << 15)),
            tap: Mutex::new(Tap {
                data: vec![0.0; TAP_LEN],
                pos: 0,
            }),
            pos_frames: AtomicU64::new(0),
            rate: AtomicU32::new(44_100),
            base_ms: AtomicU64::new(0),
            duration_ms: AtomicU64::new(0),
            paused: AtomicBool::new(true),
            volume: AtomicU32::new(volume.to_bits()),
            ended: AtomicBool::new(false),
            eof: AtomicBool::new(false),
            generation: AtomicU64::new(0),
            error: Mutex::new(None),
            alive: AtomicBool::new(true),
        }
    }
}

pub enum Cmd {
    Load(PathBuf, f64),
    Toggle,
    Seek(f64),
    Volume(f32),
    Stop,
    Quit,
}

enum WorkCmd {
    Open {
        path: PathBuf,
        start: f64,
        gen: u64,
        step: f64,
    },
    Stop,
}

struct Job {
    dec: AudioDecoder,
    rs: Resampler,
    gen: u64,
}

pub struct Player {
    tx: Sender<Cmd>,
    shared: Arc<Shared>,
}

impl Player {
    pub fn spawn(volume: f32) -> Player {
        let (tx, rx) = channel();
        let shared = Arc::new(Shared::new(volume));
        let shared2 = shared.clone();
        thread::spawn(move || {
            if let Err(e) = engine(rx, shared2.clone()) {
                set_error(&shared2, e.to_string());
            }
            shared2.alive.store(false, Ordering::Relaxed);
        });
        Player { tx, shared }
    }

    pub fn load(&self, path: &Path, start: f64) {
        let _ = self.tx.send(Cmd::Load(path.to_path_buf(), start));
    }
    pub fn toggle(&self) {
        let _ = self.tx.send(Cmd::Toggle);
    }
    pub fn seek(&self, secs: f64) {
        let _ = self.tx.send(Cmd::Seek(secs));
    }
    pub fn set_volume(&self, v: f32) {
        let _ = self.tx.send(Cmd::Volume(v.clamp(0.0, 1.5)));
    }
    pub fn stop(&self) {
        let _ = self.tx.send(Cmd::Stop);
    }
    pub fn quit(&self) {
        let _ = self.tx.send(Cmd::Quit);
    }

    pub fn paused(&self) -> bool {
        self.shared.paused.load(Ordering::Relaxed)
    }
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.shared.duration_ms.load(Ordering::Relaxed))
    }
    pub fn position(&self) -> Duration {
        let rate = self.shared.rate.load(Ordering::Relaxed).max(1) as u64;
        let frames = self.shared.pos_frames.load(Ordering::Relaxed);
        let ms = self.shared.base_ms.load(Ordering::Relaxed) + frames * 1000 / rate;
        Duration::from_millis(ms)
    }
    pub fn rate(&self) -> u32 {
        self.shared.rate.load(Ordering::Relaxed)
    }

    pub fn tap(&self, out: &mut Vec<f32>) {
        let tap = self.shared.tap.lock().unwrap_or_else(|e| e.into_inner());
        out.clear();
        out.reserve(TAP_LEN);
        out.extend_from_slice(&tap.data[tap.pos..]);
        out.extend_from_slice(&tap.data[..tap.pos]);
    }

    pub fn take_ended(&self) -> bool {
        self.shared.ended.swap(false, Ordering::Relaxed)
    }
    pub fn take_error(&self) -> Option<String> {
        self.shared
            .error
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take()
    }
    pub fn alive(&self) -> bool {
        self.shared.alive.load(Ordering::Relaxed)
    }
}

fn set_error(shared: &Arc<Shared>, msg: String) {
    *shared.error.lock().unwrap_or_else(|e| e.into_inner()) = Some(msg);
}

fn engine(rx: Receiver<Cmd>, shared: Arc<Shared>) -> Result<()> {
    let mut out = Output::new()?;
    let (wtx, wrx) = channel::<WorkCmd>();
    let ws = shared.clone();
    thread::spawn(move || worker(wrx, ws));

    let mut gen: u64 = 0;
    let mut current: Option<PathBuf> = None;

    loop {
        let cmd = match rx.recv_timeout(Duration::from_millis(80)) {
            Ok(c) => c,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => break,
        };
        match cmd {
            Cmd::Load(path, start) => match load(&mut out, &shared, &wtx, &mut gen, &path, start) {
                Ok(()) => current = Some(path),
                Err(e) => set_error(&shared, e.to_string()),
            },
            Cmd::Seek(t) => {
                if let Some(path) = current.clone() {
                    let paused = shared.paused.load(Ordering::Relaxed);
                    match load(&mut out, &shared, &wtx, &mut gen, &path, t) {
                        Ok(()) => shared.paused.store(paused, Ordering::Relaxed),
                        Err(e) => set_error(&shared, e.to_string()),
                    }
                }
            }
            Cmd::Toggle => {
                let p = shared.paused.load(Ordering::Relaxed);
                shared.paused.store(!p, Ordering::Relaxed);
            }
            Cmd::Volume(v) => shared.volume.store(v.to_bits(), Ordering::Relaxed),
            Cmd::Stop => {
                gen += 1;
                shared.generation.store(gen, Ordering::Relaxed);
                shared.ring.lock().unwrap_or_else(|e| e.into_inner()).clear();
                shared.eof.store(true, Ordering::Relaxed);
                shared.ended.store(false, Ordering::Relaxed);
                shared.pos_frames.store(0, Ordering::Relaxed);
                shared.base_ms.store(0, Ordering::Relaxed);
                shared.duration_ms.store(0, Ordering::Relaxed);
                shared.paused.store(true, Ordering::Relaxed);
                let _ = wtx.send(WorkCmd::Stop);
                current = None;
            }
            Cmd::Quit => {
                shared.paused.store(true, Ordering::Relaxed);
                let _ = wtx.send(WorkCmd::Stop);
                break;
            }
        }
    }
    Ok(())
}

fn load(
    out: &mut Output,
    shared: &Arc<Shared>,
    wtx: &Sender<WorkCmd>,
    gen: &mut u64,
    path: &Path,
    start: f64,
) -> Result<()> {
    let probe = AudioDecoder::open(path)?;
    let src_rate = probe.rate;
    let duration = probe.duration.unwrap_or(0.0);
    drop(probe);

    let out_rate = out.ensure(src_rate, shared.clone())?;

    *gen += 1;
    shared.generation.store(*gen, Ordering::Relaxed);
    shared.ring.lock().unwrap_or_else(|e| e.into_inner()).clear();
    shared.pos_frames.store(0, Ordering::Relaxed);
    shared.rate.store(out_rate, Ordering::Relaxed);
    shared.base_ms.store((start.max(0.0) * 1000.0) as u64, Ordering::Relaxed);
    shared
        .duration_ms
        .store((duration * 1000.0) as u64, Ordering::Relaxed);
    shared.eof.store(false, Ordering::Relaxed);
    shared.ended.store(false, Ordering::Relaxed);
    shared.paused.store(false, Ordering::Relaxed);

    let step = src_rate as f64 / out_rate as f64;
    let _ = wtx.send(WorkCmd::Open {
        path: path.to_path_buf(),
        start: start.max(0.0),
        gen: *gen,
        step,
    });
    Ok(())
}

fn worker(rx: Receiver<WorkCmd>, shared: Arc<Shared>) {
    let mut job: Option<Job> = None;

    loop {
        // Non blocking: absorb every pending command before decoding more.
        loop {
            match rx.try_recv() {
                Ok(cmd) => apply(cmd, &mut job, &shared),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }

        if job.is_none() {
            match rx.recv() {
                Ok(cmd) => apply(cmd, &mut job, &shared),
                Err(_) => return,
            }
            continue;
        }

        let current_gen = shared.generation.load(Ordering::Relaxed);
        if job.as_ref().map(|j| j.gen != current_gen).unwrap_or(false) {
            job = None;
            continue;
        }

        let cap = shared.rate.load(Ordering::Relaxed).max(8_000) as usize * 2 * 3;
        if shared
            .ring
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .len()
            > cap
        {
            thread::sleep(Duration::from_millis(7));
            continue;
        }

        let mut finished = false;
        {
            let j = job.as_mut().unwrap();
            match j.dec.next_chunk() {
                Ok(Some(samples)) => {
                    let mut buf: Vec<f32> = Vec::with_capacity(samples.len() + 8);
                    if j.rs.is_passthrough() {
                        buf.extend_from_slice(samples);
                    } else {
                        j.rs.process(samples, &mut buf);
                    }
                    if !buf.is_empty() {
                        shared
                            .ring
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .extend(buf);
                    }
                }
                Ok(None) => {
                    shared.eof.store(true, Ordering::Relaxed);
                    finished = true;
                }
                Err(e) => {
                    set_error(&shared, format!("decode error: {e}"));
                    shared.eof.store(true, Ordering::Relaxed);
                    finished = true;
                }
            }
        }
        if finished {
            job = None;
        }
    }
}

fn apply(cmd: WorkCmd, job: &mut Option<Job>, shared: &Arc<Shared>) {
    match cmd {
        WorkCmd::Open {
            path,
            start,
            gen,
            step,
        } => {
            let mut dec = match AudioDecoder::open(&path) {
                Ok(d) => d,
                Err(e) => {
                    set_error(&shared, format!("{}: {e}", path.display()));
                    return;
                }
            };
            if start > 0.05 {
                if let Err(e) = dec.seek(start) {
                    set_error(&shared, format!("seek failed: {e}"));
                }
            }
            shared.eof.store(false, Ordering::Relaxed);
            *job = Some(Job {
                dec,
                rs: Resampler::new(step),
                gen,
            });
        }
        WorkCmd::Stop => *job = None,
    }
}
