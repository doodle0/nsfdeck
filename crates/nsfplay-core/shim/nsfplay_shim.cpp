/* C interface to the NSFPlay xgm player, wrapped by src/lib.rs. */

#include <stdarg.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include <string>
#include <vector>

#include "xgm/xgm.h"

struct nsfp
{
    xgm::NSF nsf;
    xgm::NSFPlayerConfig config;
    xgm::NSFPlayer player;
    std::vector<uint8_t> image; // NSF::Load may keep pointers into the image
    char raw_text[3][33] = {};   // non-ASCII NSF header strings, hidden from NSF::Load
    std::string dump;            // last nsfp_dump result
    bool loaded = false;
};

// NSF::Load runs Shift-JIS header strings through iconv in place (sjis_legacy in nsf.cpp),
// which overlaps input and output: glibc aborts the process, other iconvs garble the text.
// Blank any non-ASCII field before loading and hand the raw bytes to Rust instead, whose
// decode() handles UTF-8 and Shift-JIS.
static void hide_legacy_text(nsfp *p)
{
    memset(p->raw_text, 0, sizeof(p->raw_text));
    if (p->image.size() < 0x80 || memcmp(p->image.data(), "NESM\x1a", 5) != 0) return;
    for (int f = 0; f < 3; ++f)
    {
        uint8_t *field = p->image.data() + 0x0e + 0x20 * f;
        bool ascii = true;
        for (int i = 0; i < 32 && field[i]; ++i) ascii &= field[i] < 0x80;
        if (ascii) continue;
        memcpy(p->raw_text[f], field, 32);
        memset(field, 0, 32);
    }
}

static const char *text_or_raw(const char *s, const char *raw)
{
    return s && s[0] ? s : raw[0] ? raw : s ? s : "";
}

// Read access to protected core state without modifying vendor/nsfplay. A pointer to a
// protected member may be formed through a derived class, and the resulting `T Base::*`
// can then be applied to any Base object. The pointer must be named through the derived
// class ([class.protected]). They are functions because MSVC rejects the pointer in a static
// data member initializer, where the class is still incomplete. The classes are never
// instantiated.
struct CpuPeek : xgm::NES_CPU { static constexpr auto p_context() { return &CpuPeek::context; } };
struct BankPeek : xgm::NES_BANK { static constexpr auto p_bankswitch() { return &BankPeek::bankswitch; } };
struct ApuPeek : xgm::NES_APU { static constexpr auto p_reg() { return &ApuPeek::reg; } };
struct DmcPeek : xgm::NES_DMC { static constexpr auto p_reg() { return &DmcPeek::reg; } };
struct Mmc5Peek : xgm::NES_MMC5 { static constexpr auto p_reg() { return &Mmc5Peek::reg; } };
struct N163Peek : xgm::NES_N106 { static constexpr auto p_reg() { return &N163Peek::reg; } };
struct Fme7Peek : xgm::NES_FME7 { static constexpr auto p_psg() { return &Fme7Peek::psg; } };
struct Vrc7Peek : xgm::NES_VRC7 { static constexpr auto p_opll() { return &Vrc7Peek::opll; } };
struct Vrc6Peek : xgm::NES_VRC6
{
    static constexpr auto p_freq() { return &Vrc6Peek::freq; }
    static constexpr auto p_volume() { return &Vrc6Peek::volume; }
    static constexpr auto p_duty() { return &Vrc6Peek::duty; }
    static constexpr auto p_enable() { return &Vrc6Peek::enable; }
};
struct FdsPeek : xgm::NES_FDS
{
    static constexpr auto p_freq() { return &FdsPeek::freq; }
    static constexpr auto p_master_vol() { return &FdsPeek::master_vol; }
    static constexpr auto p_wave() { return &FdsPeek::wave; }
};

static void appendf(std::string &out, const char *fmt, ...)
{
    char line[256];
    va_list args;
    va_start(args, fmt);
    vsnprintf(line, sizeof(line), fmt, args);
    va_end(args);
    out += line;
}

