<script>
  import { player } from '../player.svelte.js';

  /** @type {{ size?: number, meter?: boolean }} */
  let { size = 48, meter = true } = $props();

  const uid = $props.id();

  // The label is the app icon's 5×5 grid: its arrow at rest, and while playing a meter with
  // one column per 2A03 channel (squares, triangle, noise, DPCM; bits 0-4), five steps tall.
  const IDLE = ['G....', 'GGG..', 'GGGGY', 'GGG..', 'G....'];
  const STEP = 22.8;

  /** Lit steps (0-5) per 2A03 channel, or null when nothing is playing. */
  let levels = $derived.by(() => {
    if (!meter || !player.active) return null;
    return [0, 1, 2, 3, 4].map((bit) => {
      const c = player.channelsNow[bit];
      if (!c || player.mask & (1 << bit) || c.volume <= 0) return 0;
      return Math.ceil((c.maxVolume > 0 ? Math.min(1, c.volume / c.maxVolume) : 1) * 5);
    });
  });

  /** Dot classes, row by row from the top. */
  let dots = $derived(
    IDLE.flatMap((row, r) =>
      [...row].map((ch, c) => {
        if (levels) return 4 - r < levels[c] ? (r === 0 ? 'peak' : 'on') : 'off';
        return ch === 'G' ? 'on' : ch === 'Y' ? 'peak' : 'off';
      }),
    ),
  );

  let expansion = $derived((player.file?.chips ?? []).some((chip) => chip !== '2A03'));
</script>

<svg width={(size * 200) / 252} height={size} viewBox="0 0 200 252" aria-hidden="true">
  <defs>
    <linearGradient id="{uid}-shell" x1="0" y1="0" x2="0" y2="1">
      <stop offset="0" class="shell-top" />
      <stop offset="1" class="shell-bottom" />
    </linearGradient>
  </defs>
  <path
    class="shell"
    fill="url(#{uid}-shell)"
    d="M24 2H176A22 22 0 0 1 198 24V238A12 12 0 0 1 186 250H14A12 12 0 0 1 2 238V24A22 22 0 0 1 24 2Z"
  />
  <rect class="slot" x="74" y="14" width="52" height="6" rx="3" />
  <rect class="window" x="25.5" y="31.5" width="149" height="149" rx="14.5" />
  {#each dots as cls, i (i)}
    <circle class={cls} cx={54.4 + (i % 5) * STEP} cy={60.4 + Math.floor(i / 5) * STEP} r="8.4" />
  {/each}
  {#if expansion}
    <rect class="stripe" x="24" y="192" width="152" height="7" rx="3.5" />
  {/if}
  {#each { length: 9 }, i (i)}
    <rect class="pin" x={44.5 + i * 13} y="214" width="7" height="22" rx="2" />
  {/each}
</svg>

<style>
  svg {
    flex: none;
    display: block;
  }

  .shell-top {
    stop-color: var(--cart-shell-top);
  }

  .shell-bottom {
    stop-color: var(--cart-shell-bottom);
  }

  .shell {
    stroke: var(--cart-line);
    stroke-width: 4;
  }

  .slot {
    fill: var(--cart-line);
  }

  .window {
    fill: var(--cart-window);
    stroke: var(--cart-line);
    stroke-width: 3;
  }

  .on {
    fill: var(--accent);
  }

  .peak {
    fill: var(--note);
  }

  .off {
    fill: var(--cart-off);
  }

  .stripe {
    fill: var(--duty-0);
  }

  .pin {
    fill: var(--cart-pin);
  }
</style>
