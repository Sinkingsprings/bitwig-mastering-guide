# Mastering Guide — Roadmap

This document captures the architectural plan and phased work agreed on
2026-04-27, after studying:

- **Sapphire Plugins** (`Plugins To Study/sapphire-plugins-main/`) — reference
  CLAP architecture: lock-free audio↔UI ring buffer, `OnePoleLag` parameter
  smoothing in N-sample blocks, sample-accurate `processEventsUpTo`,
  BEGIN/END_EDIT gestures, editor-attach push refresh, tooltip-with-live-value.
- **Lapatas — *The Art of Mixing & Mastering*** — three pillars (volume/pan,
  spectral coverage, spatial positioning), instrument priority hierarchy,
  specific masking pairs (bass-vs-bass-drum 80–120 Hz, vocal-vs-guitar
  200–3 kHz), bass near-mono, mono-compatibility, macrodynamics vs
  microdynamics, do-no-harm mastering ethic.
- **Bitwig Studio User Guide v5.3** — host extension surface and project
  structure (track types, group tracks, cue markers, transport).
- **`bitwig-extensions-main`** — official Java controller-extension repo;
  authoritative reference for the Bitwig Controller API surface.

## Architecture

```
┌─ Bitwig Studio ────────────────────────────────────────────────┐
│                                                                │
│  every track  ── (optional) MasteringGuide.clap [Skipper]      │
│      │         per-track LUFS, true peak, FFT, correlation     │
│      │                                                         │
│      └────────► IPC bridge (Unix domain socket) ◄────┐         │
│                                                      │         │
│  Gilligan.bwextension (Java controller)              │         │
│   ├─ TrackBank → name/color/type/VU/vol/pan          │         │
│   ├─ publishes track metadata                        │         │
│   ├─ executes apply-fix actions                      │         │
│   └─ scheduled tick via host.scheduleTask            │         │
└────────────────────────────────────────────────────────────────┘
```

**Out of scope.** No MCP. No Claude Code integration. No Anthropic API. No
external network. Gilligan binds nothing; the IPC is a local Unix socket.

**Division of labor.**
- *Skipper* (CLAP, Rust, audio thread) — anything sample-accurate: LUFS,
  true-peak, FFT, correlation, dynamics, rule engine, GUI.
- *Gilligan* (Java extension, controller thread) — anything the Controller
  API exposes for free: track names/types/colors, post-fader VU, fader
  positions, transport, cue markers — and the *write* side: applying fixes
  via `track.volume()`, device insertion, undo blocks.

## Why Gilligan, given there is no AI

Three concrete jobs that justify the extension on its own:

1. **Eliminate per-track plugin requirement for metadata.** Today the user
   inserts MasteringGuide on every track and numbers slots manually. Gilligan
   reads name / color / type / group parent / post-fader VU directly from
   the Controller API; the track table populates automatically. The CLAP only
   needs to be on the master, plus optionally on tracks where real
   LUFS/spectrum analysis (not VU) is wanted.
2. **Make the "Fix:" advice actually do something.** Today every advice card
   ends with prose ("Reduce Synth fader by 4 dB") and the user does it by
   hand. With Gilligan, that line becomes a button — Skipper emits a
   structured `FixAction`, Gilligan executes it via `track.volume()` wrapped
   in `application.beginUndoStateBlock()`.
3. **Auto-fill track roles** (`Vocal | Drums | Bass | Harm | Pad | FX`) from
   track names / colors / types instead of a manual dropdown per instance.

## Bitwig Controller API surface (verified against `bitwig-extensions-main`)

- `host.createTrackBank(N, sends, scenes, hasFlatTrackList)`
- `track.addVuMeterObserver(rangeMax, channel, peakMode, callback)` —
  post-fader level metering
- `track.trackType()` → `"Audio" | "Instrument" | "Effect" | "Master" | "Group"`
- `track.isGroup()`, `track.color()`, `track.name()`, `track.position()`
- `track.volume()`, `pan()`, `sendBank()`, `mute()`, `solo()` (read + write)
- `track.createDeviceBank(N).setDeviceMatcher(matcher)` — locate plugins
- `device.createCursorRemoteControlsPage(N)` — drive parameters
- `host.createMasterTrack(N)`, `createEffectTrackBank(N, sends)`
- `host.createApplication()` — undo/redo, navigation
- `application.beginUndoStateBlock(name)` / `endUndoStateBlock()` — group
  multiple changes into one user-visible undo
- `host.scheduleTask(runnable, ms)` — single-threaded periodic tick;
  the only safe way to drive non-controller-thread work back into the API

## Phased plan

### Phase 9 — pure-Skipper DSP wins

Highest-leverage features from the Lapatas book that don't need Gilligan.

1. **Spectral tilt metric.** Linear regression slope of the 10 bands, in
   dB/oct. One number replaces several per-band rules. Add to `TrackFrame`
   and `evaluator.rs`.
2. **True mono-compatibility check.** Compute LUFS of `(L+R)/2` summed
   signal alongside stereo LUFS (one extra `ebur128` instance). New rule:
   `mono_loss_lu > 2.0`.
