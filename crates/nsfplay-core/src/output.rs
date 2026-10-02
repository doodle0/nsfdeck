//! Realtime playback of a [`Player`] on the default audio device.

use std::sync::{mpsc, Arc, Mutex, MutexGuard};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::{Length, Player, SPEED_1X};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    Stopped,
    Playing,
    Paused,
}

#[derive(Clone, Copy, Debug)]
pub struct Position {
    pub status: Status,
    pub track: u32,
    pub elapsed_ms: u32,
    pub length_ms: u32,
}

struct State {
    player: Option<Player>,
    status: Status,
    ended: bool,
    volume: f32,
    mask: u32,
    endless: bool,
    speed: u32,
}

/// Owns the audio stream and the player feeding it. All methods are cheap except
/// [`Output::seek`], which has to emulate up to the target time.
pub struct Output {
    state: Arc<Mutex<State>>,
    sample_rate: u32,
}

impl Output {
    /// Opens the default output device. The stream lives on its own thread because
    /// cpal streams cannot be moved between threads on every platform.
    pub fn open() -> Result<Output, String> {
        let state = Arc::new(Mutex::new(State {
            player: None,
            status: Status::Stopped,
            ended: false,
            volume: 1.0,
            mask: 0,
            endless: false,
            speed: SPEED_1X,
        }));
        let (ready_tx, ready_rx) = mpsc::channel();
        let thread_state = state.clone();
        std::thread::Builder::new()
            .name("nsfplay-audio".into())
            .spawn(move || match start_stream(thread_state) {
                Ok((stream, rate)) => {
                    let _ = ready_tx.send(Ok(rate));
                    loop {
                        std::thread::park(); // keep `stream` alive for the life of the process
                        let _ = &stream;
                    }
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            })
            .map_err(|e| e.to_string())?;
        let sample_rate = ready_rx.recv().map_err(|e| e.to_string())??;
        Ok(Output { state, sample_rate })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Replaces the current file and clears channel mutes. Playback stops. Endless mode and
    /// speed carry over to the new file.
    pub fn load(&self, mut player: Player) {
        let mut s = self.lock();
        player.set_endless(s.endless);
        player.set_speed(s.speed);
        s.player = Some(player);
        s.status = Status::Stopped;
        s.ended = false;
        s.mask = 0;
    }

    /// Starts `track` (0-based) from the beginning. `length` overrides how long it plays
    /// (see [`Player::set_length`]).
    pub fn play(&self, track: u32, length: Option<Length>) {
        let mut s = self.lock();
        let s = &mut *s;
        let Some(player) = s.player.as_mut() else { return };
        player.set_mute_mask(s.mask);
        player.set_length(length);
        player.start(track, self.sample_rate);
        s.ended = false;
        s.status = Status::Playing;
    }

    pub fn set_paused(&self, paused: bool) {
        let mut s = self.lock();
        match (s.status, paused) {
            (Status::Playing, true) => s.status = Status::Paused,
            (Status::Paused, false) => s.status = Status::Playing,
            _ => {}
        }
    }

    pub fn stop(&self) {
        self.lock().status = Status::Stopped;
    }

    /// Jumps to `ms` in the current track by emulating silently up to that point.
    pub fn seek(&self, ms: u32) {
        let mut s = self.lock();
        let s = &mut *s;
        if s.status == Status::Stopped {
            return;
        }
        if let Some(player) = s.player.as_mut() {
            player.seek(ms as u64);
        }
    }

    /// Changes the current track's length while it plays, e.g. once its analysis is done.
    pub fn set_length(&self, length: Option<Length>) {
        if let Some(player) = self.lock().player.as_mut() {
            player.set_length(length);
        }
    }

    /// Endless playback (Studio mode): the track never fades out or ends.
    pub fn set_endless(&self, endless: bool) {
        let mut s = self.lock();
        s.endless = endless;
        if let Some(player) = s.player.as_mut() {
            player.set_endless(endless);
        }
    }

    /// Playback speed as a `MULT_SPEED` value ([`SPEED_1X`] = normal).
    pub fn set_speed(&self, speed: u32) {
        let mut s = self.lock();
        s.speed = speed;
        if let Some(player) = s.player.as_mut() {
            player.set_speed(speed);
        }
    }

    /// Runs `f` on the loaded player, e.g. to change its config or read its state.
    pub fn with_player<R>(&self, f: impl FnOnce(&mut Player) -> R) -> Option<R> {
        self.lock().player.as_mut().map(f)
    }

    /// Linear gain, 0.0 to 1.0.
    pub fn set_volume(&self, volume: f32) {
        self.lock().volume = volume.clamp(0.0, 1.0);
    }

    /// Bit set = channel muted, see [`crate::CHANNELS`].
    pub fn set_mute_mask(&self, mask: u32) {
        let mut s = self.lock();
        s.mask = mask;
        if let Some(player) = s.player.as_mut() {
            player.set_mute_mask(mask);
        }
    }

    pub fn position(&self) -> Position {
        let s = self.lock();
        let active = matches!(s.status, Status::Playing | Status::Paused);
        let player = s.player.as_ref();
        Position {
            status: s.status,
            track: player.map_or(0, |p| p.track()),
            elapsed_ms: player.filter(|_| active).map_or(0, |p| p.elapsed_ms() as u32),
            length_ms: player.filter(|_| active).map_or(0, |p| p.length_ms()),
        }
    }

    /// True once after a track plays to its end.
    pub fn take_ended(&self) -> bool {
        std::mem::take(&mut self.lock().ended)
    }
}

fn start_stream(state: Arc<Mutex<State>>) -> Result<(cpal::Stream, u32), String> {
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("No audio output device found")?;
    let supported = device.default_output_config().map_err(|e| e.to_string())?;
    let format = supported.sample_format();
    let config: StreamConfig = supported.into();
    let rate = config.sample_rate;
    let stream = match format {
        SampleFormat::I16 => build::<i16>(&device, config, state),
        SampleFormat::I32 => build::<i32>(&device, config, state),
        SampleFormat::U16 => build::<u16>(&device, config, state),
        SampleFormat::F32 => build::<f32>(&device, config, state),
        SampleFormat::F64 => build::<f64>(&device, config, state),
        other => return Err(format!("Unsupported audio sample format {other}")),
    }?;
    stream.play().map_err(|e| e.to_string())?;
    Ok((stream, rate))
}

fn build<T>(device: &cpal::Device, config: StreamConfig, state: Arc<Mutex<State>>) -> Result<cpal::Stream, String>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels as usize;
    let mut pcm: Vec<i16> = Vec::new();
    device
        .build_output_stream(
            config,
            move |out: &mut [T], _| {
                let frames = out.len() / channels;
                pcm.clear();
                pcm.resize(frames * 2, 0);
                let mut gain = 0.0;
                // Never block the audio thread; a contended lock (e.g. during seek) plays silence.
                if let Ok(mut s) = state.try_lock() {
                    let s = &mut *s;
                    if let (Status::Playing, Some(player)) = (s.status, s.player.as_mut()) {
                        player.render(&mut pcm);
                        gain = s.volume / 32768.0;
                        if player.is_stopped() {
                            s.status = Status::Stopped;
                            s.ended = true;
                        }
                    }
                }
                for (frame, lr) in out.chunks_mut(channels).zip(pcm.chunks(2)) {
                    for (i, sample) in frame.iter_mut().enumerate() {
                        let v = if i < 2 { lr[i] as f32 * gain } else { 0.0 };
                        *sample = T::from_sample(v);
                    }
                }
            },
            |err| eprintln!("audio stream error: {err}"),
            None,
        )
        .map_err(|e| e.to_string())
}
