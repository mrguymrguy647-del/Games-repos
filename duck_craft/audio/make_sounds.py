"""Writes Duck Craft's music and sound effects as .wav files, from nothing
but sine waves, noise and arithmetic (Duck Echo isn't ready yet).

    python3 audio/make_sounds.py          # run in duck_craft/

writes into assets/, all at 44100 samples a second (the engine's own rate,
so nothing is resampled when the game loads them):

  music_day.wav    "Still Water": a calm piano song on a warped tape, that
                   isn't calm: flat notes, an out-of-tune music box, a
                   far choir, a backwards swell, silences; 64 s, stereo,
                   loops with no seam
  music_night.wav  "Nothing Waits": a low drone, a tone that falls
                   forever, whispers, strings that pile up and cut off,
                   bowed metal, a sinking choir, a far bell; 64 s, stereo
  music_title.wav  "The Island Doesn't Remember": a creepypasta-style menu
                   song: an old-console children's tune, slowed, off a
                   worn VHS tape, that sticks, runs backwards, and ends on
                   a broadcast tone; 32 s, stereo, loops
  wind.wav         the island's wind, 24 s, stereo, loops
  rain.wav         rain, 16 s, stereo, loops
  thunder.wav      a crack and a long roll (stereo)
  groan.wav        a Hollow, somewhere near (mono: the game pans it
                   toward the Hollow)
  groan_far.wav    the same, further off: quieter and muffled (the engine
                   plays a .wav at one loudness, so distance is a file)
  hollow_hurt.wav  a Hollow, hit (mono, panned in the game)
  hollow_die.wav   a Hollow, coming apart (mono, panned in the game)
  sting.wav        the Watcher, seen (stereo)
  heartbeat.wav    one beat, for looping when the duck is hurt (mono)
  bell_dusk.wav    nightfall (stereo)
  bell_dawn.wav    morning (stereo)

Everything comes from fixed random seeds, so the files come out the same
every time.

What makes it sound clean:
- 44.1 kHz, so the piano keeps its top end (16 kHz cut everything above
  8 kHz and sounded like a phone).
- Stereo: piano low notes to the left and high to the right, as you'd
  hear sitting at one; the pad's voices spread wide; wind different in
  each ear.
- A convolution reverb: an impulse response of decaying noise, a
  different one for each ear, with high notes dying sooner than low ones.
  (The old comb-filter reverb rang with a metallic edge.)
- Instruments with more in them: piano notes have three slightly
  detuned strings, a fast and a slow decay, and brightness that follows
  how hard they're struck; the pad is band-limited saws through a
  low-pass instead of bare sines.
- Dither when writing 16-bit, so quiet tails fade smoothly instead of
  crunching.
"""

from pathlib import Path
import wave

import numpy as np
from scipy.signal import butter, fftconvolve, sosfilt

OUT = Path(__file__).resolve().parent.parent / "assets"
RATE = 44100


# ---- Building blocks ----


def note_hz(name):
    """'D4' -> Hz. Sharps as 'C#4', flats as 'Bb3'."""
    names = {"C": 0, "C#": 1, "Db": 1, "D": 2, "D#": 3, "Eb": 3, "E": 4, "F": 5,
             "F#": 6, "Gb": 6, "G": 7, "G#": 8, "Ab": 8, "A": 9, "A#": 10, "Bb": 10, "B": 11}
    pitch, octave = name[:-1], int(name[-1])
    midi = 12 * (octave + 1) + names[pitch]
    return 440.0 * 2 ** ((midi - 69) / 12)


def snap(hz, length):
    """The nearest frequency that fits a whole number of times in `length`
    seconds, so a held tone joins up where the loop repeats."""
    return round(hz * length) / length


def time_axis(seconds):
    return np.arange(int(seconds * RATE)) / RATE


def envelope(n, attack, decay):
    """Rises over `attack` seconds, then dies away (exponentially) with
    time constant `decay`."""
    t = np.arange(n) / RATE
    rise = np.clip(t / max(attack, 1e-4), 0, 1)
    return rise * np.exp(-np.maximum(t - attack, 0) / decay)


def lowpass(x, hz, order=2):
    sos = butter(order, hz / (RATE / 2), btype="low", output="sos")
    return sosfilt(sos, x, axis=0)


def highpass(x, hz, order=2):
    sos = butter(order, hz / (RATE / 2), btype="high", output="sos")
    return sosfilt(sos, x, axis=0)


def bandpass(x, low, high, order=2):
    sos = butter(order, [low / (RATE / 2), min(high, RATE / 2 * 0.98) / (RATE / 2)], btype="band", output="sos")
    return sosfilt(sos, x, axis=0)


def pan(mono, position):
    """A mono sound placed between the ears: -1 left, 0 middle, 1 right.
    Equal power, so a sound is as loud wherever it sits (and in the
    middle each ear gets it at full level)."""
    angle = (np.clip(position, -1, 1) + 1) * np.pi / 4
    gains = np.array([np.cos(angle), np.sin(angle)]) * np.sqrt(2)
    return mono[:, None] * gains[None, :]


def place(track, sound, at_seconds):
    """Adds `sound` into `track` from `at_seconds` (cut at the end).
    Both mono (n,) or both stereo (n, 2)."""
    start = int(round(at_seconds * RATE))
    if start < 0:  # starts before the track: keep the part inside it
        sound, start = sound[-start:], 0
    end = min(len(track), start + len(sound))
    if start < len(track) and end > start:
        track[start:end] += sound[: end - start]


