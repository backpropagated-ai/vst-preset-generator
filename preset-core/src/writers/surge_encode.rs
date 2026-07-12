// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Surge XT value encodings — the single source of truth for how our abstract
// parameters translate into Surge's *internal* stored patch values.
//
// Every function and table here is verified against the Surge XT GPL-3.0 source
// at `legacy/surge/src/common/` (Parameter.cpp `set_type`, SurgePatch.cpp
// `assign` calls, SurgeStorage.h enums, ModulationSource.h). Each carries a
// `file:line` citation so the mapping can be re-checked against upstream.
//
// Surge stores *internal* values, not normalized 0..1. The value `type` written
// into the XML is `vt_int`=0 or `vt_float`=2 (see `writers/surge_init_data.rs`).

/// Format a float the way Surge writes patch floats (14 fractional digits).
pub fn ff(x: f64) -> String {
    format!("{x:.14}")
}

// ---------------------------------------------------------------------------
// Modulation-depth scales (named so the schema descriptions can cite them).
// ---------------------------------------------------------------------------

/// Abstract cutoff/filter-env modulation depth (`-1..1`) is scaled to this many
/// semitones of cutoff sweep. Cutoff is `ct_freq_audible` (semitones, 12 = one
/// octave; Parameter.cpp:644-651) and envmod is `ct_freq_mod` with a
/// **-96..+96** semitone range (Parameter.cpp:707-712). ±60 semitones = ±5
/// octaves is a musically strong, non-clipping sweep well inside that range.
pub const CUTOFF_SWEEP_SEMITONES: f64 = 60.0;

/// Abstract pitch modulation depth (`-1..1`) is scaled to this many semitones.
/// Pitch modulation destinations (`a_oscN_pitch`, `ct_pitch_semi7bp`) read depth
/// in semitones; ±12 = ±1 octave is a musical vibrato/pitch-sweep amount.
pub const PITCH_MOD_SEMITONES: f64 = 12.0;

// ---------------------------------------------------------------------------
// Continuous encodings.
// ---------------------------------------------------------------------------

/// Filter cutoff Hz → Surge internal semitone offset.
///
/// `ct_freq_audible`: displayed as `440 * 2^(v/12)` Hz, stored range -60..70
/// (Parameter.cpp:644-651, display formula ~1528-1542). Inverse: `v =
/// 12*log2(hz/440)`.
pub fn hz_to_cutoff(hz: f64) -> f64 {
    let hz = hz.max(1.0);
    (12.0 * (hz / 440.0).log2()).clamp(-60.0, 70.0)
}

/// Envelope A/D/R seconds → Surge `ct_envtime` internal value `log2(seconds)`.
///
/// `ct_envtime`: stored `log2(seconds)`, range -8..5 (Parameter.cpp:788-796;
/// DSP `seconds = 2^v`).
pub fn sec_to_envtime(s: f64) -> f64 {
    s.max(1e-4).log2().clamp(-8.0, 5.0)
}

/// Portamento seconds → Surge `ct_portatime` internal value.
///
/// `ct_portatime`: stored `log2(seconds)`, range -8..2, default -8
/// (Parameter.cpp:782-786). Abstract 0 s means "portamento off", which lands at
/// the parameter minimum (-8).
pub fn sec_to_portatime(s: f64) -> f64 {
    if s <= 0.0 {
        -8.0
    } else {
        s.log2().clamp(-8.0, 2.0)
    }
}

/// LFO rate Hz → Surge `ct_lforate` internal value `log2(Hz)`.
///
/// `ct_lforate`: stored `log2(Hz)`, range -7..9 (Parameter.cpp:829-835; DSP
/// `rate_hz = 2^v`, LFOModulationSource.cpp:288).
pub fn hz_to_lforate(hz: f64) -> f64 {
    hz.max(1e-4).log2().clamp(-7.0, 9.0)
}

