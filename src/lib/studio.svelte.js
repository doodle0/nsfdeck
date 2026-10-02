// Studio mode's view of the current track: its intro + loop timeline, the playhead within it,
// and the A–B region. Positions are in "timeline time": song time folded into the first pass
// of the loop, since every pass plays the same music.

import { untrack } from 'svelte';
import { player } from './player.svelte.js';
import { playlist, trackKey } from './playlist.svelte.js';

/** One NES frame (NTSC), the smallest step when paused. */
export const FRAME_MS = 1000 / 60.0988;

class Studio {
  path = $derived(player.file?.path ?? null);
  track = $derived(player.status.track);
  key = $derived(this.path ? trackKey(this.path, this.track) : null);
  analysis = $derived(this.key ? (playlist.analyses[this.key] ?? null) : null);
  /** @type {import('./playlist.svelte.js').Loop | null} */
  loop = $derived(this.path ? playlist.loopFor(this.path, this.track) : null);
  /** @type {import('./playlist.svelte.js').Region | null} */
  region = $derived(this.key ? (playlist.regions[this.key] ?? null) : null);
  /** Ticks while playing, so the playhead moves smoothly between status polls. */
  now = $state(0);

  /** Song time, interpolated from the last status poll. */
  elapsed = $derived.by(() => {
    const s = player.status;
    if (s.state === 'stopped') return 0;
    const since = s.state === 'playing' ? Math.max(0, this.now - player.statusAt) : 0;
    return s.elapsedMs + Math.min(since, 500) * player.speed;
  });

  /** Which pass through the loop is playing: 0 in the intro, then 1, 2, … */
  pass = $derived(
    this.loop && this.elapsed >= this.loop.startMs
      ? Math.floor((this.elapsed - this.loop.startMs) / (this.loop.endMs - this.loop.startMs)) + 1
      : 0,
  );

  /** The playhead on the timeline. */
  position = $derived(this.fold(this.elapsed));

  /** Length of the timeline: the intro plus one loop, or (no loop known) growing with playback. */
  span = $derived(
    this.loop ? this.loop.endMs : Math.max(60_000, Math.ceil((this.elapsed + 1) / 60_000) * 60_000),
  );

  /** Song time to timeline time. */
  fold(t) {
    const l = this.loop;
    if (!l || t < l.endMs) return t;
    return l.startMs + ((t - l.startMs) % (l.endMs - l.startMs));
  }

  /** Seeks to timeline time `ms` (into the first pass: same music, least emulation). */
  seekTo(ms) {
    player.seek(Math.max(0, Math.min(ms, this.span - 1)));
  }

  /** Steps the playhead by `delta` ms of timeline time. */
  step(delta) {
    this.seekTo(this.position + delta);
  }

  /** Sets the loop start or end at the playhead (by hand; also used by Listen's lengths). */
  setLoopPoint(which) {
    if (!this.path) return;
    const at = this.position;
    const l = this.loop ?? { startMs: 0, endMs: Math.max(at, 1) };
    const next = which === 'start' ? { startMs: at, endMs: l.endMs } : { startMs: l.startMs, endMs: at };
    if (next.endMs - next.startMs < 100) return;
    playlist.setLoop(this.path, this.track, next);
  }

  resetLoop() {
    if (this.path) playlist.setLoop(this.path, this.track, null);
  }

  /** @param {{ a: number, b: number, on: boolean } | null} region */
  setRegion(region) {
    if (!this.path) return;
    if (region && region.b - region.a < 50) region = null;
    playlist.setRegion(this.path, this.track, region);
  }

  toggleRegion() {
    if (this.region) this.setRegion({ ...this.region, on: !this.region.on });
  }

  /**
   * Keeps the backend in step: analyzes the track for its timeline, and sends the A–B region.
   * The region is in first-pass time, so if it turns on while a later pass plays, playback
   * first moves to the same spot in the first pass (the same music).
   */
  connect() {
    return $effect.root(() => {
      $effect(() => {
        if (player.endless && player.active && this.path) playlist.requestAnalysis(this.path, this.track);
      });
      $effect(() => {
        const r = this.region;
        void player.starts; // re-send after every track start, which clears it in the backend
        const on = !!(player.endless && player.active && r?.on);
        if (on && untrack(() => this.pass) > 1) untrack(() => this.seekTo(this.position));
        player.call('set_region', { region: on ? [r.a, r.b] : null }).catch(() => {});
      });
    });
  }
}

export const studio = new Studio();
