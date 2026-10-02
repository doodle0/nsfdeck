# UX plan: modes

NSFDeck has three modes for three kinds of users. They are views of one player in one window, not
separate apps: switching modes changes which panels are shown, and playback continues uninterrupted.
The app remembers the last mode used.

| Mode | For | Question it answers |
|---|---|---|
| **Listen** | People who want to enjoy soundtracks | "Play me this music, for as long as I like." |
| **Studio** | Sound engineers, musicians, transcribers | "What exactly is happening in this tune?" |
| **Developer** | NSF composers and tool authors | "What is the emulated hardware doing?" (scaffold only) |

Listen and Studio are deliberately different. Listen is about a *playlist* played through with
finite, fading durations. Studio is about *one track* examined in depth: it plays endlessly with no
fade-out and no playlist, and its timeline covers the track's intro and its loop. Both share the
transport, the current file, and the master volume. Opening Studio works on the track that is
currently selected; going back to Listen picks the playlist up where it was.

## Listen mode

```
┌───────────────────────────────────────────────────────────┐
│ ♪ Track title — Game · Artist               [Listen ▾]    │
│ ◀◀  ▶  ▶▶   ━━━━━━━━●━━━━━━━  1:42 / 3:10   🔀 🔁   🔊━━  │
├───────────────────────────────────────────────────────────┤
│ Playlist                                     + Add  ⋯     │
│   1  Overworld          Zelda.nsf #1        2:34  auto    │
│ ▶ 2  Dungeon            Zelda.nsf #2        3:10  2 loops │
│   3  Title              Mega Man 2.nsf #1   1:05  file    │
└───────────────────────────────────────────────────────────┘
```

### P1 (first priority)

- **Transport:** play/pause, stop, previous/next, seek, volume. These exist; previous/next now move
  through the playlist instead of the current file's tracks.
- **Playlist** (the main view in this mode):
  - Entries are *(file, track)* pairs. Adding an NSF adds all its tracks; the user can then remove the
    ones they don't want (sound effects, jingles).
  - Add by drag-and-drop, the open dialog, or a folder (recursively). Reorder by dragging; remove and
    clear; double-click to play.
  - Moves to the next entry automatically when a track ends. Repeat: off / all / one.
  - **Shuffle**: plays a random order without repeats, and reshuffles when the list wraps around.
    Turning shuffle off keeps the current entry playing.
  - Saved automatically and restored on launch. Import and export as **NSF M3U**
    (`file.nsf::NSF,track,title,time,loop,fade`, the extended M3U format used by NEZplug and
    Game_Music_Emu). That format already stores per-track durations.
- **Track duration** (plain NSF files contain no lengths):
  - Each entry has a duration source, shown as a badge: `file` (NSFe/M3U time), `auto`
    (loop detected), `custom` (set by the user), or `default`.
  - Global default: play time, fade time, and number of loops, with a "detect loops" toggle.
    Detection uses the core's `AUTO_DETECT`. A detected loop plays N times and then fades out.
  - Per-entry override, set from a context menu: *Auto (N loops)*, *Fixed m:ss*, or *Endless*.
  - Silence auto-stop toggle (skips silent tracks).

### P2

Search and filter the playlist, a "skip tracks shorter than N s" option, crossfade or gapless
playback, and media keys / OS media controls (MPRIS, SMTC, Now Playing).

## Studio mode

```
┌───────────────────────────────────────────────────────────┐
│ ♪ Track title                                 [Studio ▾]  │
│ ▶ ■  track [2 ▾]  speed [1.00×]  tempo 150 BPM 4/4  A▕━▏B │
│ ├──── intro ────┤├────────── loop ──────────┤  ⟲ ×3       │
│ |1 . . |2 . . . |3 . . . |4 ●. . . |5 . . . |6 . . .|     │  ← measure/beat ruler
├────────────────────────────┬──────────────────────────────┤
│ Mixer                      │ Keyboard                     │
│ 2A03  ━━━━●━  M S          │ Sq1 ▕▔▔█▔▔▔▔▔▔▔▔▔▔▔▔▔▕        │
│  Sq1  ━━●━━━ ◁━●━▷ M S     │ Sq2 ▕▔▔▔▔▔▔█▔▔▔▔▔▔▔▔▕         │
│  Sq2  ━━━●━━ ◁━●━▷ M S     │ Tri ▕█▔▔▔▔▔▔▔▔▔▔▔▔▔▔▔▕        │
│  …                         │ …                            │
└────────────────────────────┴──────────────────────────────┘
```

