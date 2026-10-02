//! Safe wrapper around the NSFPlay xgm emulation core.

use std::ffi::{c_char, c_int, CStr};
use std::sync::{Mutex, MutexGuard};

pub mod analysis;
#[cfg(feature = "output")]
pub mod output;

#[repr(C)]
struct RawPlayer {
    _private: [u8; 0],
}

extern "C" {
    fn nsfp_create() -> *mut RawPlayer;
    fn nsfp_destroy(p: *mut RawPlayer);
    fn nsfp_load(p: *mut RawPlayer, data: *const u8, size: u32) -> c_int;
    fn nsfp_error(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_title(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_artist(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_copyright(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_ripper(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_expansions(p: *mut RawPlayer) -> u32;
    fn nsfp_track_count(p: *mut RawPlayer) -> c_int;
    fn nsfp_track_title(p: *mut RawPlayer, track: c_int) -> *const c_char;
    fn nsfp_track_length(p: *mut RawPlayer, track: c_int) -> c_int;
    fn nsfp_start(p: *mut RawPlayer, track: c_int, rate: f64);
    fn nsfp_render(p: *mut RawPlayer, buf: *mut i16, frames: u32) -> u32;
    fn nsfp_skip(p: *mut RawPlayer, frames: u32);
    fn nsfp_stopped(p: *mut RawPlayer) -> c_int;
    fn nsfp_length(p: *mut RawPlayer) -> c_int;
    fn nsfp_set_mask(p: *mut RawPlayer, mask: u32);
    fn nsfp_fade_time(p: *mut RawPlayer) -> c_int;
    fn nsfp_fade_out(p: *mut RawPlayer, ms: c_int);
    fn nsfp_cancel_fade(p: *mut RawPlayer);
    fn nsfp_config_get(p: *mut RawPlayer, name: *const c_char, value: *mut c_int) -> c_int;
    fn nsfp_config_set(p: *mut RawPlayer, name: *const c_char, value: c_int) -> c_int;
    fn nsfp_notify(p: *mut RawPlayer, device: c_int);
    fn nsfp_dump(p: *mut RawPlayer) -> *const c_char;
    fn nsfp_read_memory(p: *mut RawPlayer, adr: u32, out: *mut u8, len: u32);
    fn nsfp_state_hash(p: *mut RawPlayer, idle: *mut c_int) -> u64;
    fn nsfp_trace_start(p: *mut RawPlayer) -> c_int;
    fn nsfp_trace_time(p: *mut RawPlayer, ms: u32);
    fn nsfp_trace_take(p: *mut RawPlayer, keys: *mut u32, times: *mut u32, cap: u32) -> u32;
    fn nsfp_detected(p: *mut RawPlayer, time: *mut c_int, looped: *mut c_int, fade: *mut c_int);
}

/// `MULT_SPEED` value for normal speed.
pub const SPEED_1X: u32 = 256;

/// Sound chips, in the bit order returned by [`Player::expansions`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Chip {
    Apu = 0,
    Fme7 = 2,
    Mmc5 = 3,
    N163 = 4,
    Vrc6 = 5,
    Vrc7 = 6,
    Fds = 7,
}

impl Chip {
    pub fn name(self) -> &'static str {
        match self {
            Chip::Apu => "2A03",
            Chip::Fme7 => "5B",
            Chip::Mmc5 => "MMC5",
            Chip::N163 => "N163",
            Chip::Vrc6 => "VRC6",
            Chip::Vrc7 => "VRC7",
            Chip::Fds => "FDS",
        }
    }
}

/// A channel that can be muted: `bit` is its position in the mute mask.
#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub bit: u32,
    pub chip: Chip,
    pub name: &'static str,
}

/// Every channel in mask order (NSFPlayerConfig::channel_name). VRC7 channels 6-8
/// are left out because the core shifts their mask bits onto N163's.
pub const CHANNELS: &[Channel] = &{
    use Chip::*;
    const fn c(bit: u32, chip: Chip, name: &'static str) -> Channel {
        Channel { bit, chip, name }
    }
    [
        c(0, Apu, "Square 1"), c(1, Apu, "Square 2"), c(2, Apu, "Triangle"), c(3, Apu, "Noise"), c(4, Apu, "DPCM"),
        c(5, Fds, "Wave"),
        c(6, Mmc5, "Square 1"), c(7, Mmc5, "Square 2"), c(8, Mmc5, "PCM"),
        c(9, Fme7, "A"), c(10, Fme7, "B"), c(11, Fme7, "C"),
        c(12, Vrc6, "Pulse 1"), c(13, Vrc6, "Pulse 2"), c(14, Vrc6, "Saw"),
        c(15, Vrc7, "FM 1"), c(16, Vrc7, "FM 2"), c(17, Vrc7, "FM 3"),
        c(18, Vrc7, "FM 4"), c(19, Vrc7, "FM 5"), c(20, Vrc7, "FM 6"),
        c(21, N163, "Wave 1"), c(22, N163, "Wave 2"), c(23, N163, "Wave 3"), c(24, N163, "Wave 4"),
        c(25, N163, "Wave 5"), c(26, N163, "Wave 6"), c(27, N163, "Wave 7"), c(28, N163, "Wave 8"),
    ]
};

/// How long a track plays: `play_ms`, then a fade-out of `fade_ms`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Length {
    pub play_ms: u32,
    pub fade_ms: u32,
}

/// Result of the core's silence and loop detection for the current track.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Detected {
    Nothing,
    /// The music between the two times repeats forever.
    Loop { start_ms: u32, end_ms: u32 },
    /// The track goes silent; `at_ms` includes about a second of trailing silence.
    Silence { at_ms: u32 },
}

