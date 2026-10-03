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
  import Timeline from './lib/components/Timeline.svelte';
  import { studio, FRAME_MS } from './lib/studio.svelte.js';
  import DropOverlay from './lib/components/DropOverlay.svelte';
  import ErrorToast from './lib/components/ErrorToast.svelte';

  let dragging = $state(false);

  onMount(() => {
    player.setVolume(player.volume);
    player.applyMode();
    const stopSaving = playlist.autosave();
    const stopStudio = studio.connect();
    playlist
      .restore()
      .then(() => invoke('initial_file'))
      .then((path) => path && playlist.add([path], { play: true }))
      .catch(() => {});

    const unsubscribe = player.subscribe();
    const unlisten = getCurrentWebview().onDragDropEvent(({ payload }) => {
      dragging = payload.type === 'enter' || payload.type === 'over';
      if (payload.type === 'drop' && payload.paths.length) playlist.add(payload.paths, { play: true }).catch(() => {});
    });
    return () => {
      unsubscribe();
      stopSaving();
      stopStudio();
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
    // Studio steps through its timeline: 1 s, or one frame while paused
    const nudge = player.status.state === 'paused' ? FRAME_MS : 1000;
    const studioMode = player.mode === 'studio';
    const actions = {
      ' ': () => player.togglePlay(),
      ArrowUp: () => player.step(-1),
      ArrowDown: () => player.step(1),
      ArrowLeft: () => (studioMode ? studio.step(-nudge) : player.seek(player.status.elapsedMs - 5000)),
      ArrowRight: () => (studioMode ? studio.step(nudge) : player.seek(player.status.elapsedMs + 5000)),
    };
    if (studioMode && !e.ctrlKey && !e.metaKey) {
      actions['+'] = actions['='] = () => studio.zoom(2);
      actions['-'] = () => studio.zoom(1 / 2);
      actions['0'] = () => studio.fit();
    }
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
    <div class="wide"><Timeline /></div>
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

  main.studio {
    grid-template-rows: auto 1fr;
  }

  .wide {
    grid-column: 1 / -1;
  }

  @media (max-width: 640px) {
    main {
      grid-template-columns: 1fr 200px;
    }
  }
</style>
