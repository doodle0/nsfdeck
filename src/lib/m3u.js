// NSF playlists in the extended M3U format used by NEZplug and Game_Music_Emu:
//
//   file.nsf::NSF,track,title,time,loop,fade
//
// Commas in the title are escaped as "\,". Times are h:m:s, m:s or plain seconds. Decimal
// track numbers count from 1; "$" hex track numbers count from 0 (assumption: this follows
// Game_Music_Emu's reading of the format). Plain lines without "::" add every track of a file.

/**
 * @typedef {{ path: string, track?: number, title?: string,
 *             duration?: import('./playlist.svelte.js').Duration }} M3uItem
 */

const dirname = (p) => p.replace(/[\\/][^\\/]*$/, '');
const isAbsolute = (p) => /^([a-zA-Z]:[\\/]|[\\/])/.test(p);

/** Splits on commas that are not escaped with a backslash, unescaping "\,". */
function splitFields(s) {
  const out = [''];
  for (let i = 0; i < s.length; i++) {
    if (s[i] === '\\' && s[i + 1] === ',') {
      out[out.length - 1] += ',';
      i++;
    } else if (s[i] === ',') out.push('');
    else out[out.length - 1] += s[i];
  }
  return out.map((f) => f.trim());
}

/** "1:02:03", "2:34", "95" or "95.5" to ms; null when empty or invalid. */
export function parseTime(s) {
  if (!s || !/^\d+(:\d+){0,2}(\.\d+)?$/.test(s.trim())) return null;
  const parts = s.trim().split(':').map(Number);
  let secs = 0;
  for (const p of parts) secs = secs * 60 + p;
  return Math.round(secs * 1000);
}

/** ms to "m:ss" (or "h:mm:ss"). */
export function formatTime(ms) {
  const s = Math.round(ms / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor(s / 60) % 60;
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}

/** @param {string} text @param {string} m3uPath @returns {M3uItem[]} */
export function parseM3u(text, m3uPath) {
  const base = dirname(m3uPath);
  const sep = m3uPath.includes('\\') && !m3uPath.includes('/') ? '\\' : '/';
  const resolve = (p) => (isAbsolute(p) ? p : `${base}${sep}${p}`);
  /** @type {M3uItem[]} */
  const items = [];
  for (const raw of text.replace(/^﻿/, '').split(/\r?\n/)) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    const at = line.indexOf('::');
    if (at < 0) {
      items.push({ path: resolve(line) });
      continue;
    }
    const [type, trackField, title, time, , fade] = splitFields(line.slice(at + 2));
    if (!/^nsfe?$/i.test(type)) continue;
    const track = trackField?.startsWith('$') ? parseInt(trackField.slice(1), 16) : parseInt(trackField, 10) - 1;
    if (!Number.isInteger(track) || track < 0) continue;
    const playMs = parseTime(time);
    const fadeMs = parseTime(fade);
    items.push({
      path: resolve(line.slice(0, at)),
      track,
      title: title || undefined,
      duration: playMs != null ? { mode: 'fixed', playMs, ...(fadeMs != null && { fadeMs }) } : undefined,
    });
  }
  return items;
}

/**
 * @param {{ path: string, track: number, title: string,
 *           resolved: import('./playlist.svelte.js').Resolved }[]} rows
 * @param {string} m3uPath
 */
export function formatM3u(rows, m3uPath) {
  const base = dirname(m3uPath);
  const rel = (p) => (p.startsWith(base) && /[\\/]/.test(p[base.length]) ? p.slice(base.length + 1) : p);
  const lines = rows.map(({ path, track, title, resolved }) => {
    const len = resolved.length;
    // endless entries have no time; entries using the file's own length leave it to the player
    const time = len && resolved.totalMs != null ? formatTime(len.playMs) : '';
    const fade = len && resolved.totalMs != null ? String(Math.round(len.fadeMs / 1000)) : '';
    return `${rel(path)}::NSF,${track + 1},${title.replaceAll(',', '\\,')},${time},,${fade}`;
  });
  return lines.join('\n') + '\n';
}
