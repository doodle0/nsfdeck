use nsfplay_core::{Chip, Player};

const ARPEGGIO: &[u8] = include_bytes!("data/arpeggio.nsf");
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
