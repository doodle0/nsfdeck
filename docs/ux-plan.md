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
│ ▶ ■  track [2 ▾]  speed [1.00×]                 A▕━▏B  ⟲  │
│ ├──── intro ────┤├────────── loop ──────────┤  pass ×3    │
│ 0:00      0:15      0:30  ●   0:45      1:00      1:15    │  ← time ruler
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
- **Timeline tools** (in time, not musical units):
  - Click or drag on the ruler to seek. Keys step by 1 s, or by one NES frame when paused.
  - **A–B loop region**: drag on the ruler and that region repeats instead of the whole loop. This is
    the most important tool for transcribing a passage. Saved per *(file, track)*.
  - *Dropped (2026-10-03): a musical timeline with measures, beats, a tempo setting, tap tempo
    and tempo detection. NSF files contain no tempo information, and NSFPlay has no tempo display
    either; the closest things it has are the play-routine rate in its info dialog ("NTSC Speed:
    60.098814Hz") and a ×1–×8 slow-down slider. Its track info window lists Vol / Freq / Key /
    Oct / Tone / Wave per channel, which remains the model for our keyboard view.*
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
   detection on, to find the loop start and end.
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
4. **Persistence:** settings, the playlist, per-track durations, loop points and A–B regions, stored as JSON in the OS
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
5. Keyboard view, then the A–B loop
6. P2 items as time permits
