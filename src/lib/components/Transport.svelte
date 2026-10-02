<script>
  import { player, formatTime } from '../player.svelte.js';
  import { playlist } from '../playlist.svelte.js';

  const SPEED_PRESETS = [0.5, 0.75, 1, 1.25, 1.5, 2];
  const REPEAT_TITLES = { off: 'Repeat: off', all: 'Repeat: whole playlist', one: 'Repeat: this track' };

  // Position the user is dragging the seek bar to (0-1000), or null when not dragging.
  let dragging = $state(/** @type {number | null} */ (null));

  let range = $derived(player.seekRangeMs);
  let progress = $derived(
    player.active && range > 0 ? Math.min(1000, (player.status.elapsedMs / range) * 1000) : 0,
  );
  let shownElapsed = $derived(
    dragging !== null ? (dragging / 1000) * range : player.active ? player.status.elapsedMs : 0,
  );
  let lastTrack = $derived((player.file?.tracks.length ?? 0) - 1);
  // previous/next walk the playlist in Listen mode and the file's tracks otherwise
  let canPrev = $derived(player.endless ? !!player.file && player.status.track > 0 : playlist.entries.length > 0);
  let canNext = $derived(player.endless ? !!player.file && player.status.track < lastTrack : playlist.entries.length > 0);

  async function commitSeek() {
    const ms = (dragging / 1000) * range;
    try {
      await player.seek(ms);
    } finally {
      dragging = null;
    }
  }
</script>

