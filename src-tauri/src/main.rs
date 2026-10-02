// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::sync::Arc;

use nsfplay_core::analysis::analyze as analyze_track;
use nsfplay_core::output::{Output, Status as OutputStatus};
use nsfplay_core::{Detected, Length, Player};
use serde::{Deserialize, Serialize};
use tauri::{Manager, State};

/// Holds the error when no audio device could be opened; commands then report it to the UI.
struct Audio(Result<Arc<Output>, String>);

impl Audio {
    fn get(&self) -> Result<&Output, String> {
        self.0.as_deref().map_err(Clone::clone)
    }

    /// A handle that can move to another thread.
    fn shared(&self) -> Result<Arc<Output>, String> {
        self.0.clone()
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct TrackInfo {
    title: String,
    length_ms: Option<u32>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ChannelInfo {
    bit: u32,
    chip: &'static str,
    name: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct FileInfo {
    path: String,
    title: String,
    artist: String,
    copyright: String,
    ripper: String,
    chips: Vec<&'static str>,
    tracks: Vec<TrackInfo>,
    channels: Vec<ChannelInfo>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Status {
    state: &'static str,
    track: u32,
    elapsed_ms: u32,
    length_ms: u32,
    /// Set once when a track finishes on its own, so the UI can advance.
    ended: bool,
}

#[derive(Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
struct LengthArg {
    play_ms: u32,
    fade_ms: u32,
}

impl From<LengthArg> for Length {
    fn from(l: LengthArg) -> Length {
        Length { play_ms: l.play_ms, fade_ms: l.fade_ms }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum Analysis {
    Loop { start_ms: u32, end_ms: u32 },
    Silence { at_ms: u32 },
    None,
}

fn read_file(path: &str) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|e| format!("Could not read {path}: {e}"))
}

fn file_info(path: String, player: &Player) -> FileInfo {
    FileInfo {
        path,
        title: player.title(),
        artist: player.artist(),
        copyright: player.copyright(),
        ripper: player.ripper(),
        chips: player.chips().into_iter().map(|c| c.name()).collect(),
        tracks: player
            .tracks()
            .into_iter()
            .map(|t| TrackInfo { title: t.title, length_ms: t.length_ms })
            .collect(),
        channels: player
            .channels()
            .into_iter()
            .map(|c| ChannelInfo { bit: c.bit, chip: c.chip.name(), name: c.name })
            .collect(),
    }
}

/// Loads a file into the audio output. Playback stops until `play`.
#[tauri::command]
fn open(path: String, audio: State<Audio>) -> Result<FileInfo, String> {
    let output = audio.get()?;
    let data: Arc<[u8]> = read_file(&path)?.into();
    let player = Player::load(&data)?;
    let info = file_info(path, &player);
    output.load(player, data);
    Ok(info)
}

/// Reads a file's metadata without touching playback, e.g. to add it to the playlist.
#[tauri::command]
async fn probe(path: String) -> Result<FileInfo, String> {
    let player = Player::load(&read_file(&path)?)?;
    Ok(file_info(path, &player))
}

/// Expands folders (recursively) into the NSF/NSFe files they contain, sorted by path.
/// Plain file paths are passed through.
#[tauri::command]
async fn scan(paths: Vec<String>) -> Vec<String> {
    fn is_nsf(p: &Path) -> bool {
        p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("nsf") || e.eq_ignore_ascii_case("nsfe"))
    }
    fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                walk(&p, out);
            } else if is_nsf(&p) {
                out.push(p);
            }
        }
    }
    let mut out = Vec::new();
    for path in paths {
        let p = PathBuf::from(&path);
        if p.is_dir() {
            let mut found = Vec::new();
            walk(&p, &mut found);
            found.sort();
            out.extend(found.into_iter().map(|f| f.to_string_lossy().into_owned()));
        } else {
            out.push(path);
        }
    }
    out
}

/// Finds a track's loop or end by playing it faster than real time (see nsfplay_core::analysis).
#[tauri::command]
async fn analyze(path: String, track: u32) -> Result<Analysis, String> {
    let data = read_file(&path)?;
    let found = tauri::async_runtime::spawn_blocking(move || analyze_track(&data, track, 10 * 60 * 1000))
        .await
        .map_err(|e| e.to_string())??;
    Ok(match found {
        Detected::Loop { start_ms, end_ms } => Analysis::Loop { start_ms, end_ms },
        Detected::Silence { at_ms } => Analysis::Silence { at_ms },
        Detected::Nothing => Analysis::None,
    })
}

#[tauri::command]
fn play(track: u32, length: Option<LengthArg>, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.play(track, length.map(Into::into));
    Ok(())
}

/// Changes the current track's length while it plays; `null` restores the file's default.
#[tauri::command]
fn set_length(length: Option<LengthArg>, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_length(length.map(Into::into));
    Ok(())
}

fn state_file(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    Ok(app.path().app_config_dir().map_err(|e| e.to_string())?.join("state.json"))
}

/// The UI's saved state (playlist, settings) as JSON, or `null` on first run.
#[tauri::command]
fn load_state(app: tauri::AppHandle) -> Result<Option<String>, String> {
    match std::fs::read_to_string(state_file(&app)?) {
        Ok(s) => Ok(Some(s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("Could not read saved state: {e}")),
    }
}

#[tauri::command]
fn save_state(json: String, app: tauri::AppHandle) -> Result<(), String> {
    let path = state_file(&app)?;
    let dir = path.parent().ok_or("No config directory")?;
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    // write then rename, so a crash never leaves a half-written file
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, json).map_err(|e| format!("Could not save state: {e}"))?;
    std::fs::rename(&tmp, &path).map_err(|e| format!("Could not save state: {e}"))
}

/// Text file access for playlist (M3U) import and export, on paths the user picked.
#[tauri::command]
fn read_text(path: String) -> Result<String, String> {
    let bytes = read_file(&path)?;
    Ok(String::from_utf8(bytes).unwrap_or_else(|e| e.into_bytes().iter().map(|&b| b as char).collect()))
}

#[tauri::command]
fn write_text(path: String, text: String) -> Result<(), String> {
    std::fs::write(&path, text).map_err(|e| format!("Could not write {path}: {e}"))
}

#[tauri::command]
fn set_paused(paused: bool, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_paused(paused);
    Ok(())
}

#[tauri::command]
fn stop(audio: State<Audio>) -> Result<(), String> {
    audio.get()?.stop();
    Ok(())
}

// async, with the emulation on a blocking thread, so a long seek holds up neither the main
// thread nor the async runtime; playback continues meanwhile
#[tauri::command]
async fn seek(ms: u32, audio: State<'_, Audio>) -> Result<(), String> {
    let output = audio.shared()?;
    tauri::async_runtime::spawn_blocking(move || output.seek(ms)).await.map_err(|e| e.to_string())
}

/// Sets the A–B loop region in song ms (`null` clears it). Used in endless playback.
#[tauri::command]
fn set_region(region: Option<(u32, u32)>, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_region(region);
    Ok(())
}

#[tauri::command]
fn set_volume(volume: f32, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_volume(volume);
    Ok(())
}

#[tauri::command]
fn set_mute_mask(mask: u32, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_mute_mask(mask);
    Ok(())
}

/// Endless playback (Studio and Developer modes): tracks never fade out or end.
#[tauri::command]
fn set_endless(endless: bool, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.set_endless(endless);
    Ok(())
}

/// `speed` is a multiplier, e.g. 1.0 for normal speed.
#[tauri::command]
fn set_speed(speed: f64, audio: State<Audio>) -> Result<(), String> {
    let mult = (speed * nsfplay_core::SPEED_1X as f64).round().clamp(1.0, u32::MAX as f64) as u32;
    audio.get()?.set_speed(mult);
    Ok(())
}

/// Plain-text emulator state for Developer mode, or an empty string with no file loaded.
#[tauri::command]
fn dump(audio: State<Audio>) -> Result<String, String> {
    Ok(audio.get()?.with_player(|p| p.dump()).unwrap_or_default())
}

#[tauri::command]
fn status(audio: State<Audio>) -> Result<Status, String> {
    let output = audio.get()?;
    let pos = output.position();
    Ok(Status {
        state: match pos.status {
            OutputStatus::Stopped => "stopped",
            OutputStatus::Playing => "playing",
            OutputStatus::Paused => "paused",
        },
        track: pos.track,
        elapsed_ms: pos.elapsed_ms,
        length_ms: pos.length_ms,
        ended: output.take_ended(),
    })
}

/// File passed on the command line, e.g. from a file manager's "Open with".
#[tauri::command]
fn initial_file() -> Option<String> {
    std::env::args().nth(1).filter(|a| !a.starts_with('-'))
}

fn main() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(Audio(Output::open().map(Arc::new)))
        .invoke_handler(tauri::generate_handler![
            open,
            probe,
            scan,
            analyze,
            play,
            set_length,
            set_region,
            load_state,
            save_state,
            read_text,
            write_text,
            set_paused,
            stop,
            seek,
            set_volume,
            set_mute_mask,
            set_endless,
            set_speed,
            dump,
            status,
            initial_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running NSFDeck");
}