### P1 (first priority)

- **Single track, played endlessly:** no playlist and no fade-out; durations are ignored. A track
  picker chooses among the current file's tracks. When the loop end is reached, playback continues
  through the loop again and the playhead wraps back to the loop start. A counter shows which pass
  through the loop is playing.
- **Intro + loop timeline:** the timeline covers the intro (from 0 to the loop start) and one pass of
  the loop (from the loop start to the loop end), shown as two labelled sections.
  - The loop points come from a **background analysis pass** (see Foundations). It runs a second
    emulator instance faster than real time, so the timeline is complete before or shortly after
    playback starts.
  - If no loop is found (through-composed tracks, very long loops), the timeline is a single section
    that grows as the track plays. The user can also set or adjust the loop start and end by hand,
    and these are saved per *(file, track)*.
  - Seeking anywhere in the loop section seeks into the current pass through the loop.
- **Speed multiplier:** 0.25× to 2×, with presets and fine adjustment. It uses the core's
  `MULT_SPEED`, which runs the play routine faster or slower while APU timing stays the same, so the
  **tempo changes but the pitch does not**. The UI should say so. The playlist duration is unaffected;
  the time display shows song time.
- **Musical timeline:**
  - The position is counted in NES frames (60.1 Hz NTSC, 50 Hz PAL) under the hood. NSF files have no
    tempo metadata, so the measure/beat grid comes from a **tempo setting**: BPM or
    FamiTracker-style *frames per row × rows per beat*, plus beats per measure and an offset for the
    first downbeat.
  - NSFPlay itself has no tempo or BPM display, so there is nothing upstream to reuse (checked
    2026-10-03). The closest things are the play-routine rate in its info dialog
    ("NTSC Speed: 60.098814Hz") and a ×1–×8 *slow-down* slider (`MULT_SPEED = 256 / n`) in its track
    info window. Its track info window also lists Vol / Freq / Key / Oct / Tone / Wave per channel,
    which is the model for our keyboard view.
  - **Auto-detected tempo, in frames:** music engines (FamiTracker and most others) advance one row
    every *n* play-routine calls. Note onsets (key-on edges and volume or pitch jumps in the channel
    info) therefore fall on multiples of the row length. The background analysis finds the row
    length as the strongest period of the onset-interval histogram, measured in frames. It then
    suggests BPM = 60 × play rate ÷ (frames per row × rows per beat), assuming 4 rows per beat.
    Engines with groove or fractional tempos (alternating 6/7 frames) show up as a fractional
    average. Measuring in frames instead of seconds keeps the result exact for the common case.
  - **Tap tempo**, and ×2 / ÷2 buttons, because the beat level is ambiguous. The suggestion can be
    wrong, so the user always confirms it.
  - Click the ruler to seek, with snapping to beat or measure. Keys step by beat or measure.
  - The grid's downbeat offset defaults to 0, and the loop start can be snapped to it.
  - **A–B loop region**: drag on the ruler and that region repeats instead of the whole loop. This is
    the most important tool for transcribing a passage.
  - Tempo settings are saved per *(file, track)*.
- **Mixer:** volume, pan, mute and solo for each channel; volume for each chip; master volume. Reset
  buttons. Uses `<DEV>_VOLUME`, `CHANNEL_nn_VOL`/`_PAN`, and `MASTER_VOLUME`, applied with
  `Notify`/`NotifyPan`. Studio only; Listen mode keeps only master volume. Mixer settings stay
  in effect in Listen mode (with an indicator and a reset), so a user can listen with, say, the DPCM
  turned down.
- **Keyboard view:** one row per active channel, showing the current note, volume (as brightness),
  and key-on state. Uses `NSFPlayer::GetInfo` / `ITrackInfo` (frequency → note, volume, key status).
  It is cheap and comes before the other visualizers.

### P2 (optional in this phase)

- **Piano roll:** a scrolling history of the same note data, aligned with the timeline. Clicking it
  seeks.
- **Oscilloscope:** first the master output (cheap), then one scope per channel. Needs a spike to
  find out whether `ITrackInfo::GetOutput` has enough resolution. The alternative, running one muted
  emulator per channel, costs about N× the CPU.
