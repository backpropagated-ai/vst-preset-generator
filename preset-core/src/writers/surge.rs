// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Surge XT `.fxp` writer — the *corrected* implementation.
//
// ## Why this file exists (the correctness fix)
//
// The legacy Python (`fxp_generator._create_surge_chunk`) embedded a small JSON
// blob inside an FXP chunk with `fxMagic='FxCk'` and a made-up plugin id. Real
// Surge XT does NOT load that — it fails with an "belongs to another plugin"
// error. This was verified directly against the Surge XT GPL-3.0 source tree:
//
//   * `SurgeSynthesizerIO.cpp::savePatchToPath` writes an FXP with
//     `chunkMagic='CcnK'`, `fxMagic='FPCh'` (chunk format), `fxID='cjs3'`,
//     `numPrograms=1`, `version=1`, `fxVersion=1`, then a length-prefixed chunk.
//   * `SurgePatch.cpp::save_patch` builds that chunk as:
//       `patch_header { tag[4]="sub3"; u32 xmlsize (LE); u32 wtsize[2][3] (LE) }`
//       followed by the UTF-8 XML patch document, then any wavetable blobs.
//   * `SurgePatch.cpp::save_xml` writes `<patch revision="22"><meta .../>
//     <parameters>...</parameters>...</patch>` where every parameter is a named
//     element (its `get_storage_name()`, e.g. `a_filter1_cutoff`) carrying a
//     `type` (0=int, 2=float) and a raw `value` in Surge's *internal* units.
//     Modulation routings are child `<modrouting .../>` elements of their
//     destination parameter (SurgePatch.cpp:3389-3428).
//   * `loadPatchByPath` accepts `fxMagic='FPCh'` + `fxID='cjs3'` only.
//
// So a loadable file requires the exact FPCh/cjs3/sub3 container plus a complete
// parameter list under storage names Surge recognises. We embed the full
// "Init Saw" default patch (extracted from Surge's own factory template) as the
// base, then override the mapped subset. A patch with the right container + all
// defaults + a correct mapped subset loads cleanly; unmapped params keep their
// Init defaults.
//
// ## Value encodings
//
// All internal value encodings and enum tables live in `surge_encode.rs`, each
// verified against the Surge source with a `file:line` citation.

use std::collections::BTreeMap;

use crate::param::ParamValue;
use crate::writers::fxp;
use crate::writers::surge_encode as enc;
use crate::writers::surge_encode::ModSource;
use crate::writers::surge_init_data::{SURGE_INIT_PARAMS, SURGE_INIT_REVISION};

/// Surge XT FXP plugin id (`'cjs3'`) — required for the file to load.
pub const SURGE_FX_ID: &[u8; 4] = b"cjs3";
/// The Surge patch XML revision this writer targets.
pub const SURGE_REVISION: u32 = SURGE_INIT_REVISION;

/// Surge value type: `vt_int`.
const VT_INT: u8 = 0;
/// Surge value type: `vt_float`.
const VT_FLOAT: u8 = 2;

/// Metadata written into the Surge `<meta>` element.
#[derive(Debug, Clone)]
pub struct SurgeMeta {
    pub name: String,
    pub category: String,
    pub comment: String,
    pub author: String,
    pub license: String,
}

