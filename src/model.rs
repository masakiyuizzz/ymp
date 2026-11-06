use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Track {
    pub path: PathBuf,
    pub folder: PathBuf,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub track_no: Option<u32>,
    pub year: Option<u32>,
    pub duration: Duration,
    pub cover: Option<Arc<Vec<u8>>>,
}

#[allow(dead_code)]
pub struct Library {
    pub root: PathBuf,
    pub tracks: Vec<Track>,
    pub album_count: usize,
}

impl Library {
    pub fn total_duration(&self) -> Duration {
        self.tracks
            .iter()
            .fold(Duration::ZERO, |acc, t| acc + t.duration)
    }
}
