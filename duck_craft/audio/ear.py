"""The ear: listens to .wav files and says what's in them, in numbers,
plain words and a picture, for whoever made them and can't hear them.

    python3 audio/ear.py assets/music_title.wav        # one file
    python3 audio/ear.py assets                        # every .wav in a folder, and a table
    python3 audio/ear.py --loop assets/wind.wav        # also check the loop's seam
    python3 audio/ear.py --compare a.wav b.wav         # how two files differ

Run in duck_craft/. Pictures go to out/ear/<name>.png: the waveform, the
loudness over time, a spectrogram (what pitches, when) and a chromagram
(which notes). The report says:

- how loud (peak, RMS, loudness in LUFS as streaming services measure it,
  true peak), and whether anything clips;
- clicks (a sample that jumps far more than its neighbours), dropouts
  (silence cutting into sound), DC offset, silence at the ends;
- for loops, whether the end runs into the start without a click or a
  jump in level;
- the tone: how the sound's energy splits between sub, low, low-mid,
  mid, presence and air, with "muddy", "harsh", "hissy" or "thin" when a
  share is far from usual;
- notes and key (from the chromagram), the pitch of a voice-like sound
  and where it goes, and tempo and onsets (from the spectral flux);
- stereo: how alike the ears are, how wide, and whether either side is
  louder;
- the loudness second by second, as a row of bars you can read.

Only numpy, scipy and matplotlib.
"""

import argparse
import sys
import wave
from pathlib import Path

import numpy as np
from scipy.signal import resample_poly, stft

BARS = " ▁▂▃▄▅▆▇█"
NOTES = ["C", "C#", "D", "Eb", "E", "F", "F#", "G", "Ab", "A", "Bb", "B"]
# Krumhansl-Schmuckler key profiles.
MAJOR = np.array([6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88])
MINOR = np.array([6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17])
BANDS = [("sub", 20, 60), ("low", 60, 250), ("low-mid", 250, 1000), ("mid", 1000, 4000),
         ("presence", 4000, 8000), ("air", 8000, 20000)]


# ---- Reading ----

def read(path):
    """Samples as floats (-1 to 1), shape (n, channels), and the rate."""
    with wave.open(str(path)) as w:
        channels, width, rate, n = w.getnchannels(), w.getsampwidth(), w.getframerate(), w.getnframes()
        raw = w.readframes(n)
    if width == 2:
        x = np.frombuffer(raw, dtype="<i2").astype(np.float64) / 32768
    elif width == 1:
        x = (np.frombuffer(raw, dtype=np.uint8).astype(np.float64) - 128) / 128
    elif width == 3:
        b = np.frombuffer(raw, dtype=np.uint8).reshape(-1, 3).astype(np.int32)
        v = b[:, 0] | (b[:, 1] << 8) | (b[:, 2] << 16)
        v = np.where(v >= 1 << 23, v - (1 << 24), v)
        x = v.astype(np.float64) / (1 << 23)
    elif width == 4:
        x = np.frombuffer(raw, dtype="<i4").astype(np.float64) / 2 ** 31
    else:
        raise ValueError(f"{width * 8}-bit samples")
    return x.reshape(-1, channels), rate


def db(v, floor=-120.0):
    return max(floor, 20 * np.log10(max(v, 1e-12)))


# ---- Loudness (ITU-R BS.1770, as LUFS meters measure it) ----

def k_weighted(x, rate):
    """The K-weighting curve: a shelf up 4 dB above ~1.5 kHz (the head),
    and a high-pass at ~38 Hz (we hear little lower)."""
    from scipy.signal import lfilter
    f0, g, q = 1681.974450955533, 3.999843853973347, 0.7071752369554196
    k = np.tan(np.pi * f0 / rate)
    vh = 10 ** (g / 20)
    vb = vh ** 0.4996667741545416
    a0 = 1 + k / q + k * k
    b_shelf = [(vh + vb * k / q + k * k) / a0, 2 * (k * k - vh) / a0, (vh - vb * k / q + k * k) / a0]
    a_shelf = [1, 2 * (k * k - 1) / a0, (1 - k / q + k * k) / a0]
    f0, q = 38.13547087602444, 0.5003270373238773
    k = np.tan(np.pi * f0 / rate)
    a0 = 1 + k / q + k * k
    b_hp = [1, -2, 1]
    a_hp = [1, 2 * (k * k - 1) / a0, (1 - k / q + k * k) / a0]
    y = lfilter(b_shelf, a_shelf, x, axis=0)
    return lfilter(b_hp, a_hp, y, axis=0)


