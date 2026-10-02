/* C interface to the NSFPlay xgm player, wrapped by src/lib.rs. */

#include <stdint.h>
#include <string.h>

#include <vector>

#include "xgm/xgm.h"

struct nsfp
{
    xgm::NSF nsf;
    xgm::NSFPlayerConfig config;
    xgm::NSFPlayer player;
    std::vector<uint8_t> image; // NSF::Load may keep pointers into the image
    char raw_text[3][33] = {};   // non-ASCII NSF header strings, hidden from NSF::Load
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

// Bit set = channel muted, in NSFPlayerConfig::channel_name order.
void nsfp_set_mask(nsfp *p, uint32_t mask)
{
    p->config["MASK"] = (int)mask;
    if (p->loaded) p->player.Notify(-1);
}

} // extern "C"
