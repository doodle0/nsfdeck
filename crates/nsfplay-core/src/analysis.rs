//! Background analysis: plays a track faster than real time to find its loop or its end.

use std::collections::hash_map::{Entry, HashMap};

use crate::{Detected, Player};

/// Analysis renders at a low rate: the detectors don't need full-quality audio.
const RATE: u32 = 16000;

/// Repeats shorter than this are not treated as loops (e.g. an engine toggling between two
/// states after the music has ended; silence detection handles those).
const MIN_LOOP_MS: u32 = 500;

/// How long the song-data reads must keep repeating beyond one full pass before a loop is
/// accepted, so a phrase repeated inside the song (AABA) is not mistaken for it.
const CONFIRM_MS: u32 = 30_000;

/// Plays `track` of `data` until its loop or its end is found, or `limit_ms` of song time has
/// passed (then the result is [`Detected::Nothing`]).
///
/// Two detectors run side by side:
///
/// - **Song-data reads** (the main one): every CPU read is traced, keeping only reads of bytes
///   not read in the last second. That leaves the song data, which the engine reads once per
///   pass, and drops code and lookup tables read every frame. Once the engine jumps back to
///   the loop point, the same bytes are read again one loop length later, regardless of
///   random number generators, free-running vibrato or fractional tempos (see
///   [`ReadPeriods`]). A loop is accepted once the reads have repeated for a full pass plus
///   [`CONFIRM_MS`].
/// - **Machine state:** RAM, WRAM and the bank map, hashed while the CPU idles between play
///   calls. When it repeats, the loop is exact after a single pass, so this often answers first.
///
/// The core's silence detection ends tracks that stop. (The core's own loop detector is not
/// used: it can't hold loops longer than about 80 s, and reports false ones.)
///
/// Renders in 1 ms slices, taking the render lock for each, so the audio callback never waits
/// long for it: with expansion chips like VRC7, longer slices caused buffer underruns.
pub fn analyze(data: &[u8], track: u32, limit_ms: u32) -> Result<Detected, String> {
    let mut player = Player::load(data)?;
    player.config_set("AUTO_STOP", 1);
    player.set_endless(true);
    player.trace_start();
    player.start(track, RATE);

    let mut state = StateLoops::default();
    let mut reads = ReadPeriods::default();
    let mut traced = Vec::new();
    let mut buf = vec![0i16; (RATE / 1000) as usize * 2];

    while player.elapsed_ms() < limit_ms as u64 {
        player.render(&mut buf);
        let now = player.elapsed_ms() as u32;
        player.trace_take(&mut traced);
        for (key, at) in traced.drain(..) {
            if let Some(found) = reads.push(key, at, now) {
                return Ok(found);
            }
        }
        if let Some(found) = player.state_hash().and_then(|h| state.push(h, now)) {
            return Ok(found);
        }
        if let found @ Detected::Silence { .. } = player.detected() {
            return Ok(found);
        }
    }
    Ok(Detected::Nothing)
}

/// Finds the first exact repeat of the machine state.
#[derive(Default)]
struct StateLoops {
    first_seen: HashMap<u64, u32>,
    last: Option<u64>,
}

impl StateLoops {
    fn push(&mut self, hash: u64, now: u32) -> Option<Detected> {
        if self.last == Some(hash) {
            return None;
        }
        self.last = Some(hash);
        match self.first_seen.entry(hash) {
            Entry::Occupied(e) if now - *e.get() >= MIN_LOOP_MS => {
                Some(Detected::Loop { start_ms: *e.get(), end_ms: now })
            }
            Entry::Occupied(_) => None,
            Entry::Vacant(e) => {
                e.insert(now);
                None
            }
        }
    }
}

/// Finds the loop in the trace of song-data reads: the period P such that nearly every read
/// of a byte was matched by a read of the same byte P earlier.
///
/// Matching is statistical rather than exact because a few reads differ between passes: a
/// byte re-read about a second apart counts as fresh on one pass and not on the next. On real
/// music (Mega Man 5, Capcom's engine) the true period matched 99.8% of reads, against at most
/// 48% for phrase-length impostors.
///
/// Candidate periods are the most common gaps between reads of the same byte. Every
/// [`CHECK_MS`], each is refined to the millisecond and scored over the most recent pass plus
/// [`CONFIRM_MS`]; the shortest one scoring at least [`MATCH_RATE`] is the loop, and its start
/// is where the matching begins.
#[derive(Default)]
struct ReadPeriods {
    /// (key, song ms), in order
    events: Vec<(u32, u32)>,
    /// Read times of each key, in order.
    reads: HashMap<u32, Vec<u32>>,
    /// Count of gaps between reads of the same key, in GAP_BIN_MS bins.
    gaps: HashMap<u32, u32>,
    next_check: u32,
}

