<script>
  import { player } from '../player.svelte.js';
  import { mixer, UNITY, CENTRE } from '../mixer.svelte.js';

  /** Channels grouped by sound chip, in file order. */
  let groups = $derived.by(() => {
    const map = new Map();
    for (const ch of player.file?.channels ?? []) {
      if (!map.has(ch.chip)) map.set(ch.chip, []);
      map.get(ch.chip).push(ch);
    }
    return [...map];
  });

  let allBits = $derived((player.file?.channels ?? []).reduce((m, ch) => m | (1 << ch.bit), 0) >>> 0);
  const muted = (bit) => !!(player.mask & (1 << bit));
  const soloed = (bit) => player.mask === (allBits & ~(1 << bit)) >>> 0 && player.mask !== 0;

  // channels without a pitch get a level meter instead of a keyboard (DPCM: on while a sample plays)
  const UNPITCHED = new Set([3, 4, 8]); // noise, DPCM, MMC5 PCM

  // timbre colours: pulse duty (2A03 and MMC5 squares) and the noise's metallic mode
  const PULSES = new Set([0, 1, 6, 7]);
  const DUTY = ['12.5%', '25%', '50%', '75%'];
  /** Colour class and description of what a channel's tone value means. */
  function timbre(bit, tone) {
    if (PULSES.has(bit)) return { cls: `duty-${tone & 3}`, label: `duty ${DUTY[tone & 3]}` };
    if (bit === 3) return tone ? { cls: 'metal', label: 'metallic noise' } : { cls: '', label: 'noise' };
    return { cls: '', label: '' };
  }
  let hasPulses = $derived((player.file?.channels ?? []).some((ch) => PULSES.has(ch.bit)));

  // keyboard strip: MIDI C1..B7
  const LOW = 24;
  const KEYS = 84;
  const BLACK = new Set([1, 3, 6, 8, 10]);
  const NAMES = ['C', 'C♯', 'D', 'D♯', 'E', 'F', 'F♯', 'G', 'G♯', 'A', 'A♯', 'B'];

  /** The note a channel plays now, or null when silent. */
  function now(bit) {
    const c = player.channelsNow[bit];
    if (!c || !player.active || muted(bit) || c.volume <= 0) return null;
    const level = c.maxVolume > 0 ? Math.min(1, c.volume / c.maxVolume) : 1;
    const midi = c.freqHz > 1 ? Math.round(69 + 12 * Math.log2(c.freqHz / 440)) : null;
    return { level, midi, key: c.key, ...timbre(bit, c.tone) };
  }

  const noteName = (midi) => `${NAMES[midi % 12]}${Math.floor(midi / 12) - 1}`;
  const pct = (v) => `${Math.round((v / UNITY) * 100)}%`;
  const panLabel = (p) => (p === CENTRE ? 'centre' : p < CENTRE ? `${Math.round(((CENTRE - p) / CENTRE) * 100)}% left` : `${Math.round(((p - CENTRE) / 127) * 100)}% right`);
</script>

