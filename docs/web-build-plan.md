# Web build plan (future)

Not started. Goal: a browser version of NSFDeck, deployable to GitHub Pages, without
changing the desktop builds: WebAssembly is for the web target only.

## Approach

- **Core:** compile the C++ core, the shim and our Rust `Player` (track clock, fades,
  seek slicing, `analysis`) to WebAssembly with Emscripten (`wasm32-unknown-emscripten`;
  the `cc` crate drives `emcc`). One implementation of the playback logic, not a JS port.
  Emscripten's musl has iconv, which the core links against.
- **Not portable:** `Output` (cpal stream, seek thread, `nsfplay-cue` worker). On the web:
  - an **audio Worker** owns the `Player`, renders ahead (~100-200 ms) and posts PCM
    chunks to an **AudioWorklet** over a `MessagePort`. Seeks skip in slices between
    render ticks so playback continues, like `Output` does with its seek thread.
  - an **analysis Worker** with its own wasm instance. Separate instances don't share
    rconv's static buffer, so `RENDER_LOCK` isn't needed there.
  - Avoid `SharedArrayBuffer`/wasm threads: they need cross-origin isolation headers,
    which GitHub Pages can't send (the `coi-serviceworker` workaround exists if needed).
- **Frontend:** put the 24 Tauri commands, the `playback` event, the dialog plugin and
  drag-drop behind one backend interface (`tauri` and `web` implementations), chosen at
  build time (a Vite mode or alias), so desktop bundles contain no wasm.
  Files currently imported from `@tauri-apps/*`: `App.svelte`, `player.svelte.js`,
  `playlist.svelte.js`, `DumpView.svelte`.
- **Web equivalents:**
  - file paths → `File` objects; store file bytes in IndexedDB (NSFs are small) so the
    playlist survives a reload; give entries stable pseudo-paths.
  - `scan` (folders) → `<input webkitdirectory>` and dropped directory entries.
  - `state.json` (`load_state`/`save_state`) → IndexedDB or `localStorage`.
  - M3U import/export (`read_text`/`write_text`) → file upload / download.
  - `initial_file` → none (maybe a `?url=` parameter later).

## Phases

1. **Toolchain spike:** build core + shim + `Player` for `wasm32-unknown-emscripten`;
   render `arpeggio.nsf` in Node and compare samples against the native build (they
   should match exactly). This is the main risk, so it goes first.
2. **Web player:** audio Worker + AudioWorklet, the backend interface, Listen mode
   (open/drop files, playlist, play/pause/stop/seek/volume/mute).
3. **The rest:** analysis Worker, Studio (A–B loop, speed, mixer/`set_config`),
   Developer dump.
4. **Deploy:** GitHub Actions workflow (setup emsdk + Rust target, `pnpm build` in web
   mode, `actions/deploy-pages`). Needs the commits pushed and Pages enabled in the repo
   settings (Settings → Pages → Source: GitHub Actions).

## Before starting

- Install emsdk (~1-1.5 GB, no sudo; the old `~/emsdk` was deleted) and
  `rustup target add wasm32-unknown-emscripten`. Ask the user where to install it.
- Decide how far the first pass goes (spike only, through Listen, or everything).
