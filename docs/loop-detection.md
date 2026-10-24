# Loop detection

How background analysis (`crates/nsfplay-core/src/analysis.rs`, `analysis::analyze`) finds where
a track loops or ends, why each rule is there, and how to check a change. The rules were tuned
on real soundtracks (Mega Man 5, Kirby's Adventure, Kid Icarus (FDS)); most of them exist
because a simpler version failed on a specific track, listed here so they aren't undone by
accident.

## Overview

`analyze` plays the track faster than real time (16 kHz, 1 ms slices so the audio callback
never waits for the render lock) for up to `limit_ms` of song time (the app passes 10 min).
Three detectors watch the same playback, checked in this order after every slice; **the
first to answer wins**. Nothing chooses between them up front: which one fires depends on
which has enough evidence first.

| Detector | Answers | Needs | Typical winner for |
|---|---|---|---|
| 1. Song-data reads (`ReadPeriods`) | `Loop` | one loop + 30 s past the first full pass, ~2P + 30 s | any engine, the only one for engines with free-running RAM (Capcom) |
| 2. Machine-state repeat (`StateLoops`, refined by `ReadPeriods::shorten`) | `Loop` | one loop, ~P | engines whose RAM repeats (most Kirby, Kid Icarus tracks) |
| 3. Silence (core's `DetectSilent`, `AUTO_STOP`) | `Silence` | ~3 s of silence | jingles, tracks that end |
| none within the limit | `Nothing` | | endless playback then stops on live silence (`SilenceStop::Detect`); Listen uses the default length |

Manual loop points (`playlist.loopOverrides`) win over analysis everywhere. Results are cached
in `state.json`; **bump `ANALYSIS_VERSION` in `src/lib/playlist.svelte.js` whenever results
change**, or users keep the old ones.

The core's own loop detector is not used: its 65536-write buffer against a 30 s window misses
loops over ~80 s and reports false ones.

## 1. Song-data reads

### The trace

`ReadTracer` (shim, `nsfplay_shim.cpp`) sits first in the CPU's device chain and sees every
read. It records `(key, ms)` only for bytes **not read in the last second** (`gap_ms`). Code and
tables read every frame drop out; what remains is mostly song data, read once per note. Keys
are ROM offsets through the bank map (so the same data in a different bank slot is the same
key), or `0x100000 + address` for RAM/WRAM. Once the engine jumps back to the loop point, the
same bytes are read again one loop later, whatever RNGs, vibrato or fractional tempo counters
do to RAM. It is statistical: ~1-6% of reads flip between fresh and not from pass to pass.

### Period (`push`, `check`, `refine`)

- Gaps between reads of the same key (last 16 reads per key, gaps >= `MIN_LOOP_MS`) are binned
  by `GAP_BIN_MS` (50 ms). Every `CHECK_MS` (5 s) the `CANDIDATES` (8) most common bins are
  tried.
- `refine`: each candidate is scored at every 2 ms within ±50 ms; the score is the fraction of
  reads matched by a read of the same key one period earlier (±`MATCH_TOLERANCE_MS`, 40 ms),
  over the last pass plus `CONFIRM_MS`. All periods within the tolerance score about the same,
  so the period is the middle of the best plateau.
- Accepted at `MATCH_RATE` (90%) with at least `MIN_READS` (200) reads; the shortest accepted
  candidate wins. On Mega Man 5 the true period matched 99.8%, phrase-length impostors at most
  48%.
- `CONFIRM_MS` (30 s) past a full pass keeps a phrase repeated inside the song (AABA) from
  passing as the loop.

### Start (`loop_start`, `change_point`)

The start is a **change point**: the read from which matched reads outweigh misses by the most
(sum of +1 per match, -`MISS_WEIGHT` per miss, from the end backward; the maximum wins). It is
placed twice, matching each read against the reads one period **later** (forward) and one
period **earlier** (backward, result shifted back by P), and **the earlier of the two wins**.

Why each part:

- **Change point, not "first matching run".** The old rule walked back in 1 s windows and took
  the first run of 12-of-16 matching reads. Parts that already repeat before the loop point
  match there too: on Kirby #1 the drums play the same beat through the end of the intro, which
  put the start 1.1 s early; also Kirby #15, Mega Man 5 #20.
- **`MISS_WEIGHT` = 3** (after the start, at least 75% must match). With 1, a part that repeats
  early outvotes one that doesn't: on Kirby #15 the squares repeat from 4.2 s but the bass only
  from ~7.2 s, and weight 1 gave 4.2 s. Results hold steady from 2 to 5; above that, real
  in-loop misses push Capcom starts late (Mega Man 5 #3 went from 15.5 s to 28.8 s at 9).
- **Contiguous bytes read in the same ms count once.** Engines copy tables (Capcom at the end of
  an intro: runs of 10-16 bytes); counted per byte, one copy outweighed dozens of notes.
- **Both directions, earlier wins.** Each direction has reads that exist on one side only and
  can only push the start late:
  - backward (pass 2 vs. pass 1): the second pass reads each channel's jump back to its loop
    point, and bytes that are fresh only after the loop's end (the intro had read them less
    than a second before the loop point on pass 1). Mega Man 5 #7: 10.6 s became 11.5 s.
  - forward (pass 1 vs. pass 2): the first pass reads code, RAM and init data for the first
    time (everything is fresh at 0 ms), never fresh again. Tracks looping from the very start
    came out 0.3-0.85 s late.
  - A real divergence (intro vs. loop) shows both ways, so the earlier change point is right.
- Tried and dropped: excusing a miss when the key was read in the second before (meant for
  the freshness asymmetry); it changed nothing once runs were collapsed.

## 2. Machine-state repeat

### The hash

`nsfp_state_hash` hashes RAM, WRAM and the bank map while the CPU idles between play calls.
`StateLoops` keeps the first time each hash was seen; when one comes back at least
`MIN_LOOP_MS` later, the music repeats exactly from that first time. It needs one pass instead
of two plus 30 s, so it usually answers first when it can. RAM never repeats with free-running
RNG, vibrato or tempo accumulators (all of Mega Man 5); comparing APU registers instead failed
for the same reason (~80% match even with ±1 frame of jitter).

### Refinement (`ReadPeriods::shorten`)

The state can take several passes of the music to repeat, and can settle late. Kirby #13: some
RAM byte flips every pass, so the state repeated every two 25.6 s loops, first seen at 16.4 s,
while the music loops from 3.6 s. So:

- **Period:** try 1/`MAX_PASSES` (1/4) up to 1/2 of the state period, shortest first. A
  fraction is accepted only if
  - the reads since the state start repeat at it (`refine`, >= `MATCH_RATE`), **and**
  - the notes repeat at it (`Notes::repeat`): every channel's pitch to the nearest semitone (or
    silence), sampled every `NOTE_STEP_MS` (10 ms) during analysis, compared with ±1 step of
    jitter, at most a quarter of sounding steps differing. Needed because song data can be
    replayed transposed: Kirby #58's bass plays its first half again two semitones lower, the
    same bytes, so the reads alone halved it wrongly. Repeating tonal channels differ in <1%
    of steps, noise in ~10% (its rate jitters; 10% rejected Kirby #34), a transposed part in
    nearly all.
- **Start:** the earlier of the state start and `loop_start` for the accepted period. Only
  when a fraction was accepted; with the full period the state start is exact and kept.

18 Kirby tracks get a halved loop from this; every one was checked against the audio and the
channel pitches. None of Mega Man 5 or Kid Icarus changed.

## Constants

| Constant | Value | Why |
|---|---|---|
| `RATE` | 16 kHz | detectors don't need quality audio |
| `MIN_LOOP_MS` | 500 | shorter repeats are engines toggling after the music ended; silence handles them |
| `CONFIRM_MS` | 30 s | beyond one full pass, against AABA impostors |
| `CHECK_MS` | 5 s | how often read periods are scored |
| `GAP_BIN_MS` | 50 | candidate bins; `refine` searches ±1 bin |
| `MATCH_TOLERANCE_MS` | 40 | a read matches within this of one period away (frame jitter) |
| `MATCH_RATE` | 0.9 | period acceptance |
| `MIN_READS` | 200 | a span with fewer reads proves nothing (~100+ song bytes/s in real music) |
| `CANDIDATES` | 8 | periods tried per check |
| `MISS_WEIGHT` | 3 | start change point; stable 2-5 |
| `MAX_PASSES` | 4 | state-period fractions tried |
| `NOTE_STEP_MS` | 10 | note sampling for `Notes` |
| tracer `gap_ms` | 1000 | "fresh" read window (shim) |

## Known limits

- **Starts are biased late**, typically 0.1-0.5 s (Kirby #1: 10.67 s vs. ~10.25 s in the audio;
  Mega Man 5 #6: 12.40 s vs. ~11.95 s), at worst ~1.3 s (Mega Man 5 #3: 15.53 s vs. ~14.25 s).
  Late is the harmless direction: the music after a late start still repeats exactly, so
  folding, seeks and A-B loops stay seamless; only the drawn intro is too long. An early start
  puts intro music into the loop. Prefer changes that keep errors on the late side.
- The read detector can't report a loop before ~2P + 30 s of song time; long loops cost
  analysis time (e.g. 82 s loop: ~3 min of song, a few seconds of CPU).
- Loops over ~4.7 min (2P + 30 s > 10 min) are never found by reads; only the state hash can.
- The read detector itself has no notes check: a transposed repeat inside an engine whose RAM
  never repeats could still be taken for the loop.
- Untested with expansion-chip soundtracks beyond Kid Icarus (FDS), and with PAL rips.

## Checking a change

There are no real NSFs in the repo. The reference files are in `~/Downloads`: `Mega Man 5.nsf`
(track 1: 12.16 s intro + 69.42 s loop), `Kirby's Adventure [...].nsf` and
`Kid Icarus [...] (FDS) [...].nsf`. Unit tests (`cargo test -p nsfplay-core`) only cover the
generated files in `tests/data/`.

1. **What moved.** Print every track's result before and after, and diff:

   ```sh
   cargo run --release -p nsfplay-core --example analyze -- ~/Downloads/"Mega Man 5.nsf" > after.txt
   ```

   `loop-detection-baseline.txt` (next to this file) holds the current results for the three
   files.

2. **Is it right.** For each moved track, judge by the audio, per channel:

   ```sh
   ex=crates/nsfplay-core/examples
   for c in 0 1 2 4; do   # 2A03 squares, triangle, DPCM; never noise (random)
     cargo run --release -q -p nsfplay-core --example analyze -- FILE TRACK --solo $c --raw /tmp/t.$c.raw
   done
   uv run --with numpy python -I $ex/loop_truth.py start PERIOD_MS FROM_MS TO_MS /tmp/t.*.raw
   uv run --with numpy python -I $ex/loop_truth.py period 2 30 /tmp/t.0.raw          # true period?
   uv run --with numpy python -I $ex/loop_truth.py compare START_MS PERIOD_MS /tmp/t.*.raw  # halving OK?
   ```

   `start` reads ~0-300 ms early by construction. Expansion channels have other bits (see
   `CHANNELS` in `src/lib.rs`).

Pitfalls found while building these checks:

- Judge per channel, not the mix: the loudest part dominates and hides a quieter one that
  doesn't repeat yet (Kirby #15's bass).
- Use magnitude spectra, not waveforms: oscillator phases differ between passes.
- Use sample-exact lags. Rounding to a 10 ms hop made correct loops (41066 ms, 4 ms off after
  rounding) look wrong.
- Note-level checks (`ChannelState` from `Player::channel`) work for most engines but not
  Capcom's: vibrato crosses semitone boundaries, so even true loops match poorly. Rounding to
  ±1 semitone didn't fix it.
- Muting any channel turns off the core's silence detection (`MASK != 0`), which is fine for
  these renders but not for checking silence ends.

## Reference results

| Track | Result | Audio | Notes |
|---|---|---|---|
| Mega Man 5 #1 | 12152 + 69422 | 12150 | read detector |
| Mega Man 5 #20 | 929 + 9872 | ~1050 | was 14 (first-matching-run rule) |
| Kirby #1 | 10665 + 42666 | ~10250 | was 9167; drums repeat before the loop |
| Kirby #13 | 3599 + 25598 | ~3550-3600 | was 16405 + 51198; state repeats every 2 loops |
| Kirby #15 | 7099 + 28792 | ~7100-7400 | was 6607; squares repeat 3 s before the bass |
| Kirby #58 | 6969 + 10666 | | half (5333) rejected: bass transposed |
| Kid Icarus (all) | unchanged | | all state hash, exact |
