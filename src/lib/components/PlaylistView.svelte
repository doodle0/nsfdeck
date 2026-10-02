<script>
  import { player } from '../player.svelte.js';
  import { playlist } from '../playlist.svelte.js';
  import { formatTime, parseTime } from '../m3u.js';
  import Menu from './Menu.svelte';
  import DurationSettings from './DurationSettings.svelte';

  /** @type {{ x: number, y: number, items: any[] } | null} */
  let menu = $state(null);
  let settingsOpen = $state(false);

  const BADGE = {
    file: ['file', 'Length from the file'],
    auto: ['auto', ''],
    custom: ['custom', 'Length set for this entry'],
    default: ['default', 'Default length: no loop or silence found'],
    pending: ['…', 'Finding the loop…'],
  };

  /** @param {import('../playlist.svelte.js').Entry} entry */
  function badge(entry, resolved) {
    const [text, title] = BADGE[resolved.source];
    if (resolved.source !== 'auto') return { text, title };
    const a = playlist.analyses[`${entry.track}:${entry.path}`];
    const loops = playlist.settings.loops;
    return {
      text,
      title:
        a?.kind === 'loop'
          ? `Loop found: intro, then the loop ${loops} ${loops === 1 ? 'time' : 'times'}, then a fade`
          : 'Ends when the track goes silent',
    };
  }

  // ---- reordering by dragging rows (pointer events: HTML5 drag-and-drop is taken by file drops) ----

  /** @type {{ from: number, to: number, startY: number, rowH: number, moved: boolean } | null} */
  let drag = $state(null);

  /** @param {PointerEvent & { currentTarget: HTMLElement }} e */
  function onRowPointerDown(e, index) {
    if (e.button !== 0) return;
    drag = { from: index, to: index, startY: e.clientY, rowH: e.currentTarget.offsetHeight, moved: false };
  }

  /** @param {PointerEvent & { currentTarget: HTMLElement }} e */
  function onRowPointerMove(e) {
    if (!drag || !(e.buttons & 1)) return;
    const dy = e.clientY - drag.startY;
    if (!drag.moved && Math.abs(dy) < 5) return;
    // Capture only once dragging: capturing on pointerdown would retarget the click and
    // dblclick events to the row, so the row's button would never see them.
    if (!drag.moved) e.currentTarget.setPointerCapture(e.pointerId);
    drag.moved = true;
    drag.to = Math.max(0, Math.min(playlist.entries.length - 1, drag.from + Math.round(dy / drag.rowH)));
  }

  function onRowPointerUp() {
    if (drag?.moved) playlist.move(drag.from, drag.to);
    // let the click that follows see `moved` so it does not select
    setTimeout(() => (drag = null));
  }

  // ---- menus ----

  /** @param {MouseEvent} e @param {import('../playlist.svelte.js').Entry} entry */
  function openRowMenu(e, entry) {
    e.preventDefault();
    playlist.selectedId = entry.id;
    const d = entry.duration;
    const loops = (n) => ({
      label: `${n} ${n === 1 ? 'loop' : 'loops'}`,
      checked: d.mode === 'loops' && d.loops === n,
      action: () => playlist.setDuration(entry.id, { mode: 'loops', loops: n }),
    });
    menu = {
      x: e.clientX,
      y: e.clientY,
      items: [
        { label: 'Play', action: () => playlist.playId(entry.id) },
        { separator: true },
        {
          label: `Default length (${playlist.settings.loops} ${playlist.settings.loops === 1 ? 'loop' : 'loops'} if found)`,
          checked: d.mode === 'auto',
          action: () => playlist.setDuration(entry.id, { mode: 'auto' }),
        },
        loops(1),
        loops(2),
        loops(3),
        loops(4),
        {
          input: 'Fixed',
          placeholder: 'm:ss',
          value: d.mode === 'fixed' ? formatTime(d.playMs) : '',
          submit: (v) => {
            const ms = parseTime(v);
            if (!ms) return false;
            playlist.setDuration(entry.id, { mode: 'fixed', playMs: ms });
            return true;
          },
        },
        {
          label: 'Endless',
          checked: d.mode === 'endless',
          action: () => playlist.setDuration(entry.id, { mode: 'endless' }),
        },
        { separator: true },
        { label: 'Remove', action: () => playlist.remove([entry.id]) },
      ],
    };
  }

  /** @param {MouseEvent & { currentTarget: HTMLElement }} e */
  function openListMenu(e) {
    const r = e.currentTarget.getBoundingClientRect();
    const empty = !playlist.entries.length;
    menu = {
      x: r.right - 200,
      y: r.bottom + 4,
      items: [
        { label: 'Add files…', action: () => playlist.browseFiles() },
        { label: 'Add folder…', action: () => playlist.browseFolder() },
        { separator: true },
        { label: 'Import M3U playlist…', action: () => playlist.importM3u().catch(() => {}) },
        { label: 'Export M3U playlist…', disabled: empty, action: () => playlist.exportM3u().catch(() => {}) },
        { separator: true },
        { label: 'Clear playlist', disabled: empty, action: () => playlist.clear() },
      ],
    };
  }

  /** @param {KeyboardEvent} e */
  function onListKey(e) {
    const id = playlist.selectedId;
    if (id == null) return;
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      const at = playlist.entries.findIndex((x) => x.id === id);
      playlist.remove([id]);
      playlist.selectedId = playlist.entries[Math.min(at, playlist.entries.length - 1)]?.id ?? null;
    } else if (e.key === 'Enter') {
      e.preventDefault();
      playlist.playId(id);
    }
  }

  let mutedCount = $derived((player.file?.channels ?? []).filter((ch) => player.mask & (1 << ch.bit)).length);

  /** Keeps the current entry visible as playback advances. */
  function scrollWhenCurrent(node, current) {
    const update = (c) => c && node.scrollIntoView({ block: 'nearest' });
    update(current);
    return { update };
  }
