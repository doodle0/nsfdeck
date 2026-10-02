<script>
  import { player, MODES } from '../player.svelte.js';

  const LABELS = { listen: 'Listen', studio: 'Studio', developer: 'Developer' };
</script>

<div class="modes" role="radiogroup" aria-label="Mode">
  {#each MODES as mode, i (mode)}
    <button
      role="radio"
      aria-checked={player.mode === mode}
      title="{LABELS[mode]} (Ctrl+{i + 1})"
      onclick={() => player.setMode(mode)}
    >
      {LABELS[mode]}
    </button>
  {/each}
</div>

<style>
  .modes {
    display: flex;
    padding: 2px;
    border: 1px solid var(--line);
    border-radius: 8px;
    background: var(--panel);
  }

  button {
    padding: 5px 12px;
    border: 0;
    border-radius: 6px;
    background: none;
    color: var(--dim);
  }

  button:hover {
    color: var(--text);
  }

  button[aria-checked='true'] {
    background: var(--panel-2);
    color: var(--text);
    font-weight: 600;
  }
</style>