impl Default for SurgeMeta {
    fn default() -> Self {
        Self {
            name: "DeepSynth Patch".into(),
            category: "DeepSynth".into(),
            comment: String::new(),
            author: "DeepSynth".into(),
            license: "CC0".into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Override model.
// ---------------------------------------------------------------------------

/// A single modulation routing to emit as a child of a destination parameter.
///
/// Surge serializes these as `<modrouting source=<int> depth=<float> muted=0
/// source_index=0 />` inside the destination `<parameter>` element (voice/scene
/// variant, no `source_scene`; SurgePatch.cpp:3401-3406). `depth` is in the
/// destination parameter's native units.
#[derive(Debug, Clone, PartialEq)]
pub struct ModRouting {
    pub source: ModSource,
    pub depth: f64,
}

/// One override to apply on top of the Init Saw defaults: a Surge value type,
/// the raw internal value string, any extra XML attributes to carry (e.g.
/// `extend_range="1"`), and any modulation routings.
#[derive(Debug, Clone)]
struct Override {
    ty: u8,
    /// The raw internal value string. Empty means "keep the init value" (used by
    /// routing-only overrides that only attach `<modrouting>` children).
    value: String,
    /// Extra attribute string appended after `value` (already quote-escaped), or
    /// `None` to keep the init-table's own trailing attrs.
    extra_attrs: Option<String>,
    routings: Vec<ModRouting>,
}

/// Builder that accumulates overrides keyed by Surge storage name. Each section
/// function pushes into one of these; the emitter walks the init table and
/// applies them.
#[derive(Debug, Default)]
struct OverrideSet {
    map: BTreeMap<String, Override>,
}

impl OverrideSet {
    /// Override a float parameter's value.
    fn float(&mut self, name: &str, value: f64) {
        self.set(name, VT_FLOAT, enc::ff(value), None);
    }

    /// Override a float parameter and set its trailing attribute string.
    fn float_attrs(&mut self, name: &str, value: f64, attrs: &str) {
        self.set(name, VT_FLOAT, enc::ff(value), Some(attrs.to_string()));
    }

    /// Override an int parameter's value.
    fn int(&mut self, name: &str, value: i64) {
        self.set(name, VT_INT, value.to_string(), None);
    }

    /// Insert or update an override's value/type/attrs, preserving any routings
    /// already attached to that destination.
    fn set(&mut self, name: &str, ty: u8, value: String, extra_attrs: Option<String>) {
        let e = self.map.entry(name.to_string()).or_insert_with(|| Override {
            ty,
            value: String::new(),
            extra_attrs: None,
            routings: Vec::new(),
        });
        e.ty = ty;
        e.value = value;
        e.extra_attrs = extra_attrs;
    }

    /// Add a modulation routing to a destination parameter. The destination
    /// keeps its existing value (init or an earlier override); only the routing
    /// child is attached.
    fn route(&mut self, dest: &str, source: ModSource, depth: f64) {
        self.map
            .entry(dest.to_string())
            .or_insert_with(|| Override {
                ty: VT_FLOAT,
                value: String::new(),
                extra_attrs: None,
                routings: Vec::new(),
            })
            .routings
            .push(ModRouting { source, depth });
    }
}

// ---------------------------------------------------------------------------
// Mapped-value accessors.
// ---------------------------------------------------------------------------

/// A small view over the mapped parameter map with typed accessors.
struct Mapped<'a>(&'a BTreeMap<String, ParamValue>);

impl Mapped<'_> {
    fn num(&self, k: &str) -> Option<f64> {
        self.0.get(k).map(|v| match v {
            ParamValue::Num(n) => *n,
            ParamValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            ParamValue::Enum(_) => 0.0,
        })
    }

    fn enum_(&self, k: &str) -> Option<&str> {
        self.0.get(k).and_then(|v| match v {
            ParamValue::Enum(s) => Some(s.as_str()),
            _ => None,
        })
    }
}

// ---------------------------------------------------------------------------
// Per-section override builders. Each operates on scene A only.
// ---------------------------------------------------------------------------

/// Oscillator types, pitch (octave + semitone decomposition), and the
/// classic-only width/unison slots.
fn section_oscillators(m: &Mapped, ov: &mut OverrideSet) {
    for (idx, key) in ["osc1_type", "osc2_type", "osc3_type"].iter().enumerate() {
        let n = idx + 1;
        let osc_kind = m.enum_(key);
        if let Some(name) = osc_kind {
            ov.int(&format!("a_osc{n}_type"), enc::osc_type_int(name) as i64);
        }

        // Pitch: decompose into octave + semitone remainder (+ extend when the
        // value can't be reached by octaves and ±7 semitones alone).
        if let Some(semi) = m.num(&format!("osc{n}_pitch")) {
            let d = enc::decompose_pitch(semi);
            ov.int(&format!("a_osc{n}_octave"), d.octave as i64);
            let attrs = if d.extend {
                "extend_range=\"1\""
            } else {
                "extend_range=\"0\""
            };
            ov.float_attrs(&format!("a_osc{n}_pitch"), d.pitch, attrs);
        }

        // Width + unison detune/voices live in per-type param slots. The slot
        // meaning is oscillator-type specific (verified from the oscillator
        // source). For the Classic oscillator (ClassicOscillator.cpp:254-272):
        //   param1 = Width 1 (ct_percent 0..1), param5 = Unison Detune
        //   (ct_oscspread 0..1), param6 = Unison Voices (ct_osccount int 1..16).
        // The Modern oscillator (ModernOscillator.cpp:573-595) uses a DIFFERENT
        // slot layout (width is param3, param1 is "Pulse"), so we only wire
        // these slots for the classic oscillator to avoid the wrong slot.
        let is_classic = matches!(osc_kind, Some("classic")) || osc_kind.is_none();
        if is_classic {
            if let Some(w) = m.num(&format!("osc{n}_width")) {
                ov.float(&format!("a_osc{n}_param1"), w.clamp(0.0, 1.0));
            }
            if let Some(voices) = m.num("unison_voices") {
                let v = voices.round().clamp(1.0, 16.0) as i64;
                ov.int(&format!("a_osc{n}_param6"), v);
                // Only apply unison detune when there is more than one voice.
                if v > 1 {
                    if let Some(det) = m.num("unison_detune") {
                        // unison_detune schema is 0..100; ct_oscspread is 0..1.
                        ov.float(
                            &format!("a_osc{n}_param5"),
                            (det / 100.0).clamp(0.0, 1.0),
                        );
                    }
                }
            }
        }
    }
}

/// Mixer levels/mutes for oscillators, noise, and ring modulators.
fn section_mixer(m: &Mapped, ov: &mut OverrideSet) {
    // Oscillator levels (a_level_oN, ct_amplitude 0..1) + unmute on non-zero.
    for (idx, key) in ["osc1_level", "osc2_level", "osc3_level"].iter().enumerate() {
        if let Some(v) = m.num(key) {
            let n = idx + 1;
            ov.float(&format!("a_level_o{n}"), v.clamp(0.0, 1.0));
            ov.int(&format!("a_mute_o{n}"), mute_for(v));
        }
    }
    if let Some(v) = m.num("noise_level") {
        ov.float("a_level_noise", v.clamp(0.0, 1.0));
        ov.int("a_mute_noise", mute_for(v));
    }
    // Ring modulator levels (a_level_ringNN, ct_amplitude_ringmod 0..1) + unmute.
    for (key, level, mute) in [
        ("ring_12", "a_level_ring12", "a_mute_ring12"),
        ("ring_23", "a_level_ring23", "a_mute_ring23"),
    ] {
        if let Some(v) = m.num(key) {
            // Preserve the deform_type attr Surge writes for ring levels.
            ov.float_attrs(level, v.clamp(0.0, 1.0), "deform_type=\"0\"");
            ov.int(mute, mute_for(v));
        }
    }
}

/// `a_mute_*` value: unmute (0) when the level is audibly non-zero.
fn mute_for(level: f64) -> i64 {
    if level > 0.0001 {
        0
    } else {
        1
    }
}

/// Filters: type, cutoff, resonance, env-mod depth, feedback, balance, config.
fn section_filters(m: &Mapped, ov: &mut OverrideSet) {
    if let Some(name) = m.enum_("filter1_type") {
        ov.int("a_filter1_type", enc::filter_type_int(name) as i64);
    }
    if let Some(name) = m.enum_("filter2_type") {
        ov.int("a_filter2_type", enc::filter_type_int(name) as i64);
    }
    if let Some(hz) = m.num("filter1_cutoff") {
        ov.float("a_filter1_cutoff", enc::hz_to_cutoff(hz));
    }
    if let Some(hz) = m.num("filter2_cutoff") {
        ov.float("a_filter2_cutoff", enc::hz_to_cutoff(hz));
    }
    if let Some(v) = m.num("filter1_resonance") {
        ov.float("a_filter1_resonance", v.clamp(0.0, 1.0));
    }
    if let Some(v) = m.num("filter2_resonance") {
        ov.float("a_filter2_resonance", v.clamp(0.0, 1.0));
    }
    // Filter-envelope depth -> a_filter1_envmod (ct_freq_mod, -96..96 semitones).
    // Scale abstract -1..1 to ±60 semitones of cutoff sweep (a strong sweep).
    if let Some(v) = m.num("filter_env_depth") {
        ov.float(
            "a_filter1_envmod",
            v.clamp(-1.0, 1.0) * enc::CUTOFF_SWEEP_SEMITONES,
        );
    }
    // Filter feedback (a_feedback, ct_filter_feedback -1..1). Our abstract is
    // 0..1; map into the positive half of the bipolar range. Keep the trailing
    // attrs Surge writes.
    if let Some(v) = m.num("filter_feedback") {
        ov.float_attrs(
            "a_feedback",
            v.clamp(0.0, 1.0),
            "extend_range=\"0\" deform_type=\"0\"",
        );
    }
    // Filter balance (a_f_balance, ct_percent_bipolar -1..1).
    if let Some(v) = m.num("filter_balance") {
        ov.float("a_f_balance", v.clamp(-1.0, 1.0));
    }
    // Filter config (fb_config, ct_fbconfig int enum). Surge fbconfig:
    // fc_serial1=0, serial2=1, serial3=2, dual1=3, dual2=4, stereo=5, ring=6,
    // wide=7. Our schema is [serial, parallel, wide, dual]; map by meaning:
    // serial->0, parallel->5 (stereo), wide->7, dual->3.
    if let Some(name) = m.enum_("filter_config") {
        let cfg = match name {
            "serial" => 0,
            "parallel" => 5,
            "wide" => 7,
            "dual" => 3,
            _ => 0,
        };
        ov.int("a_fb_config", cfg);
    }
}

/// Amp (env1) and filter (env2) ADSR envelopes.
fn section_envelopes(m: &Mapped, ov: &mut OverrideSet) {
    // Amp envelope = env1.
    if let Some(s) = m.num("amp_attack") {
        ov.float("a_env1_attack", enc::sec_to_envtime(s));
    }
    if let Some(s) = m.num("amp_decay") {
        ov.float("a_env1_decay", enc::sec_to_envtime(s));
    }
    if let Some(v) = m.num("amp_sustain") {
        ov.float("a_env1_sustain", v.clamp(0.0, 1.0));
    }
    if let Some(s) = m.num("amp_release") {
        ov.float("a_env1_release", enc::sec_to_envtime(s));
    }
    // Filter envelope = env2.
    if let Some(s) = m.num("filter_attack") {
        ov.float("a_env2_attack", enc::sec_to_envtime(s));
    }
    if let Some(s) = m.num("filter_decay") {
        ov.float("a_env2_decay", enc::sec_to_envtime(s));
    }
    if let Some(v) = m.num("filter_sustain") {
        ov.float("a_env2_sustain", v.clamp(0.0, 1.0));
    }
    if let Some(s) = m.num("filter_release") {
        ov.float("a_env2_release", enc::sec_to_envtime(s));
    }
}

/// Voice LFOs 1/2/3 (Surge voice LFOs a_lfo0/a_lfo1/a_lfo2): shape, rate,
/// phase, deform.
fn section_lfos(m: &Mapped, ov: &mut OverrideSet) {
    // Abstract lfo1/2/3 -> Surge voice LFOs a_lfo0/a_lfo1/a_lfo2 (the first
    // three of six voice LFOs; a_lfo6.. are scene LFOs). SurgePatch.cpp:550-560.
    for (idx, prefix) in ["lfo1", "lfo2", "lfo3"].iter().enumerate() {
        let s = format!("a_lfo{idx}");
        if let Some(name) = m.enum_(&format!("{prefix}_shape")) {
            ov.int(&format!("{s}_shape"), enc::lfo_shape_int(name) as i64);
        }
        if let Some(hz) = m.num(&format!("{prefix}_rate")) {
            // Keep the deactivated attr Surge writes on the rate param.
            ov.float_attrs(
                &format!("{s}_rate"),
                enc::hz_to_lforate(hz),
                "deactivated=\"0\"",
            );
        }
        if let Some(deg) = m.num(&format!("{prefix}_phase")) {
            ov.float_attrs(
                &format!("{s}_phase"),
                enc::degree_to_fraction(deg),
                "extend_range=\"0\"",
            );
        }
        if let Some(v) = m.num(&format!("{prefix}_deform")) {
            ov.float_attrs(
                &format!("{s}_deform"),
                v.clamp(-1.0, 1.0),
                "deform_type=\"0\"",
            );
        }
    }
}

/// Global/scene params: character, drift, pan, output gain, sends, pitch bend,
/// portamento, scene mode, polyphony, velocity->amp.
fn section_global(m: &Mapped, ov: &mut OverrideSet) {
    // Character (global int 0..2).
    if let Some(name) = m.enum_("character") {
        ov.int("character", enc::character_int(name) as i64);
    }
    // Oscillator drift (a_drift, ct_percent_oscdrift 0..1). Keep extend attr.
    if let Some(v) = m.num("drift") {
        ov.float_attrs("a_drift", v.clamp(0.0, 1.0), "extend_range=\"1\"");
    }
    // Scene pan (a_pan, ct_percent_bipolar_pan -1..1).
    if let Some(v) = m.num("output_pan") {
        ov.float("a_pan", v.clamp(-1.0, 1.0));
    }
    // Output gain -> global volume (ct_decibel_attenuation_clipper, -48..0 dB).
    if let Some(g) = m.num("output_gain") {
        ov.float("volume", enc::gain_to_decibel(g));
    }
    // Master volume -> scene volume (a_volume, ct_amplitude 0..1). Keep the
    // deactivated attr Surge writes.
    if let Some(v) = m.num("master_volume") {
        ov.float_attrs("a_volume", v.clamp(0.0, 1.0), "deactivated=\"0\"");
    }
    // FX sends (a_send_fx_1/2, ct_sendlevel 0..1.5874). Map 0..1 into 0..1.
    if let Some(v) = m.num("fx_send_1") {
        ov.float("a_send_fx_1", v.clamp(0.0, 1.0));
    }
    if let Some(v) = m.num("fx_send_2") {
        ov.float("a_send_fx_2", v.clamp(0.0, 1.0));
    }
    // Pitch bend range up + down (a_pbrange_up/dn, ct_pbdepth int 0..24).
    if let Some(v) = m.num("pitch_bend_range") {
        let pb = (v.round() as i64).clamp(0, 24);
        ov.int("a_pbrange_up", pb);
        ov.int("a_pbrange_dn", pb);
    }
    // Portamento time (a_portamento, ct_portatime, log2 seconds, -8..2).
    if let Some(s) = m.num("portamento_time") {
        ov.float_attrs(
            "a_portamento",
            enc::sec_to_portatime(s),
            "porta_const_rate=\"0\" porta_gliss=\"0\" porta_retrigger=\"0\" porta_curve=\"0\"",
        );
    }
    // Scene mode (scenemode, global int enum).
    if let Some(name) = m.enum_("scene_mode") {
        ov.int("scenemode", enc::scene_mode_int(name) as i64);
    }
    // Polyphony -> polylimit (int 2..64).
    if let Some(v) = m.num("polyphony") {
        ov.int("polylimit", (v.round() as i64).clamp(2, 64));
    }
    // Velocity -> amp sensitivity (a_vca_velsense, ct_decibel_attenuation
    // -48..0 dB). Abstract 0..1: 0 -> off (0 dB, no vel scaling), 1 -> full
    // sensitivity (-48 dB attenuation at zero velocity).
    if let Some(v) = m.num("mod_velocity_amp") {
        ov.float("a_vca_velsense", v.clamp(0.0, 1.0) * -48.0);
    }
}

/// Modulation routings: LFO/EG/velocity -> pitch/cutoff destinations, emitted as
/// `<modrouting>` children of the destination parameter (voice modulation).
fn section_modulation(m: &Mapped, ov: &mut OverrideSet) {
    let sweep = enc::CUTOFF_SWEEP_SEMITONES;
    let pmod = enc::PITCH_MOD_SEMITONES;

    // LFO1 -> pitch / cutoff.
    if let Some(v) = m.num("mod_lfo1_pitch") {
        route_if_nonzero(ov, "a_pitch", ModSource::Lfo1, v * pmod);
    }
    if let Some(v) = m.num("mod_lfo1_cutoff") {
        route_if_nonzero(ov, "a_filter1_cutoff", ModSource::Lfo1, v * sweep);
    }
    // LFO2 -> pitch / cutoff.
    if let Some(v) = m.num("mod_lfo2_pitch") {
        route_if_nonzero(ov, "a_pitch", ModSource::Lfo2, v * pmod);
    }
    if let Some(v) = m.num("mod_lfo2_cutoff") {
        route_if_nonzero(ov, "a_filter1_cutoff", ModSource::Lfo2, v * sweep);
    }
    // Filter envelope (env2 = filtereg) -> pitch.
    if let Some(v) = m.num("mod_env_pitch") {
        route_if_nonzero(ov, "a_pitch", ModSource::FilterEg, v * pmod);
    }
    // Velocity -> cutoff.
    if let Some(v) = m.num("mod_velocity_cutoff") {
        route_if_nonzero(ov, "a_filter1_cutoff", ModSource::Velocity, v * sweep);
    }
}

/// Add a routing only when the depth is meaningfully non-zero (avoids emitting
/// zero-depth routings that just clutter the patch).
fn route_if_nonzero(ov: &mut OverrideSet, dest: &str, source: ModSource, depth: f64) {
    if depth.abs() > 1e-6 {
        ov.route(dest, source, depth);
    }
}

/// Build the full override set from mapped abstract parameters (scene A only).
fn build_overrides(mapped: &BTreeMap<String, ParamValue>) -> OverrideSet {
    let m = Mapped(mapped);
    let mut ov = OverrideSet::default();
    section_oscillators(&m, &mut ov);
    section_mixer(&m, &mut ov);
    section_filters(&m, &mut ov);
    section_envelopes(&m, &mut ov);
    section_lfos(&m, &mut ov);
    section_global(&m, &mut ov);
    section_modulation(&m, &mut ov);
    ov
}

/// The set of Surge storage names this writer produces (for the exhaustiveness
/// test that asserts every override name exists in the init table).
#[cfg(test)]
pub(crate) fn override_names(mapped: &BTreeMap<String, ParamValue>) -> Vec<String> {
    build_overrides(mapped).map.keys().cloned().collect()
}

// ---------------------------------------------------------------------------
// XML emission.
// ---------------------------------------------------------------------------

/// XML-escape a string for use inside an attribute value.
fn xml_attr_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}