def looping_noise(n, cutoff, rng, low=0.0):
    """Noise that joins up with itself when it repeats: made in the
    frequency domain (random phases, nothing outside `low`..`cutoff`), so
    it is exactly periodic over `n` samples."""
    spectrum = np.zeros(n // 2 + 1, dtype=complex)
    freqs = np.fft.rfftfreq(n, 1 / RATE)
    keep = (freqs < cutoff) & (freqs > low)
    spectrum[keep] = np.exp(1j * rng.uniform(0, 2 * np.pi, keep.sum())) / (1 + freqs[keep] / (cutoff / 3))
    noise = np.fft.irfft(spectrum, n)
    return noise / np.std(noise)


def loop_lfo(t, length, cycles, phase=0.0):
    """0..1, `cycles` whole times round per loop, so it joins at the seam."""
    return 0.5 + 0.5 * np.sin(2 * np.pi * cycles * t / length + phase)


# ---- The room ----


def room(seconds, decay, seed, ears=2, predelay=0.02):
    """An impulse response: a few early echoes, then a tail of noise that
    dies away, low notes ringing longer than high ones (`decay` is how long
    the middle takes to fall 60 dB). A different one for each ear, so the
    tail is wide. Scaled to unit energy."""
    rng = np.random.default_rng(seed)
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    ears_out = []
    for _ in range(ears):
        noise = rng.normal(0, 1, n)
        tail = np.zeros(n)
        for low, high, scale in [(20, 250, 1.25), (250, 1000, 1.0), (1000, 3500, 0.75),
                                 (3500, 8000, 0.5), (8000, 20000, 0.3)]:
            band = bandpass(noise, low, high)
            band /= np.std(band[: int(0.5 * RATE)]) + 1e-12  # each band as strong in both ears
            tail += band * np.sqrt((high - low) / 20000) * np.exp(-6.9 * t / (decay * scale))
        tail *= np.clip((t - predelay) / 0.03, 0, 1)  # the room fills in
        early = np.zeros(n)
        for _ in range(10):
            at = int(rng.uniform(predelay * 0.4, predelay + 0.07) * RATE)
            early[at] += rng.choice([-1, 1]) * rng.uniform(0.3, 1.0)
        early = lowpass(early, 6000) * 6.0
        ir = tail + early
        ears_out.append(ir / np.sqrt(np.sum(ir ** 2)))
    return np.stack(ears_out, axis=1) if ears == 2 else ears_out[0]


def reverb(x, ir, wet):
    """`x` through the room `ir`. Stereo in, stereo out (each ear's reverb
    hears mostly its own side); mono in with a mono room, mono out."""
    if x.ndim == 1:
        out = fftconvolve(x, ir)[: len(x)]
        return (1 - wet) * x + wet * out * 0.7
    left = fftconvolve(x[:, 0] * 0.75 + x[:, 1] * 0.25, ir[:, 0])[: len(x)]
    right = fftconvolve(x[:, 1] * 0.75 + x[:, 0] * 0.25, ir[:, 1])[: len(x)]
    return (1 - wet) * x + wet * np.stack([left, right], axis=1) * 0.7


# ---- Finishing ----


def seamless(events, length, beds=()):
    """A loop that repeats with no seam. `events` are (seconds, stereo
    sound) inside one pass; `beds` are stereo sounds exactly one pass
    long. Three passes are laid down (and go through the reverb later);
    the middle one is kept: its start already holds the tails of the pass
    before it, just as the loop's start will when it repeats."""
    n = int(length * RATE)
    track = np.zeros((n * 3, 2))
    for k in range(3):
        for bed in beds:
            track[k * n:(k + 1) * n] += bed
        for at, sound in events:
            place(track, sound, k * length + at)
    return track


def middle(track, length):
    n = int(length * RATE)
    return track[n:2 * n]


def balance(x):
    """Both ears equally loud over the whole sound (things can still move
    from side to side inside it)."""
    if x.ndim == 1:
        return x
    levels = np.sqrt(np.mean(x ** 2, axis=0)) + 1e-12
    return x * (np.mean(levels) / levels)[None, :]


def loudness(x, rms, ceiling=0.92):
    """Scaled to an average loudness (`rms`, like the old files, so the
    game's mix doesn't change), with any rare peak above `ceiling`
    rounded off gently instead of clipped."""
    x = balance(x)
    x = x * (rms / (np.sqrt(np.mean(x ** 2)) + 1e-12))
    over = np.abs(x) > ceiling * 0.8
    if over.any():
        knee = ceiling * 0.8
        x = np.where(over, np.sign(x) * (knee + (ceiling - knee) * np.tanh((np.abs(x) - knee) / (ceiling - knee))), x)
    return x


def fade(x, fade_in=0.0, fade_out=0.0):
    x = x.copy()
    a, b = int(fade_in * RATE), int(fade_out * RATE)
    shape = (lambda k: k[:, None]) if x.ndim == 2 else (lambda k: k)
    if a:
        x[:a] *= shape(np.linspace(0, 1, a))
    if b:
        x[-b:] *= shape(np.linspace(1, 0, b))
    return x


def write(name, samples, seed=0):
    """A 16-bit .wav, mono or stereo, with a whisper of dither (triangular,
    one step wide) so quiet tails stay smooth."""
    rng = np.random.default_rng(1000 + seed)
    dither = (rng.random(samples.shape) - rng.random(samples.shape)) / 32768
    pcm = np.round(np.clip(samples + dither, -1, 32767 / 32768) * 32768).astype("<i2")
    channels = 1 if samples.ndim == 1 else samples.shape[1]
    with wave.open(str(OUT / name), "wb") as w:
        w.setnchannels(channels)
        w.setsampwidth(2)
        w.setframerate(RATE)
        w.writeframes(pcm.tobytes())
    seam = float(np.max(np.abs(np.atleast_1d(samples[0] - samples[-1]))))
    kind = "stereo" if channels == 2 else "mono  "
    print(f"  {name:18} {kind} {len(samples) / RATE:5.1f} s  peak {np.max(np.abs(samples)):.2f}"
          f"  rms {np.sqrt(np.mean(samples ** 2)):.3f}  end-to-start step {seam:.4f}")


# ---- Instruments ----


def piano(hz, seconds, velocity=1.0, seed=0):
    """A soft felt piano. Each note: up to sixteen harmonics, a little
    sharp as real strings are, softer notes darker; three strings a few
    hundredths of a cent apart, so the tone slowly breathes; a quick first
    decay and a long second one; a felt thump at the start. Placed in
    stereo by pitch, low left and high right."""
    rng = np.random.default_rng(seed + int(hz * 10))
    t = time_axis(seconds)
    out = np.zeros_like(t)
    darkness = max(0.06, 0.3 - 0.25 * velocity)
    for k in range(1, 17):
        f = hz * k * np.sqrt(1 + 0.00035 * k * k)
        if f > 15000:
            break
        amp = k ** -0.9 * np.exp(-(k - 1) * darkness) * (0.35 + 0.65 * abs(np.sin(np.pi * k / 7.3)))
        fast = 0.4 * (220 / hz) ** 0.3 / k ** 0.4
        slow = 5.5 * (220 / hz) ** 0.45 / k ** 0.75
        env = (0.55 * np.exp(-t / fast) + 0.45 * np.exp(-t / slow)) * np.clip(t / 0.005, 0, 1)
        strings = sum(np.sin(2 * np.pi * f * (1 + cents / 1200) * t + rng.uniform(0, 6.28))
                      for cents in (-0.35, 0.0, 0.4)) / 3
        out += amp * strings * env
    thump = bandpass(rng.normal(0, 1, len(t)), 120, 5000) * envelope(len(t), 0.001, 0.012) * 0.08
    note = (out + thump * velocity) * velocity
    note = fade(note, 0, 0.25)  # no click where it ends
    midi = 69 + 12 * np.log2(hz / 440)
    return pan(note, np.clip((midi - 60) / 28, -0.55, 0.55))


def soft_saw(hz, t, harmonics_up_to=4000):
    """A sawtooth with nothing above `harmonics_up_to` (so no aliasing),
    its harmonics rolling off a little faster than a raw saw: warm."""
    out = np.zeros_like(t)
    k = 1
    while hz * k < harmonics_up_to and k <= 24:
        out += np.sin(2 * np.pi * hz * k * t) / k ** 1.25
        k += 1
    return out


def pad(chord, seconds, level):
    """A held chord: four slightly detuned saws a note, spread across the
    stereo field, low-passed, swelling in and out."""
    t = time_axis(seconds)
    swell = np.sin(np.pi * np.clip(t / seconds, 0, 1)) ** 1.5
    out = np.zeros((len(t), 2))
    for name in chord:
        hz = note_hz(name)
        for cents, where in [(-7, -0.75), (-2.5, -0.25), (2.5, 0.25), (7, 0.75)]:
            out += pan(soft_saw(hz * 2 ** (cents / 1200), t, 5000), where)
    out = lowpass(out, 2200)
    return out * swell[:, None] * level / (len(chord) * 4)


def bell(hz, seconds, seed=0):
    """A deep bell: partials at the odd, inharmonic ratios of real bells,
    each ringing for its own time and slowly beating, the high ones a
    little to one side and the low ones to the other."""
    t = time_axis(seconds)
    rng = np.random.default_rng(seed)
    out = np.zeros((len(t), 2))
    for i, (ratio, amp, decay) in enumerate([
            (0.5, 0.6, 7.0), (1.0, 1.0, 5.0), (1.19, 0.5, 3.6), (1.5, 0.35, 3.0), (2.0, 0.4, 2.4),
            (2.52, 0.2, 1.9), (2.74, 0.25, 1.6), (3.76, 0.15, 1.1), (4.9, 0.1, 0.8), (5.4, 0.08, 0.6),
            (6.8, 0.05, 0.4)]):
        f = hz * ratio * (1 + rng.normal(0, 0.002))
        if f > 16000:
            continue
        beat = 1 + 0.15 * np.sin(2 * np.pi * rng.uniform(0.3, 1.2) * t)
        partial = amp * np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)) * envelope(len(t), 0.003, decay) * beat
        out += pan(partial, 0.35 * np.sin(i * 1.7))
    strike = bandpass(rng.normal(0, 1, len(t)), 1500, 9000) * envelope(len(t), 0.0005, 0.006) * 0.15
    return out + pan(strike, 0)


