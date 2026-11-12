"""Synthesize a realistic 'Suno-like' problem song for end-to-end testing:
drums with transients, bass, muddy mids, harsh resonant lead, sibilant bursts, persistent
metallic resonances, hiss bed, 16 kHz codec-style cutoff, and hard clipping on loud hits."""
import sys
from pathlib import Path

import numpy as np
import soundfile as sf
from scipy import signal

sr = 48000
dur = 48.0
rng = np.random.default_rng(7)
n = int(sr * dur)
t = np.arange(n) / sr
L = np.zeros(n); R = np.zeros(n)
bpm = 100; beat = 60 / bpm


def add(buf, start, sig, g=1.0):
    s = int(start * sr)
    e = min(n, s + sig.size)
    if s < n:
        buf[s:e] += g * sig[: e - s]


def env(len_s, a=0.002, d=0.2):
    k = np.arange(int(len_s * sr)) / sr
    return np.minimum(k / a, 1) * np.exp(-k / d)


kick_t = np.arange(int(0.4 * sr)) / sr
kick = np.sin(2 * np.pi * (50 * kick_t + 60 * (1 - np.exp(-kick_t * 30)) / 30)) * env(0.4, 0.001, 0.12)
snare = (rng.standard_normal(int(0.25 * sr)) * 0.6 + np.sin(2 * np.pi * 190 * np.arange(int(0.25 * sr)) / sr)) * env(0.25, 0.001, 0.07)
hat_n = signal.sosfilt(signal.butter(4, 7000, "highpass", fs=sr, output="sos"), rng.standard_normal(int(0.06 * sr)))
hat = hat_n * env(0.06, 0.0005, 0.02)
intro = 2.0
nb = int((dur - intro) / beat)
for b in range(nb):
    tb = intro + b * beat
    add(L, tb, kick, 0.9); add(R, tb, kick, 0.9)
    if b % 2 == 1:
        add(L, tb, snare, 0.45); add(R, tb, snare, 0.42)
    for h in range(2):
        add(L, tb + h * beat / 2, hat, 0.18); add(R, tb + h * beat / 2, hat, 0.24)
# bass line
notes = [41.2, 41.2, 49.0, 36.7]
bass = np.zeros(n)
for b in range(nb):
    f0 = notes[(b // 4) % 4]
    seg = np.arange(int(beat * sr)) / sr
    saw = signal.sawtooth(2 * np.pi * f0 * seg)
    bass_seg = saw * env(beat, 0.005, 0.5)
    s0 = int((intro + b * beat) * sr)
    e0 = min(n, s0 + seg.size)
    bass[s0:e0] += bass_seg[: e0 - s0]
bass = signal.sosfilt(signal.butter(4, 300, fs=sr, output="sos"), bass) * 0.35
L += bass; R += bass
# muddy pad (chords, heavy 200-450 Hz), slightly wide
pad = np.zeros((n, 2))
for f in (196, 247, 294, 392, 330):
    ph = rng.random(2) * 2 * np.pi
    pad[:, 0] += np.sin(2 * np.pi * f * t + ph[0]) + 0.3 * np.sin(2 * np.pi * 2 * f * t)
    pad[:, 1] += np.sin(2 * np.pi * f * 1.003 * t + ph[1]) + 0.3 * np.sin(2 * np.pi * 2 * f * t)
pad *= 0.05 * (t >= intro)[:, None]
pad = signal.sosfilt(signal.butter(2, [150, 600], "bandpass", fs=sr, output="sos"), pad, axis=0) * 2.2
L += pad[:, 0]; R += pad[:, 1]
# harsh lead: square-ish with 3.2 kHz resonance, comes and goes
lead = signal.square(2 * np.pi * 440 * t) * 0.05
lead = signal.sosfilt(signal.butter(2, 1000, "highpass", fs=sr, output="sos"), lead)
lead += signal.lfilter(*signal.iirpeak(3200, 6, fs=sr), lead) * 1.5
gate = (np.sin(2 * np.pi * t / 6) > 0.3) & (t > intro)
lead *= gate
L += lead * 0.9; R += lead * 1.1
# vocal-like melody (formant-filtered saw, 300 Hz-3 kHz), centred
mel = [220, 247, 262, 294, 330, 294, 262, 247]
voc = np.zeros(n)
for b in range(nb):
    seg = np.arange(int(beat * sr)) / sr
    vib = 1 + 0.004 * np.sin(2 * np.pi * 5.5 * seg)
    s = signal.sawtooth(2 * np.pi * mel[b % 8] * np.cumsum(vib) / sr) * env(beat, 0.03, 1.2)
    s0 = int((intro + b * beat) * sr); e0 = min(n, s0 + seg.size)
    voc[s0:e0] += s[: e0 - s0]
voc = sum(signal.lfilter(*signal.iirpeak(f, 4, fs=sr), voc) * g for f, g in ((700, 1.0), (1200, 0.8), (2500, 0.4)))
L += voc * 0.12; R += voc * 0.12
# metallic persistent resonances (Suno fingerprint) and sibilant bursts
for f in (3650, 4870):
    tone = signal.lfilter(*signal.iirpeak(f, 40, fs=sr), rng.standard_normal(n)) * 0.06 * (t > intro)
    L += tone; R += tone * 0.9
for b in range(nb):
    if b % 3 == 0:
        s = signal.sosfilt(signal.butter(4, [5500, 9000], "bandpass", fs=sr, output="sos"), rng.standard_normal(int(0.09 * sr)))
        s *= env(0.09, 0.01, 0.04)
        add(L, intro + b * beat + beat / 4, s, 0.5); add(R, intro + b * beat + beat / 4, s, 0.5)
# HF swirl: decorrelated shimmer 9-15 kHz
sh = signal.sosfilt(signal.butter(4, [9000, 15000], "bandpass", fs=sr, output="sos"), rng.standard_normal((n, 2)), axis=0)
sh *= (0.012 * (1 + 0.8 * np.sin(2 * np.pi * 3.1 * t)))[:, None] * (t > intro)[:, None]
L += sh[:, 0]; R += sh[:, 1]
x = np.stack([L, R], 1)
# 16 kHz codec cutoff
x = signal.sosfiltfilt(signal.ellip(8, 0.1, 80, 16000, fs=sr, output="sos"), x, axis=0)
# hiss bed
x += rng.standard_normal(x.shape) * 10 ** (-58 / 20)
# drive and hard clip at full scale
x *= 1.0 / np.percentile(np.abs(x), 99.9) * 1.05
x = np.clip(x, -1.0, 1.0)
out = Path(sys.argv[1] if len(sys.argv) > 1 else Path(__file__).parent / "test_song.wav")
sf.write(str(out), x, sr, subtype="PCM_24")
print("wrote", out, x.shape)
