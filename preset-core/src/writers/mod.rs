// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Preset file writers. Each writer takes the *mapped* parameter values from a
// mapper and emits a real preset file for the target synth:
//
//   - `fxp`   — the generic VST2 FXP header/chunk container (ported from the
//               legacy Python `fxp_generator.py`), used by Surge.
//   - `surge` — the actual Surge XT patch format written *inside* an FXP chunk.
//               Real Surge XT does NOT load the legacy JSON-in-FXP blob; this
//               writer produces the `FPCh`/`cjs3` + `sub3` XML patch Surge reads.
//   - `syx`   — the DX7 155-byte SysEx voice (ported from `syx_generator.py`).
//   - `vital` — Vital's `.vital` JSON preset.

pub mod fxp;
pub mod surge;
pub mod surge_encode;
pub mod syx;
pub mod vital;

mod surge_init_data;