# ---- Uncanny instruments ----


def stretch_rng(seed):
    return np.random.default_rng(seed)


def music_box(hz, seconds, seed=0, cents_off=0.0):
    """One tine of a music box: a bright ping with a metal click, its
    overtones at the comb's odd ratios, a little out of tune."""
    rng = stretch_rng(seed)
    hz = hz * 2 ** (cents_off / 1200)
    t = time_axis(seconds)
    out = np.zeros_like(t)
    for ratio, amp, decay in [(1.0, 1.0, 2.4), (2.0, 0.08, 0.9), (3.01, 0.22, 0.5), (5.63, 0.1, 0.2), (9.2, 0.05, 0.08)]:
        f = hz * ratio
        if f > 17000:
            continue
        out += amp * np.sin(2 * np.pi * f * t + rng.uniform(0, 6.28)) * envelope(len(t), 0.001, decay)
    click = highpass(rng.normal(0, 1, len(t)), 3000) * envelope(len(t), 0.0003, 0.003) * 0.25
    return fade(out + click, 0, 0.1)


def choir(names, seconds, vowel="oo", sag_cents=0.0, seed=0):
    """Voices humming, far off: three slightly out-of-tune singers a note,
    each with a slow wobble, shaped by a vowel's formants. With `sag_cents`
    the whole choir sinks in pitch as it holds, which no real choir does."""
    rng = stretch_rng(seed)
    formants = {"oo": [((240, 400), 1.0), ((650, 1000), 0.45), ((2200, 2900), 0.06)],
                "ah": [((600, 900), 1.0), ((1000, 1350), 0.6), ((2400, 3000), 0.1)]}[vowel]
    t = time_axis(seconds)
    sag = 2 ** (-sag_cents / 1200 * (t / seconds) ** 1.5)
    out = np.zeros((len(t), 2))
    for name in names:
        hz = note_hz(name)
        for cents, where in [(-9, -0.65), (1, 0.05), (8, 0.6)]:
            vibrato = 1 + 0.0045 * np.sin(2 * np.pi * rng.uniform(4.3, 5.4) * t + rng.uniform(0, 6.28))
            f = hz * 2 ** (cents / 1200) * vibrato * sag
            buzz = lowpass(saw(f), 3500)
            sung = sum(bandpass(buzz, low, high) * level for (low, high), level in formants)
            sung += bandpass(rng.normal(0, 1, len(t)), 500, 3000) * 0.04  # breath
            out += pan(sung, where)
    swell = np.clip(t / (seconds * 0.35), 0, 1) ** 2 * np.clip((seconds - t) / (seconds * 0.3), 0, 1)
    return out * swell[:, None] / (len(names) * 3)