/// Output gain (linear, e.g. 0.1..2.0) → Surge global `volume` in dB.
///
/// Global `volume` is `ct_decibel_attenuation_clipper`: **-48..0 dB**
/// (attenuation only — there is no positive-gain headroom; Parameter.cpp:741-746).
/// `dB = 20*log10(gain)`, clamped to -48..0 (gains above unity clamp to 0 dB).
pub fn gain_to_decibel(gain: f64) -> f64 {
    if gain <= 0.0 {
        return -48.0;
    }
    (20.0 * gain.log10()).clamp(-48.0, 0.0)
}

/// LFO phase degrees (0..360) → Surge `ct_lfophaseshuffle` fraction (0..1).
///
/// `a_lfoN_phase` is `ct_lfophaseshuffle`, stored 0..1 = fraction of one cycle
/// (Parameter.cpp:1060-1064; assign SurgePatch.cpp:569-572).
pub fn degree_to_fraction(deg: f64) -> f64 {
    (deg / 360.0).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Oscillator pitch decomposition.
// ---------------------------------------------------------------------------

/// A decomposed oscillator pitch: an integer octave shift (`a_oscN_octave`,
/// `ct_pitch_octave` int -3..3) plus a semitone remainder in `a_oscN_pitch`
/// (`ct_pitch_semi7bp` float -7..7), optionally with `extend_range` for values
/// that neither octaves nor the ±7 remainder can reach.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PitchDecomp {
    /// Integer octave shift for `a_oscN_octave` (clamped to -3..3).
    pub octave: i32,
    /// Semitone remainder stored in `a_oscN_pitch`.
    pub pitch: f64,
    /// Whether `a_oscN_pitch` must carry `extend_range="1"` (which multiplies
    /// the stored value by 12; Parameter.cpp:2309-2311).
    pub extend: bool,
}

/// Decompose a semitone pitch (schema range -48..48) into octave + remainder.
///
/// `a_oscN_octave` is `ct_pitch_octave` (int -3..3 = ±36 semitones;
/// Parameter.cpp:856-860). `a_oscN_pitch` is `ct_pitch_semi7bp` (float -7..7;
/// Parameter.cpp:636-643). Together they cover ±(36+7) = ±43 semitones exactly.
/// Beyond that we fall back to `extend_range` on the pitch param, where the
/// effective pitch is `12 * stored` (Parameter.cpp:2309-2311), i.e. `stored =
/// semitones / 12` (still on top of any octave shift).
pub fn decompose_pitch(semitones: f64) -> PitchDecomp {
    let total = semitones.clamp(-48.0, 48.0);
    // Prefer whole octaves, then a ±7 semitone remainder.
    let mut octave = (total / 12.0).trunc() as i32;
    octave = octave.clamp(-3, 3);
    let remainder = total - (octave as f64) * 12.0;
    if remainder.abs() <= 7.0 + 1e-9 {
        return PitchDecomp {
            octave,
            pitch: remainder,
            extend: false,
        };
    }
    // Remainder exceeds ±7 (only when |total| in (43, 48]). Use extend_range on
    // the pitch param: effective = 12 * stored, so stored = remainder / 12.
    // Clamp the stored value to the param's -7..7 range (12*7 = 84 semitones of
    // headroom, far more than we need).
    let stored = (remainder / 12.0).clamp(-7.0, 7.0);
    PitchDecomp {
        octave,
        pitch: stored,
        extend: true,
    }
}

// ---------------------------------------------------------------------------
// Enum int tables (each verified against the Surge source).
// ---------------------------------------------------------------------------

/// Abstract oscillator-type name → Surge `osc_type` int.
///
/// SurgeStorage.h:276-290 `osc_type`: `ot_classic=0, ot_sine=1, ot_wavetable=2,
/// ot_shnoise=3, ot_audioinput=4, ot_FM3=5, ot_FM2=6, ot_window=7, ot_modern=8,
/// ot_string=9, ot_twist=10, ot_alias=11`. "modern" is a real Surge type
/// (`ot_modern=8`).
pub fn osc_type_int(name: &str) -> i32 {
    match name {
        "classic" => 0,
        "sine" => 1,
        "wavetable" => 2,
        "fm3" => 5,
        "fm2" => 6,
        "window" => 7,
        "modern" => 8,
        _ => 0,
    }
}

