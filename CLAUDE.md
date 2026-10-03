# NSFDeck

Cross-platform NSF/NSFe player: Tauri 2 + Svelte 5 frontend, audio via cpal, emulation by
NSFPlay's `xgm` core (git submodule at `vendor/nsfplay`, compiled by
`crates/nsfplay-core/build.rs`). See README.md for the architecture diagram and layout.

## Commands

- Use **pnpm** for JS (never npm/npx) and **uv** for any Python.
- `pnpm tauri dev`: run the app with hot reload.
- `pnpm check`: svelte-check (keep at 0 errors, 0 warnings). `pnpm build`: Vite build into `dist/`.
- `cargo test -p nsfplay-core --features output`: core tests (tests/playback.rs on the files in tests/data/, plus Output's A–B rendering tests in src/output.rs).
- `cargo run -p nsfplay-core --features output --example play -- FILE.nsf [TRACK] [SECONDS]`: play audio without the GUI.
- `pnpm tauri build --debug --no-bundle`, then `target/debug/nsfdeck FILE.nsf`: run the real app on a file.

## Rules

- **Never modify `vendor/nsfplay`.** Work around upstream issues in `crates/nsfplay-core/shim/` or `src/lib.rs`, and propose real fixes upstream.
- Core source lists in `build.rs` mirror upstream `contrib/Makefile`; update them when bumping the submodule.
- Keep `nsfplay-core` free of Tauri dependencies; the app talks to it only through `Output`/`Player`.
- **The webview runs without a proxy** (`--no-proxy-server` in `app.windows[0].additionalBrowserArgs`, `src-tauri/tauri.conf.json`; Windows/WebView2 only, the option is ignored elsewhere). Without it, WebView2 resolves the system proxy (WPAD auto-detect) before its first HTTP request, which took ~27 s on the dev machine: a white screen under `tauri dev`. Release builds load from a custom protocol and never waited. Consequences: the webview ignores system and corporate proxies, so anything loaded from the network inside the webview goes direct. That's fine while the UI only loads its own assets and talks to Rust over IPC. Fetch network resources in Rust, not from the webview, or revisit this flag. Setting `additionalBrowserArgs` replaces Tauri's defaults, so the value repeats `--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection`; keep that when editing it.
- A debug exe built by `tauri dev` loads `devUrl` (`localhost:1420`); run on its own without the dev server it shows Edge's "connection refused" page. `tauri build --debug` writes the same path with embedded assets, so the last command wins.

## Upstream behavior to know about

- `NSF::Load(image, size)` leaves play/fade/loop times uninitialized; only `NSF::LoadFile` sets them. The shim mirrors LoadFile (`time_in_ms = -1`, etc.). Without that, every track ends after 50 ms.
- `RateConverter::FastRender` (upstream `xgm/devices/Audio/rconv.cpp`) uses a `static INT32 t[2]` shared by all players. `RENDER_LOCK` in `src/lib.rs` serializes `start`/`render`/`skip`, because `Reset()` also renders.
- The core `printf`s debug output unless `NDEBUG` is defined (build.rs defines it).
- `PlayerConfig::operator[]` throws `std::out_of_range` for unknown names; the shim checks `HasValue` first (`nsfp_config_get/set`).
- Mute mask bits follow `NSFPlayerConfig::channel_name`. VRC7 channels 6-8 are left out of `CHANNELS` because the core shifts their bits onto N163's.
- Text fields are UTF-8 or Shift-JIS; `decode()` falls back to Shift-JIS.
- `sjis_legacy` (upstream `nsf.cpp`) runs iconv in place on NSF header strings and aborts under glibc. The shim blanks non-ASCII header fields before `Load` and returns the raw bytes instead. macOS needs `-liconv` (build.rs). This is a candidate upstream fix.
- `NSFPlayer::Render`/`Skip` advance `time_in_ms` by a truncated whole number of ms per call, and `Skip` computes `1000 * length` in UINT32 (overflows past about 89 s at 48 kHz). So `Player` owns the track clock: the core runs with `PLAY_ADVANCE=1` (never fades out by itself), `Player` counts song time as frames × `MULT_SPEED` and calls `FadeOut` when the time is up (unless endless). It still calls the core only in whole-ms chunks (`step`) and splits long skips, so the core's loop and silence detection stays accurate. Upstream fix candidate.
- `AUTO_STOP` ends tracks of unknown length after `STOP_SEC` (3 s) of silence. Otherwise the default length is 5:00 plus a 5 s fade.

## Testing notes

- There are no real NSF files in the repo. `arpeggio.nsf` is hand-assembled 6502 code: track 1 is a square-wave arpeggio, track 2 is silent (tests auto-stop); its generator script isn't saved. `intro_loop.nsf` and `song_data_loop.nsf` have a generator (`uv run crates/nsfplay-core/tests/data/make_intro_loop.py`).
- UI without a backend: copy `dist/` or the source files and mock `@tauri-apps/api`, then screenshot with headless `google-chrome`. Desktop screenshots via PIL `ImageGrab.grab(xdisplay=":0")` capture the user's **whole screen**, so ask first.

## Status (handover, 2026-10-03)

Done: open/drag-drop/CLI-arg loading, track list, play/pause/stop/prev/next, seek, volume, metadata, chip badges, per-channel mute/solo, light/dark themes, keyboard shortcuts. The app has been built and run on Linux with the test file.
Done since (docs/ux-plan.md steps 1-2): wrapper-owned track clock, endless mode, speed (`MULT_SPEED`), generic config get/set, CI workflow (`.github/workflows/ci.yml`, not yet run on GitHub), mode switcher (Listen / Studio / Developer; mode in `localStorage`), Studio speed control, Developer state dump (`nsfp_dump`). CI passes on all three platforms.
Done (step 3): background analysis (`analysis::analyze`, loop/silence via the core's detector), per-entry lengths (`Player::set_length`), Listen playlist (`src/lib/playlist.svelte.js`: entries, durations, shuffle/repeat, analysis queue, autosave to `<app config dir>/state.json`, NSF M3U import/export in `src/lib/m3u.js`).
- Loop analysis (`src/analysis.rs`) has two detectors. Main: a `ReadTracer` device inserted first in `NSFPlayer::stack` (re-inserted after every `Reset`, which rebuilds the chain via `Reload`) records reads of bytes not read in the last second, i.e. song data rather than code or per-frame tables. The loop is the period P at which >= 90% of recent reads were also read P earlier (statistical, because ~1-6% of reads flip between fresh and not); its start is where that match begins (seconds match <20% before it, >50% inside). Fast path: an exact repeat of RAM + WRAM + bank map while the CPU idles (`nsfp_state_hash`). RAM never repeats in engines with free-running RNG/vibrato/tempo accumulators (Mega Man 5, Capcom), and register comparisons failed for the same reason (~80% match even with +-1 frame jitter). Upstream's detector is unused: its 65536-write buffer vs. 30 s window misses loops over ~80 s and reports false ones.
- Mega Man 5 (`~/Downloads/Mega Man 5.nsf`, not in the repo) is the real-world check: track 1 is a 12.16 s intro + 69.42 s loop; 16 of 24 tracks loop, 7 end in silence. Its play rate is 60.00 Hz (16666 us), not 60.1.
- Test files `intro_loop.nsf` (RAM repeats) and `song_data_loop.nsf` (RAM never repeats; only song-data reads show the loop) are generated by `tests/data/make_intro_loop.py`: 3 s intro (frame 180), 40 s loop (2400 frames).
- Mixer (`src/lib/mixer.svelte.js`): `<DEV>_VOLUME` and `CHANNEL_nn_VOL`/`_PAN` via `set_config`; `Output` keeps these overrides and applies them to every player it builds (seeks, A–B cue, new files). 2A03 = devices APU1 (pulses) + APU2 (triangle, noise, DPCM). An NSFe `mixe` chunk overrides device volumes. Saved in state.json with the playlist.
- Live channel data: the app's `playback-events` thread emits `playback` (status + per-channel freq/volume/key/tone from `NSFPlayer::GetInfo(-1, channel_track[bit])`) at 30 Hz while playing; it replaced the 100 ms status poll. Read channel info only under `Output`'s state lock: the audio thread rewrites that history (InfoBuffer deletes entries) while rendering.
- Studio timeline (`src/lib/studio.svelte.js`, `Timeline.svelte`) works in first-pass time: song time is folded into intro + one loop, and seeks go to the first pass (same music, least emulation). Manual loop points (`playlist.loopOverrides`) win over analysis everywhere, including Listen lengths.
- `Output` keeps the file data to build more players: seeks build a new player on a blocking thread and swap it in (latest `seek_seq` wins), so playback continues meanwhile; the A–B loop swaps in a cue player prepared at A by the `nsfplay-cue` worker, rendering exactly up to B and continuing from A in the same buffer. `Player::skip` takes the render lock per ~20 ms slice so this never starves the audio callback. `Output::play` clears the region; the frontend re-sends it (`player.starts`).
- Bump `ANALYSIS_VERSION` in `playlist.svelte.js` when analysis results change, so saved results are recomputed.
- Analysis renders in 1 ms slices: holding `RENDER_LOCK` for longer slices (100 ms of song) caused audio underruns with real files. A plain `std::sync::Mutex` measured fine; a fair mutex did not help.
- M3U track numbers: decimal is read as 1-based and `$hex` as 0-based (assumed to match Game_Music_Emu; unverified).
- Studio timeline zoom/scroll: view state (`zoomMs`, `viewStart`, `follow`) lives in `studio`; the view pages along with the playhead unless the user scrolled it away (a seek turns following back on). The timeline spans `studio.end`: intro + one loop, the silence end, the file's length, or (a guess, drawn hatched) Listen's default 5:05. Only a track without a loop can play past its end; the span then grows to the next whole minute after the playhead.
- Headless screenshots: Chrome fires `resize` when capturing, which closes `Menu.svelte`; suppress `resize`/`blur` in the mock page to capture menus.

Not done yet:
- Repo: https://github.com/doodle0/nsfdeck (public, branch `main`). Ask before pushing.
- License is MIT (`LICENSE`, © 2026 doodle0). Git identity is set per repo: doodle0 <49020517+doodle0@users.noreply.github.com>, matching gh account `doodle0`. `identifier` in `src-tauri/tauri.conf.json` is the placeholder `dev.nsfdeck.app`.
- Not yet tested with a real game soundtrack, or on macOS/Windows. No CI.
- Upstream fix for the rconv static buffer is not yet submitted as a PR. Its branch (`fix-rconv-static-buffer` in `~/projects/nsfplay`) is gone (as of 2026-10-03), so it must be recreated.
- `~/emsdk` is left over from an abandoned WebAssembly approach; the user may want it deleted.

P1 of docs/ux-plan.md is complete (2026-10-03): Listen playlist, Studio timeline/A–B/mixer/keyboard/speed, Developer dump.

Next planned work: see **docs/ux-plan.md** (Listen / Studio / Developer modes, P1 scope, foundations, order).
Config keys for the mixer: per-device `<DEV>_VOLUME` (`APU1 APU2 5B MMC5 N163 VRC6 VRC7 FDS`, default 128),
`MASTER_VOLUME`, per-channel `CHANNEL_nn_VOL` / `CHANNEL_nn_PAN` (default 128); apply with
`player.Notify(device_id)` / `NotifyPan(device_id)`. Speed is `MULT_SPEED` (256 = 1x).