def whisper(seconds, seed=0):
    """Breath shaped into syllables that never make words: hiss through
    vowel formants in short bursts, with a sharp 's' now and then. Mono."""
    rng = stretch_rng(seed)
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    hiss = rng.normal(0, 1, n)
    vowels = [(700, 1200), (350, 2200), (500, 950), (420, 1900)]
    voiced = [bandpass(hiss, f1 * 0.7, f1 * 1.3) * 0.8 + bandpass(hiss, f2 * 0.85, f2 * 1.15) for f1, f2 in vowels]
    sibilant = highpass(hiss, 4500) * 0.5
    out = np.zeros(n)
    at = rng.uniform(0.05, 0.2)
    while at < seconds - 0.3:
        length = rng.uniform(0.07, 0.22)
        a, b = int(at * RATE), int(min(at + length, seconds) * RATE)
        shape = np.hanning(b - a) * rng.uniform(0.5, 1.0)
        source = sibilant if rng.random() < 0.18 else voiced[rng.integers(len(vowels))]
        out[a:b] += source[a:b] * shape
        at += length + (rng.uniform(0.25, 0.6) if rng.random() < 0.2 else rng.uniform(0.02, 0.08))
    return lowpass(out, 7000)


def shepard(seconds, length, sweep_seconds, octaves=6, low=40.0, voices=6, seed=0):
    """A tone that falls forever: voices an octave apart all gliding down,
    each fading out at the bottom as another fades in at the top. Every
    voice's phase comes back round exactly at `length`, so it loops."""
    rng = stretch_rng(seed)
    n = int(seconds * RATE)
    t = np.arange(n) / RATE
    out = np.zeros((n, 2))
    for i in range(voices):
        place_in_sweep = (i / voices - t / sweep_seconds) % 1.0  # 1 at the top, falling to 0
        f = low * 2 ** (place_in_sweep * octaves)
        cycles = np.sum(f[: int(length * RATE)]) / RATE
        f *= round(cycles) / cycles  # a whole number of cycles a loop
        phase = 2 * np.pi * np.cumsum(f) / RATE + rng.uniform(0, 6.28)
        loudness_curve = np.exp(-0.5 * ((place_in_sweep - 0.5) / 0.17) ** 2)
        out += pan(np.sin(phase) * loudness_curve, 0.5 * np.sin(2 * np.pi * i / voices))
    return out / voices


def cluster_swell(names, seconds, seed=0):
    """Strings, all the wrong notes together, growing louder and louder,
    then cut off dead."""
    rng = stretch_rng(seed)
    t = time_axis(seconds)
    out = np.zeros((len(t), 2))
    for i, name in enumerate(names):
        hz = note_hz(name)
        vibrato = 1 + 0.005 * np.sin(2 * np.pi * rng.uniform(4.5, 6.5) * t + rng.uniform(0, 6))
        string = bandpass(lowpass(saw(np.full_like(t, hz) * vibrato), 6000), 250, 5000)
        out += pan(string, -0.8 + 1.6 * i / max(len(names) - 1, 1))
    grow = (t / seconds) ** 3
    return fade(out * grow[:, None] / len(names), 0, 0.012)


def bowed_metal(hz, seconds, seed=0):
    """A sheet of metal bowed slowly: inharmonic partials that creep in and
    waver, and the scrape of the bow."""
    rng = stretch_rng(seed)
    t = time_axis(seconds)
    out = np.zeros((len(t), 2))
    bow = 0.7 + 0.3 * lowpass(rng.normal(0, 1, len(t)), 8) * 3
    for i, (ratio, amp) in enumerate([(1.0, 1.0), (2.32, 0.6), (4.25, 0.4), (6.63, 0.25), (9.38, 0.15), (12.8, 0.08)]):
        f = hz * ratio * (1 + 0.002 * np.sin(2 * np.pi * rng.uniform(0.2, 0.7) * t))
        partial = np.sin(2 * np.pi * np.cumsum(f) / RATE) * amp * bow
        out += pan(partial, 0.6 * np.sin(i * 2.1))
    scrape = bandpass(rng.normal(0, 1, len(t)), hz * 3, hz * 9) * bow * 0.15
    shape = np.sin(np.pi * np.clip(t / seconds, 0, 1)) ** 2
    return (out + pan(scrape, 0.2)) * shape[:, None] / 3


def reversed_swell(sound, ir):
    """A sound and its echo played backwards: it rises out of nothing and
    stops dead, as if time ran the wrong way. Place it so it ends where
    the next thing begins."""
    wet = np.stack([fftconvolve(sound[:, c], ir[:, c]) for c in range(2)], axis=1)
    return wet[::-1].copy()


def wow(x, length, cents, seed=0):
    """A warped tape: the pitch slowly drifts sharp and flat by about
    `cents`, never quite repeating within the loop (but exactly once a
    loop, so the seam holds)."""
    rng = stretch_rng(seed)
    n = len(x)
    t = np.arange(n) / RATE
    dev = 2 ** (cents / 1200) - 1
    delay = np.zeros(n)
    for cycles, weight in [(5, 0.55), (11, 0.3), (23, 0.15)]:
        amp = weight * dev * length / (2 * np.pi * cycles)
        delay += amp * (1 + np.sin(2 * np.pi * cycles * t / length + rng.uniform(0, 6.28)))
    read = np.clip(t - delay, 0, t[-1])
    return np.stack([np.interp(read, t, x[:, c]) for c in range(x.shape[1])], axis=1)


def dropouts(n_total, length, gaps):
    """A gain that drops to nothing for each (start, seconds) in `gaps`,
    once a pass, with short fades: the music forgets itself."""
    gain = np.ones(n_total)
    t = np.arange(n_total) / RATE
    local = t % length
    for start, seconds in gaps:
        inside = np.clip(np.minimum(local - start, start + seconds - local) / 0.06, 0, 1)
        gain *= 1 - inside
    return gain[:, None]