/// Emit the `<modrouting>` children for a destination parameter (voice/scene
/// variant, no `source_scene`; SurgePatch.cpp:3401-3406).
fn emit_modroutings(xml: &mut String, routings: &[ModRouting]) {
    for r in routings {
        xml.push_str(&format!(
            "<modrouting source=\"{}\" depth=\"{}\" muted=\"0\" source_index=\"0\" />",
            r.source.int(),
            enc::ff(r.depth),
        ));
    }
}

/// Emit one parameter element, either self-closing or (when it carries
/// modulation routings) wrapping child `<modrouting>` elements.
fn emit_param(
    xml: &mut String,
    name: &str,
    ty: u8,
    value: &str,
    tail: &str,
    routings: &[ModRouting],
) {
    let open = if tail.is_empty() {
        format!("<{name} type=\"{ty}\" value=\"{value}\"")
    } else {
        format!("<{name} type=\"{ty}\" value=\"{value}\" {tail}")
    };
    if routings.is_empty() {
        xml.push_str(&open);
        xml.push_str(" />");
    } else {
        xml.push_str(&open);
        xml.push('>');
        emit_modroutings(xml, routings);
        xml.push_str(&format!("</{name}>"));
    }
}

/// Build the Surge `<patch>` XML document from init defaults plus the mapped
/// overrides.
pub fn build_patch_xml(meta: &SurgeMeta, mapped: &BTreeMap<String, ParamValue>) -> String {
    let overrides = build_overrides(mapped);

    let mut xml = String::with_capacity(48 * 1024);
    xml.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\" ?>");
    xml.push_str(&format!("<patch revision=\"{}\">", SURGE_REVISION));
    xml.push_str(&format!(
        "<meta name=\"{}\" category=\"{}\" comment=\"{}\" author=\"{}\" license=\"{}\">",
        xml_attr_escape(&meta.name),
        xml_attr_escape(&meta.category),
        xml_attr_escape(&meta.comment),
        xml_attr_escape(&meta.author),
        xml_attr_escape(&meta.license),
    ));
    xml.push_str("<tags /></meta>");
    xml.push_str("<parameters>");

    for (name, init_ty, init_val, init_tail) in SURGE_INIT_PARAMS {
        match overrides.map.get(*name) {
            Some(o) => {
                // A routing-only override (empty value) keeps the init value/type
                // and init tail. A value override replaces value/type, and its
                // `extra_attrs` (when present) replace the init tail.
                let (value, ty, tail) = if o.value.is_empty() {
                    (*init_val, *init_ty, *init_tail)
                } else {
                    (
                        o.value.as_str(),
                        o.ty,
                        o.extra_attrs.as_deref().unwrap_or(init_tail),
                    )
                };
                emit_param(&mut xml, name, ty, value, tail, &o.routings);
            }
            None => emit_param(&mut xml, name, *init_ty, init_val, init_tail, &[]),
        }
    }

    xml.push_str("</parameters>");
    // Minimal nonparamconfig block: Surge accepts a patch without it and fills
    // defaults, but including the mono/hardclip/TAM block matches what Surge
    // writes and avoids any missing-config warnings. Values are the defaults.
    xml.push_str("<nonparamconfig>");
    for sc in 0..2 {
        xml.push_str(&format!("<monoVoicePrority_{sc} v=\"0\" />"));
    }
    for sc in 0..2 {
        xml.push_str(&format!("<monoVoiceEnvelope_{sc} v=\"0\" />"));
    }
    for sc in 0..2 {
        xml.push_str(&format!("<polyVoiceRepeatedKeyMode_{sc} v=\"0\" />"));
    }
    xml.push_str("<hardclipmodes global=\"1\" sc0=\"1\" sc1=\"1\" />");
    xml.push_str("<tuningApplicationMode v=\"1\" />");
    xml.push_str("</nonparamconfig>");
    xml.push_str("</patch>");
    xml
}

