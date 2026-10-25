#!/usr/bin/env python3
"""Synthesises the arcOS system sounds.

"avvio" is the chime played when the desktop appears: three soft bell
notes rising (an add9 arpeggio on D), each a few sine partials with a quick
attack and an exponential decay, plus a little room reverb.

Output: <folder>/avvio.wav (22050 Hz, 16-bit mono).

Usage: tools/assets/gen_sounds.py [output folder]
"""
import os
import sys
import wave

import numpy as np

RATE = 22050


def bell(freq, length):
    t = np.arange(int(RATE * length)) / RATE
    tone = np.zeros_like(t)
    for mult, amp, decay in ((1.0, 1.0, 2.2), (2.0, 0.35, 3.5), (3.0, 0.12, 5.0), (4.2, 0.05, 7.0)):
        tone += amp * np.sin(2 * np.pi * freq * mult * t) * np.exp(-decay * t)
    attack = np.minimum(t / 0.012, 1.0)
    return tone * attack


def chime():
    length = 1.9
    out = np.zeros(int(RATE * length))
    # D5, A5, E6 (D add9): bright but calm.
    for start, freq, gain in ((0.0, 587.33, 0.55), (0.14, 880.0, 0.45), (0.28, 1318.5, 0.38)):
        b = bell(freq, length - start) * gain
        i = int(start * RATE)
        out[i:i + len(b)] += b
    # A few early reflections for some space.
    room = out.copy()
    for delay, amp in ((0.031, 0.25), (0.057, 0.18), (0.089, 0.12), (0.131, 0.08)):
        d = int(delay * RATE)
        room[d:] += out[:-d] * amp
    room /= np.max(np.abs(room))
    fade = np.minimum(np.arange(len(room))[::-1] / (0.25 * RATE), 1.0)
    return (room * fade * 0.85 * 32767).astype("<i2")


def save(path, samples):
    with wave.open(path, "wb") as w:
        w.setnchannels(1)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(samples.tobytes())


def main():
    out = sys.argv[1] if len(sys.argv) > 1 else "rootfs/system/sounds"
    os.makedirs(out, exist_ok=True)
    save(os.path.join(out, "avvio.wav"), chime())


if __name__ == "__main__":
    main()