def record_skip(x, at, piece, times):
    """The record sticks: `piece` seconds from `at` repeat `times` times,
    then it goes on as if nothing happened (the length stays the same)."""
    a, w = int(at * RATE), int(piece * RATE)
    chunk = x[a:a + w].copy()
    edge = int(0.004 * RATE)
    chunk[:edge] *= np.linspace(0, 1, edge)[:, None]
    chunk[-edge:] *= np.linspace(1, 0, edge)[:, None]
    out = x.copy()
    out[a - edge:a] *= np.linspace(1, 0, edge)[:, None]  # no click going in
    for k in range(times):
        out[a + k * w:a + (k + 1) * w] = chunk
    after = a + times * w
    out[after:after + edge] *= np.linspace(0, 1, edge)[:, None]  # or coming out
    return out


# ---- Music ----


def day_music():
    """'Still Water': i - VI - III - VII in D minor, slow. It sounds like a
    calm song on an old warped tape, and it isn't calm: some notes land a
    quarter-tone flat, a music box plays a lullaby a little out of tune, a
    choir hums very far off, a chord rises in backwards, and twice the
    whole song simply stops for a moment and the room rings on without it."""
    length = 64.0
    rng = np.random.default_rng(11)
    chords = [
        (["D3", "F3", "A3"], "D2", ["D4", "F4", "A4", "C5", "E5", "D5"]),
        (["Bb2", "D3", "F3"], "Bb1", ["D4", "F4", "Bb4", "A4", "C5", "D5"]),
        (["F3", "A3", "C4"], "F2", ["C4", "F4", "A4", "C5", "G4", "E4"]),
        (["C3", "E3", "G3"], "C2", ["C4", "E4", "G4", "D5", "E5", "G4"]),
    ]
    melody = []
    for c, (_, _, notes) in enumerate(chords):
        for beat in range(16):
            if beat < 15 and rng.random() < (0.5 if beat % 4 == 0 else 0.2):
                melody.append((c * 16 + beat + rng.choice([0, 0, 0.5]), rng.choice(notes), rng.uniform(0.5, 0.85)))

    piano_events = []
    pad_events = []
    for c, (chord, bass, _) in enumerate(chords):
        start = c * 16.0
        pad_events.append((start - 1.0, pad(chord, 18.0, 0.4)))
        for bar in range(4):
            velocity = 0.55 if bar % 2 == 0 else 0.35
            piano_events.append((start + bar * 4.0, piano(note_hz(bass), 7.0, velocity, seed=c * 4 + bar)))
    for i, (beat, name, velocity) in enumerate(melody):
        hz = note_hz(name)
        if i % 6 == 4:
            hz *= 2 ** (-0.5 / 12)  # a quarter-tone flat: wrong, but only just
        piano_events.append((beat, piano(hz, 6.0, velocity * 0.6, seed=100 + i)))

    # The lullaby, twice: the second time under the wrong chord.
    lullaby = ["A5", "F5", "E5", "D5", "E5", "F5", "A5", "G5", "F5", "E5", "D5", "C#5", "D5"]
    box_events = []
    for start, seed in [(2.0, 0), (34.0, 50)]:
        box_rng = np.random.default_rng(200 + seed)
        for k, name in enumerate(lullaby):
            off = box_rng.normal(0, 14) + (-30 if k == 11 else 0)
            box_events.append((start + k * 0.62 + (0.4 if k == 12 else 0),
                               pan(music_box(note_hz(name), 3.0, seed=seed + k, cents_off=off) * 0.3, 0.45)))

    far_choir = [(18.0, choir(["Bb3", "D4", "F4"], 14.0, "oo", sag_cents=35, seed=1) * 0.5),
                 (50.0, choir(["C4", "E4", "G4"], 13.0, "oo", sag_cents=50, seed=2) * 0.5)]
    swell_room = room(3.0, 2.6, seed=21)
    backwards = reversed_swell(piano(note_hz("F3"), 2.5, 0.7, seed=300) + piano(note_hz("C4"), 2.5, 0.6, seed=301), swell_room) * 0.35
    rising = [(32.0 - len(backwards) / RATE, backwards)]

    n3 = int(length * RATE) * 3
    tape = wow(seamless(piano_events + box_events, length), length, cents=16, seed=4)
    track = tape + seamless(pad_events + rising, length) + seamless(far_choir, length) * 1.1
    track *= dropouts(n3, length, [(30.3, 1.4), (57.5, 1.1)])
    track = reverb(track, room(4.0, 3.4, seed=1), wet=0.42)
    return loudness(middle(highpass(track, 30), length), 0.113)


def night_music():
    """'Nothing Waits': D and A held low, an E-flat creeping in a minor
    second above; under it a tone that falls forever and never arrives.
    Whispers come from one side, then the other. Twice, strings pile up
    on the wrong notes, louder and louder, and are cut off. Metal is
    bowed somewhere in the dark; a choir sinks on a tritone; a far bell."""
    length = 64.0
    t = time_axis(length)
    rng = np.random.default_rng(23)
    drone = np.zeros((len(t), 2))
    for name, level, cycles in [("D1", 0.8, 1), ("D2", 0.5, 2), ("A2", 0.25, 3)]:
        hz = note_hz(name)
        for detune, where in [(-0.07, -0.6), (0.0, 0.0), (0.07, 0.6)]:
            f = snap(hz + detune, length)
            tone = np.sin(2 * np.pi * f * t) + 0.12 * np.sin(2 * np.pi * 2 * f * t) + 0.05 * np.sin(2 * np.pi * 3 * f * t)
            drone += pan(level * tone * (0.6 + 0.4 * loop_lfo(t, length, cycles, detune * 10)), where)
    creep = np.sin(np.pi * t / length) ** 3
    for d, where in [(-0.1, -0.4), (0.1, 0.4)]:
        drone += pan(0.2 * np.sin(2 * np.pi * snap(note_hz("Eb3") + d, length) * t) * creep, where)
    # A slow throb deep down, like something breathing under the ground.
    throb = np.sin(2 * np.pi * snap(31.0, length) * t) * loop_lfo(t, length, 16) ** 4 * 0.35
    drone += pan(throb, 0)
    falling = shepard(length, length, sweep_seconds=length / 2, octaves=6, low=55.0, seed=5) * 0.22
    common = looping_noise(len(t), 520, rng, low=25)
    gust = 0.4 + 0.6 * loop_lfo(t, length, 3, 2.0) * loop_lfo(t, length, 5, 0.5)
    wind = np.stack([0.7 * common + 0.3 * looping_noise(len(t), 520, rng, low=25),
                     0.7 * common + 0.3 * looping_noise(len(t), 520, rng, low=25)], axis=1)
    wind *= 0.11 * gust[:, None]

    events = [(at, bell(note_hz(name), 9.0, seed=int(at)) * 0.5) for at, name in [(6.0, "D2"), (47.5, "Ab1")]]
    # Whispers: far on the left, then nearer on the right, then close behind.
    for at, seconds, where, level, seed in [(9.0, 3.5, -0.85, 1.4, 1), (23.0, 2.5, 0.8, 1.8, 2),
                                            (38.5, 4.0, -0.3, 1.2, 3), (55.0, 2.0, 0.9, 2.4, 4)]:
        events.append((at, pan(whisper(seconds, seed) * level, where)))
    events.append((13.0, cluster_swell(["D5", "Eb5", "E5", "Ab5", "A4", "Bb4", "C#5"], 7.5, seed=6) * 1.8))
    events.append((41.0, cluster_swell(["G#4", "A4", "D5", "Eb5", "F#5"], 5.0, seed=7) * 2.2))
    events.append((27.0, bowed_metal(note_hz("F#3"), 9.0, seed=8) * 0.5))
    events.append((50.5, bowed_metal(note_hz("C4"), 7.0, seed=9) * 0.4))
    events.append((30.0, choir(["D3", "G#3"], 12.0, "ah", sag_cents=80, seed=10) * 0.9))

    track = seamless(events, length, beds=[drone + falling + wind])
    track = reverb(track, room(5.0, 4.5, seed=2), wet=0.42)
    return loudness(middle(highpass(track, 25), length), 0.130)


