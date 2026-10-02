<script>
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { player } from './lib/player.svelte.js';
  import NowPlaying from './lib/components/NowPlaying.svelte';
  import Transport from './lib/components/Transport.svelte';
  import TrackList from './lib/components/TrackList.svelte';
  import Channels from './lib/components/Channels.svelte';
  import DumpView from './lib/components/DumpView.svelte';
  import { MODES } from './lib/player.svelte.js';
  import { playlist } from './lib/playlist.svelte.js';
  import PlaylistView from './lib/components/PlaylistView.svelte';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import ErrorToast from './lib/components/ErrorToast.svelte';

  let dragging = $state(false);

  onMount(() => {
    player.setVolume(player.volume);
    player.applyMode();
    const stopSaving = playlist.autosave();
    playlist
      .restore()
      .then(() => invoke('initial_file'))
      .then((path) => path && playlist.add([path], { play: true }))
      .catch(() => {});

    const timer = setInterval(() => player.poll(), 100);
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths.length) playlist.add(payload.paths, { play: true }).catch(() => {});
    });
    return () => {
      clearInterval(timer);
      stopSaving();
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
    if ((e.ctrlKey || e.metaKey) && ['1', '2', '3'].includes(e.key)) {
      e.preventDefault();
      player.setMode(MODES[Number(e.key) - 1]);
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
<main class={player.mode}>
  {#if player.mode === 'listen'}
    <PlaylistView />
  {:else if player.mode === 'studio'}
    <Channels />
    <TrackList />
  {:else}
    <DumpView />
    <TrackList />
  {/if}
</main>

{#if (!player.file && !playlist.entries.length) || dragging}
  <DropOverlay highlight={dragging} />
{/if}
<ErrorToast />

<style>
  main {
    display: grid;
    grid-template-columns: 1fr 260px;
    gap: 12px;
    min-height: 0;
  }

  main.listen {
    grid-template-columns: 1fr;
  }

  @media (max-width: 640px) {
    main {
      grid-template-columns: 1fr 200px;
    }
  }
</style>