</script>

<section class="panel">
  <h2>
    <span>
      Playlist
      {#if playlist.entries.length}<span class="dim">({playlist.entries.length})</span>{/if}
      {#if playlist.adding}<span class="dim">· adding…</span>{/if}
    </span>
    <span class="actions">
      {#if player.mask}
        <button class="link" title="Channels muted in Studio mode" onclick={() => player.setMask(0)}>
          {mutedCount} muted · Unmute all
        </button>
      {/if}
      <button class="link" aria-expanded={settingsOpen} onclick={() => (settingsOpen = !settingsOpen)}>Lengths</button>
      <button class="link" onclick={() => playlist.browseFiles()}>Add…</button>
      <button class="link more" aria-label="More playlist actions" onclick={openListMenu}>⋯</button>
    </span>
    {#if settingsOpen}
      <DurationSettings onclose={() => (settingsOpen = false)} />
    {/if}
  </h2>

  {#if playlist.entries.length}
    <!-- Delete/Enter act on the selected row; the rows themselves are buttons -->
    <!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions -->
    <ol tabindex="0" onkeydown={onListKey} aria-label="Playlist">
      {#each playlist.entries as entry, i (entry.id)}
        {@const resolved = playlist.resolve(entry)}
        {@const b = badge(entry, resolved)}
        {@const current = entry.id === playlist.currentId}
        <li
          class:current
          class:playing={current && player.active && !player.endless}
          class:selected={entry.id === playlist.selectedId}
          class:dragging={drag?.moved && drag.from === i}
          class:drop-before={drag?.moved && drag.to === i && drag.to < drag.from}
          class:drop-after={drag?.moved && drag.to === i && drag.to > drag.from}
          use:scrollWhenCurrent={current}
          onpointerdown={(e) => onRowPointerDown(e, i)}
          onpointermove={onRowPointerMove}
          onpointerup={onRowPointerUp}
          onpointercancel={() => (drag = null)}
          oncontextmenu={(e) => openRowMenu(e, entry)}
        >
          <button
            onclick={() => !drag?.moved && (playlist.selectedId = entry.id)}
            ondblclick={() => playlist.playId(entry.id)}
            title="{entry.title} — {entry.game}"
          >
            <span class="num">{i + 1}</span>
            <span class="name">
              <span class="title">{entry.title}</span>
              <span class="sub">{entry.game} · #{entry.track + 1}</span>
            </span>
            <span class="len">{resolved.totalMs == null ? '∞' : formatTime(resolved.totalMs)}</span>
            <span class="badge {resolved.source}" title={b.title}>{b.text}</span>
          </button>
        </li>
      {/each}
    </ol>
  {:else}
    <div class="empty">
      <p>The playlist is empty.</p>
      <p class="dim">
        Drop NSF files or folders here, or
        <button class="link" onclick={() => playlist.browseFiles()}>add files</button>
        ·
        <button class="link" onclick={() => playlist.browseFolder()}>add a folder</button>
      </p>
    </div>
  {/if}
  <p class="hint dim">Double-click to play · drag to reorder · right-click for length and more</p>
</section>

{#if menu}
  <Menu {...menu} onclose={() => (menu = null)} />
{/if}

<style>
  h2 {
    position: relative;
  }

  .actions {
    display: flex;
    gap: 14px;
    align-items: baseline;
  }

  .more {
    font-size: 16px;
    line-height: 1;
  }

  ol {
    flex: 1;
    margin: 0;
    padding: 4px 0;
    list-style: none;
    overflow-y: auto;
    outline: none;
  }

  li {
    touch-action: none;
  }

  li button {
    display: grid;
    grid-template-columns: 2.6em 1fr auto 4.2em;
    gap: 8px;
    align-items: center;
    width: 100%;
    padding: 5px 14px;
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
    display: flex;
    flex-direction: column;
    min-width: 0;
  }

  .title,
  .sub {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .sub {
    font-size: 12px;
    color: var(--dim);
  }

  .len {
    color: var(--dim);
    font-variant-numeric: tabular-nums;
  }

  .badge {
    justify-self: start;
    padding: 0 6px;
    border-radius: 4px;
    border: 1px solid var(--line);
    font-size: 11px;
    color: var(--dim);
  }

  .badge.auto {
    border-color: color-mix(in srgb, var(--accent) 50%, var(--line));
    color: var(--accent);
  }

  .badge.custom {
    border-color: color-mix(in srgb, var(--note) 50%, var(--line));
    color: var(--note);
  }

  .selected button {
    background: var(--panel-2);
  }

  .current button {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
  }

  .current .num {
    color: var(--note);
    font-weight: 700;
  }

  .current .title {
    font-weight: 600;
  }

  .playing .num::before {
    content: '▶ ';
    font-size: 10px;
  }

  .dragging {
    opacity: 0.5;
  }

  .drop-before {
    box-shadow: inset 0 2px 0 var(--accent);
  }

  .drop-after {
    box-shadow: inset 0 -2px 0 var(--accent);
  }

  .empty {
    flex: 1;
    display: grid;
    place-content: center;
    text-align: center;
    padding: 20px;
  }

  .empty p {
    margin: 2px 0;
  }

  .hint {
    margin: 0;
    padding: 8px 14px 10px;
    font-size: 12px;
    border-top: 1px solid var(--line);
  }
</style>