def chip(hz, seconds, duty=0.5, decay=0.0, seed=0):
    """An old games-console voice: a pulse wave (`duty` 0.5 is a square,
    0.125 thin and nasal), built from its harmonics so nothing folds back
    out of tune. With `decay` the note dies away; without, it holds."""
    rng = stretch_rng(seed)
    t = time_axis(seconds)
    out = np.zeros_like(t)
    phase = rng.uniform(0, 6.28)
    k = 1
    while hz * k < 11000 and k <= 40:
        weight = np.sin(np.pi * k * duty) / k
        if abs(weight) > 1e-3:
            out += weight * np.sin(2 * np.pi * hz * k * t + phase * k)
        k += 1
    shape = np.clip(t / 0.004, 0, 1)
    if decay:
        shape = shape * np.exp(-t / decay)
    return fade(out * shape, 0, 0.02)


def triangle(hz, seconds, seed=0):
    """The console's soft voice: a triangle wave, odd harmonics falling
    fast. Slightly stepped (4-bit), as the old chips were."""
    t = time_axis(seconds)
    out = np.zeros_like(t)
    for k in range(1, 30, 2):
        if hz * k > 11000:
            break
        out += (-1) ** ((k - 1) // 2) * np.sin(2 * np.pi * hz * k * t) / k ** 2
    out = np.round(out * 8) / 8  # 4-bit steps
    return fade(out * np.clip(t / 0.004, 0, 1), 0, 0.03)


def vhs(x, length, seed=0):
    """An old tape on an old television: the top and bottom cut off, the
    sound pushed a little hard, wobbling slowly (wow) and quickly
    (flutter), hiss underneath, and now and then the tape crinkles."""
    rng = stretch_rng(seed)
    n = len(x)
    t = np.arange(n) / RATE
    x = bandpass(x, 140, 5500, order=2)
    x = np.tanh(x * 1.6) / 1.6
    # Flutter: about 6 times a second, a whole number of times a loop.
    flutter_cycles = round(6.1 * length)
    delay = 0.00012 * (1 + np.sin(2 * np.pi * flutter_cycles * t / length))
    read = np.clip(t - delay, 0, t[-1])
    x = np.stack([np.interp(read, t, x[:, c]) for c in range(2)], axis=1)
    x = wow(x, length, cents=28, seed=seed)
    level = np.sqrt(np.mean(x ** 2))
    hiss = np.stack([looping_noise(n, 9000, rng, low=1500) for _ in range(2)], axis=1)
    hiss = hiss[: n] * level * 0.06  # about 25 dB under the music
    # Crinkles: a few spots a loop where the sound sags and crackles.
    crinkle = np.ones(n)
    local = t % length
    for at in rng.uniform(1, length - 1, 3):
        near = np.exp(-((local - at) / 0.08) ** 2)
        crinkle -= 0.7 * near
        hiss += lowpass((rng.random((n, 2)) < 0.02 * near[:, None]) * rng.normal(0, level * 1.5, (n, 2)), 4000)
    return x * crinkle[:, None] + hiss


def title_music():
    """'The Island Doesn't Remember', a menu song in the style of a
    creepypasta's: a children's game tune from an old console (an
    original one), slowed down and played off a worn VHS tape. A thin
    high note keeps ringing a half-step above where it should. Then the
    tune runs backwards, and the tape ends on a broadcast tone and hiss
    before it starts again. 32 s, loops."""
    length = 32.0
    eighth = 0.36  # slow, like a tape played at the wrong speed
    down = 2 ** (-1.4 / 12)  # and a little low, from the same wrong speed
    chords = [["D5", "F5", "A5"], ["Bb4", "D5", "F5"], ["G4", "Bb4", "D5"], ["A4", "C#5", "E5"]]
    tune = ["A5", None, "F5", "G5", "A5", None, "D6", None, "C6", "Bb5", "A5", None, "G5", None, None, None,
            "F5", None, "G5", "A5", "Bb5", None, "A5", "G5", "E5", None, "F5", "E5", "C#5", None, "D5", None]

    def arps_and_tune(start_bar=0, bars=8):
        """The tune with its arpeggios, `bars` bars from `start_bar`, as a
        stereo sound."""
        n = int(bars * 8 * eighth * RATE) + int(2.0 * RATE)
        out = np.zeros((n, 2))
        for bar in range(start_bar, start_bar + bars):
            chord = chords[(bar // 2) % 4]
            local = (bar - start_bar) * 8 * eighth
            for step in range(8):
                name = chord[[0, 1, 2, 1][step % 4]]
                note = chip(note_hz(name) * down, eighth * 1.6, duty=0.125, decay=0.18, seed=bar * 8 + step)
                place(out, pan(note * 0.16, -0.35), local + step * eighth)
            # The wrong high note: a half-step above the chord's top, held.
            if bar % 2 == 1:
                wrong = note_hz(chord[2]) * 2 * 2 ** (1 / 12) * down
                place(out, pan(chip(wrong, eighth * 7, duty=0.25, seed=bar) * 0.05, 0.4), local + eighth)
            for step in range(8):
                name = tune[(bar % 4) * 8 + step]
                if name:
                    held = 1
                    while (bar % 4) * 8 + step + held < len(tune) and tune[(bar % 4) * 8 + step + held] is None and held < 4:
                        held += 1
                    note = triangle(note_hz(name) * down, eighth * held + 0.05, seed=step)
                    place(out, pan(note * 0.3, 0.15), local + step * eighth)
            # A bass that thumps like a cardboard box.
            if True:
                root = note_hz(chord[0]) / 4 * down
                place(out, pan(triangle(root, eighth * 3.5) * 0.35, 0), local)
                place(out, pan(triangle(root, eighth * 3.5) * 0.25, 0), local + 4 * eighth)
        return out

    forwards = arps_and_tune(0, 8)  # 8 bars, 23 s
    backwards = arps_and_tune(0, 2)[::-1].copy() * 0.8  # the first two bars, reversed
    t = time_axis(length)
    bed = np.zeros((len(t), 2))
    for name, level in [("D2", 0.25), ("Eb2", 0.06)]:
        for detune, where in [(-0.05, -0.5), (0.05, 0.5)]:
            f = snap(note_hz(name) * down + detune, length)
            bed += pan(level * np.sin(2 * np.pi * f * t), where)

    end_of_tune = 8 * 8 * eighth
    events = [(0.0, fade(forwards[: int((end_of_tune + 0.4) * RATE)], 0, 0.25)),
              (end_of_tune + 0.3, fade(backwards[int(1.6 * RATE):], 0.05, 0))]
    # The tape runs out: a broadcast tone, not loud, a little off-pitch.
    tone_at = end_of_tune + 0.3 + len(backwards) / RATE - 1.6 + 0.2
    tone_len = max(0.5, length - tone_at - 1.4)
    tt = time_axis(tone_len)
    beep = (np.sin(2 * np.pi * 853 * tt) + np.sin(2 * np.pi * 960 * tt)) * 0.035
    beep *= np.clip(tt / 0.01, 0, 1) * np.clip((tone_len - tt) / 0.01, 0, 1)
    events.append((tone_at, pan(beep, 0)))
    events.append((tone_at + 0.4, pan(whisper(min(2.0, tone_len), seed=9) * 0.5, -0.6)))

    track = seamless(events, length, beds=[bed])
    track = vhs(track, length, seed=6)
    track = reverb(track, room(2.5, 1.6, seed=3), wet=0.25)
    music = middle(highpass(track, 40), length)
    music = record_skip(music, 9.5, 0.36, 3)
    return loudness(music, 0.145)


def wind_loop():
    """The island's wind, 24 s: a low rush that gusts, a faint whistle in
    the gusts, each ear a little different. Played under everything."""
    length = 24.0
    t = time_axis(length)
    rng = np.random.default_rng(31)
    gust = 0.35 + 0.65 * loop_lfo(t, length, 2, 0.3) * loop_lfo(t, length, 5, 1.9)
    ears = []
    common = looping_noise(len(t), 650, rng, low=40)
    whistle_common = looping_noise(len(t), 1500, rng, low=700)
    for side in range(2):
        rush = 0.75 * common + 0.25 * looping_noise(len(t), 650, rng, low=40)
        whistle = 0.6 * whistle_common + 0.4 * looping_noise(len(t), 1500, rng, low=700)
        lean = loop_lfo(t, length, 1, side * np.pi)  # gusts lean left, then right
        ears.append(rush * gust * (0.8 + 0.2 * lean) + 0.12 * whistle * gust ** 3)
    return loudness(np.stack(ears, axis=1), 0.092)


# ---- Effects ----


def saw(f):
    """A sawtooth with frequency `f` (an array: it can glide)."""
    phase = np.cumsum(f) / RATE
    return 2 * (phase % 1.0) - 1


def voice(f, formants, rng, breath=0.25):
    """A throat: a buzzing saw through mouth-shaped band-passes, and air."""
    buzz = saw(f)
    out = np.zeros_like(f)
    for (low, high), level in formants:
        out += bandpass(buzz, low, high) * level
    out += bandpass(rng.normal(0, 1, len(f)), 300, 2400) * breath
    return out


def peak(x, level):
    x = balance(x)
    return x / (np.max(np.abs(x)) + 1e-9) * level


SMALL_ROOM = None


def small_room():
    global SMALL_ROOM
    if SMALL_ROOM is None:
        SMALL_ROOM = room(1.6, 1.1, seed=7, ears=1, predelay=0.01)
    return SMALL_ROOM


def groan():
    """Low, slow, wavering: something that used to be a duck."""
    rng = np.random.default_rng(5)
    t = time_axis(1.8)
    f = 62 + 8 * np.sin(2 * np.pi * 0.7 * t) + 3 * np.sin(2 * np.pi * 6.5 * t) - 10 * t
    jitter = lowpass(rng.normal(0, 1, len(t)), 30) * 30
    out = voice(f + jitter, [((90, 400), 1.2), ((450, 800), 1.0), ((900, 1400), 0.4), ((2200, 3200), 0.12)], rng, breath=0.06)
    out = np.tanh(out * 2.5)  # a rasp
    out = lowpass(out, 7000)
    out *= envelope(len(t), 0.35, 0.9)
    return fade(peak(reverb(out, small_room(), 0.3), 0.7), 0, 0.1)


def groan_far():
    return lowpass(groan(), 700) * 0.35


def hollow_hurt():
    rng = np.random.default_rng(6)
    t = time_axis(0.45)
    f = 140 - 120 * t
    out = voice(f, [((350, 900), 1.0), ((1200, 2000), 0.4), ((2600, 3600), 0.15)], rng, breath=0.5)
    out = lowpass(np.tanh(out * 3), 8000)  # rough
    out *= envelope(len(t), 0.01, 0.15)
    return fade(peak(out, 0.7), 0, 0.05)


def hollow_die():
    """A falling moan that breaks into dry crackle, like ash."""
    rng = np.random.default_rng(7)
    t = time_axis(1.6)
    f = 110 * np.exp(-t * 1.4) + 30
    moan = voice(f, [((90, 400), 1.2), ((250, 650), 1.0)], rng, breath=0.08) * envelope(len(t), 0.02, 0.5)
    moan = np.tanh(moan * 2)
    crackle = (rng.random(len(t)) < 0.003 * np.clip(t * 2 - 0.3, 0, 1)) * rng.normal(0, 1, len(t))
    crackle = bandpass(crackle, 1500, 9000) * 2 * np.exp(-np.maximum(t - 0.6, 0) * 2)
    return fade(peak(reverb(moan + crackle, small_room(), 0.35), 0.75), 0, 0.2)


def sting():
    """The scare: a boom, and strings that rise all wrong together, spread
    across both ears."""
    rng = np.random.default_rng(8)
    t = time_axis(2.6)
    boom = np.sin(2 * np.pi * (55 * np.exp(-t * 3) + 30) * t) * envelope(len(t), 0.005, 0.6)
    boom += lowpass(rng.normal(0, 1, len(t)), 200) * envelope(len(t), 0.002, 0.3) * 2
    out = pan(boom * 0.9, 0)
    rise = np.clip(t / 0.25, 0, 1) * np.exp(-np.maximum(t - 0.9, 0) * 3)
    for i, name in enumerate(["D6", "Eb6", "E6", "Ab6", "A5", "Bb5"]):
        hz = note_hz(name)
        vibrato = 1 + 0.006 * np.sin(2 * np.pi * rng.uniform(5, 7) * t + rng.uniform(0, 6))
        glide = 1 + 0.03 * t  # bending up, wrong
        string = bandpass(saw(np.full_like(t, hz) * vibrato * glide), 600, 7000) / 6
        out += pan(string * rise * 1.2, -0.8 + 1.6 * i / 5)
    return fade(peak(reverb(out, room(3.0, 2.4, seed=8), 0.4), 0.85), 0, 0.3)


def heartbeat():
    """Lub-dub, one second: loops at 60 beats a minute."""
    t = time_axis(1.0)
    out = np.zeros_like(t)
    for at, hz, level in [(0.0, 55, 1.0), (0.26, 48, 0.75)]:
        local = np.clip(t - at, 0, None)
        out += np.sin(2 * np.pi * hz * local) * np.exp(-local / 0.06) * (t >= at) * level
    return peak(lowpass(out, 180), 0.8)


def bell_dusk():
    out = bell(note_hz("D2"), 7.0, seed=9)
    return fade(peak(reverb(out, room(4.0, 3.5, seed=9), 0.4), 0.7), 0, 0.5)


def bell_dawn():
    """Two small notes, high and plain: morning, for what it's worth."""
    out = np.zeros((int(4.0 * RATE), 2))
    place(out, bell(note_hz("A5"), 3.5, seed=10) * 0.6, 0.0)
    place(out, bell(note_hz("D5"), 3.0, seed=11) * 0.5, 0.45)
    return fade(peak(reverb(out, room(3.0, 2.2, seed=10), 0.35), 0.6), 0, 0.5)


def rain_loop():
    """Rain, 16 s, stereo, loops: a hiss of many drops, a little different
    in each ear, and now and then a nearer drop. Played louder outside
    than under a roof."""
    length = 16.0
    t = time_axis(length)
    rng = np.random.default_rng(41)
    ears = []
    common = looping_noise(len(t), 7000, rng, low=500)
    for side in range(2):
        hiss = 0.7 * common + 0.3 * looping_noise(len(t), 7000, rng, low=500)
        swell = 0.85 + 0.15 * loop_lfo(t, length, 3, side * 1.3)
        drops = np.zeros(len(t))
        for at in rng.uniform(0, length - 0.05, 260):
            k = int(at * RATE)
            n = int(0.012 * RATE)
            drops[k:k + n] += rng.normal(0, 1, n) * np.exp(-np.arange(n) / (0.002 * RATE)) * rng.uniform(0.3, 1.0)
        drops = bandpass(drops, 1800, 6500)
        ears.append(hiss * swell * 0.5 + drops * 0.9)
    return loudness(np.stack(ears, axis=1), 0.08)


def thunder():
    """A crack, and then the long roll, rumbling off into the distance."""
    rng = np.random.default_rng(42)
    t = time_axis(6.0)
    crack = highpass(rng.normal(0, 1, len(t)), 900) * envelope(len(t), 0.002, 0.08)
    rumble = lowpass(rng.normal(0, 1, len(t)), 160)
    rumble = rumble / (np.max(np.abs(rumble)) + 1e-9)
    roll = np.exp(-t / 1.8) * (0.6 + 0.4 * np.sin(2 * np.pi * 0.9 * t + 1.0) ** 2) * np.clip(t / 0.15, 0, 1)
    left = crack * 0.6 + rumble * roll * 2.2
    right = crack * 0.45 + np.roll(rumble, int(0.03 * RATE)) * roll * 2.2
    out = np.stack([left, right], axis=1)
    return fade(peak(reverb(out, room(4.0, 3.5, seed=43), 0.45), 0.9), 0, 0.8)


def main():
    OUT.mkdir(exist_ok=True)
    print("music:")
    write("music_day.wav", day_music(), 1)
    write("music_night.wav", night_music(), 2)
    write("music_title.wav", title_music(), 3)
    write("wind.wav", wind_loop(), 4)
    write("rain.wav", rain_loop(), 5)
    print("effects:")
    for i, (name, make) in enumerate([
            ("groan", groan), ("groan_far", groan_far), ("hollow_hurt", hollow_hurt), ("hollow_die", hollow_die),
            ("sting", sting), ("heartbeat", heartbeat), ("bell_dusk", bell_dusk), ("bell_dawn", bell_dawn),
            ("thunder", thunder)]):
        write(f"{name}.wav", make(), 10 + i)


if __name__ == "__main__":
    main()
