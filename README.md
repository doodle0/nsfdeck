# NSFDeck

A cross-platform player for NES Sound Format (NSF/NSFe) files, for Linux, macOS and
Windows. It's built with [Tauri](https://tauri.app) and [Svelte](https://svelte.dev), and
plays music through the emulation core of
[NSFPlay](https://github.com/bbbradsmith/nsfplay), which is included as a git submodule.

Features: open or drag in NSF/NSFe files, track list, play/pause/stop/prev/next,
seeking, volume, file metadata, and three modes:

- **Listen** plays a playlist: add files or whole folders, reorder, shuffle and repeat, and
  import or export NSF M3U playlists. Each track's loop is found in the background so it can
  play a set number of loops and fade out; lengths can also be fixed per track.
- **Studio** loops the current track endlessly. Its timeline shows the intro and one pass of
  the loop (found automatically, adjustable by hand), with click-to-seek and a seamless A–B
  loop (Shift-drag to mark). Its mixer sets volume per chip and volume and pan per channel,
  with mute/solo and a live keyboard showing the note each channel plays. Also a speed
  control (tempo only, not pitch).
- **Developer** loops the current track and shows a live text dump of the emulator state
  (CPU, banks, RAM, sound registers).

See [docs/ux-plan.md](docs/ux-plan.md) for where each mode is headed.

Keyboard: Space play/pause · ↑/↓ previous/next track · ←/→ seek 5 s (Studio: 1 s, or one
frame while paused) · Ctrl+O open · Ctrl+1/2/3 Listen/Studio/Developer.

## Architecture

```mermaid
flowchart TB
    subgraph webview["Webview · src/"]
        direction LR
        ui["Svelte components<br/>NowPlaying · ModeSwitch · Transport<br/>TrackList · Channels · DumpView"] <--> store["player.svelte.js<br/>shared playback state"]
    end

    subgraph app["Tauri app · src-tauri/"]
        direction LR
        dialog["Dialog plugin<br/>drag-and-drop"]
        cmds["Commands<br/>open · probe · scan · analyze · play · seek · stop<br/>set_paused · set_volume · set_mute_mask · set_length<br/>set_endless · set_speed · set_region · set_config · dump · status<br/>playback event (30 Hz)<br/>load_state · save_state · read_text · write_text"]
    end

    subgraph core["nsfplay-core · crates/nsfplay-core/"]
        output["Output<br/>Mutex&lt;State&gt;"]
        audio["Audio thread<br/>cpal callback"]
        player["Player<br/>safe Rust API"]
        shim["C shim<br/>nsfplay_shim.cpp"]
    end

    subgraph nsfplay["NSFPlay submodule · vendor/nsfplay/"]
        xgm["xgm::NSFPlayer<br/>6502 CPU · 2A03 APU<br/>VRC6 · VRC7 · FDS · MMC5 · N163 · 5B"]
    end

    file[/"NSF / NSFe file"/]
    os[("OS audio<br/>PipeWire · PulseAudio · ALSA<br/>CoreAudio · WASAPI")]

    store -- "invoke()<br/>+ 100 ms status poll" --> cmds
    store -- "pick file" --> dialog
    cmds -- "read" --> file
    cmds -- "load · play · seek · mute" --> output
    output --> player
    audio -- "try_lock" --> output
    audio -- "render PCM" --> player
    player -- "FFI" --> shim --> xgm
    audio -- "samples" --> os
```

The UI never handles audio. Commands change the shared `Output` state, and cpal's
realtime callback renders samples from the emulator on its own thread. That callback
only uses `try_lock`, so a slow command such as a long seek produces a moment of
silence rather than a stalled audio thread.

## Building

Requirements: [Rust](https://rustup.rs), Node.js with [pnpm](https://pnpm.io), a C++17 compiler, and the
[Tauri system dependencies](https://v2.tauri.app/start/prerequisites/). On Debian/Ubuntu:

    sudo apt install libwebkit2gtk-4.1-dev build-essential libssl-dev \
        libayatana-appindicator3-dev librsvg2-dev libasound2-dev

Then:

    git submodule update --init   # fetch NSFPlay into vendor/nsfplay (or clone with --recursive)
    pnpm install
    pnpm tauri dev                # run with hot reload
    pnpm tauri build              # build installers into target/release/bundle/

On Windows the app's webview (WebView2) runs with `--no-proxy-server`, set in
`src-tauri/tauri.conf.json`. Otherwise WebView2 waits for proxy auto-detection before loading
the page, which can leave `pnpm tauri dev` on a white screen for tens of seconds. The UI never
loads anything from the network, so system proxy settings don't apply to it.

## Layout

| Path | Contents |
|---|---|
| `vendor/nsfplay/` | Upstream NSFPlay (submodule). Only its `xgm/` and `vcm/` emulation core is compiled. |
| `crates/nsfplay-core/` | Rust crate that compiles the core, wraps it through a small C shim (`shim/`), and plays audio with [cpal](https://github.com/RustAudio/cpal) behind the `output` feature. It does not depend on Tauri. |
| `src-tauri/` | The Tauri app: window setup and the commands the UI calls. |
| `src/` | The Svelte 5 frontend. `lib/player.svelte.js` holds playback state; `lib/components/` holds the UI. |

The core crate can be tested and run on its own:

    cargo test -p nsfplay-core
    cargo run -p nsfplay-core --features output --example play -- FILE.nsf [TRACK] [SECONDS]

## Updating NSFPlay

    cd vendor/nsfplay && git fetch && git checkout <commit-or-tag> && cd ../..
    cargo test -p nsfplay-core
    git commit -am "Update NSFPlay"

If upstream adds or removes core source files, update the lists in
`crates/nsfplay-core/build.rs`, which mirror NSFPlay's `contrib/Makefile`.

## License

NSFDeck is released under the [MIT License](LICENSE). The bundled NSFPlay emulation core has its own
permissive terms; see [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
