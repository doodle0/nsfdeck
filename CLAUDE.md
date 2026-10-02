# NSFDeck

Cross-platform NSF/NSFe player: Tauri 2 + Svelte 5 frontend, audio via cpal, emulation by
NSFPlay's `xgm` core (git submodule at `vendor/nsfplay`, compiled by
`crates/nsfplay-core/build.rs`). See README.md for the architecture diagram and layout.

## Commands

- Use **pnpm** for JS (never npm/npx) and **uv** for any Python.
- `pnpm tauri dev`: run the app with hot reload.
- `pnpm check`: svelte-check (keep at 0 errors, 0 warnings). `pnpm build`: Vite build into `dist/`.
- `cargo test -p nsfplay-core`: core tests (tests/playback.rs, using tests/data/arpeggio.nsf).
- `cargo run -p nsfplay-core --features output --example play -- FILE.nsf [TRACK] [SECONDS]`: play audio without the GUI.
- `pnpm tauri build --debug --no-bundle`, then `target/debug/nsfdeck FILE.nsf`: run the real app on a file.

## Rules

- **Never modify `vendor/nsfplay`.** Work around upstream issues in `crates/nsfplay-core/shim/` or `src/lib.rs`, and propose real fixes upstream.
- Core source lists in `build.rs` mirror upstream `contrib/Makefile`; update them when bumping the submodule.
- Keep `nsfplay-core` free of Tauri dependencies; the app talks to it only through `Output`/`Player`.

## Upstream behavior to know about

- `NSF::Load(image, size)` leaves play/fade/loop times uninitialized; only `NSF::LoadFile` sets them. The shim mirrors LoadFile (`time_in_ms = -1`, etc.). Without that, every track ends after 50 ms.
- `RateConverter::FastRender` (upstream `xgm/devices/Audio/rconv.cpp`) uses a `static INT32 t[2]` shared by all players. `RENDER_LOCK` in `src/lib.rs` serializes `start`/`render`/`skip`, because `Reset()` also renders.
- The core `printf`s debug output unless `NDEBUG` is defined (build.rs defines it).
- Mute mask bits follow `NSFPlayerConfig::channel_name`. VRC7 channels 6-8 are left out of `CHANNELS` because the core shifts their bits onto N163's.
- Text fields are UTF-8 or Shift-JIS; `decode()` falls back to Shift-JIS.
- `sjis_legacy` (upstream `nsf.cpp`) runs iconv in place on NSF header strings and aborts under glibc. The shim blanks non-ASCII header fields before `Load` and returns the raw bytes instead. macOS needs `-liconv` (build.rs). This is a candidate upstream fix.
- `NSFPlayer::Render`/`Skip` advance `time_in_ms` by a truncated whole number of ms per call. `Player` therefore calls the core only in whole-ms chunks (`step`, e.g. 48 frames at 48 kHz) and buffers the rest; otherwise small audio buffers make tracks overrun (64 frames: 5:05 becomes 6:45). `Skip` also computes `1000 * length` in UINT32, so `Player::skip` splits long seeks into pieces under 4M frames. This is a candidate upstream fix.
- `AUTO_STOP` ends tracks of unknown length after `STOP_SEC` (3 s) of silence. Otherwise the default length is 5:00 plus a 5 s fade.

## Testing notes

- There are no real NSF files in the repo. `arpeggio.nsf` is hand-assembled 6502 code: track 1 is a square-wave arpeggio, track 2 is silent (tests auto-stop). The generator script isn't saved; rebuild it from the bytes if needed.
- UI without a backend: copy `dist/` or the source files and mock `@tauri-apps/api`, then screenshot with headless `google-chrome`. Desktop screenshots via PIL `ImageGrab.grab(xdisplay=":0")` capture the user's **whole screen**, so ask first.

## Status (handover, 2026-10-03)

Done: open/drag-drop/CLI-arg loading, track list, play/pause/stop/prev/next, seek, volume, metadata, chip badges, per-channel mute/solo, light/dark themes, keyboard shortcuts. The app has been built and run on Linux with the test file.

Not done yet:
- **Nothing is committed** except the staged submodule. Ask before committing or pushing.
- License is MIT (`LICENSE`, © 2026 doodle0). Git identity is set per repo: doodle0 <49020517+doodle0@users.noreply.github.com>, matching gh account `doodle0`. `identifier` in `src-tauri/tauri.conf.json` is the placeholder `dev.nsfdeck.app`.
- Not yet tested with a real game soundtrack, or on macOS/Windows. No CI.
- Upstream fix for the rconv static buffer is not yet submitted as a PR. Its branch (`fix-rconv-static-buffer` in `~/projects/nsfplay`) is gone (as of 2026-10-03), so it must be recreated.
- `~/emsdk` is left over from an abandoned WebAssembly approach; the user may want it deleted.

Next planned work (features in original NSFPlay that are missing here), roughly in priority order:
1. **Mixer.** Per-device volume uses config `<DEV>_VOLUME` (dev names: `APU1 APU2 5B MMC5 N163 VRC6 VRC7 FDS`, default 128) plus `MASTER_VOLUME`. Per-channel volume/pan uses `CHANNEL_nn_VOL` and `CHANNEL_nn_PAN` (default 128). Apply with `player.Notify(device_id)` / `NotifyPan(device_id)`. Needs a generic config setter in the shim.
2. Settings: default play/fade time, loops, silence auto-stop, loop detection, region (NTSC/PAL/Dendy), speed, sample rate/quality, LPF/HPF, per-chip emulation options. Persist settings.
3. Channel visualizer / keyboard view (`NSFPlayer::GetInfo`, `infobuf`).
4. WAV export, NSFe text/info view, memory viewer, presets, M3U playlists.
