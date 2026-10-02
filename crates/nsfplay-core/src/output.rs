//! Realtime playback of a [`Player`] on the default audio device.

use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard};

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
    /// The loaded file, for building more players (seeks, the A–B cue).
    data: Option<Arc<[u8]>>,
    status: Status,
    ended: bool,
    volume: f32,
    mask: u32,
    endless: bool,
    speed: u32,
    length: Option<Length>,
    rate: u32,
    /// A–B loop region in song ms, used in endless playback: on reaching `.1`, playback
    /// continues from `.0`.
    region: Option<(u32, u32)>,
    /// A player prepared in the background at the region start, swapped in at its end.
    cue: Option<Player>,
    /// Bumped whenever the current player or region changes, so background work for an old
    /// one is thrown away.
    generation: u64,
    /// Wakes the cue worker.
    cue_wanted: SyncSender<()>,
    /// Bumped by every seek, play and load: only the latest seek's player is swapped in.
    seek_seq: u64,
}

/// What it takes to rebuild the current track in another player.
struct Recipe {
    data: Arc<[u8]>,
    track: u32,
    rate: u32,
    mask: u32,
    endless: bool,
    speed: u32,
    length: Option<Length>,
    generation: u64,
}

impl Recipe {
    fn of(s: &State) -> Option<Recipe> {
        Some(Recipe {
            data: s.data.clone()?,
            track: s.player.as_ref()?.track(),
            rate: s.rate,
            mask: s.mask,
            endless: s.endless,
            speed: s.speed,
            length: s.length,
            generation: s.generation,
        })
    }

    /// A new player for the same track, already at `ms` of song time. Emulates up to that
    /// point, so call it without holding the state lock.
    fn build_at(&self, ms: u32) -> Option<Player> {
        let mut p = Player::load(&self.data).ok()?;
        p.set_endless(self.endless);
        p.set_speed(self.speed);
        p.set_mute_mask(self.mask);
        p.set_length(self.length);
        p.start(self.track, self.rate);
        p.seek(ms as u64);
        Some(p)
    }
}

/// Owns the audio stream and the player feeding it. Seeks are prepared on the calling
/// thread without interrupting playback; the A–B region's restart point is prepared on a
/// worker thread.
pub struct Output {
    state: Arc<Mutex<State>>,
    sample_rate: u32,
}

