<script>
  import { player, formatTime } from '../player.svelte.js';

  // Position the user is dragging the seek bar to (0-1000), or null when not dragging.
  let dragging = $state(/** @type {number | null} */ (null));

  let progress = $derived(
    player.active && player.lengthMs > 0 ? Math.min(1000, (player.status.elapsedMs / player.lengthMs) * 1000) : 0,
  );
  let shownElapsed = $derived(
    dragging !== null ? (dragging / 1000) * player.lengthMs : player.active ? player.status.elapsedMs : 0,
  );
  let lastTrack = $derived((player.file?.tracks.length ?? 0) - 1);

  async function commitSeek() {
    const ms = (dragging / 1000) * player.lengthMs;
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
      disabled={!player.file || player.status.track <= 0}
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
      disabled={!player.file || player.status.track >= lastTrack}
      onclick={() => player.step(1)}
    >
      <svg viewBox="0 0 24 24"><path d="M16 5h2v14h-2zM4 5v14l11-7z" /></svg>
    </button>
  </div>

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
  <span class="time">{formatTime(player.lengthMs)}</span>

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
</section>

<style>
  section {
    display: flex;
    align-items: center;
    gap: 12px;
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

  .seek {
    flex: 1;
    min-width: 80px;
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
