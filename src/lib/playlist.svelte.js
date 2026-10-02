// Listen mode's playlist: entries, durations, shuffle/repeat, background analysis and saving.

import { invoke } from '@tauri-apps/api/core';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
import { player } from './player.svelte.js';
import { parseM3u, formatM3u } from './m3u.js';
import { mixer } from './mixer.svelte.js';

/**
 * @typedef {{ mode: 'auto' } | { mode: 'loops', loops: number } |
 *           { mode: 'fixed', playMs: number, fadeMs?: number } | { mode: 'endless' }} Duration
 * @typedef {{ id: number, path: string, track: number, title: string, game: string,
 *             artist: string, fileLengthMs: number | null, duration: Duration }} Entry
 * @typedef {{ kind: 'loop', startMs: number, endMs: number } | { kind: 'silence', atMs: number } |
 *           { kind: 'none' } | { kind: 'error' }} Analysis
 * @typedef {'file' | 'auto' | 'custom' | 'default' | 'pending'} Source
 * @typedef {{ startMs: number, endMs: number, manual: boolean }} Loop
 * @typedef {{ a: number, b: number, on: boolean }} Region
 * @typedef {{ length: { playMs: number, fadeMs: number } | null, source: Source,
 *             totalMs: number | null }} Resolved
 */

export const DEFAULT_SETTINGS = { playMs: 5 * 60_000, fadeMs: 5000, loops: 2, detectLoops: true, autoStop: true };
const ENDLESS_MS = 2 ** 31; // far beyond any listening session
const STATE_VERSION = 1;
/** Bump when analysis results change meaning, to re-analyze tracks saved by older versions. */
const ANALYSIS_VERSION = 3;

const key = (/** @type {{ path: string, track: number }} */ e) => `${e.track}:${e.path}`;
export const trackKey = (path, track) => `${track}:${path}`;

function shuffled(items) {
  const a = [...items];
  for (let i = a.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [a[i], a[j]] = [a[j], a[i]];
  }
  return a;
}

class Playlist {
  /** @type {Entry[]} */
  entries = $state([]);
  /** Entry being played (or last played), by id. */
  currentId = $state(/** @type {number | null} */ (null));
  /** Highlighted row, by id. */
  selectedId = $state(/** @type {number | null} */ (null));
  /** @type {'off' | 'all' | 'one'} */
  repeat = $state('off');
  shuffle = $state(false);
  settings = $state({ ...DEFAULT_SETTINGS });
  /** Analysis results by `key(entry)`. Shared by entries of the same file and track. */
  /** @type {Record<string, Analysis>} */
  analyses = $state({});
  /** Loop points set by hand in Studio, by `key(entry)`; they win over the analysis. */
  /** @type {Record<string, { startMs: number, endMs: number }>} */
  loopOverrides = $state({});
  /** Studio's A–B regions, by `key(entry)`. */
  /** @type {Record<string, Region>} */
  regions = $state({});
  /** True while files are being scanned and probed. */
  adding = $state(false);

  currentIndex = $derived(this.entries.findIndex((e) => e.id === this.currentId));
  current = $derived(this.entries[this.currentIndex] ?? null);

  /** Shuffled play order, as entry ids. */
  #order = /** @type {number[]} */ ([]);
  #nextId = 1;
  /** Analysis queue (entry keys); the front is analyzed next. */
  #queue = /** @type {string[]} */ ([]);
  /** Keys requested for tracks outside the playlist (e.g. played in Studio). */
  #wanted = new Set();
  #analyzing = false;
  #loaded = false;

  constructor() {
    player.listen = {
      ended: () => this.advance(1, true),
      step: (delta) => this.advance(delta, false),
      playCurrent: () => this.playCurrent(),
      browse: () => this.browseFiles(),
    };
  }