3. **Per-band correlation array.** Cheap once the FFT is available — for
   each band compute `corr(L, R)` from cross/autospectra. Drives a new
   rule: *"sub-bass correlation 0.4 — bass too wide, will collapse in
   mono"* (Lapatas: "keep bass more monophonic").
4. **Macrodynamics.** 60-second ring buffer of short-term LUFS in
   `lufs.rs`; report `lufs_p95 - lufs_p5`. Two rules: low section contrast
   (<2 LU) and wild section jumps (>10 LU).
5. **Reference-track snapshot.** GUI-only — a `[Capture Ref]` button copies
   the current `TrackFrame.bands_dbfs` + LUFS/PSR/PLR into a sticky
   `Option<TrackFrame>`. Spectrum widget overlays it as a dashed line.

### Phase 10 — track-role enum

6. `TrackRole` enum param: `Vocal | Drums | Bass | Harm | Pad | FX | Auto`,
   default `Auto`.
7. Role-aware rules in `evaluate_mix_balance`: bass-vs-bass-drum collision
   at 80–120 Hz, vocal-vs-harm masking at 200 Hz–3 kHz — the specific
   examples Lapatas calls out by name.
8. `Auto` will be auto-filled by Gilligan in Phase 13.

### Phase 11 — Gilligan skeleton

9. New `gilligan/` Java/Gradle project mirroring the structure of
   `bitwig-extensions-main`. One extension definition, declared as a
   controller targeting "Mastering Guide".
10. On `init(host)`:
    - `host.createTrackBank(64, 0, 0, true)` — flat track list, every track
      visible regardless of group nesting.
    - For each track: `markInterested()` on `name() / color() /
      trackType() / isGroup() / position()`, plus
      `addVuMeterObserver(127, 0/1, true, …)` for L and R post-fader VU.
    - `host.createMasterTrack(0)` for the master row.
11. **IPC bridge.** Skipper runs in Bitwig's CLAP subprocess on Linux —
    *not* in the Bitwig JVM. Use a Unix domain socket at
    `/tmp/skipper-gilligan-${user}.sock`, length-prefixed JSON Lines.
    - Gilligan opens a worker `Thread` that owns the socket.
    - Inbound messages drain into the controller thread via a
      `BlockingQueue` polled by `host.scheduleTask`.
    - Skipper opens the socket from a non-audio thread (the GUI thread or
      a dedicated I/O thread); the audio thread never touches the socket.
12. Gilligan publishes `TrackTable { tracks: [TrackMeta { idx, name, color,
    role_hint, type, group_parent_idx, vu_post_fader_db_l, _r }] }` every
    100 ms. Skipper merges into its frame snapshot. Track names auto-populate.

### Phase 12 — apply-fix actions

13. Extend `Advice`:
    ```rust
    pub struct Advice {
        // ...existing fields
        pub action: Option<FixAction>,
    }
    pub enum FixAction {
        AdjustVolume { track_idx: u32, delta_db: f32 },
        AdjustPan    { track_idx: u32, delta: f32 },
        ToggleMute   { track_idx: u32 },
        // later: InsertDevice { device_uuid, track_idx, position }
    }
    ```
14. GUI: each advice card with `Some(action)` gets `[Apply]`. Click pushes a
    `FixAction` onto an outbound queue.
15. Gilligan polls the queue from `scheduleTask` and executes:
    ```java
    application.beginUndoStateBlock("Mastering Guide: " + actionName);
    track.volume().inc(deltaNormalized, range);
    application.endUndoStateBlock();
    ```
    One click in the plugin = one undoable change in Bitwig.

### Phase 13 — auto track-role

16. In Gilligan, regex over `track.name()` (`kick|snare|drum|perc|hat`,
    `bass|sub`, `vox|vocal|lead|harm`, `pad|atmos|fx`, etc.) plus
    `trackType()` and `color()` palette matching, fills `role_hint` for
    every slot.
17. Skipper's `TrackRole::Auto` reads the hint; manual override remains.

### Phase 14 — section-aware analysis (optional, last)

18. Gilligan reads `host.createTransport().getPosition()` and the cue-marker
    bank, emits `Section { name, start_beat, end_beat, current }`.
19. Skipper buckets short-term LUFS / PSR by section name. New rules:
    *"Verse 1 –22 LUFS vs Chorus 1 –10 LUFS — 12 LU jump. Automate the
    master fader rather than relying on compression."*

## What's deliberately not in this plan

- No DSP duplication in Gilligan — it never re-implements LUFS / FFT.
- No network exposure — IPC is Unix domain socket, local-only.
- No mandatory Gilligan — Phases 9–10 keep the plugin standalone-useful
  for users who don't install the extension.
- No AI / MCP / external-control bridge.

## DSP polish from Sapphire (apply when relevant)

- Polyphase FIR true-peak (vs. current 4× linear interpolation).
- 1/3-octave FFT bands (needed before spectral-tilt rule is reliable).
- `OnePoleLag` parameter smoothing in 8-sample blocks for any user-facing
  knobs we add (nih-plug already smooths declared params; this matters for
  any DSP-side smoothed values we introduce).
- `stateLoad → snap()` to skip smoothing ramp on patch load.

## First move

Phase 9 items 1–3 (spectral tilt, mono-compat, per-band correlation). Each
is roughly 50 lines, well-bounded, and changes no architecture.
