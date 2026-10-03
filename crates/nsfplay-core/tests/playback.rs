use nsfplay_core::{Chip, Player, SilenceStop};

const ARPEGGIO: &[u8] = include_bytes!("data/arpeggio.nsf");
const INTRO_LOOP: &[u8] = include_bytes!("data/intro_loop.nsf");
const SONG_DATA_LOOP: &[u8] = include_bytes!("data/song_data_loop.nsf");
const RATE: u32 = 48000;

fn peak(player: &mut Player, frames: usize) -> i16 {
    let mut buf = vec![0i16; frames * 2];
    player.render(&mut buf);
    buf.iter().map(|s| s.saturating_abs()).max().unwrap()
}

#[test]
fn reads_metadata() {
    let player = Player::load(ARPEGGIO).unwrap();
    assert_eq!(player.title(), "Test Arpeggio");
    assert_eq!(player.artist(), "nsfplay tests");
    assert_eq!(player.copyright(), "public domain");
    assert_eq!(player.chips(), vec![Chip::Apu]);
    assert_eq!(player.channels().len(), 5);
    let tracks = player.tracks();
    assert_eq!(tracks.len(), 2);
    assert_eq!(tracks[0].title, "Track 1");
    assert_eq!(tracks[0].length_ms, None);
}

#[test]
fn rejects_garbage() {
    assert!(Player::load(b"not an nsf file at all").is_err());
}

#[test]
fn renders_audio_and_mutes() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    assert_eq!(player.length_ms(), 5 * 60 * 1000 + 5000, "default play time plus fade");
    assert!(peak(&mut player, RATE as usize / 2) > 1000);
    assert!(!player.is_stopped());

    player.set_mute_mask(1); // square 1, the only channel the tune uses
    peak(&mut player, RATE as usize / 10); // let the DC filter settle
    assert!(peak(&mut player, RATE as usize / 10) < 200);
}

#[test]
fn silent_track_stops_early() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(1, RATE);
    let mut buf = vec![0i16; 4096 * 2];
    let mut seconds = 0.0;
    while !player.is_stopped() && seconds < 30.0 {
        player.render(&mut buf);
        seconds += 4096.0 / RATE as f64;
    }
    assert!(player.is_stopped(), "silence detection should end the track");
    // STOP_SEC defaults to 3 seconds of silence
    assert!((2.5..10.0).contains(&seconds), "stopped after {seconds}s");
}

#[test]
fn reads_shift_jis_header() {
    // Upstream converts these in place with iconv, which aborts under glibc; the shim avoids it.
    let mut image = ARPEGGIO.to_vec();
    let (sjis, _, _) = encoding_rs::SHIFT_JIS.encode("ドラゴンクエスト");
    image[0x0e..0x2e].fill(0);
    image[0x0e..0x0e + sjis.len()].copy_from_slice(&sjis);
    let player = Player::load(&image).unwrap();
    assert_eq!(player.title(), "ドラゴンクエスト");
    assert_eq!(player.artist(), "nsfplay tests");
}

#[test]
fn stops_on_time_with_small_buffers() {
    // The core's track clock truncates to whole ms per call; Player must hide that.
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, 44100);
    let mut buf = vec![0i16; 64 * 2];
    let mut frames = 0u64;
    while !player.is_stopped() && frames < 44100 * 400 {
        player.render(&mut buf);
        frames += 64;
    }
    let secs = frames as f64 / 44100.0;
    assert!((secs - 305.0).abs() < 0.1, "stopped after {secs:.2} s instead of 305 s");
}

#[test]
fn skip_keeps_time() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, 44100);
    for _ in 0..1000 {
        player.skip(4410 + 7); // odd sizes that never land on a whole ms
    }
    let mut buf = vec![0i16; 4410 * 2];
    let mut frames = 1000 * 4417u64;
    while !player.is_stopped() {
        player.render(&mut buf);
        frames += 4410;
    }
    let secs = frames as f64 / 44100.0;
    assert!((secs - 305.0).abs() < 0.2, "stopped after {secs:.2} s instead of 305 s");
}

#[test]
fn long_seek_keeps_time() {
    // One skip past ~89 s overflows the core's clock arithmetic; Player must split it.
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, 48000);
    player.skip(300 * 48000);
    let mut buf = vec![0i16; 512 * 2];
    let mut frames = 300 * 48000u64;
    while !player.is_stopped() && frames < 48000 * 400 {
        player.render(&mut buf);
        frames += 512;
    }
    let secs = frames as f64 / 48000.0;
    assert!((secs - 305.0).abs() < 0.1, "stopped after {secs:.2} s instead of 305 s");
}