def loudness(x, rate):
    """Integrated loudness (LUFS, gated), and short-term loudness (3 s
    windows) every second."""
    y = k_weighted(x, rate)
    power = (y ** 2).sum(axis=1)  # each ear counts the same
    block, step = int(0.4 * rate), int(0.1 * rate)
    if len(power) < block:
        return -70.0, []
    starts = range(0, len(power) - block + 1, step)
    z = np.array([power[s:s + block].mean() for s in starts])
    lk = -0.691 + 10 * np.log10(np.maximum(z, 1e-12))
    gated = z[lk > -70]
    if len(gated) == 0:
        return -70.0, []
    rel = -0.691 + 10 * np.log10(gated.mean()) - 10
    gated = z[(lk > -70) & (lk > rel)]
    integrated = -0.691 + 10 * np.log10(gated.mean()) if len(gated) else -70.0
    short = []
    win = int(3 * rate)
    for s in range(0, max(1, len(power) - win + 1), rate):
        seg = power[s:s + win]
        short.append(-0.691 + 10 * np.log10(max(seg.mean(), 1e-12)))
    return integrated, short


def true_peak(x):
    """Peak after 4x oversampling (what a speaker might really be asked for)."""
    n = min(len(x), 44100 * 120)
    up = resample_poly(x[:n], 4, 1, axis=0)
    return np.abs(up).max()


# ---- Faults ----

def clicks(mono, rate, limit=12):
    """Samples that jump far more than the sound around them: the second
    difference against its local level (a click is a corner in the wave)."""
    if len(mono) < 64:
        return []
    d2 = np.abs(np.diff(mono, 2))
    win = int(0.01 * rate)
    kernel = np.ones(win) / win
    local = np.sqrt(np.convolve(d2 ** 2, kernel, mode="same")) + 1e-6
    level = np.sqrt(np.convolve(mono[1:-1] ** 2, kernel, mode="same"))
    # A real click stands well out of its neighbourhood and isn't buried.
    spikes = np.where((d2 > 8 * local) & (d2 > 0.02) & (d2 > 0.5 * level))[0]
    found, last = [], -rate
    for i in spikes:
        if i - last > rate * 0.05:
            found.append(((i + 1) / rate, d2[i], kind_of_corner(mono, rate, i)))
            last = i
        if len(found) >= limit:
            break
    return found


def kind_of_corner(mono, rate, i):
    """What a sharp corner at sample `i` is: "attack" (a sound starting:
    much louder just after than just before), "crackle" (inside noise:
    static, rain, ash), or "click" (in the middle of a clean sound, which
    is almost never meant)."""
    w = int(0.005 * rate)
    before = np.sqrt((mono[max(0, i - w):i - 1] ** 2).mean()) if i > 2 else 0.0
    after = np.sqrt((mono[i + 2:i + w] ** 2).mean()) if i + 3 < len(mono) else 0.0
    if after > 2 * before + 1e-4:
        return "attack"
    return "crackle" if noisy(mono, rate, i) else "click"


def noisy(mono, rate, i):
    """Whether the sound round sample `i` is noise (static, rain, ash):
    its spectrum is flat. A crackle in noise is usually meant; a click in a
    clean tone isn't."""
    half = int(0.05 * rate)
    seg = mono[max(0, i - half): i + half]
    if len(seg) < 256:
        return False
    spec = np.abs(np.fft.rfft(seg * np.hanning(len(seg)))) ** 2 + 1e-18
    f = np.fft.rfftfreq(len(seg), 1 / rate)
    spec = spec[(f > 500) & (f < 6000)]  # where static and rain live
    flatness = np.exp(np.log(spec).mean()) / spec.mean()
    return flatness > 0.15