- WAV export (the whole track, the A–B region, or one file per channel). Export with notes as MIDI.

## Developer mode

The full feature set is backlog: the rest of NSFPlay, i.e. a proper memory viewer, CPU log, NSFe
chunk inspector, per-chip emulation quirk options, region override (NTSC/PAL/Dendy), sample rate and
quality, LPF/HPF and other filter settings, and NSFPlay presets. Until this mode is built out,
region and per-chip options can sit in a plain Settings dialog.

### P1: scaffold

The mode exists in the switcher and shows a **plain-text state dump** of the running emulator,
refreshed a few times per second while it is visible (with pause/freeze and copy-to-clipboard):

- **File:** format and version, load/init/play addresses, initial banks, region flags, expansion
  chips, NSF2 flags, track count.
- **CPU:** A, X, Y, S, P (as flags), PC, and the current frame number.
- **Memory:** hex dump of `$0000–07FF` (RAM) and `$6000–7FFF` (WRAM, when used), plus the bank map
  for `$8000–FFFF` (and `$6000` for FDS).
- **Sound registers:** `$4000–4017` (2A03), and the register files of the expansion chips the file
  uses (e.g. MMC5 `$5000–5015`, N163 internal RAM, VRC6, 5B, VRC7, FDS).

Most of this state lives in `protected` members of the core (`NES_CPU::context`, `NES_MEM::image`,
`NES_BANK::bankswitch`, `NES_APU::reg`, …). The shim reads it without modifying `vendor/nsfplay` by
taking pointers to members through a derived class (`struct Peek : NES_APU { using NES_APU::reg; }`
and then `apu->*&Peek::reg`), which is well-defined C++. It formats one text block, which is passed
to the UI as a string. Chips that keep no register array (if any) are left out of the dump or
shadowed from bus writes.

## Foundations to build first

These are not visible features, but the P1 work depends on them:

1. **Track clock owned by the wrapper.** The core's `time_in_ms` drops fractional milliseconds and
   overflows on long seeks. At speed ≠ 1× the per-call time is fractional again, so the current
   whole-ms chunking workaround stops working. The plan is to count time in samples in `Player`, run
   the core endless (which Studio mode needs anyway), and call `FadeOut` ourselves when a Listen
   entry's duration is reached. This also makes per-entry durations and A–B loops straightforward.
2. **Background track analysis:** a second `Player` renders the track faster than real time with loop
   detection on, to find the loop start and end and to collect note onsets for tempo detection.
   Upstream's detector (`NESDetector`, `xgm/devices/Misc/detect.cpp`) watches APU register writes
   and reports a loop once the last `DETECT_TIME` (30 s by default) of writes repeats, giving the loop
   start and end in ms. Analysis therefore renders about intro + loop + 30 s, which takes a few
   seconds at the core's speed. Loop points can then be snapped to NES frame boundaries. Tracks whose
   loop is longer than `DETECT_TIME` need a longer window or manual points.
   Results are cached per *(file, track)*. Listen mode uses the same analysis for `auto` durations,
   so durations are known before a track plays, not only after it has looped. Note: `RENDER_LOCK`
   serializes all players because of the shared rconv buffer, so analysis takes turns with
   playback. It is fast enough (the core renders faster than real time), but this is one more
   reason to get the upstream rconv fix merged.
3. **Generic config get/set in the shim**, used by the mixer, speed, durations and settings.
4. **Persistence:** settings, the playlist, per-track durations and tempo, stored as JSON in the OS
   config directory.
5. **Live channel data:** push playback state and channel info from Rust at about 30–60 Hz, using
   Tauri events or a channel instead of the current 100 ms `status` poll.
6. **CI** on Linux, macOS and Windows (build and run `cargo test -p nsfplay-core`).

## Suggested order

1. Foundations 1, 3, 6 (clock, config, CI)
2. Mode switcher and layout shell, with the existing UI as Listen mode and the Developer dump
   scaffold
3. Background analysis (foundation 2), then playlist, durations, shuffle and repeat (with persistence)
4. Studio: endless single-track playback, the intro + loop timeline, mixer, speed
5. Keyboard view, then the beat grid, tap tempo, A–B loop, then auto-detecting tempo
6. P2 items as time permits