/// Renders in `chunk`-frame buffers until the track stops; returns wall-clock seconds.
fn play_out(player: &mut Player, rate: u32, chunk: usize, limit_secs: u64) -> f64 {
    let mut buf = vec![0i16; chunk * 2];
    let mut frames = 0u64;
    while !player.is_stopped() && frames < rate as u64 * limit_secs {
        player.render(&mut buf);
        frames += chunk as u64;
    }
    frames as f64 / rate as f64
}

#[test]
fn endless_never_fades() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    player.set_endless(true);
    player.seek(304_000);
    assert_eq!(play_out(&mut player, RATE, 512, 20), 20.0, "stopped in endless mode");
    assert!(peak(&mut player, 4800) > 1000, "endless playback faded");
    assert!(player.elapsed_ms() >= 324_000);

    player.set_endless(false); // past the end: fades out right away
    assert!(play_out(&mut player, RATE, 512, 20) <= 5.1);
}

#[test]
fn endless_cancels_fade() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    player.seek(302_000); // in the middle of the fade
    peak(&mut player, 4800);
    player.set_endless(true);
    peak(&mut player, RATE as usize); // fade gain is restored at once, let the DC filter settle
    assert!(peak(&mut player, 4800) > 2000);
}

#[test]
fn endless_stops_at_silence() {
    // track 2 is silent: endless playback ignores the core's detector unless asked to stop on it
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(1, RATE);
    player.set_endless(true);
    assert!(play_out(&mut player, RATE, 512, 10) >= 10.0, "stopped without a silence stop");

    player.start(1, RATE); // the silence stop is kept across starts
    player.set_silence_stop(Some(SilenceStop::Detect));
    let secs = play_out(&mut player, RATE, 512, 10);
    assert!((3.0..3.5).contains(&secs), "live detection stopped at {secs} s");

    // a stop time from analysis ends even a track that keeps playing
    player.start(0, RATE);
    player.set_silence_stop(Some(SilenceStop::At(1500)));
    let secs = play_out(&mut player, RATE, 512, 10);
    assert!((1.5..1.53).contains(&secs), "stopped at {secs} s, not 1.5 s");
    assert!(player.elapsed_ms() >= 1500);
}

#[test]
fn speed_changes_song_time() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    player.set_speed(nsfplay_core::SPEED_1X * 2);
    assert_eq!(player.config_get("MULT_SPEED"), Some(512));
    player.seek(200_000);
    assert_eq!(player.elapsed_ms(), 200_000);
    // 100 s of song left at 2x is 50 s, then a 5 s fade in real time
    let secs = play_out(&mut player, RATE, 512, 120);
    assert!((secs - 55.0).abs() < 0.1, "stopped after {secs:.2} s instead of 55 s");
}

#[test]
fn config_access() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    assert_eq!(player.config_get("APU1_VOLUME"), Some(128));
    assert!(player.config_set("APU1_VOLUME", 0));
    player.notify(Some(0));
    assert_eq!(player.config_get("NO_SUCH_SETTING"), None);
    assert!(!player.config_set("NO_SUCH_SETTING", 1));

    player.start(0, RATE);
    peak(&mut player, RATE as usize / 10);
    assert!(peak(&mut player, RATE as usize / 10) < 200, "APU1 at volume 0 is still audible");
}

#[test]
fn dumps_state() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    peak(&mut player, RATE as usize);
    let dump = player.dump();
    for section in ["TIME", "FILE", "CPU", "BANKS", "RAM", "WRAM", "2A03"] {
        assert!(dump.lines().any(|l| l.split(" (").next() == Some(section)), "missing {section} in:\n{dump}");
    }
    assert!(dump.contains("PC="));
    assert!(dump.contains("  $4000:"));
}

#[test]
fn detected_length_does_not_carry_over() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(1, RATE); // silent track: auto-stop shortens its length
    play_out(&mut player, RATE, 4800, 20);
    assert!(player.length_ms() < 10_000);
    player.start(0, RATE);
    assert_eq!(player.length_ms(), 305_000);
}

#[test]
fn analysis_finds_loop_and_silence() {
    use nsfplay_core::{analysis::analyze, Detected};
    // Track 1 repeats a short arpeggio from the start.
    match analyze(ARPEGGIO, 0, 120_000).unwrap() {
        Detected::Loop { start_ms, end_ms } => assert!(start_ms < 100 && end_ms - start_ms < 5000, "{start_ms}..{end_ms}"),
        other => panic!("expected a loop, got {other:?}"),
    }
    assert!(matches!(analyze(ARPEGGIO, 1, 120_000).unwrap(), Detected::Silence { at_ms } if at_ms < 5000));
    assert!(analyze(b"garbage", 0, 1000).is_err());
}

