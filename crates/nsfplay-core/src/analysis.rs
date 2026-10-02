//! Background analysis: plays a track faster than real time to find its loop or its end.

use std::collections::hash_map::{Entry, HashMap};

use crate::{Detected, Player};

/// Analysis renders at a low rate: neither detector needs full-quality audio.
const RATE: u32 = 16000;

/// Repeats shorter than this are not treated as loops (e.g. an engine toggling between two
/// states after the music has ended; silence detection handles those).
const MIN_LOOP_MS: u32 = 500;

/// Plays `track` of `data` until its loop or its end is found, or `limit_ms` of song time has
/// passed (then the result is [`Detected::Nothing`]).
///
/// Loops are found by hashing the machine state each time the CPU idles between play calls:
/// playback is deterministic, so the first state that repeats marks an exact loop, after a
/// single pass of it and whatever its length. Some engines keep a counter that stops RAM from
/// ever repeating; for those the core's own detector is the fallback. It compares register
/// writes, finds loops up to roughly 80 s, and needs an extra 30 s of playback. The core's
/// silence detection ends tracks that stop.
///
/// Renders in 1 ms slices, taking the render lock for each, so the audio callback never waits
/// long for it: with expansion chips like VRC7, longer slices caused buffer underruns.
pub fn analyze(data: &[u8], track: u32, limit_ms: u32) -> Result<Detected, String> {
    let mut player = Player::load(data)?;
    player.config_set("AUTO_DETECT", 1);
    player.config_set("AUTO_STOP", 1);
    player.set_endless(true);
    player.start(track, RATE);

    let mut first_seen: HashMap<u64, u32> = HashMap::new();
    let mut last = None;
    // the core's loop, and how long to keep looking for an exact one after it
    let mut fallback: Option<(Detected, u32)> = None;
    let mut buf = vec![0i16; (RATE / 1000) as usize * 2];

    while player.elapsed_ms() < limit_ms as u64 {
        player.render(&mut buf);
        let now = player.elapsed_ms() as u32;

        if let Some(h) = player.state_hash().filter(|&h| last != Some(h)) {
            last = Some(h);
            match first_seen.entry(h) {
                Entry::Occupied(e) if now - *e.get() >= MIN_LOOP_MS => {
                    return Ok(Detected::Loop { start_ms: *e.get(), end_ms: now });
                }
                Entry::Occupied(_) => {}
                Entry::Vacant(e) => {
                    e.insert(now);
                }
            }
        }

        match player.detected() {
            found @ Detected::Silence { .. } => return Ok(found),
            found @ Detected::Loop { start_ms, end_ms } if fallback.is_none() => {
                fallback = Some((found, now + (end_ms - start_ms) + 10_000));
            }
            _ => {}
        }
        if let Some((found, until)) = fallback {
            if now >= until {
                return Ok(found);
            }
        }
    }
    Ok(fallback.map_or(Detected::Nothing, |(found, _)| found))
}
