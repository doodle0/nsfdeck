<script>
  import { player } from '../player.svelte.js';

  // Hide each error after a few seconds; a new error restarts the timer.
  $effect(() => {
    if (!player.error) return;
    const timer = setTimeout(() => (player.error = ''), 6000);
    return () => clearTimeout(timer);
  });
</script>

{#if player.error}
  <div role="alert">{player.error}</div>
{/if}

<style>
  div {
    position: fixed;
    left: 50%;
    bottom: 18px;
    transform: translateX(-50%);
    max-width: calc(100% - 32px);
    padding: 9px 14px;
    border-radius: 8px;
    background: var(--danger);
    color: #fff;
    font-weight: 500;
    box-shadow: 0 4px 18px rgb(0 0 0 / 0.3);
  }
</style>