  /** How long `entry` plays, and why. @returns {Resolved} */
  resolve(entry) {
    const s = this.settings;
    const d = entry.duration;
    if (d.mode === 'endless') return { length: { playMs: ENDLESS_MS, fadeMs: 0 }, source: 'custom', totalMs: null };
    if (d.mode === 'fixed') {
      const fadeMs = d.fadeMs ?? s.fadeMs;
      return { length: { playMs: d.playMs, fadeMs }, source: 'custom', totalMs: d.playMs + fadeMs };
    }
    if (entry.fileLengthMs != null) return { length: null, source: 'file', totalMs: entry.fileLengthMs };

    const a = this.analyses[key(entry)];
    const loop = this.loopFor(entry.path, entry.track);
    const loops = d.mode === 'loops' ? d.loops : s.loops;
    if (loop && (s.detectLoops || d.mode === 'loops' || loop.manual)) {
      const playMs = loop.startMs + loops * (loop.endMs - loop.startMs);
      return {
        length: { playMs, fadeMs: s.fadeMs },
        source: d.mode === 'loops' ? 'custom' : 'auto',
        totalMs: playMs + s.fadeMs,
      };
    }
    if (a?.kind === 'silence' && s.autoStop) {
      return { length: { playMs: a.atMs, fadeMs: 0 }, source: 'auto', totalMs: a.atMs };
    }
    const needsAnalysis = !a && (s.detectLoops || s.autoStop || d.mode === 'loops');
    return {
      length: { playMs: s.playMs, fadeMs: s.fadeMs },
      source: needsAnalysis ? 'pending' : 'default',
      totalMs: s.playMs + s.fadeMs,
    };
  }

  /** The loop of a track: set by hand, or found by analysis. @returns {Loop | null} */
  loopFor(path, track) {
    const k = trackKey(path, track);
    const o = this.loopOverrides[k];
    if (o) return { ...o, manual: true };
    const a = this.analyses[k];
    return a?.kind === 'loop' ? { startMs: a.startMs, endMs: a.endMs, manual: false } : null;
  }

  /** Sets a track's loop by hand, or (null) goes back to the analysis. */
  setLoop(path, track, loop) {
    const k = trackKey(path, track);
    if (loop) this.loopOverrides[k] = { startMs: Math.round(loop.startMs), endMs: Math.round(loop.endMs) };
    else delete this.loopOverrides[k];
    this.#applyLength();
  }

  /** @param {Region | null} region */
  setRegion(path, track, region) {
    const k = trackKey(path, track);
    if (region) this.regions[k] = { a: Math.round(region.a), b: Math.round(region.b), on: region.on };
    else delete this.regions[k];
  }

  /** Analyzes a track soon even if it is not in the playlist (e.g. for Studio's timeline). */
  requestAnalysis(path, track) {
    const k = trackKey(path, track);
    if (this.analyses[k]) return;
    this.#wanted.add(k);
    this.#queue = [k, ...this.#queue.filter((q) => q !== k)];
    this.#pump();
  }

  // ---- adding and removing ----

  /** Adds files and folders; every track of each file becomes an entry. */
  async add(paths, { play = false } = {}) {
    this.adding = true;
    /** @type {Entry[]} */
    const added = [];
    const failed = [];
    try {
      /** @type {string[]} */
      const files = await invoke('scan', { paths });
      for (const path of files) {
        try {
          added.push(...this.#entriesFor(path, await invoke('probe', { path })));
        } catch (e) {
          failed.push(String(e));
        }
      }
    } finally {
      this.adding = false;
    }
    this.#insert(added);
    if (failed.length) {
      player.error = failed.length === 1 ? failed[0] : `${failed.length} files could not be opened. ${failed[0]}`;
    }
    if (play && added.length) await this.playId(added[0].id);
    return added;
  }

  async browseFiles() {
    const picked = await openDialog({
      multiple: true,
      filters: [{ name: 'NES Sound Format', extensions: ['nsf', 'nsfe'] }],
    });
    if (picked?.length) await this.add(picked, { play: !player.active });
  }

  async browseFolder() {
    const dir = await openDialog({ directory: true });
    if (dir) await this.add([dir], { play: !player.active });
  }

  /** @param {string} path @param {import('./player.svelte.js').FileInfo} info @returns {Entry[]} */
  #entriesFor(path, info, overrides = new Map()) {
    const game = info.title || path.split(/[\\/]/).pop();
    return info.tracks.map((t, track) => ({
      id: this.#nextId++,
      path,
      track,
      title: overrides.get(track)?.title || t.title,
      game,
      artist: info.artist,
      fileLengthMs: t.lengthMs,
      duration: overrides.get(track)?.duration ?? { mode: 'auto' },
    }));
  }

