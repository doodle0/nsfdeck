"""Generates two test NSFs, each with a 3 s intro and then a 40 s loop that repeats forever.

intro_loop.nsf: every frame the play routine advances a 16-bit frame counter in RAM, wrapping
from END back to INTRO, and writes the counter's low byte to the square 1 pitch register 60
times. At about 3600 writes per second, NSFPlay's loop detector (a 65536-write ring buffer
that compares the last 30 s) can never see the loop. The RAM state repeats exactly.

song_data_loop.nsf: like a real engine, the play routine reads one byte of song data per frame
from a table in ROM, jumping back to the loop point at the end, and also advances an 8-bit
random number generator (period 255 frames) that never lines up with the loop, so the RAM
state never repeats. Only the order of song-data reads reveals the loop.

    uv run crates/nsfplay-core/tests/data/make_intro_loop.py
"""

from pathlib import Path

INTRO = 180  # frames (~3.0 s at 60.1 Hz)
LOOP = 2400  # frames (~39.9 s)
END = INTRO + LOOP
WRITES_PER_FRAME = 60


def assemble(base: int, program: list) -> tuple[bytes, dict[str, int]]:
    """Two-pass assembler for a list of byte values, ("label",) definitions and ("rel", label)
    branch operands."""
    labels, pc = {}, base
    for item in program:
        if isinstance(item, tuple) and item[0] != "rel":
            labels[item[0]] = pc
        else:
            pc += 1
    out, pc = bytearray(), base
    for item in program:
        if isinstance(item, tuple) and item[0] != "rel":
            continue
        if isinstance(item, tuple):
            offset = labels[item[1]] - (pc + 1)
            assert -128 <= offset < 128, item
            out.append(offset & 0xFF)
        else:
            out.append(item)
        pc += 1
    return bytes(out), labels


LDA_IMM, LDA_ZP, STA_ZP, STA_ABS = 0xA9, 0xA5, 0x85, 0x8D
INC_ZP, CMP_IMM, BNE, LDX_IMM, DEX, RTS = 0xE6, 0xC9, 0xD0, 0xA2, 0xCA, 0x60
LDY_IMM, LDA_IND_Y, ASL_A, BCC, EOR_IMM, AND_IMM = 0xA0, 0xB1, 0x0A, 0x90, 0x49, 0x29

counter_code, counter_labels = assemble(0x8000, [
    ("init",),
    LDA_IMM, 0x00, STA_ZP, 0x00, STA_ZP, 0x01,
    LDA_IMM, 0x01, STA_ABS, 0x15, 0x40,  # enable square 1
    LDA_IMM, 0xBF, STA_ABS, 0x00, 0x40,  # duty 50%, constant volume 15
    LDA_IMM, 0x01, STA_ABS, 0x03, 0x40,  # pitch high bits, start the note
    RTS,
    ("play",),
    INC_ZP, 0x00, BNE, ("rel", "counted"), INC_ZP, 0x01,
    ("counted",),
    LDA_ZP, 0x01, CMP_IMM, END >> 8, BNE, ("rel", "write"),
    LDA_ZP, 0x00, CMP_IMM, END & 0xFF, BNE, ("rel", "write"),
    LDA_IMM, INTRO & 0xFF, STA_ZP, 0x00, LDA_IMM, INTRO >> 8, STA_ZP, 0x01,
    ("write",),
    LDA_ZP, 0x00, LDX_IMM, WRITES_PER_FRAME,
    ("again",),
    STA_ABS, 0x02, 0x40, DEX, BNE, ("rel", "again"),
    RTS,
])

# song data follows the code; $00/$01 point at the next byte, $02 is the random generator
TABLE = 0x8060


def song_data_program():
    loop_at, end_at = TABLE + INTRO, TABLE + END
    return [
        ("init",),
        LDA_IMM, TABLE & 0xFF, STA_ZP, 0x00, LDA_IMM, TABLE >> 8, STA_ZP, 0x01,
        LDA_IMM, 0x01, STA_ZP, 0x02,
        LDA_IMM, 0x09, STA_ABS, 0x15, 0x40,  # enable square 1 and noise
        LDA_IMM, 0xBF, STA_ABS, 0x00, 0x40,
        LDA_IMM, 0x01, STA_ABS, 0x03, 0x40,
        LDA_IMM, 0x34, STA_ABS, 0x0C, 0x40,  # noise at volume 4
        RTS,
        ("play",),
        # 8-bit Galois LFSR (x^8 + x^4 + x^3 + x^2 + 1), drives the noise period
        LDA_ZP, 0x02, ASL_A, BCC, ("rel", "no_xor"), EOR_IMM, 0x1D,
        ("no_xor",),
        STA_ZP, 0x02, AND_IMM, 0x0F, STA_ABS, 0x0E, 0x40,
        # play the next byte of song data
        LDY_IMM, 0x00, LDA_IND_Y, 0x00, STA_ABS, 0x02, 0x40,
        INC_ZP, 0x00, BNE, ("rel", "advanced"), INC_ZP, 0x01,
        ("advanced",),
        LDA_ZP, 0x01, CMP_IMM, end_at >> 8, BNE, ("rel", "done"),
        LDA_ZP, 0x00, CMP_IMM, end_at & 0xFF, BNE, ("rel", "done"),
        LDA_IMM, loop_at & 0xFF, STA_ZP, 0x00, LDA_IMM, loop_at >> 8, STA_ZP, 0x01,
        ("done",),
        RTS,
    ]


song_code, song_labels = assemble(0x8000, song_data_program())
assert len(song_code) <= TABLE - 0x8000, len(song_code)
song_code = song_code.ljust(TABLE - 0x8000, b"\0") + bytes((i * 7 + 3) & 0xFF for i in range(END))


def text(s: str) -> bytes:
    return s.encode().ljust(32, b"\0")


def nsf(title: str, labels: dict[str, int], code: bytes) -> bytes:
    header = (
        b"NESM\x1a"
        + bytes([1, 1, 1])  # version, songs, starting song
        + (0x8000).to_bytes(2, "little")  # load
        + labels["init"].to_bytes(2, "little")
        + labels["play"].to_bytes(2, "little")
        + text(title)
        + text("nsfplay tests")
        + text("public domain")
        + (16639).to_bytes(2, "little")  # NTSC play period, us
        + bytes(8)  # no bank switching
        + (19997).to_bytes(2, "little")  # PAL play period, us
        + bytes([0, 0])  # NTSC, no expansion chips
        + bytes(4)
    )
    assert len(header) == 0x80
    return header + code


for name, data in [
    ("intro_loop.nsf", nsf("Intro and Loop", counter_labels, counter_code)),
    ("song_data_loop.nsf", nsf("Song Data Loop", song_labels, song_code)),
]:
    out = Path(__file__).with_name(name)
    out.write_bytes(data)
    print(f"wrote {out} ({len(data)} bytes)")
