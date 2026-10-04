//! The CD's music (0x80014804 and the calls around it, `iface_sound`'s
//! slots 2 to 6): thirteen songs, disc tracks 2 to 14, each played from its
//! start and around again at its end (the position reports, 0x80014754).
//! The game asks; whoever owns the drive carries the asks out.

/// How many songs there are.
pub const SONGS: u8 = 13;

/// What the game asks of the CD.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CdAsk {
    /// 0x80014804: song `n` (disc track n + 2) from its start, round and
    /// round.
    Play(u8),
    /// 0x800149b8: stopped, back to the song's start.
    Stop,
    /// 0x80014a0c: held where it is.
    Pause,
    /// 0x80014a94: on from where it was.
    Resume,
    /// 0x80014b6c: the volume, from the music setting (0 to 255).
    Volume(u8),
}

/// The songs' artists and titles (0x800be8cc and 0x800be900, by song),
/// as the Boom Box shows them.
pub fn songs(byte: &dyn Fn(u32) -> u8) -> Vec<(String, String)> {
    let word = |a: u32| u32::from_le_bytes([byte(a), byte(a + 1), byte(a + 2), byte(a + 3)]);
    let text = |a: u32| (0..64).map(|k| byte(a + k)).take_while(|&b| b != 0).map(char::from).collect::<String>();
    (0..SONGS as u32).map(|k| (text(word(0x800b_e8cc + 4 * k)), text(word(0x800b_e900 + 4 * k)))).collect()
}