  /** @param {Entry[]} added */
  #insert(added) {
    if (!added.length) return;
    this.entries.push(...added);
    if (this.shuffle) {
      // new entries go at random positions after the current one
      const at = Math.max(0, this.#order.indexOf(this.currentId ?? -1) + 1);
      const rest = shuffled([...this.#order.slice(at), ...added.map((e) => e.id)]);
      this.#order = [...this.#order.slice(0, at), ...rest];
    }
    for (const e of added) this.#enqueue(e);
  }

  remove(ids) {
    const gone = new Set(ids);
    this.entries = this.entries.filter((e) => !gone.has(e.id));
    this.#order = this.#order.filter((id) => !gone.has(id));
    if (gone.has(this.selectedId)) this.selectedId = null;
  }

  clear() {
    this.entries = [];
    this.#order = [];
    this.selectedId = null;
  }

  /** Moves the entry at `from` so it ends up at index `to`. */
  move(from, to) {
    if (from === to || from < 0 || from >= this.entries.length) return;
    const [e] = this.entries.splice(from, 1);
    this.entries.splice(Math.min(Math.max(to, 0), this.entries.length), 0, e);
  }

  /** @param {number} id @param {Duration} duration */
  setDuration(id, duration) {
    const e = this.entries.find((x) => x.id === id);
    if (!e) return;
    e.duration = duration;
    if (duration.mode === 'loops') this.#enqueue(e, true);
    if (id === this.currentId) this.#applyLength();
  }

  setSettings(patch) {
    Object.assign(this.settings, patch);
    if (this.settings.detectLoops || this.settings.autoStop) for (const e of this.entries) this.#enqueue(e);
    this.#applyLength();
  }

  // ---- playback ----

  async playId(id) {
    const e = this.entries.find((x) => x.id === id);
    if (!e) return;
    this.currentId = id;
    this.selectedId = id;
    await player.open(e.path);
    await player.play(e.track, this.resolve(e).length);
    this.#enqueue(e, true);
  }

  async playCurrent() {
    const id = this.current ? this.currentId : this.entries[0]?.id;
    if (id != null) await this.playId(id);
    else await this.browseFiles();
  }

  /**
   * Moves `delta` entries through the play order. `auto` means the previous track ended on
   * its own: then repeat-one replays it and the end of the list stops playback.
   */
  async advance(delta, auto) {
    if (!this.entries.length) return;
    if (auto && this.repeat === 'one' && this.current) return this.playId(this.current.id);
    const next = this.#neighbor(delta, auto);
    if (next != null) await this.playId(next);
  }

  /** The entry id `delta` steps from the current one, or null at the end of the list. */
  #neighbor(delta, auto) {
    const ids = this.shuffle ? this.#syncOrder() : this.entries.map((e) => e.id);
    const at = ids.indexOf(this.currentId ?? -1);
    const target = at + delta;
    if (target >= 0 && target < ids.length) return ids[target];
    if (this.repeat === 'off' && auto) return null;
    if (this.repeat === 'off' && !auto) return at < 0 ? ids[0] : null;
    // repeat all: wrap around, with a fresh shuffle that doesn't start with the last entry
    if (this.shuffle && delta > 0) {
      const last = this.currentId;
      this.#order = shuffled(this.entries.map((e) => e.id));
      if (this.#order.length > 1 && this.#order[0] === last) this.#order.push(this.#order.shift());
      return this.#order[0];
    }
    return ids[(target + ids.length) % ids.length];
  }

  /** Keeps the shuffle order in step with the entries (e.g. after loading saved state). */
  #syncOrder() {
    const ids = new Set(this.entries.map((e) => e.id));
    this.#order = this.#order.filter((id) => ids.has(id));
    const missing = [...ids].filter((id) => !this.#order.includes(id));
    if (missing.length) this.#order.push(...shuffled(missing));
    return this.#order;
  }

  toggleShuffle() {
    this.shuffle = !this.shuffle;
    if (this.shuffle) {
      const rest = this.entries.map((e) => e.id).filter((id) => id !== this.currentId);
      this.#order = this.currentId != null ? [this.currentId, ...shuffled(rest)] : shuffled(rest);
    }
  }

  cycleRepeat() {
    this.repeat = this.repeat === 'off' ? 'all' : this.repeat === 'all' ? 'one' : 'off';
  }

  /** Re-sends the current entry's length, e.g. after its analysis or its duration changed. */
  #applyLength() {
    const e = this.current;
    if (!e || player.endless || !player.active || player.file?.path !== e.path || player.status.track !== e.track) return;
    player.call('set_length', { length: this.resolve(e).length }).catch(() => {});
  }

  // ---- background analysis ----

  #enqueue(entry, urgent = false) {
    if (entry.fileLengthMs != null) return; // the file says how long it is
    const k = key(entry);
    if (this.analyses[k]) return;
    this.#queue = this.#queue.filter((q) => q !== k);
    if (urgent) this.#queue.unshift(k);
    else this.#queue.push(k);
    this.#pump();
  }

