# Mastering Guide

A CLAP audio plugin for Bitwig Studio that acts as an always-on mastering
engineer. It monitors every track in your session in real time, then runs a
rule-based analysis engine to tell you exactly what to fix — and how to fix it.

No internet connection, no LLM, no subscription. All intelligence is built
into the plugin itself.

---

## What it does

- Measures **LUFS** (integrated, short-term, momentary), **true peak**,
  **RMS**, **PLR**, **PSR**, and **stereo correlation** on every track
- Analyses **10-band octave spectrum** and compares it against a
  genre-appropriate reference curve
- Runs **~30 mastering rules** covering loudness, dynamics, frequency balance,
  stereo field, and technical issues
- Shows the results as prioritised, actionable advice:
  - ⛔ **Critical** — will cause problems at delivery (true peak violation, clipping, phase inversion)
  - ⚠ **Warning** — audible quality issues (over-compression, mud, harshness)
  - → **Suggestion** — improvements that could strengthen the master
  - ✓ **Good** — positive confirmation of things working well
- Supports **7 genre targets** and **7 streaming platform targets**
- **Auto-analyze** mode re-runs the engine every 5 seconds while you work

---

## Screenshots

**Master view** — spectrum chart, track table, and advice panel:

```
MASTERING GUIDE                           #1  [Master ◀▶]
──────────────────────────────────────────────────────────
MASTER BUS
  Int LUFS  True Peak   PLR     PSR min
  -14.2     -0.8 dBTP   9.1 LU  8.4 LU
Corr [████████████░░░░░░░░░]  +0.62

SPECTRUM  (bars = master bus · line = genre ref)
 0 ┤
-18┤  ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓ ▓▓  ← genre ref line
-36┤
-54┤
    31 63 125 250 500 1k  2k  4k  8k 16k

TRACK READINGS
  Track    LUFS    Peak    PLR    Corr
  Kick     -18.4   -2.1    16.3   +0.92
  Bass     -16.2   -1.8    14.4   +0.95
  Synth    -9.8 ⚠  -0.9    8.9    +0.45
──────────────────────────────────────────────────────────
Genre [Pop/R&B ◀▶]  Platform [Spotify ◀▶]
[Analyze Now]  ☑ Auto (5s)  ☑ Track spectrum
──────────────────────────────────────────────────────────
⛔ True peak exceeds platform ceiling
   Master bus true peak is -0.8 dBTP, but Spotify requires -1.0 dBTP...
   Fix: Set your limiter ceiling to -1.5 dBTP...

⚠ Synth is significantly louder than the mix
   Synth is at -9.8 LUFS — 4.4 LU louder than master bus average...
```

---

## Requirements