def dropouts(mono, rate):
    """Moments of near-silence (20 ms or more) with sound on both sides."""
    win = int(0.01 * rate)
    n = len(mono) // win
    if n < 5:
        return []
    rms = np.sqrt((mono[:n * win].reshape(n, win) ** 2).mean(axis=1))
    loud = rms > 10 ** (-40 / 20)
    quiet = rms < 10 ** (-70 / 20)
    found = []
    i = 0
    while i < n:
        if quiet[i]:
            j = i
            while j < n and quiet[j]:
                j += 1
            before = loud[max(0, i - 3):i].any()
            after = loud[j:j + 3].any()
            if before and after and j - i >= 2:
                found.append((i * win / rate, (j - i) * win / rate))
            i = j
        else:
            i += 1
    return found


def seam(x, rate):
    """How the end runs into the start, for a loop: the jump in the wave
    at the join against the usual sample-to-sample step, and the level
    either side of it."""
    step = np.abs(np.diff(x, axis=0)).mean(axis=0) + 1e-9
    jump = np.abs(x[0] - x[-1])
    ratio = (jump / step).max()
    n = int(0.3 * rate)
    end, start = np.sqrt((x[-n:] ** 2).mean()), np.sqrt((x[:n] ** 2).mean())
    return ratio, db(end) - db(start)


# ---- Tone, notes, rhythm ----

def spectrum(mono, rate):
    f, _, z = stft(mono, rate, nperseg=4096, noverlap=3072)
    return f, np.abs(z) ** 2


def band_shares(f, power):
    total = power.sum() + 1e-12
    return {name: power[(f >= lo) & (f < hi)].sum() / total for name, lo, hi in BANDS}


def chroma(f, power):
    """Energy in each of the 12 notes, over time (from 55 Hz to 4 kHz)."""
    keep = (f >= 55) & (f <= 4000)
    midi = 69 + 12 * np.log2(f[keep] / 440)
    pitch_class = np.round(midi).astype(int) % 12
    c = np.zeros((12, power.shape[1]))
    for k in range(12):
        c[k] = power[keep][pitch_class == k].sum(axis=0)
    return c


def key_of(c):
    profile = c.sum(axis=1)
    if profile.sum() <= 0:
        return None, 0.0
    best = (None, -2.0)
    for tonic in range(12):
        for name, shape in [("major", MAJOR), ("minor", MINOR)]:
            r = np.corrcoef(profile, np.roll(shape, tonic))[0, 1]
            if r > best[1]:
                best = (f"{NOTES[tonic]} {name}", r)
    return best


def pitch_track(mono, rate):
    """The fundamental every 50 ms (autocorrelation), where it's clear."""
    win, hop = int(0.08 * rate), int(0.05 * rate)
    out = []
    for s in range(0, len(mono) - win, hop):
        seg = mono[s:s + win]
        if np.sqrt((seg ** 2).mean()) < 0.01:
            continue
        seg = seg - seg.mean()
        ac = np.fft.irfft(np.abs(np.fft.rfft(seg, 2 * win)) ** 2)[:win]
        # Normalised for the shrinking overlap, then the highest peak
        # after the first dip below zero (lag 0 itself is always highest).
        ac = ac / (ac[0] + 1e-12) / np.maximum(1 - np.arange(win) / win, 0.1)
        lo, hi = int(rate / 1200), int(rate / 40)
        below = np.where(ac[:hi] < 0)[0]
        if len(below) == 0:
            continue
        start = max(lo, below[0])
        if start >= hi:
            continue
        # The first peak nearly as high as the highest (later ones are the
        # same period again, twice, three times).
        part = ac[start:hi]
        peaks = [k for k in range(1, len(part) - 1) if part[k] >= part[k - 1] and part[k] >= part[k + 1]]
        if not peaks:
            continue
        best = max(part[k] for k in peaks)
        if best <= 0.5:
            continue  # nothing repeats clearly: no pitch here
        lag = start + next(k for k in peaks if part[k] >= 0.9 * best)
        if ac[lag] > 0.5 and 0 < lag < win - 1:
            # Between samples: a parabola through the peak.
            a, b, c = ac[lag - 1], ac[lag], ac[lag + 1]
            shift = 0.5 * (a - c) / (a - 2 * b + c) if a - 2 * b + c != 0 else 0
            out.append((s / rate, rate / (lag + shift)))
    return out


