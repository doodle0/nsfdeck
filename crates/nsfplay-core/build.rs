// Compiles the xgm/vcm emulation core from the NSFPlay submodule (same sources as its
// contrib/Makefile) plus our C shim.

use std::path::PathBuf;

const CPP_SRCS: &[&str] = &[
    "xgm/devices/Audio/MedianFilter.cpp",
    "xgm/devices/Audio/echo.cpp",
    "xgm/devices/Audio/filter.cpp",
    "xgm/devices/Audio/rconv.cpp",
    "xgm/devices/CPU/nes_cpu.cpp",
    "xgm/devices/Memory/nes_bank.cpp",
    "xgm/devices/Memory/nes_mem.cpp",
    "xgm/devices/Memory/nsf2_vectors.cpp",
    "xgm/devices/Memory/ram64k.cpp",
    "xgm/devices/Misc/detect.cpp",
    "xgm/devices/Misc/log_cpu.cpp",
    "xgm/devices/Misc/nes_detect.cpp",
    "xgm/devices/Misc/nsf2_irq.cpp",
    "xgm/devices/Sound/nes_apu.cpp",
    "xgm/devices/Sound/nes_dmc.cpp",
    "xgm/devices/Sound/nes_fds.cpp",
    "xgm/devices/Sound/nes_fme7.cpp",
    "xgm/devices/Sound/nes_mmc5.cpp",
    "xgm/devices/Sound/nes_n106.cpp",
    "xgm/devices/Sound/nes_vrc6.cpp",
    "xgm/devices/Sound/nes_vrc7.cpp",
    "xgm/player/nsf/nsf.cpp",
    "xgm/player/nsf/nsfconfig.cpp",
    "xgm/player/nsf/nsfplay.cpp",
    "xgm/player/nsf/pls/ppls.cpp",
    "xgm/player/nsf/pls/sstream.cpp",
    "xgm/fileutil.cpp",
    "vcm/group.cpp",
    "vcm/value.cpp",
];

const C_SRCS: &[&str] = &[
    "xgm/devices/Sound/legacy/emu2149.c",
    "xgm/devices/Sound/legacy/emu2212.c",
    "xgm/devices/Sound/legacy/emu2413.c",
];

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let root = manifest.join("../../vendor/nsfplay");
    if !root.join("xgm/xgm.h").exists() {
        panic!("NSFPlay sources not found in vendor/nsfplay. Run: git submodule update --init");
    }

    cc::Build::new()
        .cpp(true)
        .std("c++17")
        .define("NDEBUG", None) // otherwise the core printf()s debug info to stdout
        .include(&root)
        .warnings(false)
        .files(CPP_SRCS.iter().map(|s| root.join(s)))
        .file(manifest.join("shim/nsfplay_shim.cpp"))
        .compile("nsfplay_xgm");

    cc::Build::new()
        .define("NDEBUG", None)
        .warnings(false)
        .files(C_SRCS.iter().map(|s| root.join(s)))
        .compile("nsfplay_emu");

    // xgm/fileutil.cpp uses iconv, which is in libc on Linux but a separate library on macOS
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=iconv");
    }

    for dir in ["xgm", "vcm"] {
        println!("cargo:rerun-if-changed={}", root.join(dir).display());
    }
    println!("cargo:rerun-if-changed=shim");
}