<section aria-label="Playback">
  <div class="buttons">
    <button
      class="icon"
      title="Previous track (↑)"
      aria-label="Previous track"
      disabled={!canPrev}
      onclick={() => player.step(-1)}
    >
      <svg viewBox="0 0 24 24"><path d="M6 5h2v14H6zM20 5v14L9 12z" /></svg>
    </button>
    <button
      class="icon primary"
      title="Play / pause (Space)"
      aria-label={player.playing ? 'Pause' : 'Play'}
      onclick={() => player.togglePlay()}
    >
      {#if player.playing}
        <svg viewBox="0 0 24 24"><path d="M6 4h4v16H6zM14 4h4v16h-4z" /></svg>
      {:else}
        <svg viewBox="0 0 24 24"><path d="M7 4v16l13-8z" /></svg>
      {/if}
    </button>
    <button class="icon" title="Stop" aria-label="Stop" disabled={!player.active} onclick={() => player.stop()}>
      <svg viewBox="0 0 24 24"><path d="M6 6h12v12H6z" /></svg>
    </button>
    <button
      class="icon"
      title="Next track (↓)"
      aria-label="Next track"
      disabled={!canNext}
      onclick={() => player.step(1)}
    >
      <svg viewBox="0 0 24 24"><path d="M16 5h2v14h-2zM4 5v14l11-7z" /></svg>
    </button>
  </div>

  {#if player.mode !== 'studio'}
    <span class="time">{formatTime(shownElapsed)}</span>
    <input
      class="seek"
      type="range"
      min="0"
      max="1000"
      step="1"
      aria-label="Seek"
      disabled={!player.active}
      value={dragging ?? progress}
      oninput={(e) => (dragging = Number(e.currentTarget.value))}
      onchange={commitSeek}
    />
    {#if player.endless}
      <span class="time infinite" title="Loops endlessly">∞</span>
    {:else}
      <span class="time">{formatTime(player.lengthMs)}</span>
    {/if}
  {:else}
    <span class="studio-note dim">Loops endlessly · use the timeline to seek</span>
  {/if}

  <!-- wraps onto its own row in narrow windows -->
  <div class="extras">
    {#if !player.endless}
      <button
        class="icon toggle"
        title={playlist.shuffle ? 'Shuffle: on' : 'Shuffle: off'}
        aria-label="Shuffle"
        aria-pressed={playlist.shuffle}
        onclick={() => playlist.toggleShuffle()}
      >
        <svg viewBox="0 0 24 24">
          <path d="M4 7h3.5l9 10H20M4 17h3.5l2.4-2.7M14.1 9.7 16.5 7H20M17.5 4.5 20 7l-2.5 2.5M17.5 14.5 20 17l-2.5 2.5" />
        </svg>
      </button>
      <button
        class="icon toggle"
        title={REPEAT_TITLES[playlist.repeat]}
        aria-label="Repeat"
        aria-pressed={playlist.repeat !== 'off'}
        onclick={() => playlist.cycleRepeat()}
      >
        <svg viewBox="0 0 24 24">
          <path d="M5 12V9a2 2 0 0 1 2-2h11M15.5 4.5 18 7l-2.5 2.5M19 12v3a2 2 0 0 1-2 2H6M8.5 19.5 6 17l2.5-2.5" />
        </svg>
        {#if playlist.repeat === 'one'}<span class="one">1</span>{/if}
      </button>
    {/if}
    {#if player.endless}
      <label class="speed" title="Speed: changes tempo, not pitch. Double-click to reset.">
        <span>{player.speed.toFixed(2)}×</span>
        <input
          type="range"
          min="0.25"
          max="2"
          step="0.05"
          aria-label="Speed"
          list="speed-presets"
          value={player.speed}
          oninput={(e) => player.setSpeed(Number(e.currentTarget.value))}
          ondblclick={() => player.setSpeed(1)}
        />
      </label>
      <datalist id="speed-presets">
        {#each SPEED_PRESETS as p (p)}<option value={p}></option>{/each}
      </datalist>
    {/if}

    <label class="volume" title="Volume">
      <svg viewBox="0 0 24 24" aria-hidden="true">
        <path d="M4 9h4l5-4v14l-5-4H4z" />
        <path class="wave" d="M16 8.5a5 5 0 0 1 0 7M18.5 6a8.5 8.5 0 0 1 0 12" />
      </svg>
      <input
        type="range"
        min="0"
        max="100"
        aria-label="Volume"
        value={player.volume}
        oninput={(e) => player.setVolume(Number(e.currentTarget.value))}
      />
    </label>
  </div>
</section>

<style>
  section {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
    padding: 10px 14px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 10px;
  }

  .buttons {
    display: flex;
    gap: 4px;
  }

  .icon {
    display: grid;
    place-items: center;
    width: 34px;
    height: 34px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
  }

  .icon svg {
    width: 18px;
    height: 18px;
    fill: currentColor;
  }

  .icon:hover {
    background: var(--panel-2);
  }

  .icon:disabled {
    opacity: 0.35;
    cursor: default;
    background: transparent;
  }

  .icon.toggle {
    position: relative;
    color: var(--dim);
  }

  .icon.toggle svg {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .icon.toggle[aria-pressed='true'] {
    color: var(--accent);
  }

  .one {
    position: absolute;
    right: 4px;
    bottom: 3px;
    font-size: 9px;
    font-weight: 700;
  }

  .icon.primary {
    width: 40px;
    height: 40px;
    background: var(--accent);
    color: var(--accent-ink);
  }

  .icon.primary:hover {
    filter: brightness(1.08);
  }

  .time {
    font-variant-numeric: tabular-nums;
    color: var(--dim);
    min-width: 3.2em;
    text-align: center;
  }

  .studio-note {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .infinite {
    font-size: 18px;
    line-height: 1;
  }

  .seek {
    flex: 1;
    min-width: 120px;
  }

  .extras {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-left: auto;
  }

  .speed {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .speed span {
    min-width: 3.2em;
    text-align: right;
  }

  .speed input {
    width: 90px;
  }

  .volume {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--dim);
  }

  .volume svg {
    width: 18px;
    height: 18px;
    fill: currentColor;
  }

  .volume .wave {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
  }

  .volume input {
    width: 90px;
  }
</style>
