// Prevents an extra console window on Windows in release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use nsfplay_core::output::{Output, Status as OutputStatus};
use nsfplay_core::Player;
use serde::Serialize;
use tauri::State;

/// Holds the error when no audio device could be opened; commands then report it to the UI.
struct Audio(Result<Output, String>);

impl Audio {
    fn get(&self) -> Result<&Output, String> {
        self.0.as_ref().map_err(Clone::clone)
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

#[tauri::command]
fn open(path: String, audio: State<Audio>) -> Result<FileInfo, String> {
    let output = audio.get()?;
    let data = std::fs::read(&path).map_err(|e| format!("Could not read {path}: {e}"))?;
    let player = Player::load(&data)?;
    let info = FileInfo {
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
    };
    output.load(player);
    Ok(info)
}

#[tauri::command]
fn play(track: u32, audio: State<Audio>) -> Result<(), String> {
    audio.get()?.play(track);
    Ok(())
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

// async so the potentially long emulation runs off the main thread
#[tauri::command]
async fn seek(ms: u32, audio: State<'_, Audio>) -> Result<(), String> {
    audio.get()?.seek(ms);
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
        .manage(Audio(Output::open()))
        .invoke_handler(tauri::generate_handler![
            open,
            play,
            set_paused,
            stop,
            seek,
            set_volume,
            set_mute_mask,
            status,
            initial_file
        ])
        .run(tauri::generate_context!())
        .expect("error while running NSFDeck");
}
