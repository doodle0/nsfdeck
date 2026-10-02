<script>
  /**
   * @typedef {{ label: string, action: () => void, checked?: boolean, disabled?: boolean } |
   *           { separator: true } |
   *           { input: string, placeholder?: string, value?: string, submit: (value: string) => boolean }} MenuItem
   */

  /**
   * A small popup menu at a screen position. Items are buttons, separators, or an inline text
   * field (e.g. for typing a time).
   * @type {{ x: number, y: number, items: MenuItem[], onclose: () => void }}
   */
  let { x, y, items, onclose } = $props();

  let el = $state(/** @type {HTMLElement | undefined} */ (undefined));
  let pos = $state({ left: 0, top: 0 });

  // keep the menu inside the window
  $effect(() => {
    if (!el) return;
    const r = el.getBoundingClientRect();
    pos = {
      left: Math.max(4, Math.min(x, innerWidth - r.width - 4)),
      top: Math.max(4, Math.min(y, innerHeight - r.height - 4)),
    };
  });

  function run(action) {
    onclose();
    action();
  }

  /** @param {PointerEvent} e */
  function onWindowPointer(e) {
    if (el && !el.contains(/** @type {Node} */ (e.target))) onclose();
  }
</script>

<svelte:window
  onpointerdown={onWindowPointer}
  onkeydown={(e) => e.key === 'Escape' && onclose()}
  onblur={onclose}
  onresize={onclose}
/>

<div class="menu" role="menu" bind:this={el} style:left="{pos.left}px" style:top="{pos.top}px">
  {#each items as item, i (i)}
    {#if 'separator' in item}
      <hr />
    {:else if 'input' in item}
      <form
        class="input"
        onsubmit={(e) => {
          e.preventDefault();
          const field = /** @type {HTMLInputElement} */ (e.currentTarget.elements.namedItem('v'));
          if (item.submit(field.value)) onclose();
          else field.setCustomValidity('Use m:ss, e.g. 2:30'), field.reportValidity();
        }}
      >
        <label>
          {item.input}
          <input
            name="v"
            value={item.value ?? ''}
            placeholder={item.placeholder}
            autocomplete="off"
            oninput={(e) => e.currentTarget.setCustomValidity('')}
          />
        </label>
        <button type="submit">Set</button>
      </form>
    {:else}
      <button
        role="menuitemcheckbox"
        aria-checked={!!item.checked}
        disabled={item.disabled}
        onclick={() => run(item.action)}
      >
        <span class="check" aria-hidden="true">{item.checked ? '✓' : ''}</span>
        {item.label}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    z-index: 20;
    min-width: 180px;
    padding: 4px;
    background: var(--panel);
    border: 1px solid var(--line);
    border-radius: 8px;
    box-shadow: 0 8px 24px rgb(0 0 0 / 0.35);
  }

  button {
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 5px 10px 5px 4px;
    border: 0;
    border-radius: 5px;
    background: none;
    text-align: left;
  }

  button:hover:not(:disabled) {
    background: var(--panel-2);
  }

  button:disabled {
    color: var(--dim);
    cursor: default;
  }

  .check {
    width: 1.2em;
    text-align: center;
    color: var(--accent);
  }

  hr {
    margin: 4px 0;
    border: 0;
    border-top: 1px solid var(--line);
  }

  .input {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 4px 4px calc(1.2em + 10px);
  }

  .input label {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1;
  }

  .input input {
    width: 5em;
    padding: 3px 6px;
    border: 1px solid var(--line);
    border-radius: 4px;
    background: var(--bg);
    color: inherit;
    font: inherit;
  }

  .input button {
    width: auto;
    padding: 3px 10px;
    border: 1px solid var(--line);
  }
</style>