/// Build the `sub3` patch chunk: `patch_header` + XML (no wavetables).
///
/// `patch_header` layout (little-endian, from `PatchFileHeaderStructs.h`):
///   `char tag[4] = "sub3"; u32 xmlsize; u32 wtsize[2][3] (all zero)`.
pub fn build_patch_chunk(xml: &str) -> Vec<u8> {
    let xml_bytes = xml.as_bytes();
    let mut chunk = Vec::with_capacity(4 + 4 + 24 + xml_bytes.len());
    chunk.extend_from_slice(b"sub3");
    chunk.extend_from_slice(&(xml_bytes.len() as u32).to_le_bytes());
    // wtsize[2][3] = 6 * u32, all zero (no wavetable data).
    for _ in 0..6 {
        chunk.extend_from_slice(&0u32.to_le_bytes());
    }
    chunk.extend_from_slice(xml_bytes);
    chunk
}

/// Build a complete, loadable Surge XT `.fxp` from mapped parameters.
pub fn build_fxp(meta: &SurgeMeta, mapped: &BTreeMap<String, ParamValue>) -> Vec<u8> {
    let xml = build_patch_xml(meta, mapped);
    let chunk = build_patch_chunk(&xml);
    // FPCh container, fxID='cjs3', fxVersion=1, numPrograms=1.
    fxp::build_chunk_fxp(SURGE_FX_ID, 1, &meta.name, 1, &chunk)
}

