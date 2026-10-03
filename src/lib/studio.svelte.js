// Studio mode's view of the current track: its intro + loop timeline, the playhead within it,
// and the A–B region. Positions are in "timeline time": song time folded into the first pass
// of the loop, since every pass plays the same music.

import { untrack } from 'svelte';
import { player } from './player.svelte.js';
import { playlist, trackKey } from './playlist.svelte.js';

/** One NES frame (NTSC), the smallest step when paused. */
export const FRAME_MS = 1000 / 60.0988;
/** The longest the playhead moves on its own between status polls, in real time. */
const EXTRAPOLATE_MS = 500;
/** The closest the timeline zooms in: this much of it fills the view. */
const MIN_VIEW_MS = 1000;

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

  /** How far past the last status poll `elapsed` may run (polls come every 33 ms). */
  ahead = $derived(EXTRAPOLATE_MS * player.speed);

  /**
   * Song time, interpolated from the last status poll. It never runs past an end where playback
   * stops, so the playhead halts there instead of overshooting until the stop is reported.
   */
  elapsed = $derived.by(() => {
    const s = player.status;
    if (s.state === 'stopped') return Math.min(s.elapsedMs, this.end); // 0, or where it ended
    const since = s.state === 'playing' ? Math.max(0, this.now - player.statusAt) : 0;
    const t = s.elapsedMs + Math.min(since * player.speed, this.ahead);
    return this.ending.source === 'silence' && this.ending.hard ? Math.min(t, this.end) : t;
  });

  /** Which pass through the loop is playing: 0 in the intro, then 1, 2, … */
  pass = $derived(
    this.loop && this.elapsed >= this.loop.startMs
      ? Math.floor((this.elapsed - this.loop.startMs) / (this.loop.endMs - this.loop.startMs)) + 1
      : 0,
  );

  /** The playhead on the timeline. */
  position = $derived(this.fold(this.elapsed));

  /** Whether Studio stops the track where it goes silent: no loop, and Listen's auto-stop on. */
  stopsAtSilence = $derived(player.mode === 'studio' && playlist.settings.autoStop && !this.loop);

  /**
   * Where the track ends on the timeline, how that is known, and whether playback can pass it
   * (`hard: false`). The end is one pass of its loop (the playhead folds back, so it is never
   * passed), the silence found by analysis or where playback stopped for it (hard when Studio
   * stops there), its length in the file, or (`guess`) Listen's default length while analysis
   * runs or when it found neither.
   * @type {{ ms: number, source: 'loop' | 'silence' | 'file' | 'guess', hard: boolean }}
   */
  ending = $derived.by(() => {
    if (this.loop) return { ms: this.loop.endMs, source: 'loop', hard: true };
    if (this.analysis?.kind === 'silence') {
      return { ms: this.analysis.atMs, source: 'silence', hard: this.stopsAtSilence };
    }
    if (player.ended && player.status.track === this.track) {
      return { ms: player.status.elapsedMs, source: 'silence', hard: true };
    }
    const fileMs = player.file?.tracks[this.track]?.lengthMs;
    if (fileMs) return { ms: fileMs, source: 'file', hard: false };
    return { ms: playlist.settings.playMs + playlist.settings.fadeMs, source: 'guess', hard: false };
  });
  end = $derived(this.ending.ms);

  /**
   * Length of the timeline: the track's end, or (once playback can run past it) the next whole
   * minute after where it may be. That follows the reported song time plus the most the playhead
   * can run ahead of it, never the interpolated playhead itself, so the span only changes when
   * playback really passes the end, and the playhead always fits.
   */
  span = $derived.by(() => {
    if (this.ending.hard) return this.end;
    const reach = player.active ? player.status.elapsedMs + this.ahead : 0;
    return reach <= this.end ? this.end : Math.ceil(reach / 60_000) * 60_000;
  });

  // ---- the visible part of the timeline: zoom and scroll ----

  /** How much of the timeline the view shows when zoomed in; null shows all of it. */
  zoomMs = $state(/** @type {number | null} */ (null));
  /** Where the view starts, as last set; `viewFrom` keeps it inside the timeline. */
  viewStart = $state(0);
  viewMs = $derived(Math.min(this.zoomMs ?? Infinity, this.span));
  viewFrom = $derived(Math.max(0, Math.min(this.viewStart, this.span - this.viewMs)));
  /** Whether the view pages along with the playhead; off while the user looks elsewhere. */
  follow = true;

  /** Whether timeline time `ms` is in view. */
  inView(ms) {
    return ms >= this.viewFrom && ms <= this.viewFrom + this.viewMs;
  }

  /** Zooms in by `factor` (below 1 zooms out), keeping timeline time `at` in place on screen. */
  zoom(factor, at = this.position) {
    const old = this.viewMs;
    if (!this.inView(at)) at = this.viewFrom + old / 2;
    const next = Math.max(Math.min(MIN_VIEW_MS, this.span), Math.min(this.span, old / factor));
    this.viewStart = at - ((at - this.viewFrom) / old) * next;
    this.zoomMs = next >= this.span ? null : next;
    this.follow = this.inView(this.position);
  }

  /** Shows the whole timeline. */
  fit() {
    this.zoomMs = null;
    this.viewStart = 0;
    this.follow = true;
  }

  /** Scrolls the view by `delta` ms. */
  pan(delta) {
    this.viewStart = this.viewFrom + delta;
    this.follow = this.inView(this.position);
  }

  /** Scrolls the view, a page at a time, so that `ms` is in it. */
  reveal(ms) {
    if (!this.inView(ms)) this.viewStart = ms - this.viewMs * 0.05;
  }

  /** Song time to timeline time. */
  fold(t) {
    const l = this.loop;
    if (!l || t < l.endMs) return t;
    return l.startMs + ((t - l.startMs) % (l.endMs - l.startMs));
  }

  /** Seeks to timeline time `ms` (into the first pass: same music, least emulation). */
  seekTo(ms) {
    ms = Math.max(0, Math.min(ms, this.span - 1));
    this.follow = true;
    this.reveal(ms);
    player.seek(ms);
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
   * Also shows each new track whole, pages the view along with the playhead, and stops a track
   * without a loop where it goes silent, as Listen does: at the silence analysis found, or
   * (analysis found no end within its limit) when the core's live detector hears 3 s of it.
   * The region is in first-pass time, so if it turns on while a later pass plays, playback
   * first moves to the same spot in the first pass (the same music).
   */
  connect() {
    return $effect.root(() => {
      $effect(() => {
        void this.key;
        untrack(() => this.fit());
      });
      $effect(() => {
        const at = this.position;
        untrack(() => this.follow && this.reveal(at));
      });
      $effect(() => {
        if (player.endless && player.active && this.path) playlist.requestAnalysis(this.path, this.track);
      });
      $effect(() => {
        void player.starts; // re-send after every track start, which clears it in the backend
        const a = this.analysis;
        const on = this.stopsAtSilence;
        player
          .call('set_silence_stop', {
            atMs: on && a?.kind === 'silence' ? a.atMs : null,
            detect: on && a?.kind === 'none',
          })
          .catch(() => {});
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