#[derive(Clone, Debug)]
pub struct Track {
    /// Label from the file, or "Track N" when it has none.
    pub title: String,
    /// Length including fade, when the file specifies one.
    pub length_ms: Option<u32>,
}

/// NSFPlay's rate converter keeps a static scratch buffer (xgm/devices/Audio/rconv.cpp),
/// so two players must never render at the same time. Background work must render in short
/// slices so the audio callback never waits long for this lock.
static RENDER_LOCK: Mutex<()> = Mutex::new(());

fn render_lock() -> MutexGuard<'static, ()> {
    RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// One loaded NSF/NSFe file and the emulator playing it.
///
/// `Player` owns the track clock. The core runs endlessly (`PLAY_ADVANCE`), and `Player` starts
/// the fade-out itself when the elapsed song time plus the fade reaches the track length, unless
/// it is in endless mode. The core's own clock (`time_in_ms`) is only used for its loop and
/// silence detection.
pub struct Player {
    raw: *mut RawPlayer,
    rate: u32,
    track: u32,
    mask: u32,
    speed: u32,
    endless: bool,
    /// Set by the caller (e.g. a playlist entry's duration); otherwise the core's length is used.
    length: Option<Length>,
    fading: bool,
    /// Frames handed out since `start`, each weighted by the speed at the time (256 = 1×).
    song_ticks: u64,
    /// Frames per core call. NSFPlayer::Render/Skip advance the core's clock by a truncated
    /// whole number of ms per call, so calls cover whole milliseconds to keep loop and silence
    /// detection accurate.
    step: usize,
    /// Rendered frames not yet handed out, as interleaved stereo; read from `pending_pos`.
    pending: Vec<i16>,
    pending_pos: usize,
}

// The C++ player has no thread affinity; callers serialize access through &mut.
unsafe impl Send for Player {}

impl Player {
    /// Loads an NSF or NSFe image. On failure returns the core's error message.
    pub fn load(data: &[u8]) -> Result<Player, String> {
        let size = u32::try_from(data.len()).map_err(|_| "File too large".to_string())?;
        let player = Player {
            raw: unsafe { nsfp_create() },
            rate: 48000,
            track: 0,
            mask: 0,
            speed: SPEED_1X,
            endless: false,
            length: None,
            fading: false,
            song_ticks: 0,
            step: 48,
            pending: Vec::new(),
            pending_pos: 0,
        };
        if unsafe { nsfp_load(player.raw, data.as_ptr(), size) } == 0 {
            let err = decode(unsafe { nsfp_error(player.raw) });
            return Err(if err.is_empty() { "Not a valid NSF file".into() } else { err });
        }
        Ok(player)
    }

