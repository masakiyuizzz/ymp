use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::Arc;
use std::time::Duration;

use lofty::prelude::*;
use lofty::probe::Probe;
use walkdir::WalkDir;

use crate::model::{Library, Track};

const AUDIO_EXT: &[&str] = &[
    "mp3", "flac", "m4a", "mp4", "aac", "ogg", "oga", "opus", "wav", "aiff", "aif", "alac", "wv",
    "ape", "mpc", "mka",
];

const COVER_NAMES: &[&str] = &[
    "cover", "folder", "front", "album", "albumart", "artwork", "folderart",
];
const COVER_EXT: &[&str] = &["jpg", "jpeg", "png", "webp"];

pub enum ScanMsg {
    Progress(usize, PathBuf),
    Done(Box<Library>),
    Failed(String),
}

pub fn is_audio(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| AUDIO_EXT.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn loose_cover(folder: &Path) -> Option<Arc<Vec<u8>>> {
    let entries = std::fs::read_dir(folder).ok()?;
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stem = path
            .file_stem()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        let ext = path
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| s.to_ascii_lowercase())
            .unwrap_or_default();
        if COVER_NAMES.contains(&stem.as_str()) && COVER_EXT.contains(&ext.as_str()) {
            if let Ok(bytes) = std::fs::read(&path) {
                return Some(Arc::new(bytes));
            }
        }
    }
    None
}

fn read_track(path: &Path) -> Track {
    let folder = path.parent().unwrap_or(Path::new(".")).to_path_buf();
    let fallback_title = path
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown".into());

    let mut title = fallback_title.clone();
    let mut artist = String::new();
    let mut album = String::new();
    let mut track_no = None;
    let mut year = None;
    let mut duration = Duration::ZERO;
    let mut cover: Option<Arc<Vec<u8>>> = None;

    if let Ok(tagged) = Probe::open(path).and_then(|p| p.read()) {
        duration = tagged.properties().duration();
        for tag in tagged.tags() {
            if !title.is_empty() && title != fallback_title {
                break;
            }
            if let Some(t) = tag.title() {
                let t = t.trim().to_string();
                if !t.is_empty() {
                    title = t;
                }
            }
            if artist.is_empty() {
                if let Some(a) = tag.artist() {
                    artist = a.trim().to_string();
                }
            }
            if album.is_empty() {
                if let Some(a) = tag.album() {
                    album = a.trim().to_string();
                }
            }
            if track_no.is_none() {
                track_no = tag.track();
            }
            if year.is_none() {
                year = tag.year();
            }
            if cover.is_none() {
                if let Some(pic) = tag.pictures().first() {
                    if !pic.data().is_empty() {
                        cover = Some(Arc::new(pic.data().to_vec()));
                    }
                }
            }
        }
    }

    if album.is_empty() {
        album = folder
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_default();
    }
    if artist.is_empty() {
        artist = "Unknown Artist".to_string();
    }

    Track {
        path: path.to_path_buf(),
        folder,
        title,
        artist,
        album,
        track_no,
        year,
        duration,
        cover,
    }
}

pub fn scan(root: &Path, recursive: bool, tx: Option<&Sender<ScanMsg>>) -> Library {
    let mut files: Vec<PathBuf> = Vec::new();
    let walker = WalkDir::new(root)
        .max_depth(if recursive { 16 } else { 1 })
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| {
            let name = e.file_name().to_string_lossy();
            !(e.file_type().is_dir() && (name.starts_with('.') || name == "node_modules"))
        });

    for entry in walker.flatten() {
        if entry.file_type().is_file() && is_audio(entry.path()) {
            files.push(entry.path().to_path_buf());
            if files.len() % 8 == 0 {
                if let Some(tx) = tx {
                    let _ = tx.send(ScanMsg::Progress(files.len(), entry.path().to_path_buf()));
                }
            }
        }
    }

    let mut covers: HashMap<PathBuf, Option<Arc<Vec<u8>>>> = HashMap::new();
    let mut tracks: Vec<Track> = Vec::with_capacity(files.len());
    for (i, file) in files.iter().enumerate() {
        let mut track = read_track(file);
        if track.cover.is_none() {
            let folder = track.folder.clone();
            let cached = covers.entry(folder.clone()).or_insert_with(|| loose_cover(&folder));
            track.cover = cached.clone();
        }
        tracks.push(track);
        if let Some(tx) = tx {
            if i % 8 == 0 {
                let _ = tx.send(ScanMsg::Progress(i + 1, file.clone()));
            }
        }
    }

    tracks.sort_by(|a, b| {
        a.album
            .to_lowercase()
            .cmp(&b.album.to_lowercase())
            .then(a.track_no.unwrap_or(u32::MAX).cmp(&b.track_no.unwrap_or(u32::MAX)))
            .then(a.title.to_lowercase().cmp(&b.title.to_lowercase()))
            .then(a.path.cmp(&b.path))
    });

    let mut albums: Vec<String> = tracks
        .iter()
        .map(|t| {
            if t.album.is_empty() {
                t.folder.display().to_string()
            } else {
                t.album.clone()
            }
        })
        .collect();
    albums.sort();
    albums.dedup();

    Library {
        root: root.to_path_buf(),
        album_count: albums.len(),
        tracks,
    }
}

pub fn spawn_scan(root: PathBuf, recursive: bool) -> Receiver<ScanMsg> {
    let (tx, rx) = channel();
    std::thread::spawn(move || {
        if !root.exists() {
            let _ = tx.send(ScanMsg::Failed(format!(
                "folder not found: {}",
                root.display()
            )));
            return;
        }
        let lib = scan(&root, recursive, Some(&tx));
        if lib.tracks.is_empty() {
            let _ = tx.send(ScanMsg::Failed(format!(
                "no playable audio files in {}",
                root.display()
            )));
        } else {
            let _ = tx.send(ScanMsg::Done(Box::new(lib)));
        }
    });
    rx
}