<section class="panel">
  <h2>
    Mixer
    <span class="actions">
      {#if player.mask}
        <button class="link" onclick={() => player.setMask(0)}>Unmute all</button>
      {/if}
      {#if !mixer.isDefault}
        <button class="link" onclick={() => mixer.reset()}>Reset levels</button>
      {/if}
    </span>
  </h2>
  <div class="groups">
    {#if groups.length}
      <div class="row head dim" aria-hidden="true">
        <span></span><span></span><span></span><span>Volume</span><span>Pan</span><span>Now playing</span><span></span>
      </div>
    {/if}
    {#each groups as [chip, channels] (chip)}
      <div class="group">
        <div class="chip">
          <h3>{chip}</h3>
          <input
            type="range"
            min="0"
            max="256"
            step="4"
            aria-label="{chip} volume"
            title="{chip} volume {pct(mixer.chipVolume(chip))} (double-click to reset)"
            value={mixer.chipVolume(chip)}
            oninput={(e) => mixer.setChipVolume(chip, Number(e.currentTarget.value))}
            ondblclick={() => mixer.setChipVolume(chip, UNITY)}
          />
          <span class="dim value">{pct(mixer.chipVolume(chip))}</span>
        </div>
        {#each channels as ch (ch.bit)}
          {@const c = mixer.channel(ch.bit)}
          {@const n = now(ch.bit)}
          <div class="row" class:silent={muted(ch.bit)}>
            <span class="name" title="{chip} {ch.name}">{ch.name}</span>
            <button
              class="toggle"
              aria-pressed={muted(ch.bit)}
              title="Mute"
              onclick={() => player.toggleChannel(ch.bit)}>M</button
            >
            <button class="toggle solo" aria-pressed={soloed(ch.bit)} title="Solo" onclick={() => player.solo(ch.bit)}
              >S</button
            >
            <input
              class="vol"
              type="range"
              min="0"
              max="256"
              step="4"
              aria-label="{ch.name} volume"
              title="Volume {pct(c.vol)} (double-click to reset)"
              value={c.vol}
              oninput={(e) => mixer.setChannel(ch.bit, { vol: Number(e.currentTarget.value) })}
              ondblclick={() => mixer.setChannel(ch.bit, { vol: UNITY })}
            />
            <input
              class="pan"
              type="range"
              min="0"
              max="255"
              step="1"
              aria-label="{ch.name} pan"
              title="Pan {panLabel(c.pan)} (double-click to centre)"
              value={c.pan}
              oninput={(e) => mixer.setChannel(ch.bit, { pan: Number(e.currentTarget.value) })}
              ondblclick={() => mixer.setChannel(ch.bit, { pan: CENTRE })}
            />
            <svg class="keys" viewBox="0 0 {KEYS} 10" preserveAspectRatio="none" aria-hidden="true">
              {#if n?.label}<title>{n.label}</title>{/if}
              {#if UNPITCHED.has(ch.bit)}
                <rect class="meter-bg" x="0" y="2" width={KEYS} height="6" />
                {#if n}<rect class="meter {n.cls}" x="0" y="2" width={KEYS * n.level} height="6" />{/if}
              {:else}
                {#each { length: KEYS } as _, i (i)}
                  {#if BLACK.has((LOW + i) % 12)}<rect class="black" x={i} y="0" width="1" height="10" />{/if}
                  {#if (LOW + i) % 12 === 0}<rect class="octave" x={i} y="0" width="0.08" height="10" />{/if}
                {/each}
                {#if n?.midi != null && n.midi >= LOW && n.midi < LOW + KEYS}
                  <rect class="on {n.cls}" x={n.midi - LOW - 0.2} y="0" width="1.4" height="10" opacity={0.35 + 0.65 * n.level} />
                {/if}
              {/if}
            </svg>
            <span class="note">{n?.midi != null && !UNPITCHED.has(ch.bit) ? noteName(n.midi) : ''}</span>
          </div>
        {/each}
      </div>
    {/each}
  </div>
  <p class="hint dim">
    M mutes, S solos. Double-click a slider to reset it.
    {#if hasPulses}
      <span class="legend">
        Duty
        {#each DUTY as d, i (d)}<span class="swatch duty-{i}"></span>{d}{/each}
        <span class="swatch metal"></span>metallic noise
      </span>
    {/if}
  </p>
</section>

<style>
  .panel {
    container-type: inline-size;
  }

  .actions {
    display: flex;
    gap: 14px;
  }

  .groups {
    flex: 1;
    overflow-y: auto;
    padding: 4px 14px;
  }

  .group {
    margin: 6px 0 10px;
  }

  .chip {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 4px;
  }

  h3 {
    margin: 0;
    min-width: 5.5em;
    font-size: 12px;
    font-weight: 650;
    color: var(--dim);
  }

  .chip input[type='range'] {
    width: 160px;
    flex: none;
  }

  .value {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .row {
    display: grid;
    grid-template-columns: 66px 22px 22px 90px 56px 1fr 32px;
    gap: 6px;
    align-items: center;
    padding: 2px 0;
  }

  .head {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    padding: 4px 0 0;
  }

  .name {
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .silent .name,
  .silent .note {
    color: var(--dim);
    text-decoration: line-through;
  }

  .toggle {
    width: 22px;
    height: 20px;
    padding: 0;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--panel);
    font-size: 11px;
    font-weight: 650;
    color: var(--dim);
  }

  .toggle[aria-pressed='true'] {
    background: var(--danger);
    border-color: var(--danger);
    color: #fff;
  }

  .toggle.solo[aria-pressed='true'] {
    background: var(--note);
    border-color: var(--note);
    color: #000;
  }

  input[type='range'] {
    min-width: 0;
    width: 100%;
  }

  .keys {
    width: 100%;
    height: 14px;
    border: 1px solid var(--line);
    border-radius: 3px;
    background: var(--panel-2);
  }

  .black {
    fill: var(--muted-bg);
  }

  .octave {
    fill: var(--line);
  }

  .on {
    fill: var(--accent);
  }

  .meter-bg {
    fill: transparent;
  }

  .meter {
    fill: var(--accent);
    opacity: 0.8;
  }

  .on.duty-0,
  .meter.duty-0,
  .swatch.duty-0 {
    fill: var(--duty-0);
    background: var(--duty-0);
  }

  .on.duty-1,
  .meter.duty-1,
  .swatch.duty-1 {
    fill: var(--duty-1);
    background: var(--duty-1);
  }

  .swatch.duty-2 {
    background: var(--duty-2);
  }

  .on.duty-3,
  .meter.duty-3,
  .swatch.duty-3 {
    fill: var(--duty-3);
    background: var(--duty-3);
  }

  .meter.metal,
  .swatch.metal {
    fill: var(--metal);
    background: var(--metal);
  }

  .legend {
    display: inline-flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px;
    margin-left: 12px;
  }

  .swatch {
    display: inline-block;
    width: 9px;
    height: 9px;
    margin-left: 6px;
    border-radius: 2px;
  }

  .note {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    color: var(--note);
  }

  .hint {
    margin: 0;
    padding: 8px 14px 10px;
    font-size: 12px;
    border-top: 1px solid var(--line);
  }

  /* narrow panels: the keyboard goes on its own line */
  @container (max-width: 420px) {
    .head {
      display: none;
    }

    .row {
      grid-template-columns: 66px 22px 22px 1fr 48px;
    }

    .keys {
      grid-column: 1 / 5;
    }

    .note {
      grid-column: 5;
    }
  }
</style>