def onsets(mono, rate):
    """Spectral flux: when new sound starts. Times of the clear onsets, and
    a tempo from how they repeat (if they do)."""
    f, t, z = stft(mono, rate, nperseg=2048, noverlap=1536)
    mag = np.log1p(np.abs(z) * 100)
    flux = np.maximum(np.diff(mag, axis=1), 0).sum(axis=0)
    if len(flux) < 8:
        return [], None
    flux = (flux - flux.mean()) / (flux.std() + 1e-9)
    hop = 512 / rate
    peaks = [i for i in range(1, len(flux) - 1)
             if flux[i] > 2.0 and flux[i] >= flux[i - 1] and flux[i] >= flux[i + 1]]
    times = []
    for i in peaks:
        if not times or (i + 1) * hop - times[-1] > 0.08:
            times.append((i + 1) * hop)
    tempo = None
    ac = np.correlate(flux, flux, "full")[len(flux) - 1:]
    lo, hi = int(60 / 200 / hop), int(60 / 40 / hop)
    if hi < len(ac) and len(times) >= 6:
        lag = lo + np.argmax(ac[lo:hi])
        if ac[lag] > 0.3 * ac[0]:
            tempo = 60 / (lag * hop)
    return times, tempo


def note_name(hz):
    midi = 69 + 12 * np.log2(hz / 440)
    m = int(round(midi))
    return f"{NOTES[m % 12]}{m // 12 - 1}{'+' if midi - m > 0.25 else '-' if midi - m < -0.25 else ''}"


def bars(values, low, high):
    span = max(high - low, 1e-9)
    return "".join(BARS[int(np.clip((v - low) / span, 0, 1) * (len(BARS) - 1))] for v in values)


# ---- The report ----