// hexdump-style: 16 bytes per row, runs of identical rows collapsed to "*".
template <typename Get>
static void hex_rows(std::string &out, uint32_t base, uint32_t size, Get get)
{
    uint8_t prev[16];
    bool have_prev = false, starred = false;
    for (uint32_t row = 0; row < size; row += 16)
    {
        uint8_t b[16];
        uint32_t n = size - row < 16 ? size - row : 16;
        for (uint32_t i = 0; i < n; ++i) b[i] = get(row + i);
        if (have_prev && n == 16 && row + 16 < size && memcmp(b, prev, 16) == 0)
        {
            if (!starred) out += "  *\n";
            starred = true;
            continue;
        }
        starred = false;
        appendf(out, "  $%04X:", base + row);
        for (uint32_t i = 0; i < n; ++i) appendf(out, i == 8 ? "  %02X" : " %02X", b[i]);
        out += "\n";
        memcpy(prev, b, 16);
        have_prev = n == 16;
    }
}

static uint8_t read_mem(nsfp *p, uint32_t adr)
{
    xgm::UINT32 v = 0;
    if (adr >= 0x8000) p->player.bank.Read(adr, v);
    else p->player.mem.Read(adr, v);
    return (uint8_t)v;
}

extern "C" {

nsfp *nsfp_create()
{
    nsfp *p = new nsfp();
    p->config["APU2_OPTION5"] = 0; // disable randomized noise phase at reset
    p->config["APU2_OPTION7"] = 0; // disable randomized tri phase at reset
    p->config["PLAY_ADVANCE"] = 1; // never fade out on its own; Player decides when a track ends
    p->player.SetConfig(&p->config);
    return p;
}

void nsfp_destroy(nsfp *p)
{
    delete p;
}

int nsfp_load(nsfp *p, const uint8_t *data, uint32_t size)
{
    p->image.assign(data, data + size);
    hide_legacy_text(p);
    p->nsf.SetDefaults(p->config["PLAY_TIME"], p->config["FADE_TIME"], p->config["LOOP_NUM"]);
    if (!p->nsf.Load(p->image.data(), size)) return p->loaded = false;

    // NSF::LoadFile sets these from its playlist item after Load; mirror it for a plain file.
    p->nsf.playlist_mode = false;
    p->nsf.title_unknown = true;
    p->nsf.enable_multi_tracks = true;
    p->nsf.time_in_ms = -1;
    p->nsf.loop_in_ms = -1;
    p->nsf.fade_in_ms = -1;
    p->nsf.loop_num = -1;
    p->nsf.playtime_unknown = true;

    p->loaded = p->player.Load(&p->nsf);
    return p->loaded;
}

const char *nsfp_error(nsfp *p)
{
    return p->nsf.LoadError();
}

const char *nsfp_title(nsfp *p)     { return text_or_raw(p->nsf.title, p->raw_text[0]); }
const char *nsfp_artist(nsfp *p)    { return text_or_raw(p->nsf.artist, p->raw_text[1]); }
const char *nsfp_copyright(nsfp *p) { return text_or_raw(p->nsf.copyright, p->raw_text[2]); }
const char *nsfp_ripper(nsfp *p)    { return p->nsf.ripper ? p->nsf.ripper : ""; }

// bit order matches NSFPlayerConfig::dname: APU1 APU2 5B MMC5 N163 VRC6 VRC7 FDS
uint32_t nsfp_expansions(nsfp *p)
{
    const xgm::NSF &n = p->nsf;
    return (n.use_fme7 ? 1 << 2 : 0) | (n.use_mmc5 ? 1 << 3 : 0) | (n.use_n106 ? 1 << 4 : 0) |
           (n.use_vrc6 ? 1 << 5 : 0) | (n.use_vrc7 ? 1 << 6 : 0) | (n.use_fds  ? 1 << 7 : 0);
}

// Tracks are numbered in playlist order (NSFe plst), matching NSF::SetSong.
int nsfp_track_count(nsfp *p)
{
    return p->nsf.nsfe_plst ? p->nsf.nsfe_plst_size : p->nsf.total_songs;
}

static int entry_index(nsfp *p, int track)
{
    return p->nsf.nsfe_plst ? p->nsf.nsfe_plst[track] : track;
}

// Returns an empty string when the file has no label for this track.
const char *nsfp_track_title(nsfp *p, int track)
{
    const char *t = p->nsf.nsfe_entry[entry_index(p, track)].tlbl;
    return t ? t : "";
}

// Length in ms including fade, or -1 if the file does not specify one.
int nsfp_track_length(nsfp *p, int track)
{
    const xgm::NSFE_Entry &e = p->nsf.nsfe_entry[entry_index(p, track)];
    if (e.time < 0) return -1;
    return e.time + (e.fade >= 0 ? e.fade : p->nsf.default_fadetime);
}

void nsfp_start(nsfp *p, int track, double rate)
{
    p->player.SetPlayFreq(rate);
    p->player.SetChannels(2);
    p->player.SetSong(track);
    // Silence and loop detection store the length they find in the NSF, and Reset() does not
    // clear it, so without this every later track would inherit the previous track's length.
    p->nsf.time_in_ms = -1;
    p->nsf.loop_in_ms = -1;
    p->nsf.fade_in_ms = -1;
    p->player.Reset();
}

uint32_t nsfp_render(nsfp *p, int16_t *buf, uint32_t frames)
{
    return p->player.Render(buf, frames);
}

void nsfp_skip(nsfp *p, uint32_t frames)
{
    p->player.Skip(frames);
}

int nsfp_stopped(nsfp *p)
{
    return p->player.IsStopped();
}

// Current track length in ms, which may change once silence is detected.
int nsfp_length(nsfp *p)
{
    return p->player.GetLength();
}

// Fade length of the current track in ms (from the file, or FADE_TIME).
int nsfp_fade_time(nsfp *p)
{
    return p->nsf.GetFadeTime();
}

void nsfp_fade_out(nsfp *p, int ms)
{
    p->player.FadeOut(ms);
}

// Cancels a fade in progress, e.g. when switching to endless playback.
void nsfp_cancel_fade(nsfp *p)
{
    p->player.fader.Reset();
}

// Generic access to NSFPlayerConfig values. Unknown names return 0 instead of throwing.
int nsfp_config_get(nsfp *p, const char *name, int *value)
{
    if (!p->config.HasValue(name)) return 0;
    *value = p->config[name].GetInt();
    return 1;
}

int nsfp_config_set(nsfp *p, const char *name, int value)
{
    if (!p->config.HasValue(name)) return 0;
    p->config[name] = value;
    return 1;
}

// Applies changed device settings (volume, options, pan); -1 means all devices.
void nsfp_notify(nsfp *p, int device)
{
    if (!p->loaded) return;
    p->player.Notify(device); // also calls NotifyPan
}

// Plain-text dump of the emulator state for Developer mode. Valid until the next call.
const char *nsfp_dump(nsfp *p)
{
    std::string &o = p->dump;
    o.clear();
    if (!p->loaded) return o.c_str();
    xgm::NSFPlayer &pl = p->player;
    const xgm::NSF &n = p->nsf;

    o += "FILE\n";
    appendf(o, "  version %d   songs %d   current %d\n", n.version, n.songs, n.song + 1);
    appendf(o, "  load $%04X   init $%04X   play $%04X\n", n.load_address, n.init_address, n.play_address);
    appendf(o, "  initial banks  %02X %02X %02X %02X %02X %02X %02X %02X\n", n.bankswitch[0], n.bankswitch[1],
            n.bankswitch[2], n.bankswitch[3], n.bankswitch[4], n.bankswitch[5], n.bankswitch[6], n.bankswitch[7]);
    appendf(o, "  speed  NTSC %u us   PAL %u us\n", n.speed_ntsc, n.speed_pal);
    appendf(o, "  region flags $%02X   chips $%02X   NSF2 flags $%02X\n", n.pal_ntsc, n.soundchip, n.nsf2_bits);

    const K6502_Context &c = pl.cpu.*CpuPeek::p_context();
    const char *flags = "NV-BDIZC";
    char fl[9];
    for (int i = 0; i < 8; ++i) fl[i] = (c.P >> (7 - i)) & 1 ? flags[i] : '.';
    fl[8] = 0;
    o += "\nCPU\n";
    appendf(o, "  A=%02X X=%02X Y=%02X S=%02X P=%02X [%s] PC=%04X\n", c.A & 0xff, c.X & 0xff, c.Y & 0xff,
            c.S & 0xff, c.P & 0xff, fl, c.PC & 0xffff);

    const int *banks = pl.bank.*BankPeek::p_bankswitch();
    o += "\nBANKS (4 KB pages)\n ";
    for (int i = n.use_fds ? 6 : 8; i < 16; ++i) appendf(o, " $%X000=%02X", i, banks[i] & 0xff);
    o += "\n";

    o += "\nRAM\n";
    hex_rows(o, 0x0000, 0x800, [&](uint32_t a) { return read_mem(p, a); });
    o += "\nWRAM\n";
    hex_rows(o, 0x6000, 0x2000, [&](uint32_t a) { return read_mem(p, 0x6000 + a); });

    o += "\n2A03\n";
    const xgm::UINT8 *apu = pl.apu->*ApuPeek::p_reg();
    const xgm::UINT8 *dmc = pl.dmc->*DmcPeek::p_reg();
    hex_rows(o, 0x4000, 8, [&](uint32_t a) { return apu[a]; });
    hex_rows(o, 0x4008, 0x10, [&](uint32_t a) { return dmc[a]; });

    if (n.use_mmc5)
    {
        o += "\nMMC5\n";
        const xgm::UINT8 *r = pl.mmc5->*Mmc5Peek::p_reg();
        hex_rows(o, 0x5000, 8, [&](uint32_t a) { return r[a]; });
    }
    if (n.use_vrc6)
    {
        o += "\nVRC6\n";
        const xgm::NES_VRC6 &v = *pl.vrc6;
        for (int i = 0; i < 3; ++i)
            appendf(o, "  %-7s freq %4u  vol %2d  duty %d  %s\n", i == 2 ? "saw" : i ? "pulse 2" : "pulse 1",
                    (v.*Vrc6Peek::p_freq())[i], (v.*Vrc6Peek::p_volume())[i], i < 2 ? (v.*Vrc6Peek::p_duty())[i] : 0,
                    (v.*Vrc6Peek::p_enable())[i] ? "on" : "off");
    }
    if (n.use_fme7)
    {
        o += "\n5B (registers 0-F)\n";
        const PSG *psg = pl.fme7->*Fme7Peek::p_psg();
        hex_rows(o, 0, 0x10, [&](uint32_t a) { return psg->reg[a]; });
    }
    if (n.use_vrc7)
    {
        o += "\nVRC7 (registers 00-3F)\n";
        const OPLL *opll = pl.vrc7->*Vrc7Peek::p_opll();
        hex_rows(o, 0, 0x40, [&](uint32_t a) { return opll->reg[a]; });
    }
    if (n.use_n106)
    {
        o += "\nN163 (internal RAM)\n";
        const xgm::UINT32 *r = pl.n106->*N163Peek::p_reg();
        hex_rows(o, 0, 0x80, [&](uint32_t a) { return (uint8_t)r[a]; });
    }
    if (n.use_fds)
    {
        o += "\nFDS\n";
        const xgm::NES_FDS &f = *pl.fds;
        appendf(o, "  wave freq %u   mod freq %u   master vol %u\n  wave", (f.*FdsPeek::p_freq())[1],
                (f.*FdsPeek::p_freq())[0], f.*FdsPeek::p_master_vol());
        for (int i = 0; i < 64; ++i) appendf(o, "%s%02d", i % 32 ? " " : "\n   ", (f.*FdsPeek::p_wave())[1][i]);
        o += "\n";
    }
    return o.c_str();
}

// Bit set = channel muted, in NSFPlayerConfig::channel_name order.
void nsfp_set_mask(nsfp *p, uint32_t mask)
{
    p->config["MASK"] = (int)mask;
    if (p->loaded) p->player.Notify(-1);
}

} // extern "C"