    pub fn title(&self) -> String {
        decode(unsafe { nsfp_title(self.raw) })
    }

    pub fn artist(&self) -> String {
        decode(unsafe { nsfp_artist(self.raw) })
    }

    pub fn copyright(&self) -> String {
        decode(unsafe { nsfp_copyright(self.raw) })
    }

    pub fn ripper(&self) -> String {
        decode(unsafe { nsfp_ripper(self.raw) })
    }

    /// Chips this file uses; the 2A03 APU is always present.
    pub fn chips(&self) -> Vec<Chip> {
        let bits = unsafe { nsfp_expansions(self.raw) };
        let mut chips = vec![Chip::Apu];
        for chip in [Chip::Fds, Chip::Mmc5, Chip::Fme7, Chip::Vrc6, Chip::Vrc7, Chip::N163] {
            if bits & (1 << chip as u32) != 0 {
                chips.push(chip);
            }
        }
        chips
    }

    /// Channels belonging to the chips this file uses.
    pub fn channels(&self) -> Vec<Channel> {
        let chips = self.chips();
        CHANNELS.iter().copied().filter(|c| chips.contains(&c.chip)).collect()
    }

    pub fn tracks(&self) -> Vec<Track> {
        let count = unsafe { nsfp_track_count(self.raw) };
        (0..count)
            .map(|i| {
                let title = decode(unsafe { nsfp_track_title(self.raw, i) });
                let length = unsafe { nsfp_track_length(self.raw, i) };
                Track {
                    title: if title.is_empty() { format!("Track {}", i + 1) } else { title },
                    length_ms: u32::try_from(length).ok(),
                }
            })
            .collect()
    }

    /// Resets the emulator and starts `track` (0-based, playlist order) from the beginning.
    pub fn start(&mut self, track: u32, sample_rate: u32) {
        let _lock = render_lock(); // Reset() renders a few samples to warm up the filters
        unsafe { nsfp_start(self.raw, track as c_int, sample_rate as f64) }
        unsafe { nsfp_set_mask(self.raw, self.mask) }
        self.rate = sample_rate.max(1);
        self.track = track;
        self.step = (self.rate / gcd(self.rate, 1000)) as usize;
        self.pending.clear();
        self.pending_pos = 0;
        self.song_ticks = 0;
        self.fading = false;
    }

    pub fn track(&self) -> u32 {
        self.track
    }

    /// Fills `buf` with interleaved stereo samples.
    pub fn render(&mut self, mut buf: &mut [i16]) {
        let _lock = render_lock();
        while buf.len() >= 2 {
            let frames = if self.pending_pos < self.pending.len() {
                let n = self.take_pending(buf.len());
                buf[..n].copy_from_slice(&self.pending[self.pending_pos - n..self.pending_pos]);
                n / 2
            } else if buf.len() / 2 >= self.step {
                let n = self.piece(buf.len() / 2);
                unsafe { nsfp_render(self.raw, buf.as_mut_ptr(), n as u32) };
                n
            } else {
                self.refill();
                continue;
            };
            buf = &mut buf[frames * 2..];
            self.advance(frames);
        }
    }

    /// Advances emulation by `frames` output frames without producing audio.
    pub fn skip(&mut self, frames: u64) {
        let _lock = render_lock();
        let mut left = frames;
        while left > 0 {
            let n = if self.pending_pos < self.pending.len() {
                self.take_pending((left * 2).min(usize::MAX as u64) as usize) as u64 / 2
            } else if left >= self.step as u64 {
                // NSFPlayer::Skip computes `1000 * length` in UINT32, so keep calls under ~4.29M frames.
                let n = (left.min(4_000_000) as usize / self.step * self.step) as u64;
                unsafe { nsfp_skip(self.raw, n as u32) };
                n
            } else {
                self.refill();
                continue;
            };
            left -= n;
            self.advance(n as usize);
        }
    }

    /// Moves to `ms` of song time, restarting the track when seeking backwards.
    pub fn seek(&mut self, ms: u64) {
        if ms < self.elapsed_ms() {
            self.start(self.track, self.rate);
        }
        let ticks = ms * self.rate as u64 * SPEED_1X as u64 / 1000;
        let frames = ticks.saturating_sub(self.song_ticks).div_ceil(self.speed as u64);
        self.skip(frames);
    }

