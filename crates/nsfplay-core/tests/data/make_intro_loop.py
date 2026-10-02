"""Generates intro_loop.nsf: a 3 s intro, then a 40 s loop that repeats forever.

Every frame the play routine advances a 16-bit frame counter in RAM, wrapping from END back to
INTRO, and writes the counter's low byte to the square 1 pitch register 60 times. At about
3600 writes per second, NSFPlay's loop detector (a 65536-write ring buffer that compares the
last 30 s) can never see the loop, so the test shows that analysis finds it anyway.

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

code, labels = assemble(0x8000, [
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


def text(s: str) -> bytes:
    return s.encode().ljust(32, b"\0")


header = (
    b"NESM\x1a"
    + bytes([1, 1, 1])  # version, songs, starting song
    + (0x8000).to_bytes(2, "little")  # load
    + labels["init"].to_bytes(2, "little")
    + labels["play"].to_bytes(2, "little")
    + text("Intro and Loop")
    + text("nsfplay tests")
    + text("public domain")
    + (16639).to_bytes(2, "little")  # NTSC play period, us
    + bytes(8)  # no bank switching
    + (19997).to_bytes(2, "little")  # PAL play period, us
    + bytes([0, 0])  # NTSC, no expansion chips
    + bytes(4)
)
assert len(header) == 0x80

out = Path(__file__).with_name("intro_loop.nsf")
out.write_bytes(header + code)
print(f"wrote {out} ({len(header) + len(code)} bytes)")