/// Abstract filter-type name → Surge `fut_*` int.
///
/// FilterConfiguration.h `fu_type`: `fut_none=0, fut_lp12=1, fut_lp24=2,
/// fut_lpmoog=3, fut_hp12=4, fut_hp24=5, fut_bp12=6, fut_notch12=7,
/// fut_comb_pos=8, fut_SNH=9`.
pub fn filter_type_int(name: &str) -> i32 {
    match name {
        "lp12" => 1,
        "lp24" => 2,
        "lp_ladder" => 3,
        "hp12" => 4,
        "hp24" => 5,
        "bp12" => 6,
        "notch" => 7,
        "comb" => 8,
        "sample_hold" => 9,
        _ => 1,
    }
}

/// Abstract LFO-shape name → Surge `lfo_type` int.
///
/// SurgeStorage.h `lfo_type`: `lt_sine=0, lt_tri=1, lt_square=2, lt_ramp=3,
/// lt_noise=4, lt_snh=5, lt_envelope=6, ...`.
pub fn lfo_shape_int(name: &str) -> i32 {
    match name {
        "sine" => 0,
        "triangle" => 1,
        "square" => 2,
        "saw" => 3,
        "noise" => 4,
        "s&h" => 5,
        _ => 0,
    }
}

/// Abstract scene-mode name → Surge `scene_mode` int.
///
/// SurgeStorage.h:155-163: `sm_single=0, sm_split=1 ("Key Split"), sm_dual=2,
/// sm_chsplit=3 ("Channel Split")`. Mapped by NAME (our schema order differs
/// from Surge's only in labels, but the ints are matched by meaning here).
pub fn scene_mode_int(name: &str) -> i32 {
    match name {
        "single" => 0,
        "key_split" => 1,
        "dual" => 2,
        "channel_split" => 3,
        _ => 0,
    }
}

/// Abstract character name → Surge `character_mode` int.
///
/// SurgeStorage.h:261-273: `cm_warm=0, cm_neutral=1, cm_bright=2` (default 1).
pub fn character_int(name: &str) -> i32 {
    match name {
        "warm" => 0,
        "neutral" => 1,
        "bright" => 2,
        _ => 1,
    }
}

/// Surge `modsources` integer for a voice modulation source.
///
/// ModulationSource.h:40-84 (counting the enum): `ms_velocity=1, ms_ampeg=15,
/// ms_filtereg=16, ms_lfo1=17, ms_lfo2=18, ms_lfo3=19`. `ms_lfo1..6` are the six
/// VOICE LFOs (a_lfo0..a_lfo5), which is what our abstract lfo1/2/3 map to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModSource {
    Velocity,
    AmpEg,
    FilterEg,
    Lfo1,
    Lfo2,
    Lfo3,
}