def listen(path, as_loop=False, picture=True, quiet=False):
    x, rate = read(path)
    n, channels = x.shape
    seconds = n / rate
    mono = x.mean(axis=1)
    peak = np.abs(x).max()
    rms = np.sqrt((x ** 2).mean())
    lufs, short = loudness(x, rate)
    tp = true_peak(x)
    clipped = int((np.abs(x) >= 0.999).sum())
    dc = x.mean(axis=0)
    found_clicks = clicks(mono, rate)
    gaps = dropouts(mono, rate)
    f, power = spectrum(mono, rate)
    shares = band_shares(f, power)
    centroid = (f[:, None] * power).sum() / (power.sum() + 1e-12)
    c = chroma(f, power)
    key, key_fit = key_of(c)
    times, tempo = onsets(mono, rate)
    heard = np.abs(mono) > 10 ** (-60 / 20)
    lead = np.argmax(heard) / rate if heard.any() else seconds
    tail = np.argmax(heard[::-1]) / rate if heard.any() else seconds

    notes = []
    problems = []
    if clipped:
        problems.append(f"clips: {clipped} samples at full scale")
    if tp > 1.0:
        problems.append(f"true peak {db(tp):+.1f} dBTP: over full scale between samples")
    for ch, v in enumerate(dc):
        if abs(v) > 0.005:
            problems.append(f"DC offset {v:+.4f} in channel {ch + 1}")
    real = [(t, size) for t, size, kind in found_clicks if kind == "click"]
    crackles = [t for t, _, kind in found_clicks if kind == "crackle"]
    attacks = [t for t, _, kind in found_clicks if kind == "attack"]
    if len(real) >= 5 and real[-1][0] - real[0][0] < 3 * len(real):
        # Many close together: a crackle (meant, or a fault) more than clicks.
        problems.append(f"crackle: {len(real)} sharp clicks between {real[0][0]:.2f} and {real[-1][0]:.2f} s "
                        "(single-sample ticks; if meant, soften them into short bursts)")
    else:
        for t, size in real:
            problems.append(f"click at {t:.3f} s (a corner {size:.3f} tall, in clean sound)")
    if crackles:
        notes.append(f"crackle in noise at {', '.join(f'{t:.2f}' for t in crackles[:6])} s (meant, if it's static or ash)")
    if attacks:
        notes.append(f"sharp attacks at {', '.join(f'{t:.2f}' for t in attacks[:6])} s")
    for t, length in gaps[:6]:
        problems.append(f"dropout at {t:.2f} s for {length * 1000:.0f} ms")
    if as_loop:
        ratio, level = seam(x, rate)
        if ratio > 6:
            problems.append(f"loop seam: the wave jumps {ratio:.0f}x its usual step at the join (a click every loop)")
        # (Only for a steady sound like wind or rain: music may well start
        # louder than it ends.)
        half = int(0.5 * rate)
        blocks = mono[: len(mono) // half * half].reshape(-1, half)
        levels = 20 * np.log10(np.sqrt((blocks ** 2).mean(axis=1)) + 1e-9)
        steady = len(levels) >= 6 and np.std(levels) < 1.5
        if abs(level) > 4 and steady:
            problems.append(f"loop seam: {level:+.1f} dB from the end into the start (a bump every loop)")
        notes.append(f"loop seam: jump {ratio:.1f}x the usual step, level {level:+.1f} dB")
    if lead > 0.25 and not as_loop:
        notes.append(f"{lead:.2f} s of silence at the start")
    if tail > 0.5 and not as_loop:
        notes.append(f"{tail:.2f} s of silence at the end")

    # Tone, against what usually sounds balanced.
    if shares["low"] + shares["low-mid"] > 0.8 and shares["mid"] + shares["presence"] < 0.08:
        notes.append("muddy: nearly all low and low-mid")
    if shares["presence"] > 0.3:
        notes.append("harsh: a lot of 4-8 kHz")
    if shares["air"] > 0.25:
        notes.append("hissy: a lot above 8 kHz")
    if shares["sub"] + shares["low"] < 0.02 and shares["mid"] + shares["presence"] + shares["air"] > 0.8:
        notes.append("thin: almost nothing low")

    # A laptop's or a phone's speaker plays little below about 180 Hz.
    from scipy.signal import butter, sosfilt
    small = sosfilt(butter(4, 180, "highpass", fs=rate, output="sos"), x, axis=0)
    small_lufs, _ = loudness(small, rate)
    lost = lufs - small_lufs
    if lost > 8:
        problems.append(f"small speakers: {lost:.0f} dB quieter on a laptop or phone (it's nearly all below 180 Hz)")
    elif lost > 4:
        notes.append(f"small speakers: {lost:.0f} dB quieter on a laptop or phone")

    # Stereo.
    if channels == 2:
        left, right = x[:, 0], x[:, 1]
        corr = np.corrcoef(left, right)[0, 1] if left.std() > 0 and right.std() > 0 else 1.0
        mid, side = (left + right) / 2, (left - right) / 2
        width = np.sqrt((side ** 2).mean()) / (np.sqrt((mid ** 2).mean()) + 1e-12)
        balance = db(np.sqrt((left ** 2).mean())) - db(np.sqrt((right ** 2).mean()))
        stereo = f"ears alike {corr:+.2f}, width {width:.2f}, left {balance:+.1f} dB over right"
        if corr < 0:
            problems.append("stereo: the ears cancel (negative correlation): it nearly vanishes in mono")
        if abs(balance) > 3:
            notes.append(f"stereo: leans {'left' if balance > 0 else 'right'} by {abs(balance):.1f} dB")
        if corr > 0.999:
            notes.append("stereo: both ears the same (it's really mono)")
    else:
        stereo = "mono"

    lines = [f"== {Path(path).name}: {seconds:.2f} s, {'stereo' if channels == 2 else 'mono'}, {rate} Hz",
             f"   level: peak {db(peak):+.1f} dBFS, true peak {db(tp):+.1f} dBTP, RMS {db(rms):+.1f} dBFS, "
             f"loudness {lufs:.1f} LUFS ({small_lufs:.1f} on small speakers), crest {db(peak) - db(rms):.1f} dB",
             f"   tone: " + ", ".join(f"{k} {v * 100:.0f}%" for k, v in shares.items())
             + f"; centre {centroid:.0f} Hz",
             f"   stereo: {stereo}"]
    if short:
        lo, hi = min(short), max(short)
        lines.append(f"   loudness by second ({lo:.0f} to {hi:.0f} LUFS): {bars(short, lo, hi)}")
    top = np.argsort(c.sum(axis=1))[::-1][:5]
    total = c.sum() + 1e-12
    lines.append("   notes: " + ", ".join(f"{NOTES[k]} {c[k].sum() / total * 100:.0f}%" for k in top)
                 + (f"; key {key} (fit {key_fit:.2f})" if key else ""))
    if seconds <= 8:
        track = pitch_track(mono, rate)
        if len(track) >= 3:
            hz = [h for _, h in track]
            lines.append(f"   pitch: {note_name(hz[0])} ({hz[0]:.0f} Hz) to {note_name(hz[-1])} ({hz[-1]:.0f} Hz), "
                         f"{len(track)} clear moments")
    lines.append(f"   onsets: {len(times)}" + (f" (first at {', '.join(f'{t:.2f}' for t in times[:8])})" if times else "")
                 + (f"; tempo about {tempo:.0f} bpm" if tempo else ""))
    for note in notes:
        lines.append(f"   note: {note}")
    for problem in problems:
        lines.append(f"   PROBLEM: {problem}")
    if not problems:
        lines.append("   no problems heard")
    if picture:
        out = Path("out/ear")
        out.mkdir(parents=True, exist_ok=True)
        image = out / (Path(path).stem + ".png")
        draw(x, rate, short, f, power, c, Path(path).name, image)
        lines.append(f"   picture: {image}")
    report = "\n".join(lines)
    if not quiet:
        print(report)
    return {"name": Path(path).name, "seconds": seconds, "lufs": lufs, "small": small_lufs, "peak": db(peak), "tp": db(tp),
            "problems": problems, "shares": shares, "report": report}


def draw(x, rate, short, f, power, c, title, image):
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    fig, axes = plt.subplots(4, 1, figsize=(12, 9), gridspec_kw={"height_ratios": [1.2, 0.7, 2, 1]})
    n = len(x)
    t = np.arange(n) / rate
    cols = 1500
    per = max(1, n // cols)
    for ch, color in zip(range(x.shape[1]), ["#2a6fdb", "#db2a6f"]):
        m = x[: per * (n // per), ch].reshape(-1, per)
        tt = t[: per * (n // per)].reshape(-1, per)[:, 0]
        axes[0].fill_between(tt, m.min(axis=1), m.max(axis=1), color=color, alpha=0.6, lw=0)
    axes[0].set_ylim(-1.05, 1.05)
    axes[0].axhline(1, color="red", lw=0.5)
    axes[0].axhline(-1, color="red", lw=0.5)
    axes[0].set_title(title)
    axes[0].set_ylabel("wave")
    if short:
        axes[1].plot(np.arange(len(short)), short, color="black")
    axes[1].set_ylabel("LUFS (3 s)")
    tt = np.linspace(0, n / rate, power.shape[1])
    keep = (f > 30) & (f < 20000)
    axes[2].pcolormesh(tt, f[keep], 10 * np.log10(power[keep] + 1e-12), shading="auto", cmap="magma",
                       vmin=10 * np.log10(power.max() + 1e-12) - 90)
    axes[2].set_yscale("log")
    axes[2].set_ylabel("Hz")
    axes[3].imshow(c / (c.max(axis=0, keepdims=True) + 1e-12), aspect="auto", origin="lower", cmap="Greys",
                   extent=[0, n / rate, -0.5, 11.5])
    axes[3].set_yticks(range(12))
    axes[3].set_yticklabels(NOTES, fontsize=7)
    axes[3].set_xlabel("seconds")
    fig.tight_layout()
    fig.savefig(image, dpi=80)
    plt.close(fig)


def zoom(path, at):
    """40 ms of the wave round `at` seconds, sample by sample: what a click
    looks like up close."""
    import matplotlib
    matplotlib.use("Agg")
    import matplotlib.pyplot as plt
    x, rate = read(path)
    a, b = max(0, int((at - 0.02) * rate)), min(len(x), int((at + 0.02) * rate))
    t = np.arange(a, b) / rate
    fig, ax = plt.subplots(figsize=(12, 3.5))
    for ch, color in zip(range(x.shape[1]), ["#2a6fdb", "#db2a6f"]):
        ax.plot(t, x[a:b, ch], color=color, lw=0.8, marker=".", ms=2)
    ax.axvline(at, color="grey", lw=0.5)
    ax.set_title(f"{Path(path).name} at {at:.3f} s")
    out = Path("out/ear") / f"{Path(path).stem}_at_{at:.3f}.png"
    out.parent.mkdir(parents=True, exist_ok=True)
    fig.tight_layout()
    fig.savefig(out, dpi=80)
    plt.close(fig)
    print(f"   zoom: {out}")


def compare(a, b):
    """How two files differ: the offset that lines them up, the gain
    between them, how alike they are, and the difference by band."""
    xa, ra = read(a)
    xb, rb = read(b)
    if ra != rb:
        print(f"rates differ: {ra} and {rb}")
        return
    ma, mb = xa.mean(axis=1), xb.mean(axis=1)
    n = min(len(ma), len(mb), ra * 20)
    from scipy.signal import correlate
    lag_all = correlate(mb[:n], ma[:n], mode="full", method="fft")
    lag = int(np.argmax(np.abs(lag_all)) - (n - 1))
    if lag >= 0:
        sa, sb = ma[: len(ma) - 0], mb[lag:]
    else:
        sa, sb = ma[-lag:], mb
    m = min(len(sa), len(sb))
    sa, sb = sa[:m], sb[:m]
    gain = (sa @ sb) / (sa @ sa + 1e-12)
    corr = np.corrcoef(sa, sb)[0, 1]
    residual = sb - gain * sa
    print(f"== {Path(a).name} vs {Path(b).name}")
    print(f"   offset {lag / ra * 1000:+.1f} ms, gain {db(abs(gain)):+.1f} dB, alike {corr:+.4f}, "
          f"what's left after lining up: {db(np.sqrt((residual ** 2).mean())) - db(np.sqrt((sb ** 2).mean())):+.1f} dB")
    fa, pa = spectrum(sa, ra)
    _, pb = spectrum(sb, ra)
    for name, lo, hi in BANDS:
        k = (fa >= lo) & (fa < hi)
        print(f"   {name:9} {10 * np.log10((pb[k].sum() + 1e-12) / (pa[k].sum() * gain ** 2 + 1e-12)):+.1f} dB")


def main():
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("paths", nargs="+")
    p.add_argument("--loop", action="store_true", help="check the loop seam too")
    p.add_argument("--compare", action="store_true", help="compare two files")
    p.add_argument("--no-picture", action="store_true")
    p.add_argument("--zoom", type=float, metavar="SECONDS",
                   help="also draw 40 ms of the wave round this time (out/ear/<name>_at_<time>.png)")
    args = p.parse_args()
    if args.compare:
        if len(args.paths) != 2:
            sys.exit("--compare takes two files")
        compare(*args.paths)
        return
    files = []
    for path in args.paths:
        path = Path(path)
        files += sorted(path.glob("*.wav")) if path.is_dir() else [path]
    results = [listen(f, as_loop=args.loop, picture=not args.no_picture) for f in files]
    if args.zoom is not None:
        for f in files:
            zoom(f, args.zoom)
    if len(results) > 1:
        print("\n== all of them")
        print(f"   {'file':20} {'secs':>6} {'LUFS':>6} {'small':>6} {'peak':>6} {'TP':>6}  problems")
        for r in results:
            print(f"   {r['name']:20} {r['seconds']:6.1f} {r['lufs']:6.1f} {r['small']:6.1f} {r['peak']:6.1f} {r['tp']:6.1f}  {len(r['problems'])}")


if __name__ == "__main__":
    main()
