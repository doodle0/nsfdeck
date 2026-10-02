<script>
  import { invoke } from '@tauri-apps/api/core';
  import { player } from '../player.svelte.js';

  let text = $state('');
  let frozen = $state(false);
  let copied = $state(false);

  async function refresh() {
    try {
      text = await invoke('dump');
    } catch {
      // backend errors were already reported by the command that hit them
    }
  }

  // Refresh a few times per second while playing; once when paused, stopped or a file loads.
  $effect(() => {
    if (frozen || !player.file) return;
    refresh();
    if (!player.playing) return;
    const timer = setInterval(refresh, 250);
    return () => clearInterval(timer);
  });

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 1200);
    } catch (e) {
      player.error = `Could not copy: ${e}`;
    }
  }
</script>

<section class="panel">
  <h2>
    Emulator state
    <span class="actions">
      <button class="link" aria-pressed={frozen} onclick={() => (frozen = !frozen)}>
        {frozen ? 'Resume' : 'Freeze'}
      </button>
      <button class="link" onclick={copy} disabled={!text}>{copied ? 'Copied' : 'Copy'}</button>
    </span>
  </h2>
  {#if text}
    <pre>{text}</pre>
  {:else}
    <p class="dim empty">Load a file to see the emulator state.</p>
  {/if}
</section>

<style>
  .actions {
    display: flex;
    gap: 14px;
  }

  pre {
    flex: 1;
    margin: 0;
    padding: 10px 14px;
    overflow: auto;
    font: 12px/1.45 ui-monospace, 'SF Mono', 'Cascadia Mono', 'DejaVu Sans Mono', monospace;
    user-select: text;
    tab-size: 4;
  }

  .empty {
    margin: 0;
    padding: 14px;
  }
</style>
