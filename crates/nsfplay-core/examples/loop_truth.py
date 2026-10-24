"""Ground truth for loop detection, from the audio. See docs/loop-detection.md.

Takes raw mono 16-bit 48 kHz PCM, one file per channel, written by
`cargo run --release -p nsfplay-core --example analyze -- FILE TRACK --solo BIT --raw OUT`.
Leave out the noise channel (bit 3): it is random, so its spectrum never repeats.

  uv run --with numpy python -I loop_truth.py start PERIOD_MS FROM_MS TO_MS RAW...
      Per channel, the earliest time in [FROM, TO] from which the audio keeps repeating one
      PERIOD later. The loop starts once every channel repeats: the latest of them.
      Reads ~0-300 ms early (half the windows of a 500 ms block may fail).

  uv run --with numpy python -I loop_truth.py period FROM_S TO_S RAW...
      The best repeat periods (0.5-60 s) over [FROM, TO), to catch a loop found at a
      multiple of its real length.

  uv run --with numpy python -I loop_truth.py compare START_MS PERIOD_MS RAW...
      Per channel, how well the audio repeats at PERIOD versus 2 * PERIOD: a correct
      half-length loop repeats about as well at both.

Similarity is the L1 difference of magnitude spectra (20 ms windows), so oscillator phase does
not matter. Lags must be sample-exact: rounding a lag to the 10 ms hop made correct loops look
4 ms off and score badly.
"""
import sys

import numpy as np

RATE = 48_000
N = 960  # 20 ms window
HOP = 480


def load(path):
    return np.fromfile(path, dtype="<i2").astype(np.float64)


def spectra(x, start_sample, count):
    idx = start_sample + np.arange(N)[None, :] + HOP * np.arange(count)[:, None]
    return np.abs(np.fft.rfft(x[idx] * np.hanning(N), axis=1))


def diff(x, at_ms, lag_ms, length_ms):
    """L1 spectral difference of [at, at+length) against lag_ms later; 0 when both are silent."""
    count = length_ms * RATE // 1000 // HOP
    a, b = at_ms * RATE // 1000, (at_ms + lag_ms) * RATE // 1000
    if b + count * HOP + N > len(x):
        return None
    s1, s2 = spectra(x, a, count), spectra(x, b, count)
    total = s1.sum() + s2.sum()
    return 0.0 if total < 1e3 * N * count else float(np.abs(s1 - s2).sum() / total)


def start(period, lo, hi, raws):
    found = []
    for raw in raws:
        x = load(raw)
        cache = {}

        def sim(t):
            if t not in cache:
                d = min((diff(x, t, period + j, 50) for j in (-1, 0, 1)), key=lambda v: 1 if v is None else v)
                cache[t] = 0.0 if d is None else 1 - d
            return cache[t]

        def repeats_from(t):
            blocks = range(t, hi + 3000, 500)
            return all(np.median([sim(u) for u in range(b, b + 500, 50)]) >= 0.8 for b in blocks)

        hit = next((t for t in range(max(lo, 0), hi + 1, 50) if repeats_from(t)), None)
        found.append(hit)
        print(f"{raw}: repeats from {hit} ms")
    print("loop start:", None if None in found else max(found), "ms")


def period(lo_s, hi_s, raws):
    for raw in raws:
        x = load(raw)
        n = (len(x) - N) // HOP
        spec = spectra(x, 0, n)
        fa, fb = int(lo_s * 100), int(hi_s * 100)
        scores = []
        for lag in range(50, 6000):
            if fb + lag > n:
                break
            s1, s2 = spec[fa:fb], spec[fa + lag:fb + lag]
            scores.append((np.abs(s1 - s2).sum() / (s1.sum() + s2.sum() + 1e-9), lag))
        best = ", ".join(f"{lag / 100:.2f} s ({d:.3f})" for d, lag in sorted(scores)[:6])
        print(f"{raw}: best periods (difference): {best}")


def compare(st, q, raws):
    for raw in raws:
        x = load(raw)

        def per_second(lag):
            out = []
            for t in range(st + 500, st + 2 * q - 1000, 1000):
                d = diff(x, t, lag, 1000)
                if d is None:
                    break
                out.append(d)
            return np.array(out)

        a, b = per_second(q), per_second(2 * q)
        k = min(len(a), len(b))
        print(f"{raw}: at {q} ms median {np.median(a):.3f}, at {2 * q} ms median {np.median(b):.3f}, "
              f"worst excess {np.max(a[:k] - b[:k]):+.3f}")


if __name__ == "__main__":
    mode, rest = sys.argv[1], sys.argv[2:]
    if mode == "start":
        start(int(rest[0]), int(rest[1]), int(rest[2]), rest[3:])
    elif mode == "period":
        period(float(rest[0]), float(rest[1]), rest[2:])
    elif mode == "compare":
        compare(int(rest[0]), int(rest[1]), rest[2:])
    else:
        sys.exit(__doc__)
