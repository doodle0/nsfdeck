<script>
  import { player, formatTime } from '../player.svelte.js';

  /** Keeps the current track visible as playback advances. */
  function scrollWhenCurrent(node, current) {
    const update = (c) => c && node.scrollIntoView({ block: 'nearest' });
    update(current);
    return { update };
  }
</script>

<section class="panel">
  <h2>
    <span>
      Tracks
      {#if player.file}<span class="dim">({player.file.tracks.length})</span>{/if}
    </span>
  </h2>
  <ol>
    {#each player.file?.tracks ?? [] as track, i (i)}
      {@const current = i === player.status.track}
      <li class:current use:scrollWhenCurrent={current}>
        <button onclick={() => player.play(i)} title={track.title}>
          <span class="num">{i + 1}</span>
          <span class="name">{track.title}</span>
          <span class="len">{track.lengthMs == null ? '' : formatTime(track.lengthMs)}</span>
        </button>
      </li>
    {/each}
  </ol>
</section>

<style>
  ol {
    flex: 1;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    overflow-y: auto;
  }

  li button {
    display: grid;
    grid-template-columns: 2.6em 1fr auto;
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 6px 14px;
    border: 0;
    background: none;
    text-align: left;
  }

  li button:hover {
    background: var(--panel-2);
  }

  .num {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
    text-align: right;
  }

  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .len {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .current button {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .current .num {
    color: var(--note);
    font-weight: 700;
  }

  .current .name {
    font-weight: 600;
  }
</style>