#[test]
fn analysis_finds_long_loop_after_intro() {
    use nsfplay_core::{analysis::analyze, Detected};
    // 3 s intro, then a 40 s loop, with far too many register writes for the core's detector
    // (see tests/data/make_intro_loop.py). Its first play call runs at 0 ms, so frame n plays
    // at (n - 1) * 16.639 ms.
    let frame = 16.639;
    match analyze(INTRO_LOOP, 0, 300_000).unwrap() {
        Detected::Loop { start_ms, end_ms } => {
            assert!((start_ms as f64 - 179.0 * frame).abs() < 3.0, "loop starts at {start_ms} ms");
            assert!((((end_ms - start_ms) as f64) - 2400.0 * frame).abs() < 3.0, "loop ends at {end_ms} ms");
        }
        other => panic!("expected a loop, got {other:?}"),
    }
}

#[test]
fn analysis_finds_loop_from_song_data_reads() {
    use nsfplay_core::{analysis::analyze, Detected};
    // Same intro and loop, but a random number generator keeps the RAM from ever repeating, as
    // in Capcom's engine; only the order of song-data reads gives the loop away. Frame n reads
    // song byte n, and the loop jumps back to byte 180.
    let frame = 16.639;
    match analyze(SONG_DATA_LOOP, 0, 300_000).unwrap() {
        Detected::Loop { start_ms, end_ms } => {
            assert!((start_ms as f64 - 180.0 * frame).abs() < 20.0, "loop starts at {start_ms} ms");
            assert!((((end_ms - start_ms) as f64) - 2400.0 * frame).abs() < 20.0, "loop ends at {end_ms} ms");
        }
        other => panic!("expected a loop, got {other:?}"),
    }
}

#[test]
fn length_override() {
    use nsfplay_core::Length;
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.set_length(Some(Length { play_ms: 10_000, fade_ms: 2_000 }));
    player.start(0, RATE);
    assert_eq!(player.length_ms(), 12_000);
    let secs = play_out(&mut player, RATE, 512, 60);
    assert!((secs - 12.0).abs() < 0.05, "stopped after {secs:.2} s instead of 12 s");

    player.set_length(None);
    player.start(0, RATE);
    assert_eq!(player.length_ms(), 305_000);
}

#[test]
fn channel_state_follows_the_music() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    peak(&mut player, RATE as usize / 2);
    let sq1 = player.channel(0).expect("square 1 info");
    assert!(sq1.key != 0 && sq1.volume > 0 && sq1.volume <= sq1.max_volume, "{sq1:?}");
    assert!(sq1.freq_hz > 50.0 && sq1.freq_hz < 5000.0, "{sq1:?}");
    let tri = player.channel(2).expect("triangle info");
    assert_eq!(tri.volume, 0, "the tune only uses square 1: {tri:?}");
}

/// Peak of the left and right channels separately.
fn peaks(player: &mut Player, frames: usize) -> (i16, i16) {
    let mut buf = vec![0i16; frames * 2];
    player.render(&mut buf);
    let side = |o: usize| buf.iter().skip(o).step_by(2).map(|s| s.saturating_abs()).max().unwrap();
    (side(0), side(1))
}

#[test]
fn mixer_volume_and_pan() {
    let mut player = Player::load(ARPEGGIO).unwrap();
    player.start(0, RATE);
    peaks(&mut player, RATE as usize / 4);
    let (l, r) = peaks(&mut player, RATE as usize / 4);
    assert!(l > 1000 && (l - r).abs() < l / 10, "centred: {l} {r}");

    player.config_set("CHANNEL_00_PAN", 0); // hard left
    player.notify(Some(nsfplay_core::CHANNEL_DEVICE[0]));
    peaks(&mut player, RATE as usize / 4); // let the DC filter settle
    let (l, r) = peaks(&mut player, RATE as usize / 4);
    assert!(l > 1000 && r < l / 10, "panned left: {l} {r}");

    player.config_set("CHANNEL_00_PAN", 128);
    player.config_set("CHANNEL_00_VOL", 32); // a quarter
    player.notify(None);
    peaks(&mut player, RATE as usize / 4);
    let (quiet, _) = peaks(&mut player, RATE as usize / 4);
    assert!(quiet < l / 2, "quieter: {quiet} vs {l}");

    player.config_set("CHANNEL_00_VOL", 128);
    player.config_set("APU1_VOLUME", 0); // the device holding both squares
    player.notify(Some(0));
    peaks(&mut player, RATE as usize / 4);
    let (l, r) = peaks(&mut player, RATE as usize / 4);
    assert!(l < 200 && r < 200, "device muted: {l} {r}");
}