impl Output {
    /// Opens the default output device. The stream lives on its own thread because
    /// cpal streams cannot be moved between threads on every platform.
    pub fn open() -> Result<Output, String> {
        let (cue_tx, cue_rx) = mpsc::sync_channel(1);
        let state = Arc::new(Mutex::new(State {
            player: None,
            data: None,
            status: Status::Stopped,
            ended: false,
            volume: 1.0,
            mask: 0,
            endless: false,
            speed: SPEED_1X,
            length: None,
            rate: 48000,
            region: None,
            cue: None,
            generation: 0,
            cue_wanted: cue_tx,
            seek_seq: 0,
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
        lock(&state).rate = sample_rate;

        let worker_state = state.clone();
        std::thread::Builder::new()
            .name("nsfplay-cue".into())
            .spawn(move || cue_worker(worker_state, cue_rx))
            .map_err(|e| e.to_string())?;
        Ok(Output { state, sample_rate })
    }

    fn lock(&self) -> MutexGuard<'_, State> {
        lock(&self.state)
    }

    /// Replaces the current file and clears channel mutes. Playback stops. Endless mode and
    /// speed carry over to the new file. `data` is the file `player` was loaded from.
    pub fn load(&self, mut player: Player, data: Arc<[u8]>) {
        let mut s = self.lock();
        player.set_endless(s.endless);
        player.set_speed(s.speed);
        s.player = Some(player);
        s.data = Some(data);
        s.status = Status::Stopped;
        s.ended = false;
        s.mask = 0;
        s.region = None;
        s.cue = None;
        s.generation += 1;
        s.seek_seq += 1;
    }

    /// Starts `track` (0-based) from the beginning. `length` overrides how long it plays
    /// (see [`Player::set_length`]). Clears the A–B region.
    pub fn play(&self, track: u32, length: Option<Length>) {
        let mut s = self.lock();
        let s = &mut *s;
        let Some(player) = s.player.as_mut() else { return };
        player.set_mute_mask(s.mask);
        player.set_length(length);
        player.start(track, self.sample_rate);
        s.length = length;
        s.ended = false;
        s.status = Status::Playing;
        s.region = None;
        s.cue = None;
        s.generation += 1;
        s.seek_seq += 1;
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

    /// Jumps to `ms` of song time in the current track. Short forward jumps are emulated in
    /// place; anything else builds a new player at the target while the current one keeps
    /// playing, then swaps it in (if no newer seek, play or load came in meanwhile). Blocks
    /// the calling thread while it emulates.
    pub fn seek(&self, ms: u32) {
        let (recipe, seq) = {
            let mut s = self.lock();
            let s = &mut *s;
            if s.status == Status::Stopped {
                return;
            }
            s.seek_seq += 1;
            let Some(player) = s.player.as_mut() else { return };
            let elapsed = player.elapsed_ms();
            if ms as u64 >= elapsed && ms as u64 - elapsed <= 250 {
                player.seek(ms as u64);
                return;
            }
            (Recipe::of(s), s.seek_seq)
        };
        let Some(recipe) = recipe else { return };
        let Some(player) = recipe.build_at(ms) else { return };
        let mut s = self.lock();
        if s.seek_seq == seq {
            s.player = Some(player);
            s.generation += 1;
            s.cue = None;
            if s.region.is_some() {
                let _ = s.cue_wanted.try_send(());
            }
        }
    }

    /// Sets or clears the A–B loop region (song ms; applies in endless playback). Playback
    /// that reaches `b` continues from `a`, seamlessly once the restart point is prepared.
    pub fn set_region(&self, region: Option<(u32, u32)>) {
        let mut s = self.lock();
        let region = region.filter(|&(a, b)| b > a);
        if s.region == region {
            return;
        }
        s.region = region;
        s.cue = None;
        s.generation += 1;
        if region.is_some() {
            let _ = s.cue_wanted.try_send(());
        }
    }

    /// Changes the current track's length while it plays, e.g. once its analysis is done.
    pub fn set_length(&self, length: Option<Length>) {
        let mut s = self.lock();
        s.length = length;
        if let Some(player) = s.player.as_mut() {
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
        if let Some(cue) = s.cue.as_mut() {
            cue.set_endless(endless);
        }
    }

    /// Playback speed as a `MULT_SPEED` value ([`SPEED_1X`] = normal).
    pub fn set_speed(&self, speed: u32) {
        let mut s = self.lock();
        s.speed = speed;
        if let Some(player) = s.player.as_mut() {
            player.set_speed(speed);
        }
        if let Some(cue) = s.cue.as_mut() {
            cue.set_speed(speed);
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
        if let Some(cue) = s.cue.as_mut() {
            cue.set_mute_mask(mask);
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

fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
    state.lock().unwrap_or_else(|e| e.into_inner())
}

/// Keeps a player ready at the A–B region's start, so the audio callback can loop the region
/// without a gap. Woken whenever the region or the current player changes.
fn cue_worker(state: Arc<Mutex<State>>, wake: Receiver<()>) {
    while wake.recv().is_ok() {
        let job = {
            let s = lock(&state);
            match s.region {
                Some((a, _)) if s.cue.is_none() => Recipe::of(&s).map(|r| (r, a)),
                _ => None,
            }
        };
        let Some((recipe, a)) = job else { continue };
        let Some(cue) = recipe.build_at(a) else { continue };
        let mut s = lock(&state);
        if s.generation == recipe.generation && s.region.is_some_and(|r| r.0 == a) {
            s.cue = Some(cue);
        }
    }
}

/// Renders `pcm` (interleaved stereo) from the current player, looping the A–B region: the
/// part up to the region end comes from the current player, the rest from the cue player.
fn render(s: &mut State, pcm: &mut [i16]) {
    let Some(player) = s.player.as_mut() else { return };
    let Some((_, b)) = s.region.filter(|_| s.endless) else {
        player.render(pcm);
        return;
    };
    let elapsed = player.elapsed_ms();
    let frames = pcm.len() / 2;
    // output frames until the song reaches `b`, at the current speed
    let until_b = (b as u64).saturating_sub(elapsed) * s.rate as u64 * SPEED_1X as u64;
    let until_b = until_b.div_ceil(1000 * player.speed() as u64) as usize;
    if until_b >= frames {
        player.render(pcm);
        return;
    }
    let (head, tail) = pcm.split_at_mut(until_b * 2);
    player.render(head);
    match s.cue.take() {
        Some(mut cue) => {
            cue.set_mute_mask(s.mask);
            cue.render(tail);
            s.player = Some(cue);
            s.generation += 1;
            let _ = s.cue_wanted.try_send(());
        }
        // not prepared yet (just after setting the region): play on until it is
        None => s.player.as_mut().unwrap().render(tail),
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
                // Never block the audio thread; a contended lock plays silence.
                if let Ok(mut s) = state.try_lock() {
                    let s = &mut *s;
                    if s.status == Status::Playing && s.player.is_some() {
                        render(s, &mut pcm);
                        gain = s.volume / 32768.0;
                        if s.player.as_ref().is_some_and(|p| p.is_stopped()) {
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

#[cfg(test)]
mod tests {
    use super::*;

    const ARPEGGIO: &[u8] = include_bytes!("../tests/data/arpeggio.nsf");
    const RATE: u32 = 48000;

    fn state(region: Option<(u32, u32)>) -> (State, Receiver<()>) {
        let (tx, rx) = mpsc::sync_channel(1);
        let data: Arc<[u8]> = ARPEGGIO.into();
        let mut player = Player::load(&data).unwrap();
        player.set_endless(true);
        player.start(0, RATE);
        let s = State {
            player: Some(player),
            data: Some(data),
            status: Status::Playing,
            ended: false,
            volume: 1.0,
            mask: 0,
            endless: true,
            speed: SPEED_1X,
            length: None,
            rate: RATE,
            region,
            cue: None,
            generation: 0,
            cue_wanted: tx,
            seek_seq: 0,
        };
        (s, rx)
    }

    #[test]
    fn region_loops_seamlessly() {
        let (mut s, wake) = state(Some((1000, 2000)));
        s.cue = Recipe::of(&s).unwrap().build_at(1000);
        let mut pcm = vec![0i16; 512 * 2];
        // play up to just before the region end
        while s.player.as_ref().unwrap().elapsed_ms() + 10 < 2000 {
            render(&mut s, &mut pcm);
        }
        let before = s.player.as_ref().unwrap().elapsed_ms();
        render(&mut s, &mut pcm);
        let after = s.player.as_ref().unwrap().elapsed_ms();
        assert!(before < 2000 && (1000..1011).contains(&after), "{before} -> {after}");
        assert!(wake.try_recv().is_ok(), "the worker is asked for the next cue");

        // the samples after the jump are exactly what a player started at the region start plays
        let frames_before_b = ((2000 - before) * RATE as u64).div_ceil(1000) as usize;
        let mut reference = Recipe::of(&s).unwrap().build_at(1000).unwrap();
        let mut expected = vec![0i16; pcm.len() - frames_before_b * 2];
        reference.render(&mut expected);
        assert_eq!(&pcm[frames_before_b * 2..], &expected[..]);
    }

    #[test]
    fn region_waits_for_its_cue() {
        let (mut s, _wake) = state(Some((1000, 2000)));
        let mut pcm = vec![0i16; 512 * 2];
        for _ in 0..200 {
            render(&mut s, &mut pcm);
        }
        // no cue prepared: playback just continues past the region end
        assert!(s.player.as_ref().unwrap().elapsed_ms() > 2000);
        s.cue = Recipe::of(&s).unwrap().build_at(1000);
        render(&mut s, &mut pcm);
        assert!(s.player.as_ref().unwrap().elapsed_ms() < 1100, "jumps back once the cue is ready");
    }

    #[test]
    fn no_region_outside_endless_playback() {
        let (mut s, _wake) = state(Some((1000, 2000)));
        s.endless = false;
        s.cue = Recipe::of(&s).unwrap().build_at(1000);
        let mut pcm = vec![0i16; 512 * 2];
        for _ in 0..200 {
            render(&mut s, &mut pcm);
        }
        assert!(s.player.as_ref().unwrap().elapsed_ms() > 2000);
    }
}
