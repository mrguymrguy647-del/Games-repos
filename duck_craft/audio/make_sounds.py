"""Writes Duck Craft's music and sound effects as .wav files, from nothing
but sine waves, noise and arithmetic (Duck Echo isn't ready yet).

    python3 audio/make_sounds.py          # run in duck_craft/

writes into assets/, all at 44100 samples a second (the engine's own rate,
so nothing is resampled when the game loads them):

  music_day.wav    "Still Water": slow felt piano over a soft pad, D minor,
                   64 s, stereo, loops with no seam
  music_night.wav  "Nothing Waits": a low drone, a minor second that comes
                   and goes, wind, a far bell; 64 s, stereo, loops
  music_title.wav  "The Island Doesn't Remember": a falling piano figure
                   over the drone; 32 s, stereo, loops
  wind.wav         the island's wind, 24 s, stereo, loops
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
            tail += bandpass(noise, low, high) * np.exp(-6.9 * t / (decay * scale))
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


def loudness(x, rms, ceiling=0.92):
    """Scaled to an average loudness (`rms`, like the old files, so the
    game's mix doesn't change), with any rare peak above `ceiling`
    rounded off gently instead of clipped."""
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


# ---- Music ----


def day_music():
    """'Still Water': i - VI - III - VII in D minor, four bars a chord at
    60 beats a minute. A felt piano wanders over a pad; bass notes on the
    first beat. Sparse on purpose: lots of room for the wind."""
    length = 64.0
    rng = np.random.default_rng(11)
    chords = [
        (["D3", "F3", "A3"], "D2", ["D4", "F4", "A4", "C5", "E5", "D5"]),
        (["Bb2", "D3", "F3"], "Bb1", ["D4", "F4", "Bb4", "A4", "C5", "D5"]),
        (["F3", "A3", "C4"], "F2", ["C4", "F4", "A4", "C5", "G4", "E4"]),
        (["C3", "E3", "G3"], "C2", ["C4", "E4", "G4", "D5", "E5", "G4"]),
    ]
    # The melody, chosen once (the same each pass): per chord, 16 beats,
    # a note on some of them.
    melody = []
    for c, (_, _, notes) in enumerate(chords):
        for beat in range(16):
            # Fewer notes as each chord goes on; never on the very last beat.
            if beat < 15 and rng.random() < (0.55 if beat % 4 == 0 else 0.22):
                melody.append((c * 16 + beat + rng.choice([0, 0, 0.5]), rng.choice(notes), rng.uniform(0.5, 0.85)))

    events = []
    for c, (chord, bass, _) in enumerate(chords):
        start = c * 16.0
        events.append((start - 1.0, pad(chord, 18.0, 0.55)))
        for bar in range(4):
            velocity = 0.55 if bar % 2 == 0 else 0.35
            events.append((start + bar * 4.0, piano(note_hz(bass), 7.0, velocity, seed=c * 4 + bar)))
    for i, (beat, name, velocity) in enumerate(melody):
        events.append((beat, piano(note_hz(name), 6.0, velocity * 0.6, seed=100 + i)))

    track = seamless(events, length)
    track = reverb(track, room(4.0, 3.2, seed=1), wet=0.38)
    return loudness(middle(highpass(track, 30), length), 0.113)


def night_music():
    """'Nothing Waits': D and A held low, an E-flat that creeps in a minor
    second above (and leaves), wind, a thin whine that drifts from ear to
    ear, and a bell now and then, far away."""
    length = 64.0
    t = time_axis(length)
    rng = np.random.default_rng(23)
    drone = np.zeros((len(t), 2))
    for name, level, cycles in [("D1", 0.9, 1), ("D2", 0.55, 2), ("A2", 0.30, 3)]:
        hz = note_hz(name)
        for detune, where in [(-0.07, -0.6), (0.0, 0.0), (0.07, 0.6)]:
            f = snap(hz + detune, length)
            tone = np.sin(2 * np.pi * f * t) + 0.12 * np.sin(2 * np.pi * 2 * f * t) + 0.05 * np.sin(2 * np.pi * 3 * f * t)
            drone += pan(level * tone * (0.6 + 0.4 * loop_lfo(t, length, cycles, detune * 10)), where)
    # The minor second: comes in over the first half of the loop, leaves
    # over the second.
    creep = np.sin(np.pi * t / length) ** 3
    for d, where in [(-0.1, -0.4), (0.1, 0.4)]:
        drone += pan(0.18 * np.sin(2 * np.pi * snap(note_hz("Eb3") + d, length) * t) * creep, where)
    # A faint high whine, a tritone above, wandering slowly between the ears.
    whine = 0.03 * np.sin(2 * np.pi * snap(note_hz("G#5"), length) * t) * loop_lfo(t, length, 2, 1.0)
    drift = np.sin(2 * np.pi * t / length)
    angle = (np.clip(drift * 0.8, -1, 1) + 1) * np.pi / 4
    drone += np.stack([whine * np.cos(angle), whine * np.sin(angle)], axis=1) * np.sqrt(2)
    # Wind: noise, low-passed, gusting, not quite the same in each ear.
    common = looping_noise(len(t), 520, rng, low=25)
    gust = 0.4 + 0.6 * loop_lfo(t, length, 3, 2.0) * loop_lfo(t, length, 5, 0.5)
    wind = np.stack([0.7 * common + 0.3 * looping_noise(len(t), 520, rng, low=25),
                     0.7 * common + 0.3 * looping_noise(len(t), 520, rng, low=25)], axis=1)
    wind *= 0.12 * gust[:, None]

    events = [(at, bell(note_hz(name), 9.0, seed=int(at)) * 0.6) for at, name in [(6.0, "D2"), (29.0, "Ab2"), (47.5, "D2")]]
    track = seamless(events, length, beds=[drone + wind])
    track = reverb(track, room(5.0, 4.5, seed=2), wet=0.4)
    return loudness(middle(highpass(track, 25), length), 0.130)


def title_music():
    """'The Island Doesn't Remember': a four-note figure falling through
    D minor, over the low drone, slowing, then the bell."""
    length = 32.0
    t = time_axis(length)
    drone = np.zeros((len(t), 2))
    for name, level in [("D2", 0.5), ("A2", 0.25)]:
        for detune, where in [(-0.06, -0.5), (0.06, 0.5)]:
            f = snap(note_hz(name) + detune, length)
            drone += pan(level * (np.sin(2 * np.pi * f * t) + 0.1 * np.sin(2 * np.pi * 2 * f * t)), where)
    drone *= (0.7 + 0.3 * np.sin(2 * np.pi * t / length))[:, None]
    figure = [("A4", 0.0), ("F4", 1.0), ("E4", 2.0), ("D4", 3.0),
              ("G4", 4.0), ("E4", 5.0), ("D4", 6.0), ("C#4", 7.0),
              ("F4", 8.0), ("D4", 9.0), ("C4", 10.0), ("Bb3", 11.0),
              ("A3", 12.5), ("D4", 14.5)]
    events = [(1.0 + beat * 1.1, piano(note_hz(name), 7.0, 0.55, seed=i)) for i, (name, beat) in enumerate(figure)]
    events.append((19.0, bell(note_hz("D2"), 10.0, seed=3) * 0.3))
    track = seamless(events, length, beds=[drone])
    track = reverb(track, room(5.0, 4.0, seed=3), wet=0.45)
    return loudness(middle(highpass(track, 25), length), 0.145)


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


def main():
    OUT.mkdir(exist_ok=True)
    print("music:")
    write("music_day.wav", day_music(), 1)
    write("music_night.wav", night_music(), 2)
    write("music_title.wav", title_music(), 3)
    write("wind.wav", wind_loop(), 4)
    print("effects:")
    for i, (name, make) in enumerate([
            ("groan", groan), ("groan_far", groan_far), ("hollow_hurt", hollow_hurt), ("hollow_die", hollow_die),
            ("sting", sting), ("heartbeat", heartbeat), ("bell_dusk", bell_dusk), ("bell_dawn", bell_dawn)]):
        write(f"{name}.wav", make(), 10 + i)


if __name__ == "__main__":
    main()
