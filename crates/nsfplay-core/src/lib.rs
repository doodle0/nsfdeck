//! Safe wrapper around the NSFPlay xgm emulation core.

use std::ffi::{c_char, c_int, CStr};
use std::sync::{Mutex, MutexGuard};

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
}

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

#[derive(Clone, Debug)]
pub struct Track {
    /// Label from the file, or "Track N" when it has none.
    pub title: String,
    /// Length including fade, when the file specifies one.
    pub length_ms: Option<u32>,
}

/// NSFPlay's rate converter keeps a static scratch buffer (xgm/devices/Audio/rconv.cpp),
/// so two players must never render at the same time.
static RENDER_LOCK: Mutex<()> = Mutex::new(());

fn render_lock() -> MutexGuard<'static, ()> {
    RENDER_LOCK.lock().unwrap_or_else(|e| e.into_inner())
}

/// One loaded NSF/NSFe file and the emulator playing it.
pub struct Player {
    raw: *mut RawPlayer,
    /// Frames per core call. NSFPlayer::Render/Skip advance the track clock by a truncated
    /// whole number of ms per call, so calls must cover whole milliseconds or the clock
    /// falls behind and tracks play past their length (by 33% with 64-frame buffers).
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
        let player = Player { raw: unsafe { nsfp_create() }, step: 1, pending: Vec::new(), pending_pos: 0 };
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

    /// Resets the emulator and starts `track` (0-based, playlist order).
    pub fn start(&mut self, track: u32, sample_rate: u32) {
        let _lock = render_lock(); // Reset() renders a few samples to warm up the filters
        unsafe { nsfp_start(self.raw, track as c_int, sample_rate as f64) }
        self.step = (sample_rate / gcd(sample_rate, 1000)).max(1) as usize;
        self.pending.clear();
        self.pending_pos = 0;
    }

    /// Fills `buf` with interleaved stereo samples.
    pub fn render(&mut self, mut buf: &mut [i16]) {
        let _lock = render_lock();
        let n = self.take_pending(buf.len());
        buf[..n].copy_from_slice(&self.pending[self.pending_pos - n..self.pending_pos]);
        buf = &mut buf[n..];

        let whole = buf.len() / 2 / self.step * self.step;
        if whole > 0 {
            unsafe { nsfp_render(self.raw, buf.as_mut_ptr(), whole as u32) };
            buf = &mut buf[whole * 2..];
        }
        if !buf.is_empty() {
            self.refill();
            let n = self.take_pending(buf.len());
            buf.copy_from_slice(&self.pending[..n]);
        }
    }

    /// Advances emulation without producing audio, for seeking.
    pub fn skip(&mut self, frames: u32) {
        let _lock = render_lock();
        let mut samples = frames as usize * 2;
        samples -= self.take_pending(samples);

        // NSFPlayer::Skip computes `1000 * length` in UINT32, which overflows past ~4.29M frames.
        let max = 4_000_000 / self.step * self.step;
        let mut whole = samples / 2 / self.step * self.step;
        samples -= whole * 2;
        while whole > 0 {
            let n = whole.min(max);
            unsafe { nsfp_skip(self.raw, n as u32) };
            whole -= n;
        }
        if samples > 0 {
            self.refill();
            self.take_pending(samples);
        }
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

    /// True once the track has finished fading out.
    pub fn is_stopped(&self) -> bool {
        unsafe { nsfp_stopped(self.raw) != 0 }
    }

    /// Length of the current track including fade. May shrink once silence is detected.
    pub fn length_ms(&self) -> u32 {
        unsafe { nsfp_length(self.raw) }.max(0) as u32
    }

    /// Mutes every channel whose bit is set (see [`CHANNELS`]).
    pub fn set_mute_mask(&mut self, mask: u32) {
        unsafe { nsfp_set_mask(self.raw, mask) }
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