impl ModSource {
    /// The Surge `modsources` integer.
    pub fn int(self) -> i32 {
        match self {
            ModSource::Velocity => 1,
            ModSource::AmpEg => 15,
            ModSource::FilterEg => 16,
            ModSource::Lfo1 => 17,
            ModSource::Lfo2 => 18,
            ModSource::Lfo3 => 19,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Each test cites the Surge source location the encoding is derived from.

    #[test]
    fn cutoff_hz_to_semitone() {
        // ct_freq_audible: 440 Hz = internal 0; 523.25 Hz ≈ 3 (Parameter.cpp:644-651).
        assert!((hz_to_cutoff(440.0)).abs() < 1e-9);
        assert!((hz_to_cutoff(523.25) - 3.0).abs() < 1e-3);
        // 880 Hz = one octave up = +12.
        assert!((hz_to_cutoff(880.0) - 12.0).abs() < 1e-9);
        // Clamp to the -60..70 range.
        assert!(hz_to_cutoff(1e9) <= 70.0 + 1e-9);
    }

    #[test]
    fn envtime_log2_seconds() {
        // ct_envtime: log2(seconds), range -8..5 (Parameter.cpp:788-796).
        assert!((sec_to_envtime(1.0)).abs() < 1e-9); // log2(1) = 0
        assert!((sec_to_envtime(2.0) - 1.0).abs() < 1e-9);
        assert!((sec_to_envtime(0.5) + 1.0).abs() < 1e-9);
        assert_eq!(sec_to_envtime(1e-9), -8.0); // clamp min
        assert_eq!(sec_to_envtime(1000.0), 5.0); // clamp max
    }

    #[test]
    fn portatime_off_is_minimum() {
        // ct_portatime: range -8..2, off (0s) -> -8 (Parameter.cpp:782-786).
        assert_eq!(sec_to_portatime(0.0), -8.0);
        assert_eq!(sec_to_portatime(-1.0), -8.0);
        assert!((sec_to_portatime(1.0)).abs() < 1e-9);
        assert_eq!(sec_to_portatime(100.0), 2.0); // clamp max
    }

    #[test]
    fn lforate_log2_hz() {
        // ct_lforate: log2(Hz), range -7..9 (Parameter.cpp:829-835).
        assert!((hz_to_lforate(1.0)).abs() < 1e-9);
        assert!((hz_to_lforate(2.0) - 1.0).abs() < 1e-9);
        assert_eq!(hz_to_lforate(1e-9), -7.0);
        assert_eq!(hz_to_lforate(1e9), 9.0);
    }

    #[test]
    fn gain_to_db_attenuation_only() {
        // ct_decibel_attenuation_clipper: -48..0 dB (Parameter.cpp:741-746).
        assert!((gain_to_decibel(1.0)).abs() < 1e-9); // unity = 0 dB
        // Gain above unity clamps to 0 (no positive headroom).
        assert_eq!(gain_to_decibel(2.0), 0.0);
        // Half gain ≈ -6 dB.
        assert!((gain_to_decibel(0.5) + 6.0206).abs() < 1e-3);
        assert_eq!(gain_to_decibel(0.0), -48.0);
        assert!(gain_to_decibel(1e-9) >= -48.0);
    }

    #[test]
    fn phase_degrees_to_fraction() {
        // ct_lfophaseshuffle: 0..1 fraction of cycle (Parameter.cpp:1060-1064).
        assert!((degree_to_fraction(0.0)).abs() < 1e-9);
        assert!((degree_to_fraction(180.0) - 0.5).abs() < 1e-9);
        assert!((degree_to_fraction(360.0) - 1.0).abs() < 1e-9);
        assert_eq!(degree_to_fraction(720.0), 1.0); // clamp
    }

    #[test]
    fn pitch_decomposition_boundaries() {
        // Within ±7: no octave needed.
        assert_eq!(
            decompose_pitch(7.0),
            PitchDecomp {
                octave: 0,
                pitch: 7.0,
                extend: false
            }
        );
        // 8 semitones: trunc(8/12)=0, remainder 8 > 7 -> extend, stored 8/12.
        let d8 = decompose_pitch(8.0);
        assert_eq!(d8.octave, 0);
        assert!(d8.extend);
        assert!((d8.pitch - 8.0 / 12.0).abs() < 1e-9);
        // -12 (the classic "octave-down bass"): octave -1, pitch 0, no extend.
        assert_eq!(
            decompose_pitch(-12.0),
            PitchDecomp {
                octave: -1,
                pitch: 0.0,
                extend: false
            }
        );
        // +12: octave 1, pitch 0.
        assert_eq!(
            decompose_pitch(12.0),
            PitchDecomp {
                octave: 1,
                pitch: 0.0,
                extend: false
            }
        );
        // -7 exactly: octave 0, pitch -7.
        assert_eq!(
            decompose_pitch(-7.0),
            PitchDecomp {
                octave: 0,
                pitch: -7.0,
                extend: false
            }
        );
        // +48 (schema max): octave 3 (=36) leaves remainder 12 > 7 -> extend,
        // stored = 12/12 = 1.0 with extend -> effective 12 -> total 48.
        let d48 = decompose_pitch(48.0);
        assert_eq!(d48.octave, 3);
        assert!(d48.extend);
        assert!((d48.pitch - 1.0).abs() < 1e-9);
        // -48: octave -3, remainder -12 -> extend, stored -1.0.
        let dm48 = decompose_pitch(-48.0);
        assert_eq!(dm48.octave, -3);
        assert!(dm48.extend);
        assert!((dm48.pitch + 1.0).abs() < 1e-9);
        // 43 = 3 octaves (36) + 7: representable exactly without extend.
        assert_eq!(
            decompose_pitch(43.0),
            PitchDecomp {
                octave: 3,
                pitch: 7.0,
                extend: false
            }
        );
    }

    #[test]
    fn osc_type_table_exact_ints() {
        // SurgeStorage.h osc_type enum.
        assert_eq!(osc_type_int("classic"), 0);
        assert_eq!(osc_type_int("sine"), 1);
        assert_eq!(osc_type_int("wavetable"), 2);
        assert_eq!(osc_type_int("fm3"), 5);
        assert_eq!(osc_type_int("fm2"), 6);
        assert_eq!(osc_type_int("window"), 7);
        assert_eq!(osc_type_int("modern"), 8);
        assert_eq!(osc_type_int("garbage"), 0);
    }

    #[test]
    fn filter_type_table_exact_ints() {
        // FilterConfiguration.h fu_type enum.
        assert_eq!(filter_type_int("lp12"), 1);
        assert_eq!(filter_type_int("lp24"), 2);
        assert_eq!(filter_type_int("lp_ladder"), 3);
        assert_eq!(filter_type_int("hp12"), 4);
        assert_eq!(filter_type_int("hp24"), 5);
        assert_eq!(filter_type_int("bp12"), 6);
        assert_eq!(filter_type_int("notch"), 7);
        assert_eq!(filter_type_int("comb"), 8);
        assert_eq!(filter_type_int("sample_hold"), 9);
        assert_eq!(filter_type_int("garbage"), 1);
    }

    #[test]
    fn lfo_shape_table_exact_ints() {
        // SurgeStorage.h lfo_type enum.
        assert_eq!(lfo_shape_int("sine"), 0);
        assert_eq!(lfo_shape_int("triangle"), 1);
        assert_eq!(lfo_shape_int("square"), 2);
        assert_eq!(lfo_shape_int("saw"), 3);
        assert_eq!(lfo_shape_int("noise"), 4);
        assert_eq!(lfo_shape_int("s&h"), 5);
        assert_eq!(lfo_shape_int("garbage"), 0);
    }

    #[test]
    fn scene_mode_table_exact_ints() {
        // SurgeStorage.h:155-163 scene_mode enum (mapped by name).
        assert_eq!(scene_mode_int("single"), 0);
        assert_eq!(scene_mode_int("key_split"), 1);
        assert_eq!(scene_mode_int("dual"), 2);
        assert_eq!(scene_mode_int("channel_split"), 3);
        assert_eq!(scene_mode_int("garbage"), 0);
    }

    #[test]
    fn character_table_exact_ints() {
        // SurgeStorage.h:261-273 character_mode enum (default neutral=1).
        assert_eq!(character_int("warm"), 0);
        assert_eq!(character_int("neutral"), 1);
        assert_eq!(character_int("bright"), 2);
        assert_eq!(character_int("garbage"), 1);
    }

    #[test]
    fn mod_source_ints_from_enum() {
        // ModulationSource.h:40-84 modsources enum.
        assert_eq!(ModSource::Velocity.int(), 1);
        assert_eq!(ModSource::AmpEg.int(), 15);
        assert_eq!(ModSource::FilterEg.int(), 16);
        assert_eq!(ModSource::Lfo1.int(), 17);
        assert_eq!(ModSource::Lfo2.int(), 18);
        assert_eq!(ModSource::Lfo3.int(), 19);
    }
}