const CHECK_MS: u32 = 5_000;
const GAP_BIN_MS: u32 = 50;
const MATCH_TOLERANCE_MS: u32 = 40;
const MATCH_RATE: f64 = 0.9;
/// While walking back to the loop start, a second whose reads match at least this often is
/// still inside the loop. Within the loop the second pass can dip to ~60% against the first
/// (bytes counted fresh on one pass but not the other), while before it seconds match <20%.
const BOUNDARY_RATE: f64 = 0.5;
const CANDIDATES: usize = 8;
/// Fewest reads a span needs to be scored. Real music reads ~100+ song bytes per second; a
/// span with only a handful of reads (e.g. a code path run every 256 frames) proves nothing.
const MIN_READS: usize = 200;

impl ReadPeriods {
    fn push(&mut self, key: u32, at: u32, now: u32) -> Option<Detected> {
        self.events.push((key, at));
        let times = self.reads.entry(key).or_default();
        for &u in times.iter().rev().take(16) {
            if at - u >= MIN_LOOP_MS {
                *self.gaps.entry((at - u) / GAP_BIN_MS).or_default() += 1;
            }
        }
        times.push(at);
        if now < self.next_check {
            return None;
        }
        self.next_check = now + CHECK_MS;
        self.check(now)
    }

    /// Was `key` read within the tolerance of `at`?
    fn read_near(&self, key: u32, at: i64) -> bool {
        let Some(times) = self.reads.get(&key) else { return false };
        let i = times.partition_point(|&u| (u as i64) < at - MATCH_TOLERANCE_MS as i64);
        times.get(i).is_some_and(|&u| (u as i64) <= at + MATCH_TOLERANCE_MS as i64)
    }

    /// Fraction of the reads in `[from, to)` matched by a read `period` earlier, or `None`
    /// if there are fewer than `min_reads` of them.
    fn score(&self, period: u32, from: u32, to: u32, min_reads: usize) -> Option<f64> {
        let lo = self.events.partition_point(|e| e.1 < from);
        let hi = self.events.partition_point(|e| e.1 < to);
        if hi - lo < min_reads {
            return None;
        }
        let hits = self.events[lo..hi]
            .iter()
            .filter(|&&(k, t)| self.read_near(k, t as i64 - period as i64))
            .count();
        Some(hits as f64 / (hi - lo) as f64)
    }

    fn check(&self, now: u32) -> Option<Detected> {
        let mut bins: Vec<(u32, u32)> = self.gaps.iter().map(|(&b, &n)| (b, n)).collect();
        bins.sort_by_key(|&(b, n)| (std::cmp::Reverse(n), b));
        let mut found: Option<(u32, u32)> = None; // (start, period)
        for &(bin, _) in bins.iter().take(CANDIDATES) {
            let rough = bin * GAP_BIN_MS + GAP_BIN_MS / 2;
            // the reads must have repeated for a full pass plus CONFIRM_MS
            let span = rough + CONFIRM_MS;
            if now < rough + span || found.is_some_and(|(_, p)| p <= rough) {
                continue;
            }
            // Every period within the match tolerance of the true one scores (nearly) the same,
            // so take the middle of the best-scoring plateau.
            let scored: Vec<(u32, f64)> = (rough.saturating_sub(GAP_BIN_MS)..=rough + GAP_BIN_MS)
                .step_by(2)
                .filter_map(|p| Some((p, self.score(p, now - span, now, MIN_READS)?)))
                .collect();
            let Some(rate) = scored.iter().map(|s| s.1).max_by(f64::total_cmp) else { continue };
            let plateau: Vec<u32> = scored.iter().filter(|s| s.1 >= rate - 0.002).map(|s| s.0).collect();
            let period = (plateau[0] + plateau[plateau.len() - 1]) / 2;
            if rate >= MATCH_RATE {
                found = Some((self.loop_start(period, now), period));
            }
        }
        found.map(|(start_ms, period)| Detected::Loop { start_ms, end_ms: start_ms + period })
    }

    /// Where the repetition with `period` begins: walks back from `now` (inside the repeating
    /// part) one second at a time while the match holds, then pins the boundary to the first
    /// matching read from which the following reads keep matching.
    fn loop_start(&self, period: u32, now: u32) -> u32 {
        let mut t = now;
        while t >= period + 1000 {
            match self.score(period, t - 1000, t, 16) {
                Some(rate) if rate < BOUNDARY_RATE => break,
                _ => t -= 1000,
            }
        }
        // first read in the boundary second from which the following reads keep matching
        let lo = self.events.partition_point(|e| e.1 < t.saturating_sub(1000));
        let hi = self.events.partition_point(|e| e.1 < t + 1000);
        let matched = |i: usize| {
            let (k, at) = self.events[i];
            at >= period && self.read_near(k, at as i64 - period as i64)
        };
        let first = (lo..hi)
            .find(|&i| matched(i) && (i..(i + 16).min(self.events.len())).filter(|&j| matched(j)).count() >= 12)
            .unwrap_or(hi.min(self.events.len() - 1));
        self.events[first].1.max(period) - period
    }
}
