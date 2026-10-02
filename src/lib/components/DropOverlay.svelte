<script>
  import { player } from '../player.svelte.js';

  /** @type {{ highlight: boolean }} */
  let { highlight } = $props();
</script>

<div class="overlay">
  <div class="box" class:highlight>
    <img src="/cart.png" alt="" width="64" height="64" />
    <p class="title">Drop NSF or NSFe files or folders</p>
    <p class="dim">or <button class="link" onclick={() => player.browse()}>browse for files</button></p>
  </div>
</div>

<style>
  .overlay {
    position: fixed;
    inset: 0;
    display: grid;
    place-items: center;
    background: color-mix(in srgb, var(--bg) 92%, transparent);
  }

  .box {
    display: grid;
    justify-items: center;
    gap: 4px;
    padding: 36px 56px;
    border: 2px dashed var(--line);
    border-radius: 16px;
    text-align: center;
  }

  .box.highlight {
    border-color: var(--accent);
  }

  img {
    image-rendering: pixelated;
    margin-bottom: 8px;
  }

  p {
    margin: 0;
  }

  .title {
    font-size: 17px;
    font-weight: 600;
  }

  .link {
    font-size: inherit;
  }
</style>
