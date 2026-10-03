<script>
  import { player } from '../player.svelte.js';
  import { studio } from '../studio.svelte.js';

  const HEIGHT = 58;
  const BAND_Y = 18;
  const BAND_H = 26;
  const HANDLE_PX = 6;

  let width = $state(600);
  let svg = $state(/** @type {SVGSVGElement | undefined} */ (undefined));

  // move the playhead smoothly while playing
  $effect(() => {
    if (!player.playing) return;
    let frame = requestAnimationFrame(function tick(t) {
      studio.now = t;
      frame = requestAnimationFrame(tick);
    });
    return () => cancelAnimationFrame(frame);
  });

  let span = $derived(Math.max(1, studio.span));
  let from = $derived(studio.viewFrom);
  let view = $derived(Math.max(1, studio.viewMs));
  let zoomed = $derived(view < span);
  const x = (ms) => ((ms - from) / view) * width;
  const msAt = (px) => Math.max(0, Math.min(span, from + (px / width) * view));
  /** How far the track is known to run: up to here it's drawn solid, after it hatched. */
  let knownEnd = $derived(studio.ending.source === 'guess' ? 0 : studio.end);
  /** An end that isn't the loop's (that one is the band's own end), marked with a line. */
  let endMark = $derived(studio.ending.source === 'silence' || studio.ending.source === 'file');

  /** Position on the overview, which always shows the whole timeline. */
  const pct = (ms) => `${(Math.max(0, Math.min(span, ms)) / span) * 100}%`;

  /** m:ss.t */
  function fmt(ms) {
    const t = Math.max(0, Math.round(ms / 100));
    return `${Math.floor(t / 600)}:${String(Math.floor(t / 10) % 60).padStart(2, '0')}.${t % 10}`;
  }

  /** Ruler ticks in view, at a step that leaves room for their labels. */
  let tickStep = $derived(
    [0.1, 0.2, 0.5, 1, 2, 5, 10, 15, 30, 60, 120, 300].map((s) => s * 1000).find((s) => (s / view) * width >= 56) ??
      600_000,
  );
  let ticks = $derived.by(() => {
    const first = Math.ceil(from / tickStep);
    const count = Math.floor((from + view) / tickStep) - first + 1;
    return Array.from({ length: Math.max(0, count) }, (_, i) => (first + i) * tickStep);
  });
  /** Tick labels show tenths only when ticks are less than a second apart. */
  const tickLabel = (ms) => (tickStep < 1000 ? fmt(ms) : fmt(ms).replace(/\.\d$/, ''));

  // ---- wheel: scroll the view; Ctrl/⌘-wheel (or a touchpad pinch) zooms around the pointer ----

  $effect(() => {
    const el = svg;
    if (!el) return;
    /** @param {WheelEvent} e */
    function onWheel(e) {
      if (e.ctrlKey || e.metaKey) {
        e.preventDefault();
        studio.zoom(Math.exp(-e.deltaY * 0.002), msAt(e.clientX - el.getBoundingClientRect().left));
        return;
      }
      if (!zoomed) return; // nothing to scroll: leave the wheel to the page
      e.preventDefault();
      const d = Math.abs(e.deltaX) > Math.abs(e.deltaY) ? e.deltaX : e.deltaY;
      studio.pan((d / width) * view);
    }
    // not passive, so it can keep Ctrl-wheel from zooming the page
    el.addEventListener('wheel', onWheel, { passive: false });
    return () => el.removeEventListener('wheel', onWheel);
  });

  // ---- overview: the whole timeline, with the view as a thumb to drag ----

  /** @type {{ x: number, from: number } | null} */
  let thumbDrag = null;

  /** @param {PointerEvent & { currentTarget: HTMLDivElement }} e */
  function onOverviewDown(e) {
    if (e.button !== 0 || !zoomed) return;
    const r = e.currentTarget.getBoundingClientRect();
    const at = ((e.clientX - r.left) / r.width) * span;
    // a click beside the thumb centers the view there, then drags it as usual
    if (!studio.inView(at)) studio.pan(at - view / 2 - from);
    thumbDrag = { x: e.clientX, from: studio.viewFrom };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  /** @param {PointerEvent & { currentTarget: HTMLDivElement }} e */
  function onOverviewMove(e) {
    if (!thumbDrag) return;
    const r = e.currentTarget.getBoundingClientRect();
    studio.pan(thumbDrag.from + ((e.clientX - thumbDrag.x) / r.width) * span - studio.viewFrom);
  }

  // ---- pointer: scrub to seek; Shift-drag (or drag on the region) to mark A–B ----

  /** @type {{ kind: 'seek' | 'mark' | 'a' | 'b', from: number, at: number } | null} */
  let drag = $state(null);
  let hover = $state(/** @type {number | null} */ (null));

  /** @param {PointerEvent} e */
  function pointerMs(e) {
    const r = /** @type {SVGSVGElement} */ (svg).getBoundingClientRect();
    return msAt(e.clientX - r.left);
  }

  /** @param {PointerEvent & { currentTarget: SVGSVGElement }} e */
  function onDown(e) {
    if (e.button !== 0 || !player.active) return;
    const at = pointerMs(e);
    const r = studio.region;
    const near = (ms) => Math.abs(x(ms) - x(at)) <= HANDLE_PX;
    const kind = r && near(r.a) ? 'a' : r && near(r.b) ? 'b' : e.shiftKey ? 'mark' : 'seek';
    drag = { kind, from: at, at };
    e.currentTarget.setPointerCapture(e.pointerId);
  }

  /** @param {PointerEvent} e */
  function onMove(e) {
    const at = pointerMs(e);
    hover = at;
    if (!drag) return;
    drag.at = at;
    const r = studio.region;
    if (drag.kind === 'a' && r) studio.setRegion({ ...r, a: Math.min(at, r.b - 50) });
    if (drag.kind === 'b' && r) studio.setRegion({ ...r, b: Math.max(at, r.a + 50) });
  }

  function onUp() {
    if (!drag) return;
    const { kind, from, at } = drag;
    drag = null;
    if (kind === 'seek') studio.seekTo(at);
    if (kind === 'mark') studio.setRegion({ a: Math.min(from, at), b: Math.max(from, at), on: true });
  }

  let shownRegion = $derived(
    drag?.kind === 'mark' ? { a: Math.min(drag.from, drag.at), b: Math.max(drag.from, drag.at), on: true } : studio.region,
  );

  let status = $derived.by(() => {
    const l = studio.loop;
    if (l) {
      const intro = l.startMs < 50 ? 'no intro' : `intro ${fmt(l.startMs)}`;
      return `${intro} · loop ${fmt(l.endMs - l.startMs)}${l.manual ? ' (set by hand)' : ''}`;
    }
    const a = studio.analysis;
    if (!a) return 'Finding the loop…';
    if (a.kind === 'silence') return `Ends at ${fmt(a.atMs)}`;
    return 'No loop found. Set one with the loop buttons.';
  });
</script>

<section class="panel timeline" aria-label="Timeline">
  <header>
    <span class="pos" title="Position in the timeline">{fmt(drag?.kind === 'seek' ? drag.at : studio.position)}</span>
    {#if studio.loop && studio.pass > 0}
      <span class="pass" title="Pass through the loop">pass {studio.pass}</span>
    {/if}
    <span class="dim status">{status}</span>
    <span class="actions">
      <button class="link" title="Set the loop start at the playhead" disabled={!player.active} onclick={() => studio.setLoopPoint('start')}>
        Loop start
      </button>
      <button class="link" title="Set the loop end at the playhead" disabled={!player.active} onclick={() => studio.setLoopPoint('end')}>
        Loop end
      </button>
      {#if studio.loop?.manual}
        <button class="link" title="Use the loop found by analysis" onclick={() => studio.resetLoop()}>Reset loop</button>
      {/if}
      <span class="sep" aria-hidden="true"></span>
      {#if studio.region}
        <button
          class="link ab"
          aria-pressed={studio.region.on}
          title={studio.region.on ? 'A–B loop on: click to play straight through' : 'A–B loop off: click to repeat the region'}
          onclick={() => studio.toggleRegion()}
        >
          A–B {studio.region.on ? 'on' : 'off'}
        </button>
        <button class="link" title="Remove the A–B region" onclick={() => studio.setRegion(null)}>Clear</button>
      {:else}
        <span class="dim hint">Shift-drag to mark an A–B loop</span>
      {/if}
    </span>
  </header>

  <div class="ruler">
    <div class="measure" bind:clientWidth={width}></div>
    <svg
      bind:this={svg}
      {width}
      height={HEIGHT}
      role="slider"
      tabindex="-1"
      aria-label="Timeline position"
      aria-valuemin="0"
      aria-valuemax={Math.round(span)}
      aria-valuenow={Math.round(studio.position)}
      onpointerdown={onDown}
      onpointermove={onMove}
      onpointerup={onUp}
      onpointerleave={() => (hover = null)}
    >
      <!-- zoomed in, the timeline extends past both edges -->
      <clipPath id="timeline-clip"><rect y="-4" {width} height={HEIGHT + 8} /></clipPath>
      <pattern id="timeline-hatch" patternUnits="userSpaceOnUse" width="6" height="6" patternTransform="rotate(45)">
        <rect class="hatch-bg" width="6" height="6" />
        <line class="hatch-line" x1="0" y1="0" x2="0" y2="6" />
      </pattern>
      <g clip-path="url(#timeline-clip)">
      {#each ticks as t (t)}
        <line class="tick" x1={x(t)} x2={x(t)} y1="11" y2={BAND_Y} />
        {#if x(t) + 6 * tickLabel(t).length < width}<text class="label" x={x(t) + 3} y="10">{tickLabel(t)}</text>{/if}
      {/each}

      {#if studio.loop}
        {@const l = studio.loop}
        {@const introX = Math.max(0, x(0))}
        {@const loopX = Math.max(0, x(l.startMs))}
        <rect class="band intro" x={x(0)} y={BAND_Y} width={x(l.startMs) - x(0)} height={BAND_H} />
        <rect class="band loop" x={x(l.startMs)} y={BAND_Y} width={x(l.endMs) - x(l.startMs)} height={BAND_H} />
        {#if x(l.startMs) - introX > 40}<text class="band-label" x={introX + 6} y={BAND_Y + 17}>intro</text>{/if}
        {#if x(l.endMs) - loopX > 50}<text class="band-label" x={loopX + 6} y={BAND_Y + 17}>loop ⟲</text>{/if}
        <line class="loop-mark" x1={x(l.startMs)} x2={x(l.startMs)} y1={BAND_Y - 4} y2={BAND_Y + BAND_H + 4} />
      {:else}
        <rect class="band intro" x={x(0)} y={BAND_Y} width={x(knownEnd) - x(0)} height={BAND_H} />
      {/if}
      {#if span > knownEnd}
        <!-- the track's length is a guess, or playback has run past its end -->
        {@const restX = Math.max(0, x(knownEnd))}
        <rect class="band unknown" x={x(knownEnd)} y={BAND_Y} width={x(span) - x(knownEnd)} height={BAND_H} />
        {#if x(span) - restX > 110}
          <text class="band-label" x={restX + 6} y={BAND_Y + 17}>{knownEnd ? 'past the end' : 'length unknown'}</text>
        {/if}
      {/if}

      {#if shownRegion}
        {@const r = shownRegion}
        <rect class="region" class:off={!r.on} x={x(r.a)} y={BAND_Y - 6} width={Math.max(1, x(r.b) - x(r.a))} height={BAND_H + 12} />
        <text class="region-label" x={x(r.a) + 3} y={BAND_Y + BAND_H + 13}>A</text>
        <text class="region-label" x={x(r.b) - 9} y={BAND_Y + BAND_H + 13}>B</text>
      {/if}

      {#if hover !== null && !drag}
        <line class="hover" x1={x(hover)} x2={x(hover)} y1={BAND_Y} y2={BAND_Y + BAND_H} />
      {/if}
      </g>
      <!-- outside the clip, so these show whole at either edge -->
      {#if endMark && x(studio.end) >= 0 && x(studio.end) <= width}
        <line class="end-mark" x1={x(studio.end)} x2={x(studio.end)} y1={BAND_Y - 4} y2={BAND_Y + BAND_H + 4} />
      {/if}
      {#if player.active}
        {@const px = x(drag?.kind === 'seek' ? drag.at : studio.position)}
        {#if px >= 0 && px <= width}
          <line class="playhead" x1={px} x2={px} y1={BAND_Y - 6} y2={BAND_Y + BAND_H + 6} />
        {/if}
      {/if}
    </svg>
    <div class="overview-row">
      <div
        class="overview"
        class:zoomed
        role="scrollbar"
        tabindex="-1"
        aria-label="Visible part of the timeline"
        aria-orientation="horizontal"
        aria-controls="timeline-clip"
        aria-valuemin="0"
        aria-valuemax={Math.round(span - view)}
        aria-valuenow={Math.round(from)}
        onpointerdown={onOverviewDown}
        onpointermove={onOverviewMove}
        onpointerup={() => (thumbDrag = null)}
      >
        {#if studio.loop}
          <span class="o-band intro" style:left="0" style:width={pct(studio.loop.startMs)}></span>
          <span class="o-band loop" style:left={pct(studio.loop.startMs)} style:right="0"></span>
        {:else}
          <span class="o-band intro" style:left="0" style:width={pct(knownEnd)}></span>
        {/if}
        {#if span > knownEnd}
          <span class="o-band unknown" style:left={pct(knownEnd)} style:right="0"></span>
        {/if}
        {#if shownRegion}
          <span class="o-region" style:left={pct(shownRegion.a)} style:width={pct(shownRegion.b - shownRegion.a)}></span>
        {/if}
        {#if player.active}
          <span class="o-playhead" style:left={pct(studio.position)}></span>
        {/if}
        <span class="o-thumb" style:left={pct(from)} style:width={pct(view)}></span>
      </div>
      <span class="zoom">
        <button class="link" title="Zoom out (− or Ctrl+wheel)" disabled={!zoomed} onclick={() => studio.zoom(1 / 2)}>−</button>
        <button class="link" title="Zoom in (+ or Ctrl+wheel)" disabled={view <= 1000} onclick={() => studio.zoom(2)}>+</button>
        <button class="link" title="Show the whole timeline (0)" disabled={!zoomed} onclick={() => studio.fit()}>Fit</button>
      </span>
    </div>
  </div>
</section>

<style>
  .timeline {
    padding-bottom: 6px;
  }

  header {
    display: flex;
    align-items: baseline;
    gap: 10px;
    padding: 8px 14px 4px;
    min-width: 0;
  }

  .pos {
    font-variant-numeric: tabular-nums;
    font-weight: 650;
    font-size: 15px;
  }

  .pass {
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid color-mix(in srgb, var(--accent) 50%, var(--line));
    color: var(--accent);
    font-size: 11px;
  }

  .status {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 12px;
  }

  .actions {
    display: flex;
    align-items: baseline;
    gap: 12px;
    flex: none;
  }

  .actions .link:disabled {
    color: var(--dim);
    cursor: default;
    text-decoration: none;
  }

  .sep {
    width: 1px;
    align-self: stretch;
    background: var(--line);
  }

  .hint {
    font-size: 12px;
  }

  .ab[aria-pressed='false'] {
    color: var(--dim);
  }

  .ruler {
    padding: 0 14px;
  }

  .measure {
    height: 0;
  }

  svg {
    display: block;
    cursor: pointer;
    touch-action: none;
    overflow: visible;
  }

  .overview-row {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-top: 2px;
  }

  .overview {
    position: relative;
    flex: 1;
    height: 8px;
    border-radius: 3px;
    overflow: hidden;
    touch-action: none;
  }

  .overview.zoomed {
    cursor: grab;
  }

  .overview > span {
    position: absolute;
    top: 0;
    bottom: 0;
    pointer-events: none;
  }

  .o-band.intro {
    background: var(--panel-2);
  }

  .o-band.unknown {
    background: repeating-linear-gradient(45deg, var(--panel-2) 0 3px, var(--line) 3px 5px);
  }

  .o-band.loop {
    background: color-mix(in srgb, var(--accent) 18%, var(--panel));
  }

  .o-region {
    background: color-mix(in srgb, var(--note) 45%, transparent);
  }

  .o-playhead {
    width: 2px;
    margin-left: -1px;
    background: var(--text);
  }

  .o-thumb {
    box-sizing: border-box;
    border: 1.5px solid var(--dim);
    border-radius: 3px;
  }

  .overview:not(.zoomed) .o-thumb {
    border-color: transparent;
  }

  .zoom {
    display: flex;
    gap: 10px;
    flex: none;
  }

  .zoom .link {
    min-width: 12px;
    font-size: 13px;
    text-align: center;
  }

  .zoom .link:disabled {
    color: var(--dim);
    cursor: default;
    text-decoration: none;
  }

  .tick {
    stroke: var(--line);
  }

  .label {
    fill: var(--dim);
    font-size: 10px;
    font-variant-numeric: tabular-nums;
  }

  .band.intro {
    fill: var(--panel-2);
  }

  .band.loop {
    fill: color-mix(in srgb, var(--accent) 18%, var(--panel));
  }

  .band.unknown {
    fill: url(#timeline-hatch);
  }

  .hatch-bg {
    fill: var(--panel-2);
  }

  .hatch-line {
    stroke: var(--line);
    stroke-width: 2;
  }

  .end-mark {
    stroke: var(--dim);
    stroke-width: 2;
  }

  .band-label {
    fill: var(--dim);
    font-size: 11px;
    pointer-events: none;
  }

  .loop-mark {
    stroke: var(--accent);
    stroke-width: 2;
  }

  .region {
    fill: color-mix(in srgb, var(--note) 22%, transparent);
    stroke: var(--note);
    stroke-width: 1.5;
  }

  .region.off {
    fill: none;
    stroke-dasharray: 4 3;
  }

  .region-label {
    fill: var(--note);
    font-size: 10px;
    font-weight: 700;
    pointer-events: none;
  }

  .hover {
    stroke: var(--dim);
    stroke-dasharray: 2 2;
    pointer-events: none;
  }

  .playhead {
    stroke: var(--text);
    stroke-width: 2;
    pointer-events: none;
  }

  @media (max-width: 760px) {
    .hint {
      display: none;
    }
  }
</style>
