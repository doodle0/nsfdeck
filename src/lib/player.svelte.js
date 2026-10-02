// Shared player state and the commands that drive the Rust backend (src-tauri/src/main.rs).

import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';

/**
 * @typedef {{ title: string, lengthMs: number | null }} Track
 * @typedef {{ bit: number, chip: string, name: string }} Channel
 * @typedef {{ path: string, title: string, artist: string, copyright: string, ripper: string,
 *             chips: string[], tracks: Track[], channels: Channel[] }} FileInfo
 * @typedef {{ state: 'stopped' | 'playing' | 'paused', track: number, elapsedMs: number,
 *             lengthMs: number, ended?: boolean }} Status
 * @typedef {'listen' | 'studio' | 'developer'} Mode
 */

/** @type {Mode[]} */
export const MODES = ['listen', 'studio', 'developer'];
const MODE_KEY = 'nsfdeck.mode';

/** @returns {Mode} */
function savedMode() {
  try {
    const m = localStorage.getItem(MODE_KEY);
    if (MODES.includes(/** @type {Mode} */ (m))) return /** @type {Mode} */ (m);
  } catch {
    // storage unavailable: fall back to the default
  }
  return 'listen';
}

class Player {
  /** @type {FileInfo | null} */
  file = $state(null);
  /** @type {Status} */
  status = $state({ state: 'stopped', track: 0, elapsedMs: 0, lengthMs: 0 });
  /** `performance.now()` when `status` was last updated, for interpolating the position. */
  statusAt = $state(0);
  /** Counts track starts; the backend forgets per-track settings (the A–B region) on each. */
  starts = $state(0);
  /** Bit set = channel muted. */
  mask = $state(0);
  /** 0-100 slider position. */
  volume = $state(80);
  error = $state('');
  /** Listen plays tracks to their end; Studio and Developer loop the current track endlessly. */
  mode = $state(savedMode());
  /** Speed multiplier used in Studio and Developer modes; Listen always plays at 1×. */
  speed = $state(1);
  /**
   * Listen mode's playlist (set by playlist.svelte.js, which imports this module).
   * @type {{ ended(): void, step(delta: number): void, playCurrent(): void, browse(): void } | null}
   */
  listen = null;

  active = $derived(this.status.state !== 'stopped');
  endless = $derived(this.mode !== 'listen');
  playing = $derived(this.status.state === 'playing');
  /** Length shown for the current track: live from the engine while playing, else from the file. */
  lengthMs = $derived(
    this.active ? this.status.lengthMs : (this.file?.tracks[this.status.track]?.lengthMs ?? 0),
  );
  /**
   * Range of the seek bar. In endless modes playback runs past the track's length, so the range
   * grows to the next whole minute after the playhead (until Studio gets its intro + loop timeline).
   */
  seekRangeMs = $derived(
    this.endless
      ? Math.max(this.lengthMs, Math.ceil((this.status.elapsedMs + 1) / 60000) * 60000)
      : this.lengthMs,
  );

  /** Invokes a backend command, surfacing failures in the error toast. */
  async call(cmd, args) {
    try {
      return await invoke(cmd, args);
    } catch (e) {
      this.error = String(e);
      throw e;
    }
  }

  /** Sends the current mode's playback settings to the backend. */
  applyMode() {
    this.call('set_endless', { endless: this.endless }).catch(() => {});
    this.call('set_speed', { speed: this.endless ? this.speed : 1 }).catch(() => {});
  }

  /** @param {Mode} mode */
  setMode(mode) {
    this.mode = mode;
    try {
      localStorage.setItem(MODE_KEY, mode);
    } catch {
      // not persisted; the mode still applies for this session
    }
    this.applyMode();
  }

  setSpeed(speed) {
    this.speed = Math.min(2, Math.max(0.25, Math.round(speed * 100) / 100));
    if (this.endless) this.call('set_speed', { speed: this.speed }).catch(() => {});
  }

  /** Loads `path` into the audio output unless it is already loaded. Playback stops. */
  async open(path) {
    if (this.file?.path === path) return this.file;
    /** @type {FileInfo} */
    const info = await this.call('open', { path });
    this.file = info;
    this.mask = 0;
    this.status = { state: 'stopped', track: 0, elapsedMs: 0, lengthMs: 0 };
    const name = info.title || path.split(/[\\/]/).pop();
    getCurrentWindow().setTitle(`${name} — NSFDeck`).catch(() => {});
    return info;
  }

  browse() {
    this.listen?.browse();
  }

  /**
   * Plays `track` of the loaded file. `length` ({ playMs, fadeMs }) overrides how long it
   * plays in Listen mode; null uses the file's length or the defaults.
   */
  async play(track, length = null) {
    if (!this.file) return;
    await this.call('play', { track, length });
    this.starts++;
    this.status = { ...this.status, state: 'playing', track, elapsedMs: 0 };
    this.statusAt = performance.now();
  }

  async togglePlay() {
    if (!this.file) return this.endless ? this.browse() : this.listen?.playCurrent();
    if (!this.active) return this.endless || !this.listen ? this.play(this.status.track) : this.listen.playCurrent();
    await this.call('set_paused', { paused: this.playing });
    await this.poll();
  }

  async stop() {
    await this.call('stop');
    await this.poll();
  }

  /** Previous/next: through the playlist in Listen mode, through the file's tracks otherwise. */
  step(delta) {
    if (!this.endless && this.listen) return this.listen.step(delta);
    const track = this.status.track + delta;
    if (this.file && track >= 0 && track < this.file.tracks.length) this.play(track);
  }

  async seek(ms) {
    if (!this.active) return;
    await this.call('seek', { ms: Math.max(0, Math.round(ms)) });
    await this.poll();
  }

  setVolume(value) {
    this.volume = value;
    const v = value / 100;
    this.call('set_volume', { volume: v * v }).catch(() => {}); // squared: closer to perceived loudness
  }

  setMask(mask) {
    this.mask = mask >>> 0;
    this.call('set_mute_mask', { mask: this.mask }).catch(() => {});
  }

  toggleChannel(bit) {
    this.setMask(this.mask ^ (1 << bit));
  }

  /** Mutes every other channel; soloing the only audible channel again unmutes everything. */
  solo(bit) {
    const all = this.file.channels.reduce((m, ch) => m | (1 << ch.bit), 0);
    const others = (all & ~(1 << bit)) >>> 0;
    this.setMask(this.mask === others ? 0 : others);
  }

  async poll() {
    if (!this.file) return;
    try {
      /** @type {Status} */
      const s = await invoke('status');
      this.status = s;
      this.statusAt = performance.now();
      if (s.ended && !this.endless) this.listen?.ended();
    } catch {
      // backend errors (e.g. no audio device) were already reported by the command that hit them
    }
  }
}

export const player = new Player();

export function formatTime(ms) {
  const s = Math.floor(Math.max(0, ms) / 1000);
  const h = Math.floor(s / 3600);
  const m = Math.floor(s / 60) % 60;
  const ss = String(s % 60).padStart(2, '0');
  return h ? `${h}:${String(m).padStart(2, '0')}:${ss}` : `${m}:${ss}`;
}
