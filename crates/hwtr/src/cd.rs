//! The CD drive's music, as the game asks for it ([`CdAsk`]): song n is
//! the disc's track n + 2, read whole off the image into memory and played
//! through the sound chip's mixer, looping.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use hwtr_game::cd::CdAsk;

use crate::spu::Spu;

pub struct CdPlayer {
    /// Each audio track's file and where in it the track starts (sectors).
    tracks: HashMap<u32, (PathBuf, u32)>,
    loaded: HashMap<u8, Arc<[i16]>>,
    spu: Arc<Mutex<Spu>>,
    /// The song last asked for (0x800d2484).
    pub song: u8,
}

impl CdPlayer {
    pub fn new(cue: &Path, spu: Arc<Mutex<Spu>>) -> Result<CdPlayer, String> {
        let disc = rrt::disc::Image::open(cue).map_err(|e| e.to_string())?;
        let cue = disc.cue.as_ref().ok_or("the disc was not opened through a CUE sheet")?;
        let tracks = cue
            .tracks
            .iter()
            .filter(|t| t.kind == rrt::disc::cue::TrackKind::Audio)
            .map(|t| (t.number, (t.file.clone(), t.index1)))
            .collect();
        if let Ok(mut s) = spu.lock() {
            s.cd_volume(128);
        }
        Ok(CdPlayer { tracks, loaded: HashMap::new(), spu, song: 0 })
    }

    /// Song `n`'s samples, read the first time.
    fn pcm(&mut self, n: u8) -> Option<Arc<[i16]>> {
        if let Some(p) = self.loaded.get(&n) {
            return Some(p.clone());
        }
        let (file, index1) = self.tracks.get(&(n as u32 + 2))?;
        let bytes = std::fs::read(file).map_err(|e| rrt::tracing::warn!("{}: {e}", file.display())).ok()?;
        let from = (*index1 as usize * rrt::disc::RAW_SECTOR).min(bytes.len());
        let pcm: Arc<[i16]> = bytes[from..].chunks_exact(2).map(|b| i16::from_le_bytes([b[0], b[1]])).collect();
        self.loaded.insert(n, pcm.clone());
        Some(pcm)
    }

    /// 0x80014b6c: the music volume from the settings (0 to 255).
    pub fn set_volume(&mut self, v: u8) {
        let v = ((v as i32 * 127) / 255).min(128);
        if let Ok(mut s) = self.spu.lock() {
            s.cd_volume(v);
        }
    }

    pub fn ask(&mut self, ask: CdAsk) {
        match ask {
            CdAsk::Play(n) => {
                self.song = n;
                let Some(pcm) = self.pcm(n) else { return };
                rrt::tracing::info!("cd: song {n} (track {})", n + 2);
                if let Ok(mut s) = self.spu.lock() {
                    s.cd_play(pcm);
                }
            }
            CdAsk::Stop => {
                if let Ok(mut s) = self.spu.lock() {
                    s.cd_stop();
                }
            }
            CdAsk::Pause => {
                if let Ok(mut s) = self.spu.lock() {
                    s.cd_pause();
                }
            }
            CdAsk::Volume(v) => self.set_volume(v),
            CdAsk::Resume => {
                if let Ok(mut s) = self.spu.lock() {
                    s.cd_resume();
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rrt::audio::Source;

    /// Song 0 (track 2) plays off the image, holds silent when paused, and
    /// goes on where it was.
    #[test]
    fn a_song_plays_and_pauses() {
        let work = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../work");
        let Ok(cue) = rrt::disc::Image::find(&work.join("disc")) else {
            eprintln!("skipped: no disc under work/disc");
            return;
        };
        let spu = Arc::new(Mutex::new(Spu::default()));
        let mut cd = CdPlayer::new(&cue, spu.clone()).expect("the CD");
        cd.ask(CdAsk::Play(0));
        let loud = |spu: &Arc<Mutex<Spu>>| {
            let mut out = vec![0i16; 2 * 44100];
            spu.lock().unwrap().render(&mut out);
            out.iter().map(|v| v.unsigned_abs() as u64).sum::<u64>() / out.len() as u64
        };
        let _ = loud(&spu); // the song's quiet opening
        let playing = loud(&spu);
        assert!(playing > 300, "song 0 is quiet: {playing}");
        cd.ask(CdAsk::Pause);
        assert_eq!(loud(&spu), 0, "still playing when paused");
        cd.ask(CdAsk::Resume);
        assert!(loud(&spu) > 300, "not going on");
    }
}
