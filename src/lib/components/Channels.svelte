<script>
  import { player } from '../player.svelte.js';

  /** Channels grouped by sound chip, in file order. */
  let groups = $derived.by(() => {
    const map = new Map();
    for (const ch of player.file?.channels ?? []) {
      if (!map.has(ch.chip)) map.set(ch.chip, []);
      map.get(ch.chip).push(ch);
    }
    return [...map];
  });
</script>

<section class="panel">
  <h2>
    Channels
    {#if player.mask}
      <button class="link" onclick={() => player.setMask(0)}>Unmute all</button>
    {/if}
  </h2>
  <div class="groups">
    {#each groups as [chip, channels] (chip)}
      <div class="group">
        <h3>{chip}</h3>
        <div class="toggles">
          {#each channels as ch (ch.bit)}
            <button
              class="channel"
              title="{chip} {ch.name}"
              aria-pressed={!(player.mask & (1 << ch.bit))}
              onclick={() => player.toggleChannel(ch.bit)}
              oncontextmenu={(e) => {
                e.preventDefault();
                player.solo(ch.bit);
              }}
            >
              {ch.name}
            </button>
          {/each}
        </div>
      </div>
    {/each}
  </div>
  <p class="hint dim">Click to mute. Right-click to solo.</p>
</section>

<style>
  .groups {
    flex: 1;
    overflow-y: auto;
    padding: 6px 14px;
  }

  .group {
    margin: 6px 0 10px;
  }

  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--dim);
  }

  .toggles {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }

  .channel {
    padding: 4px 9px;
    border-radius: 999px;
    border: 1px solid color-mix(in srgb, var(--accent) 55%, var(--line));
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    font-size: 12px;
  }

  .channel[aria-pressed='false'] {
    border-color: var(--line);
    background: var(--muted-bg);
    color: var(--dim);
    text-decoration: line-through;
  }

  .hint {
    margin: 0;
    padding: 8px 14px 12px;
    font-size: 12px;
  }
</style>