- [Bitwig Studio](https://www.bitwig.com) 4.3 or later (CLAP support required)
- Linux x86-64 (macOS and Windows builds are possible; see [Building](#building))
- To build from source: [Rust](https://rustup.rs) 1.70+

---

## Installation

### Pre-built (Linux)

1. Download `MasteringGuide.clap` from the
   [Releases](https://github.com/Sinkingsprings/bitwig-mastering-guide/releases) page
2. Copy it to `~/.clap/`
3. In Bitwig: **Preferences → Plug-ins → Rescan**
4. Search for "Mastering Guide" in the plugin browser

### Build from source

```bash
git clone https://github.com/Sinkingsprings/bitwig-mastering-guide.git
cd bitwig-mastering-guide/mastering_guide
cargo run --package xtask -- bundle --release
cp target/release/bundled/MasteringGuide.clap ~/.clap/
```

---

## Usage

### Setup in Bitwig

1. Add a **Track mode** instance of Mastering Guide to every track you want to
   monitor. It passes audio through unchanged — insert it anywhere in the FX chain.

2. Add a **Master mode** instance to the **Master bus**. Switch the `Mode`
   slider to `Master`.

3. Press **Play** (or load a project) to let the meters stabilise for a few
   seconds.

4. Press **Analyze Now** in the Master instance. The advice panel will populate
   with findings sorted from most critical to most subtle.

### Tips

- **Genre and Platform** selectors in the Master view change the rule
  thresholds. Set them to match your project before analyzing.
- **Auto (5 s)** re-runs the engine automatically while you make changes,
  so the advice panel stays current without manual clicks.
- **Track spectrum** overlay shows each track's 10-band spectrum as coloured
  lines on top of the master spectrum chart — useful for spotting which tracks
  are causing a frequency imbalance.
- Slot numbers (`#1`, `#2`, …) are assigned automatically. Note which number
  corresponds to which track so you can read the track table correctly.

---

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    Bitwig Studio                        │
│                                                         │
│  [Track 1] → [MasteringGuide #1] → audio passthrough   │
│  [Track 2] → [MasteringGuide #2] → audio passthrough   │
│  [Track N] → [MasteringGuide #N] → audio passthrough   │
│  [Master]  → [MasteringGuide #M]                        │
│                        │                                │
│              Process-global slot table                  │
│              (OnceLock<Mutex<Vec<Slot>>>)                │
│                        │                                │
│                  Rule Engine                            │
│                  ├── ~30 rules                          │
│                  ├── 7 genre curves                     │
│                  └── 7 platform targets                 │
│                        │                                │
│                   egui GUI                              │
└─────────────────────────────────────────────────────────┘
```

**All instances share data inside the same host process** — no OS-level IPC,
no network calls, no files on disk.

### DSP pipeline (per instance, audio thread)

| Component | Implementation |
|---|---|
| LUFS (momentary / short-term / integrated) | `ebur128` crate — ITU-R BS.1770-4 |
| True peak | 4× linear-interpolation oversampling |
| RMS | 300 ms sliding window |
| PSR / PLR | Derived from LUFS + true peak |
| Stereo correlation | Pearson correlation coefficient |
| 10-band octave spectrum | `realfft` + IEC 61260 band sums |

### Rule categories

| Category | Rules | Examples |
|---|---|---|
| Technical | 5 | True peak ceiling, digital clipping, DC offset |
| Loudness | 5 | LUFS vs. platform target, track outliers |
| Dynamics | 6 | PSR / PLR vs. genre minimum, over-compression |
| Frequency | 8 | Mud, harshness, missing air, sub-bass excess |
| Stereo | 6 | Phase inversion, mono compatibility, narrow field |
| Mix balance | 4 | Low-end competition, frequency masking |

---

## Supported genres

Pop/R&B · Rock · EDM/Dance · Hip-Hop · Jazz/Acoustic · Classical · Folk

## Supported platforms

Spotify (−14 LUFS / −1 dBTP) · Apple Music (−16 / −1) · YouTube (−13 / −1) ·
Amazon Music (−14 / −2) · Tidal (−14 / −1) · Broadcast EBU R128 (−23 / −1) ·
SoundCloud (no normalization / −0.3)

---

## Project structure

```
mastering_guide/
├── src/
│   ├── analysis/      # DSP — runs on audio thread
│   │   ├── lufs.rs    # LUFS via ebur128
│   │   ├── peak.rs    # True peak + RMS
│   │   ├── spectrum.rs # FFT → 10-band octave RMS
│   │   ├── stereo.rs  # Correlation + M/S
│   │   ├── dynamics.rs # PSR / PLR / DR
│   │   └── frame.rs   # TrackFrame snapshot (double-buffered)
│   ├── engine/        # Rule engine — main thread
│   │   ├── evaluator.rs # Runs all rules, returns Vec<Advice>
│   │   ├── genres.rs  # Genre spectral curves
│   │   └── platforms.rs # Platform targets
│   ├── ipc/           # Process-global multi-instance registry
│   │   └── registry.rs
│   └── gui/           # egui UI
│       ├── mod.rs     # Track + master views
│       └── spectrum.rs # Spectrum chart widget
└── xtask/             # cargo xtask bundle
```

---

## Contributing

Issues and PRs welcome. Some ideas for future work:

- [ ] Read track name from `clap.track-info` host extension
- [ ] 1/3-octave spectrum for finer frequency analysis
- [ ] Polyphase FIR true-peak oversampler (more accurate than linear interp)
- [ ] Export analysis report as text/JSON
- [ ] macOS and Windows builds via CI
- [ ] Per-track history graphs

---

## License

MIT
