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
  const x = (ms) => (ms / span) * width;
  const msAt = (px) => Math.max(0, Math.min(span, (px / width) * span));

  /** m:ss.t */
  function fmt(ms) {
    const t = Math.max(0, Math.round(ms / 100));
    return `${Math.floor(t / 600)}:${String(Math.floor(t / 10) % 60).padStart(2, '0')}.${t % 10}`;
  }

  /** Ruler ticks at a step that leaves room for their labels. */
  let ticks = $derived.by(() => {
    const step = [1, 2, 5, 10, 15, 30, 60, 120, 300].map((s) => s * 1000).find((s) => x(s) >= 56) ?? 600_000;
    return Array.from({ length: Math.floor(span / step) + 1 }, (_, i) => i * step);
  });

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
      {#each ticks as t (t)}
        <line class="tick" x1={x(t)} x2={x(t)} y1="11" y2={BAND_Y} />
        <text class="label" x={x(t) + 3} y="10">{fmt(t).replace(/\.\d$/, '')}</text>
      {/each}

      {#if studio.loop}
        {@const l = studio.loop}
        <rect class="band intro" x="0" y={BAND_Y} width={x(l.startMs)} height={BAND_H} />
        <rect class="band loop" x={x(l.startMs)} y={BAND_Y} width={x(l.endMs) - x(l.startMs)} height={BAND_H} />
        {#if x(l.startMs) > 40}<text class="band-label" x="6" y={BAND_Y + 17}>intro</text>{/if}
        <text class="band-label" x={x(l.startMs) + 6} y={BAND_Y + 17}>loop ⟲</text>
        <line class="loop-mark" x1={x(l.startMs)} x2={x(l.startMs)} y1={BAND_Y - 4} y2={BAND_Y + BAND_H + 4} />
      {:else}
        <rect class="band unknown" x="0" y={BAND_Y} {width} height={BAND_H} />
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
      {#if player.active}
        {@const px = x(drag?.kind === 'seek' ? drag.at : studio.position)}
        <line class="playhead" x1={px} x2={px} y1={BAND_Y - 6} y2={BAND_Y + BAND_H + 6} />
      {/if}
    </svg>
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
    fill: var(--panel-2);
    opacity: 0.6;
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
