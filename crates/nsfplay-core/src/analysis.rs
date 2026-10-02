//! Background analysis: plays a track faster than real time to find its loop or its end.

use crate::{Detected, Player};

/// Analysis renders at a low rate; detection works on register writes and on silence, which
/// do not need full-quality audio.
const RATE: u32 = 16000;

/// Plays `track` of `data` until the core detects a loop or silence, or `limit_ms` of song time
/// has passed (then the result is [`Detected::Nothing`]).
///
/// The loop detector reports a loop once the last `DETECT_TIME` (30 s) of APU register writes
/// repeats, so this renders roughly intro + loop + 30 s. Takes the global render lock in short
/// slices, so it can run alongside playback.
pub fn analyze(data: &[u8], track: u32, limit_ms: u32) -> Result<Detected, String> {
    let mut player = Player::load(data)?;
    player.config_set("AUTO_DETECT", 1);
    player.config_set("AUTO_STOP", 1);
    player.set_endless(true);
    player.start(track, RATE);
    let mut buf = vec![0i16; RATE as usize / 10 * 2]; // 100 ms slices
    while player.elapsed_ms() < limit_ms as u64 {
        player.render(&mut buf);
        let found = player.detected();
        if found != Detected::Nothing {
            return Ok(found);
        }
    }
    Ok(Detected::Nothing)
}
