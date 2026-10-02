//! Plays a track on the default audio device without the GUI.
//! Usage: cargo run --example play --features output -- FILE.nsf [TRACK] [SECONDS]

use std::time::Duration;

use nsfplay_core::output::Output;
use nsfplay_core::Player;

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let path = args.next().ok_or("usage: play FILE.nsf [TRACK] [SECONDS]")?;
    let track: u32 = args.next().map_or(Ok(1), |s| s.parse()).map_err(|_| "bad track")?;
    let seconds: u64 = args.next().map_or(Ok(10), |s| s.parse()).map_err(|_| "bad seconds")?;

    let data = std::fs::read(&path).map_err(|e| format!("{path}: {e}"))?;
    let player = Player::load(&data)?;
    println!("{} - {} ({} tracks)", player.title(), player.artist(), player.tracks().len());

    let output = Output::open()?;
    output.load(player);
    output.play(track.saturating_sub(1), None);
    for _ in 0..seconds * 4 {
        std::thread::sleep(Duration::from_millis(250));
        let pos = output.position();
        println!("{:?} {} / {} ms", pos.status, pos.elapsed_ms, pos.length_ms);
        if output.take_ended() {
            break;
        }
    }
    Ok(())
}
