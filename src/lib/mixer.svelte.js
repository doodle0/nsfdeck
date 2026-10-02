// The mixer: volume per chip and per channel, and pan per channel. Values use the core's
// scale, where 128 is 100% (volume) or centre (pan, 0 = left, 255 = right). Settings apply
// to every file and are saved with the playlist.

import { player } from './player.svelte.js';

export const UNITY = 128;
export const CENTRE = 128;

/** The core's devices behind each chip; the 2A03 is two (pulses, and the rest). */
export const CHIP_DEVICES = {
  '2A03': ['APU1', 'APU2'],
  FDS: ['FDS'],
  MMC5: ['MMC5'],
  '5B': ['5B'],
  VRC6: ['VRC6'],
  VRC7: ['VRC7'],
  N163: ['N163'],
};

const pad = (bit) => String(bit).padStart(2, '0');

class Mixer {
  /** Device volume by device name. @type {Record<string, number>} */
  devices = $state({});
  /** Per-channel volume and pan by channel bit. @type {Record<number, { vol: number, pan: number }>} */
  channels = $state({});

  isDefault = $derived(
    Object.values(this.devices).every((v) => v === UNITY) &&
      Object.values(this.channels).every((c) => c.vol === UNITY && c.pan === CENTRE),
  );

  chipVolume(chip) {
    return this.devices[CHIP_DEVICES[chip]?.[0]] ?? UNITY;
  }

  channel(bit) {
    return this.channels[bit] ?? { vol: UNITY, pan: CENTRE };
  }

  setChipVolume(chip, vol) {
    const v = Math.round(vol);
    for (const d of CHIP_DEVICES[chip] ?? []) this.devices[d] = v;
    this.#send((CHIP_DEVICES[chip] ?? []).map((d) => [`${d}_VOLUME`, v]));
  }

  setChannel(bit, patch) {
    const c = { ...this.channel(bit), ...patch };
    c.vol = Math.round(c.vol);
    c.pan = Math.round(c.pan);
    this.channels[bit] = c;
    this.#send([
      [`CHANNEL_${pad(bit)}_VOL`, c.vol],
      [`CHANNEL_${pad(bit)}_PAN`, c.pan],
    ]);
  }

  reset() {
    const values = [
      ...Object.keys(this.devices).map((d) => [`${d}_VOLUME`, UNITY]),
      ...Object.keys(this.channels).flatMap((b) => [
        [`CHANNEL_${pad(b)}_VOL`, UNITY],
        [`CHANNEL_${pad(b)}_PAN`, CENTRE],
      ]),
    ];
    this.devices = {};
    this.channels = {};
    this.#send(values);
  }

  /** Sends every setting, e.g. after restoring them at startup. */
  applyAll() {
    this.#send([
      ...Object.entries(this.devices).map(([d, v]) => [`${d}_VOLUME`, v]),
      ...Object.entries(this.channels).flatMap(([b, c]) => [
        [`CHANNEL_${pad(b)}_VOL`, c.vol],
        [`CHANNEL_${pad(b)}_PAN`, c.pan],
      ]),
    ]);
  }

  #send(values) {
    if (values.length) player.call('set_config', { values }).catch(() => {});
  }

  snapshot() {
    return { devices: this.devices, channels: this.channels };
  }

  restore(saved) {
    this.devices = { ...saved?.devices };
    this.channels = { ...saved?.channels };
    this.applyAll();
  }
}

export const mixer = new Mixer();