  async #pump() {
    if (this.#analyzing) return;
    this.#analyzing = true;
    try {
      while (this.#queue.length) {
        const k = this.#queue.shift();
        const sep = k.indexOf(':');
        const track = Number(k.slice(0, sep));
        const path = k.slice(sep + 1);
        const needed = this.#wanted.has(k) || this.entries.some((e) => e.path === path && e.track === track);
        if (this.analyses[k] || !needed) continue;
        try {
          this.analyses[k] = await invoke('analyze', { path, track });
        } catch {
          this.analyses[k] = { kind: 'error' };
        }
        if (this.current && key(this.current) === k) this.#applyLength();
      }
    } finally {
      this.#analyzing = false;
    }
  }

  // ---- M3U ----

  async importM3u() {
    const path = await openDialog({ filters: [{ name: 'Playlist', extensions: ['m3u', 'm3u8'] }] });
    if (!path) return;
    const items = parseM3u(await player.call('read_text', { path }), path);
    // group by file so each file is probed once; keep the playlist's order
    /** @type {Map<string, Map<number, { title: string, duration?: Duration }> | null>} */
    const byFile = new Map();
    for (const it of items) {
      if (it.track == null) byFile.set(it.path, null);
      else if (byFile.get(it.path) !== null) {
        const m = byFile.get(it.path) ?? new Map();
        m.set(it.track, { title: it.title, duration: it.duration });
        byFile.set(it.path, m);
      }
    }
    const added = [];
    const failed = [];
    for (const [file, tracks] of byFile) {
      try {
        const all = this.#entriesFor(file, await invoke('probe', { path: file }), tracks ?? new Map());
        added.push(...(tracks ? all.filter((e) => tracks.has(e.track)) : all));
      } catch (e) {
        failed.push(String(e));
      }
    }
    // put entries in the order the playlist lists them
    const rank = new Map(items.map((it, i) => [`${it.track}:${it.path}`, i]));
    added.sort((a, b) => (rank.get(key(a)) ?? rank.get(`undefined:${a.path}`) ?? 0) - (rank.get(key(b)) ?? rank.get(`undefined:${b.path}`) ?? 0));
    this.#insert(added);
    if (failed.length) player.error = `${failed.length} file(s) in the playlist could not be opened. ${failed[0]}`;
  }

  async exportM3u() {
    const path = await saveDialog({ defaultPath: 'playlist.m3u', filters: [{ name: 'Playlist', extensions: ['m3u'] }] });
    if (!path) return;
    const rows = this.entries.map((e) => ({ path: e.path, track: e.track, title: e.title, resolved: this.resolve(e) }));
    await player.call('write_text', { path, text: formatM3u(rows, path) });
  }

  // ---- saving ----

  async restore() {
    try {
      /** @type {string | null} */
      const json = await invoke('load_state');
      const s = json ? JSON.parse(json) : null;
      if (s?.version === STATE_VERSION) {
        this.entries = s.entries ?? [];
        this.#nextId = Math.max(0, ...this.entries.map((e) => e.id)) + 1;
        this.currentId = s.currentId ?? null;
        this.selectedId = this.currentId;
        this.repeat = s.repeat ?? 'off';
        this.shuffle = !!s.shuffle;
        this.#order = s.order ?? [];
        this.settings = { ...DEFAULT_SETTINGS, ...s.settings };
        this.analyses = s.analysisVersion === ANALYSIS_VERSION ? (s.analyses ?? {}) : {};
        this.loopOverrides = s.loopOverrides ?? {};
        this.regions = s.regions ?? {};
        mixer.restore(s.mixer);
        for (const e of this.entries) this.#enqueue(e);
      }
    } catch (e) {
      player.error = `Could not restore the playlist: ${e}`;
    } finally {
      this.#loaded = true;
    }
    // show the current entry's file without starting playback
    if (this.current) await player.open(this.current.path).catch(() => {});
  }

  snapshot() {
    return {
      version: STATE_VERSION,
      entries: this.entries,
      currentId: this.currentId,
      repeat: this.repeat,
      shuffle: this.shuffle,
      order: this.shuffle ? this.#order : [],
      settings: this.settings,
      analysisVersion: ANALYSIS_VERSION,
      analyses: this.analyses,
      loopOverrides: this.loopOverrides,
      regions: this.regions,
      mixer: mixer.snapshot(),
    };
  }

  /** Saves the playlist shortly after it changes, for as long as the app runs. */
  autosave() {
    let timer;
    return $effect.root(() => {
      $effect(() => {
        const json = JSON.stringify(this.snapshot());
        if (!this.#loaded) return;
        clearTimeout(timer);
        timer = setTimeout(() => invoke('save_state', { json }).catch((e) => (player.error = String(e))), 400);
      });
    });
  }
}

export const playlist = new Playlist();
