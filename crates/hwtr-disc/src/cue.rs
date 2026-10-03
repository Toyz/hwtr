//! CUE sheets: one FILE per track, as the Redump-style dump of this disc has.

use std::path::{Path, PathBuf};

use crate::{Error, RAW_SECTOR, Result};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TrackKind {
    Mode1Raw,
    Mode2Raw,
    Audio,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub number: u32,
    pub kind: TrackKind,
    pub file: PathBuf,
    /// INDEX 00, in sectors from the start of the track's file, when present.
    pub index0: Option<u32>,
    /// INDEX 01, in sectors from the start of the track's file.
    pub index1: u32,
}

impl Track {
    /// Sectors in the track's file, read from its length on disk.
    pub fn file_sectors(&self) -> Result<u32> {
        let len = std::fs::metadata(&self.file).map_err(|e| Error::Io(self.file.clone(), e))?.len();
        Ok((len / RAW_SECTOR as u64) as u32)
    }
}

#[derive(Clone, Debug)]
pub struct Cue {
    pub path: PathBuf,
    pub tracks: Vec<Track>,
}

fn msf_to_sectors(text: &str) -> Result<u32> {
    let parts: Vec<u32> = text
        .split(':')
        .map(|p| p.parse::<u32>())
        .collect::<std::result::Result<_, _>>()
        .map_err(|_| Error::Cue(format!("bad MSF {text:?}")))?;
    match parts.as_slice() {
        [m, s, f] => Ok((m * 60 + s) * 75 + f),
        _ => Err(Error::Cue(format!("bad MSF {text:?}"))),
    }
}

impl Cue {
    pub fn load(path: &Path) -> Result<Cue> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::Io(path.to_path_buf(), e))?;
        Cue::parse(&text, path.parent().unwrap_or(Path::new(".")), path)
    }

    pub fn parse(text: &str, dir: &Path, path: &Path) -> Result<Cue> {
        let mut tracks: Vec<Track> = Vec::new();
        let mut file: Option<PathBuf> = None;
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("FILE ") {
                let name = rest
                    .strip_prefix('"')
                    .and_then(|r| r.rfind('"').map(|end| &r[..end]))
                    .ok_or_else(|| Error::Cue(format!("unquoted FILE {rest:?}")))?;
                file = Some(dir.join(name));
            } else if let Some(rest) = line.strip_prefix("TRACK ") {
                let mut words = rest.split_whitespace();
                let number = words.next().and_then(|n| n.parse().ok()).ok_or_else(|| Error::Cue(line.into()))?;
                let kind = match words.next() {
                    Some("MODE2/2352") => TrackKind::Mode2Raw,
                    Some("MODE1/2352") => TrackKind::Mode1Raw,
                    Some("AUDIO") => TrackKind::Audio,
                    other => return Err(Error::Cue(format!("track {number}: unsupported type {other:?}"))),
                };
                let file = file.clone().ok_or_else(|| Error::Cue(format!("track {number} before any FILE")))?;
                tracks.push(Track { number, kind, file, index0: None, index1: 0 });
            } else if let Some(rest) = line.strip_prefix("INDEX ") {
                let track = tracks.last_mut().ok_or_else(|| Error::Cue("INDEX before any TRACK".into()))?;
                let mut words = rest.split_whitespace();
                let which: u32 = words.next().and_then(|n| n.parse().ok()).ok_or_else(|| Error::Cue(line.into()))?;
                let at = msf_to_sectors(words.next().ok_or_else(|| Error::Cue(line.into()))?)?;
                match which {
                    0 => track.index0 = Some(at),
                    1 => track.index1 = at,
                    _ => {}
                }
            }
        }
        Ok(Cue { path: path.to_path_buf(), tracks })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_data_and_audio_tracks() {
        let text = "FILE \"a (Track 01).bin\" BINARY\n  TRACK 01 MODE2/2352\n    INDEX 01 00:00:00\n\
                    FILE \"a (Track 02).bin\" BINARY\n  TRACK 02 AUDIO\n    INDEX 00 00:00:00\n    INDEX 01 00:02:00\n";
        let cue = Cue::parse(text, Path::new("/d"), Path::new("/d/a.cue")).unwrap();
        assert_eq!(cue.tracks.len(), 2);
        assert_eq!(cue.tracks[0].kind, TrackKind::Mode2Raw);
        assert_eq!(cue.tracks[1].kind, TrackKind::Audio);
        assert_eq!(cue.tracks[1].index0, Some(0));
        assert_eq!(cue.tracks[1].index1, 150);
        assert_eq!(cue.tracks[1].file, Path::new("/d/a (Track 02).bin"));
    }
}
