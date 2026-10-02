<script>
  import { playlist, DEFAULT_SETTINGS } from '../playlist.svelte.js';
  import { formatTime, parseTime } from '../m3u.js';

  /** @type {{ onclose: () => void }} */
  let { onclose } = $props();

  let el = $state(/** @type {HTMLElement | undefined} */ (undefined));
  let s = $derived(playlist.settings);

  /** @param {Event & { currentTarget: HTMLInputElement }} e */
  function setPlayTime(e) {
    const ms = parseTime(e.currentTarget.value);
    if (ms && ms >= 1000) playlist.setSettings({ playMs: ms });
    else e.currentTarget.value = formatTime(s.playMs);
  }
</script>

<svelte:window
  onpointerdown={(e) => el && !el.contains(/** @type {Node} */ (e.target)) && onclose()}
  onkeydown={(e) => e.key === 'Escape' && onclose()}
/>

<div class="popover" bind:this={el} role="dialog" aria-label="Track length settings">
  <h3>Track length</h3>
  <p class="dim">Used for entries set to <em>Default</em>, unless the file gives a length.</p>

  <label class="check">
    <input
      type="checkbox"
      checked={s.detectLoops}
      onchange={(e) => playlist.setSettings({ detectLoops: e.currentTarget.checked })}
    />
    Find loops; play them
    <input
      class="num"
      type="number"
      min="1"
      max="9"
      value={s.loops}
      aria-label="Number of loops"
      onchange={(e) => playlist.setSettings({ loops: Math.min(9, Math.max(1, Number(e.currentTarget.value) || 1)) })}
    />
    ×
  </label>
  <label class="check">
    <input
      type="checkbox"
      checked={s.autoStop}
      onchange={(e) => playlist.setSettings({ autoStop: e.currentTarget.checked })}
    />
    End tracks that go silent
  </label>

  <div class="row">
    <label>
      Otherwise play for
      <input class="time" value={formatTime(s.playMs)} onchange={setPlayTime} aria-label="Default play time" />
    </label>
    <label>
      then fade out over
      <input
        class="num"
        type="number"
        min="0"
        max="60"
        value={s.fadeMs / 1000}
        aria-label="Fade time in seconds"
        onchange={(e) => playlist.setSettings({ fadeMs: Math.min(60, Math.max(0, Number(e.currentTarget.value) || 0)) * 1000 })}
      />
      s
    </label>
  </div>

  <button class="link" onclick={() => playlist.setSettings({ ...DEFAULT_SETTINGS })}>Reset to defaults</button>
</div>

<style>
  .popover {
    position: absolute;
    z-index: 15;
    top: 40px;
    right: 8px;
    width: 330px;
    padding: 12px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
    text-transform: none;
    letter-spacing: 0;
    font-weight: 400;
    font-size: 13px;
    color: var(--text);
  }

  h3 {
    margin: 0 0 2px;
    font-size: 13px;
  }

  p {
    margin: 0 0 10px;
    font-size: 12px;
  }

  label {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
  }

  .check {
    margin-bottom: 8px;
  }

  .row {
    display: grid;
    gap: 6px;
    margin: 4px 0 10px;
  }

  input:not([type='checkbox']) {
    padding: 3px 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--bg);
    color: inherit;
    font: inherit;
  }

  input[type='checkbox'] {
    accent-color: var(--accent);
    margin: 0;
  }

  .num {
    width: 3.5em;
  }

  .time {
    width: 5em;
  }
</style>