    /// Song time since the start of the track, which runs faster or slower than real time
    /// when the speed is changed.
    pub fn elapsed_ms(&self) -> u64 {
        self.song_ticks * 1000 / (self.rate as u64 * SPEED_1X as u64)
    }

    /// True once the track has finished fading out.
    pub fn is_stopped(&self) -> bool {
        unsafe { nsfp_stopped(self.raw) != 0 }
    }

    /// Length of the current track including fade. Without an override from
    /// [`Player::set_length`] it comes from the core, and may shrink once silence is detected
    /// or change once a loop is detected.
    pub fn length_ms(&self) -> u32 {
        match self.length {
            Some(l) => l.play_ms.saturating_add(l.fade_ms),
            None => unsafe { nsfp_length(self.raw) }.max(0) as u32,
        }
    }

    /// Overrides how long the track plays before fading out, or restores the core's length.
    /// Kept across [`Player::start`].
    pub fn set_length(&mut self, length: Option<Length>) {
        self.length = length;
        self.check_end();
    }

    /// Reads `len` bytes of the emulated CPU address space starting at `adr` (RAM, WRAM or
    /// banked ROM; addresses wrap at $FFFF). Has no side effects on the hardware.
    pub fn read_memory(&self, adr: u16, len: usize) -> Vec<u8> {
        let mut out = vec![0; len];
        unsafe { nsfp_read_memory(self.raw, adr as u32, out.as_mut_ptr(), len as u32) };
        out
    }

    /// Hash of the emulated machine state (RAM, WRAM, banks) while the CPU idles between play
    /// calls, or `None` while it is running. When it repeats an earlier value, the music loops
    /// exactly from that point.
    pub fn state_hash(&self) -> Option<u64> {
        let mut idle = 0;
        let h = unsafe { nsfp_state_hash(self.raw, &mut idle) };
        (idle != 0).then_some(h)
    }

    /// Starts tracing reads of song data (bytes not read in the last second), for loop
    /// analysis. Returns false if no file is loaded.
    pub(crate) fn trace_start(&mut self) -> bool {
        unsafe { nsfp_trace_start(self.raw) != 0 }
    }

    /// Moves the traced reads since the last call into `out` as (key, song ms), and stamps
    /// later reads with the current song time.
    pub(crate) fn trace_take(&mut self, out: &mut Vec<(u32, u32)>) {
        let mut keys = [0u32; 256];
        let mut times = [0u32; 256];
        loop {
            let n = unsafe { nsfp_trace_take(self.raw, keys.as_mut_ptr(), times.as_mut_ptr(), 256) } as usize;
            out.extend(keys[..n].iter().copied().zip(times[..n].iter().copied()));
            if n < 256 {
                break;
            }
        }
        unsafe { nsfp_trace_time(self.raw, self.elapsed_ms() as u32) }
    }

    /// What the core's silence or loop detection has found for the current track so far.
    pub fn detected(&self) -> Detected {
        let (mut time, mut looped, mut fade) = (0, 0, 0);
        unsafe { nsfp_detected(self.raw, &mut time, &mut looped, &mut fade) };
        if time < 0 {
            Detected::Nothing
        } else if looped > 0 {
            Detected::Loop { start_ms: (time - looped).max(0) as u32, end_ms: time as u32 }
        } else {
            Detected::Silence { at_ms: time as u32 }
        }
    }

    /// In endless mode the track never fades out. Turning it on cancels a fade in progress.
    pub fn set_endless(&mut self, endless: bool) {
        self.endless = endless;
        if endless && self.fading {
            unsafe { nsfp_cancel_fade(self.raw) }
            self.fading = false;
        }
        self.check_end();
    }

    pub fn endless(&self) -> bool {
        self.endless
    }

    /// Playback speed as a `MULT_SPEED` value ([`SPEED_1X`] = normal). Changes tempo, not pitch.
    pub fn set_speed(&mut self, speed: u32) {
        self.speed = speed.clamp(SPEED_1X / 8, SPEED_1X * 8);
        self.config_set("MULT_SPEED", self.speed as i32);
    }

