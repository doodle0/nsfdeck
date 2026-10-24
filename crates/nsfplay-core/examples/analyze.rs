//! Runs the background analysis on every track of a file (or one), for checking loop detection.
//! See docs/loop-detection.md.
//!
//! `analyze FILE [TRACK]` prints one line per track (tracks are 1-based); diff the output of
//! two builds to see what a change moved.
//!
//! `analyze FILE TRACK --solo BIT --raw OUT [--seconds S]` also writes the track's audio with only
//! channel BIT unmuted (see `CHANNELS`), as raw mono 16-bit 48 kHz PCM, for
//! `examples/loop_truth.py`. It renders S seconds, by default enough for the found loop to play
//! three times.
use nsfplay_core::{analysis::analyze, Detected, Player};

const LIMIT_MS: u32 = 600_000;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| args.iter().position(|a| a == name).map(|i| args[i + 1].as_str());
    let Some(path) = args.get(1) else {
        eprintln!("usage: analyze FILE [TRACK] [--solo BIT --raw OUT [--seconds S]]");
        std::process::exit(2);
    };
    let data = std::fs::read(path).unwrap();
    let count = Player::load(&data).unwrap().tracks().len() as u32;
    let only: Option<u32> = args.get(2).filter(|a| !a.starts_with("--")).map(|a| a.parse().unwrap());

    for track in (1..=count).filter(|t| only.is_none_or(|o| o == *t)) {
        let started = std::time::Instant::now();
        let found = analyze(&data, track - 1, LIMIT_MS).unwrap();
        let took = started.elapsed().as_secs_f64();
        match found {
            Detected::Loop { start_ms, end_ms } => println!(
                "#{track:<3} loop  start {start_ms:>6} ms  end {end_ms:>6} ms  length {:>6} ms  ({took:.1} s)",
                end_ms - start_ms
            ),
            Detected::Silence { at_ms } => println!("#{track:<3} silence at {at_ms} ms  ({took:.1} s)"),
            Detected::Nothing => println!("#{track:<3} nothing within {LIMIT_MS} ms  ({took:.1} s)"),
        }

        if let (Some(bit), Some(out)) = (flag("--solo"), flag("--raw")) {
            let seconds: u32 = flag("--seconds").map_or_else(
                || match found {
                    Detected::Loop { start_ms, end_ms } => (start_ms + 3 * (end_ms - start_ms)) / 1000 + 3,
                    _ => 60,
                },
                |s| s.parse().unwrap(),
            );
            let mut player = Player::load(&data).unwrap();
            player.set_endless(true);
            player.start(track - 1, 48_000);
            player.set_mute_mask(!(1u32 << bit.parse::<u32>().unwrap()));
            let mut buf = vec![0i16; 2 * 4800];
            let mut pcm = Vec::with_capacity(seconds as usize * 96_000);
            for _ in 0..seconds * 10 {
                player.render(&mut buf);
                pcm.extend(buf.chunks(2).flat_map(|f| f[0].to_le_bytes()));
            }
            std::fs::write(out, pcm).unwrap();
        }
    }
}
