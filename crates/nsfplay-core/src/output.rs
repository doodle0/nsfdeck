//! Realtime playback of a [`Player`] on the default audio device.

use std::sync::{mpsc, Arc, Mutex, MutexGuard};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, SampleFormat, SizedSample, StreamConfig};

use crate::Player;

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
    track: u32,
    frames: u64,
    ended: bool,
    volume: f32,
    mask: u32,
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
            track: 0,
            frames: 0,
            ended: false,
            volume: 1.0,
            mask: 0,
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

    /// Replaces the current file and clears channel mutes. Playback stops.
    pub fn load(&self, player: Player) {
        let mut s = self.lock();
        s.player = Some(player);
        s.status = Status::Stopped;
        s.track = 0;
        s.frames = 0;
        s.ended = false;
        s.mask = 0;
    }

    /// Starts `track` (0-based) from the beginning.
    pub fn play(&self, track: u32) {
        let mut s = self.lock();
        let s = &mut *s;
        let Some(player) = s.player.as_mut() else { return };
        player.start(track, self.sample_rate);
        player.set_mute_mask(s.mask);
        s.track = track;
        s.frames = 0;
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
        let mut s = self.lock();
        s.status = Status::Stopped;
        s.frames = 0;
    }

    /// Jumps to `ms` in the current track by emulating silently up to that point.
    pub fn seek(&self, ms: u32) {
        let mut s = self.lock();
        let s = &mut *s;
        if s.status == Status::Stopped {
            return;
        }
        let Some(player) = s.player.as_mut() else { return };
        let target = ms as u64 * self.sample_rate as u64 / 1000;
        if target < s.frames {
            player.start(s.track, self.sample_rate);
            player.set_mute_mask(s.mask);
            s.frames = 0;
        }
        player.skip((target - s.frames) as u32);
        s.frames = target;
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
        Position {
            status: s.status,
            track: s.track,
            elapsed_ms: (s.frames * 1000 / self.sample_rate as u64) as u32,
            length_ms: match (&s.player, s.status) {
                (Some(p), Status::Playing | Status::Paused) => p.length_ms(),
                _ => 0,
            },
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
                        s.frames += frames as u64;
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