    pub fn speed(&self) -> u32 {
        self.speed
    }

    /// Mutes every channel whose bit is set (see [`CHANNELS`]).
    pub fn set_mute_mask(&mut self, mask: u32) {
        self.mask = mask;
        unsafe { nsfp_set_mask(self.raw, mask) }
    }

    /// Reads an `NSFPlayerConfig` value, or `None` if there is no such setting.
    pub fn config_get(&self, name: &str) -> Option<i32> {
        let name = std::ffi::CString::new(name).ok()?;
        let mut value = 0;
        (unsafe { nsfp_config_get(self.raw, name.as_ptr(), &mut value) } != 0).then_some(value)
    }

    /// Writes an `NSFPlayerConfig` value, returning false if there is no such setting. Device
    /// settings (volume, pan, options) take effect after [`Player::notify`].
    pub fn config_set(&mut self, name: &str, value: i32) -> bool {
        let Ok(name) = std::ffi::CString::new(name) else { return false };
        unsafe { nsfp_config_set(self.raw, name.as_ptr(), value) != 0 }
    }

    /// Applies changed settings of one device (see `NSFPlayerConfig::dname`), or all if `None`.
    pub fn notify(&mut self, device: Option<u32>) {
        unsafe { nsfp_notify(self.raw, device.map_or(-1, |d| d as c_int)) }
    }

    /// Plain-text dump of the emulator state (file header, CPU, banks, memory, sound
    /// registers) for Developer mode.
    pub fn dump(&mut self) -> String {
        let core = unsafe { CStr::from_ptr(nsfp_dump(self.raw)) }.to_string_lossy();
        let ms = self.elapsed_ms();
        format!(
            "TIME\n  {}:{:02}.{:03}   speed {:.2}x{}\n\n{core}",
            ms / 60_000,
            ms / 1000 % 60,
            ms % 1000,
            self.speed as f64 / SPEED_1X as f64,
            if self.endless { "   endless" } else { "" },
        )
    }

    /// Counts `frames` handed out and starts the fade-out when the track's time is up.
    fn advance(&mut self, frames: usize) {
        self.song_ticks += frames as u64 * self.speed as u64;
        self.check_end();
    }

    fn check_end(&mut self) {
        if self.endless || self.fading {
            return;
        }
        let length = self.length_ms() as u64;
        let fade = match self.length {
            Some(l) => l.fade_ms as c_int,
            None => unsafe { nsfp_fade_time(self.raw) }.max(0),
        };
        if length > 0 && self.elapsed_ms() + fade as u64 >= length {
            unsafe { nsfp_fade_out(self.raw, fade) }
            self.fading = true;
        }
    }

    /// Whole steps to render directly, at most about 10 ms so fades start on time.
    fn piece(&self, frames: usize) -> usize {
        let max = (self.rate as usize / 100).div_ceil(self.step) * self.step;
        frames.min(max) / self.step * self.step
    }

    /// Consumes up to `samples` pending samples, returning how many it took.
    fn take_pending(&mut self, samples: usize) -> usize {
        let n = samples.min(self.pending.len() - self.pending_pos);
        self.pending_pos += n;
        n
    }

    /// Renders one step into the empty pending buffer. Callers hold the render lock.
    fn refill(&mut self) {
        self.pending.resize(self.step * 2, 0);
        self.pending_pos = 0;
        unsafe { nsfp_render(self.raw, self.pending.as_mut_ptr(), self.step as u32) };
    }
}

impl Drop for Player {
    fn drop(&mut self) {
        unsafe { nsfp_destroy(self.raw) }
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a } else { gcd(b, a % b) }
}

/// NSF text is usually ASCII or UTF-8, but older Japanese rips use Shift-JIS.
fn decode(s: *const c_char) -> String {
    if s.is_null() {
        return String::new();
    }
    let bytes = unsafe { CStr::from_ptr(s) }.to_bytes();
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim().to_string(),
        Err(_) => encoding_rs::SHIFT_JIS.decode(bytes).0.trim().to_string(),
    }
}
