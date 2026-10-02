<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { player } from './lib/player.svelte.js';
  import NowPlaying from './lib/components/NowPlaying.svelte';
  import Transport from './lib/components/Transport.svelte';
  import TrackList from './lib/components/TrackList.svelte';
  import Channels from './lib/components/Channels.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import ErrorToast from './lib/components/ErrorToast.svelte';

  let dragging = $state(false);

  onMount(() => {
    player.setVolume(player.volume);
    invoke('initial_file').then((path) => path && player.load(path).catch(() => {}));

    const timer = setInterval(() => player.poll(), 100);
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths.length) player.load(payload.paths[0]).catch(() => {});
    });
    return () => {
      clearInterval(timer);
      unlisten.then((f) => f());
    };
  });

  /** @param {KeyboardEvent} e */
  function onkeydown(e) {
    if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'o') {
      e.preventDefault();
      player.browse();
      return;
    }
    if (e.target instanceof HTMLInputElement && e.key !== ' ') return;
    const actions = {
      ' ': () => player.togglePlay(),
      ArrowUp: () => player.step(-1),
      ArrowDown: () => player.step(1),
      ArrowLeft: () => player.seek(player.status.elapsedMs - 5000),
      ArrowRight: () => player.seek(player.status.elapsedMs + 5000),
    };
    if (actions[e.key]) {
      e.preventDefault();
      actions[e.key]();
    }
  }
</script>

<svelte:window {onkeydown} />

<NowPlaying />
<Transport />
<main>
  <TrackList />
  <Channels />
</main>

{#if !player.file || dragging}
  <DropOverlay highlight={dragging} />
{/if}
<ErrorToast />

<style>
  main {
    display: grid;
    grid-template-columns: 1fr 250px;
    gap: 12px;
    min-height: 0;
  }

  @media (max-width: 640px) {
    main {
      grid-template-columns: 1fr 200px;
    }
  }
</style>