/// Parse a Surge `.fxp` back into `(meta, xml)` — used for round-trip tests.
pub fn parse_fxp(data: &[u8]) -> Result<(String, String), String> {
    let header = fxp::parse_fxp(data)?;
    if !header.is_chunk_format() {
        return Err("not an FPCh chunk-format FXP".into());
    }
    if &header.fx_id != SURGE_FX_ID {
        return Err(format!("not a Surge fxID (got {:?})", header.fx_id));
    }
    let chunk = &header.chunk;
    if chunk.len() < 4 + 4 + 24 || &chunk[0..4] != b"sub3" {
        return Err("chunk is not a sub3 patch".into());
    }
    let xmlsize = u32::from_le_bytes(chunk[4..8].try_into().unwrap()) as usize;
    let start = 4 + 4 + 24;
    let end = start + xmlsize;
    if end > chunk.len() {
        return Err("sub3 xmlsize exceeds chunk".into());
    }
    let xml = String::from_utf8_lossy(&chunk[start..end]).into_owned();
    Ok((header.name, xml))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every override name produced from a full-coverage input must exist in the
    /// init table, or the override is silently discarded (a bug). This is the
    /// exhaustiveness guard the review asks for.
    #[test]
    fn every_override_name_exists_in_init_table() {
        let mapper = crate::mappers::mapper_for("surge").unwrap();
        // Start from full defaults, then force the values that trigger every
        // optional override path (unison voices > 1, non-zero mod depths).
        let mut mapped = mapper.defaults();
        mapped.insert("unison_voices".into(), ParamValue::Num(4.0));
        mapped.insert("unison_detune".into(), ParamValue::Num(20.0));
        for k in [
            "mod_lfo1_pitch",
            "mod_lfo1_cutoff",
            "mod_lfo2_pitch",
            "mod_lfo2_cutoff",
            "mod_env_pitch",
            "mod_velocity_cutoff",
        ] {
            mapped.insert(k.into(), ParamValue::Num(0.5));
        }
        let names = override_names(&mapped);
        assert!(!names.is_empty());
        let init: std::collections::HashSet<&str> =
            SURGE_INIT_PARAMS.iter().map(|(n, ..)| *n).collect();
        for n in &names {
            assert!(
                init.contains(n.as_str()),
                "override name {n:?} is not in SURGE_INIT_PARAMS (silently discarded)"
            );
        }
    }
}
