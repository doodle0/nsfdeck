//! Background analysis: plays a track faster than real time to find its loop or its end.

use std::collections::hash_map::{Entry, HashMap};

use crate::{ChannelState, Detected, Player};

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
/// Three detectors watch the same playback, and the first to answer wins (docs/loop-detection.md
/// explains each rule and how to check changes):
///
/// - **Song-data reads** (the main one): every CPU read is traced, keeping only reads of bytes
///   not read in the last second. That leaves the song data, which the engine reads once per
///   pass, and drops code and lookup tables read every frame. Once the engine jumps back to
///   the loop point, the same bytes are read again one loop length later, regardless of
///   random number generators, free-running vibrato or fractional tempos (see
///   [`ReadPeriods`]). A loop is accepted once the reads have repeated for a full pass plus
///   [`CONFIRM_MS`].
/// - **Machine state:** RAM, WRAM and the bank map, hashed while the CPU idles between play
///   calls. When it repeats, the music repeats exactly after a single pass, so this often
///   answers first. The state can take several passes of the music to repeat, so
///   [`ReadPeriods::shorten`] checks fractions of its period against the reads and the notes.
/// - **Silence:** the core's detection ends tracks that stop.
///
/// (The core's own loop detector is not used: it can't hold loops longer than about 80 s, and
/// reports false ones.)
///
/// Renders in 1 ms slices, taking the render lock for each, so the audio callback never waits
/// long for it: with expansion chips like VRC7, longer slices caused buffer underruns.
pub fn analyze(data: &[u8], track: u32, limit_ms: u32) -> Result<Detected, String> {
    let mut player = Player::load(data)?;
    player.config_set("AUTO_STOP", 1);
    player.set_endless(true);
    player.trace_start();
    player.start(track, RATE);

    let mut notes = Notes::new(&player);
    let mut state = StateLoops::default();
    let mut reads = ReadPeriods::default();
    let mut traced = Vec::new();
    let mut buf = vec![0i16; (RATE / 1000) as usize * 2];

    while player.elapsed_ms() < limit_ms as u64 {
        player.render(&mut buf);
        let now = player.elapsed_ms() as u32;
        notes.record(&player, now);
        player.trace_take(&mut traced);
        for (key, at) in traced.drain(..) {
            if let Some(found) = reads.push(key, at, now) {
                return Ok(found);
            }
        }
        if let Some((start, end)) = player.state_hash().and_then(|h| state.push(h, now)) {
            return Ok(reads.shorten(&notes, start, end, now));
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
    /// Returns (first seen, now) when the state repeats.
    fn push(&mut self, hash: u64, now: u32) -> Option<(u32, u32)> {
        if self.last == Some(hash) {
            return None;
        }
        self.last = Some(hash);
        match self.first_seen.entry(hash) {
            Entry::Occupied(e) if now - *e.get() >= MIN_LOOP_MS => Some((*e.get(), now)),
            Entry::Occupied(_) => None,
            Entry::Vacant(e) => {
                e.insert(now);
                None
            }
        }
    }
}

/// What each channel plays, every [`NOTE_STEP_MS`]: the nearest semitone, or silence. Checks
/// a loop found in the read trace against what is actually played.
struct Notes {
    bits: Vec<u32>,
    /// `bits.len()` values per step
    steps: Vec<i16>,
}

const NOTE_STEP_MS: u32 = 10;
const SILENT: i16 = i16::MIN;

impl Notes {
    fn new(player: &Player) -> Notes {
        Notes { bits: player.channels().iter().map(|c| c.bit).collect(), steps: Vec::new() }
    }

    fn record(&mut self, player: &Player, now: u32) {
        while (self.steps.len() / self.bits.len().max(1)) as u32 * NOTE_STEP_MS <= now && !self.bits.is_empty() {
            for &bit in &self.bits {
                self.steps.push(match player.channel(bit) {
                    Some(ChannelState { volume, key, freq_hz, .. }) if volume > 0 && key != 0 => {
                        (12.0 * (freq_hz.max(1.0) / 440.0).log2()).round() as i16
                    }
                    _ => SILENT,
                });
            }
        }
    }

    /// Does every channel play the same in `[from, to)` as `period` later? Allows a step of
    /// jitter, and up to a quarter of steps differing: tonal channels differ in under 1% of steps
    /// when they repeat, noise in ~10% (its rate jitters), and a transposed part in nearly all.
    fn repeat(&self, from: u32, to: u32, period: u32) -> bool {
        let n = self.bits.len();
        let len = self.steps.len() / n.max(1);
        let lag = ((period + NOTE_STEP_MS / 2) / NOTE_STEP_MS) as usize;
        let first = from.div_ceil(NOTE_STEP_MS) as usize;
        let last = ((to - period) / NOTE_STEP_MS) as usize;
        (0..n).all(|c| {
            let at = |i: usize| self.steps[i * n + c];
            let (mut played, mut differ) = (0, 0);
            for i in first..last.min(len.saturating_sub(lag + 1)) {
                let later = [at(i + lag - 1), at(i + lag), at(i + lag + 1)];
                if at(i) == SILENT && later.iter().all(|&v| v == SILENT) {
                    continue;
                }
                played += 1;
                if !later.contains(&at(i)) {
                    differ += 1;
                }
            }
            differ * 4 <= played
        })
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
/// [`CONFIRM_MS`]; the shortest one scoring at least [`MATCH_RATE`] is the loop, and
/// [`ReadPeriods::loop_start`] places its start.
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
/// How many matching reads a missing one outweighs when placing the loop start: after it, at
/// least 75% of reads must match. Results on real music hold steady from 2 to 5.
const MISS_WEIGHT: i64 = 3;
const CANDIDATES: usize = 8;
/// Most passes of the music a repeat of the machine state is checked for.
const MAX_PASSES: u32 = 4;
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
            let Some((period, rate)) = self.refine(rough, now - span, now) else { continue };
            if rate >= MATCH_RATE {
                found = Some((self.loop_start(period, now), period));
            }
        }
        found.map(|(start_ms, period)| Detected::Loop { start_ms, end_ms: start_ms + period })
    }

    /// The period within [`GAP_BIN_MS`] of `rough` whose reads in `[from, to)` match best, and
    /// its score. Every period within the match tolerance of the true one scores (nearly) the
    /// same, so this is the middle of the best-scoring plateau.
    fn refine(&self, rough: u32, from: u32, to: u32) -> Option<(u32, f64)> {
        let scored: Vec<(u32, f64)> = (rough.saturating_sub(GAP_BIN_MS)..=rough + GAP_BIN_MS)
            .step_by(2)
            .filter_map(|p| Some((p, self.score(p, from, to, MIN_READS)?)))
            .collect();
        let rate = scored.iter().map(|s| s.1).max_by(f64::total_cmp)?;
        let plateau: Vec<u32> = scored.iter().filter(|s| s.1 >= rate - 0.002).map(|s| s.0).collect();
        Some(((plateau[0] + plateau[plateau.len() - 1]) / 2, rate))
    }

    /// Turns a repeat of the machine state, first seen at `start` and again at `end`, into the
    /// loop. The state can take several passes of the music to repeat (a byte that flips every
    /// pass: Kirby's Adventure track 13 repeats its state every two 25.6 s loops), so the loop is
    /// the shortest whole fraction of that period over which the reads since `start` repeat, and
    /// so do the notes: the same song data can be played again transposed (the bass on track 58
    /// repeats its first half two semitones lower). The
    /// music can also repeat before the state does (on that track, from 3.6 s instead of 16.4 s),
    /// so the start is the earlier of `start` and where the reads begin to repeat.
    fn shorten(&self, notes: &Notes, start: u32, end: u32, now: u32) -> Detected {
        let full = end - start;
        let period = (2..=MAX_PASSES)
            .rev()
            .filter(|&n| full / n >= MIN_LOOP_MS)
            .find_map(|n| {
                let (p, rate) = self.refine(full / n, start + full / n, now)?;
                (rate >= MATCH_RATE && notes.repeat(start, end, p)).then_some(p)
            })
            .unwrap_or(full);
        let start_ms = if period < full { start.min(self.loop_start(period, now)) } else { start };
        Detected::Loop { start_ms, end_ms: start_ms + period }
    }

    /// Where the repetition with `period` begins: the change point between reads that don't
    /// recur one period later (the intro) and reads that do (the loop). Placed both ways, matching
    /// each read against the reads one period later and against those one period earlier, and
    /// the earlier of the two wins.
    ///
    /// Either way, extra reads on one side only look like divergence and push the start late:
    /// the second pass reads each channel's jump back to the loop point and bytes that are fresh
    /// only after the loop's end; the first pass reads code, RAM and init data for the first
    /// time. A real divergence between intro and loop shows both ways.
    fn loop_start(&self, period: u32, now: u32) -> u32 {
        let lo = self.events.partition_point(|e| e.1 < period);
        let hi = self.events.partition_point(|e| e.1 + period + MATCH_TOLERANCE_MS < now);
        let forward = self.change_point(0..hi, period as i64);
        let backward = self.change_point(lo..self.events.len(), -(period as i64)).max(period) - period;
        forward.min(backward)
    }

    /// The time of the read in `range` from which reads matched `offset` ms away outweigh the
    /// misses by the most, with a miss weighing [`MISS_WEIGHT`] matches.
    ///
    /// - A change point, not the first matching run: parts that already repeat before the loop
    ///   point (a drum pattern running through the end of the intro) match there too, which put
    ///   the start over a second early (Kirby's Adventure tracks 1 and 15, Mega Man 5 track 20).
    /// - Misses weigh more than matches, or a part that repeats early (the melody on Kirby's
    ///   track 15) outvotes a quieter one that doesn't yet (its bass).
    /// - Contiguous bytes read in the same ms count once: a table copied at the end of an intro
    ///   would otherwise outweigh dozens of notes.
    fn change_point(&self, range: std::ops::Range<usize>, offset: i64) -> u32 {
        let (mut sum, mut best, mut first) = (0i64, i64::MIN, range.end.saturating_sub(1));
        for i in range.rev() {
            let (k, at) = self.events[i];
            if i > 0 && self.events[i - 1] == (k.wrapping_sub(1), at) {
                continue;
            }
            sum += if self.read_near(k, at as i64 + offset) { 1 } else { -MISS_WEIGHT };
            if sum >= best {
                (best, first) = (sum, i);
            }
        }
        self.events[first].1
    }
}
