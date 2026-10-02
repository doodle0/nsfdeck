<script>
  import { player } from '../player.svelte.js';
  import ModeSwitch from './ModeSwitch.svelte';

  let file = $derived(player.file);
  let title = $derived(file ? file.title || file.path.split(/[\\/]/).pop() : 'No file loaded');
  let credits = $derived(
    file ? [file.copyright, file.ripper && `ripped by ${file.ripper}`].filter(Boolean).join(' · ') : '',
  );
</script>

<header>
  <img class="cart" src="/cart.png" alt="" width="48" height="48" />
  <div class="meta">
    <h1>{title}</h1>
    <p>{file?.artist ?? ''}</p>
    <p class="dim">{credits}</p>
  </div>
  {#if file}
    <ul class="chips" aria-label="Sound chips">
      {#each file.chips as chip (chip)}
        <li>{chip}</li>
      {/each}
    </ul>
  {/if}
  <ModeSwitch />
  <button class="open" title="Open file (Ctrl+O)" onclick={() => player.browse()}>Open…</button>
</header>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 14px;
    min-width: 0;
  }

  .cart {
    image-rendering: pixelated;
    flex: none;
  }

  .meta {
    min-width: 0;
    flex: 1;
  }

  h1 {
    margin: 0;
    font-size: 20px;
    font-weight: 650;
  }

  h1,
  p {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }

  p {
    margin: 0;
  }

  .chips {
    display: flex;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .chips li {
    padding: 2px 8px;
    border-radius: 4px;
    background: var(--panel-2);
    border: 1px solid var(--line);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.03em;
  }

  .open {
    padding: 7px 14px;
    border-radius: 6px;
    border: 1px solid var(--line);
    background: var(--panel);
  }

  .open:hover {
    background: var(--panel-2);
  }

  @media (max-width: 640px) {
    .chips {
      display: none;
    }
  }
</style>
