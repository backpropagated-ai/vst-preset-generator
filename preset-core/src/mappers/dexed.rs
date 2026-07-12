// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Dexed / Yamaha DX7 (6-operator FM) parameter mapper.
//
// Faithful port of the legacy Python `vst_mappers/dexed.py`. The table is built
// procedurally: 6 operators each with the same ~21 parameters, plus the global
// (algorithm/feedback/LFO/pitch-EG/controller) parameters. Because the DX7 has
// ~170 parameters with per-operator names generated at runtime, we build the
// spec table once into a leaked `&'static` slice.

use std::sync::OnceLock;

use crate::mappers::Mapper;
use crate::param::ParamSpec;

/// The Dexed / DX7 parameter mapper.
pub struct DexedMapper;

impl Mapper for DexedMapper {
    fn id(&self) -> &'static str {
        "dexed"
    }
    fn specs(&self) -> &'static [ParamSpec] {
        dexed_specs()
    }
}

const CURVES: &[&str] = &["lin_neg", "exp_neg", "exp_pos", "lin_pos"];
const CTRL_TARGETS: &[&str] = &["pitch", "amplitude", "eg_bias", "all"];
const LFO_WAVES: &[&str] = &["triangle", "saw_down", "saw_up", "square", "sine", "s&h"];

/// Names are generated at runtime, so we leak them (once) to get `&'static str`.
fn leak(s: String) -> &'static str {
    Box::leak(s.into_boxed_str())
}

fn build_specs() -> Vec<ParamSpec> {
    let mut p: Vec<ParamSpec> = Vec::new();

    // 6 operators (each with the same parameter block).
    for op in 1..=6 {
        let pre = format!("op{op}_");
        // core
        p.push(ParamSpec::discrete(
            leak(format!("{pre}level")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}coarse")),
            0.0,
            31.0,
            1.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}fine")),
            0.0,
            99.0,
            0.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}detune")),
            -7.0,
            7.0,
            0.0,
            1.0,
        ));
        // envelope rates R1-R4
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_r1")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_r2")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_r3")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_r4")),
            0.0,
            99.0,
            0.0,
            1.0,
        ));
        // envelope levels L1-L4
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_l1")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_l2")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_l3")),
            0.0,
            99.0,
            99.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}eg_l4")),
            0.0,
            99.0,
            0.0,
            1.0,
        ));
        // keyboard level scaling
        p.push(ParamSpec::discrete(
            leak(format!("{pre}lev_scale_break")),
            0.0,
            99.0,
            60.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}lev_scale_left_depth")),
            0.0,
            99.0,
            0.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}lev_scale_right_depth")),
            0.0,
            99.0,
            0.0,
            1.0,
        ));
        p.push(ParamSpec::enum_(
            leak(format!("{pre}lev_scale_left_curve")),
            CURVES,
        ));
        p.push(ParamSpec::enum_(
            leak(format!("{pre}lev_scale_right_curve")),
            CURVES,
        ));
        // rate scaling / sensitivities
        p.push(ParamSpec::discrete(
            leak(format!("{pre}rate_scale")),
            0.0,
            7.0,
            0.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}amp_mod_sens")),
            0.0,
            3.0,
            0.0,
            1.0,
        ));
        p.push(ParamSpec::discrete(
            leak(format!("{pre}vel_sens")),
            0.0,
            7.0,
            0.0,
            1.0,
        ));
        // frequency mode
        p.push(ParamSpec::enum_(
            leak(format!("{pre}mode")),
            &["ratio", "fixed"],
        ));
        p.push(ParamSpec::exponential(
            leak(format!("{pre}freq_fixed")),
            1.0,
            10000.0,
            1000.0,
            3.0,
        ));
    }

    // Global
    p.push(ParamSpec::discrete("algorithm", 1.0, 32.0, 1.0, 1.0));
    p.push(ParamSpec::discrete("feedback", 0.0, 7.0, 0.0, 1.0));
    p.push(ParamSpec::boolean("osc_sync", 0.0));

    // LFO
    p.push(ParamSpec::discrete("lfo_speed", 0.0, 99.0, 35.0, 1.0));
    p.push(ParamSpec::discrete("lfo_delay", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::discrete("lfo_pmd", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::discrete("lfo_amd", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::boolean("lfo_sync", 0.0));
    p.push(ParamSpec::enum_("lfo_wave", LFO_WAVES));
    p.push(ParamSpec::discrete(
        "lfo_pitch_mod_sens",
        0.0,
        7.0,
        3.0,
        1.0,
    ));

    // Pitch envelope
    p.push(ParamSpec::discrete("pitch_eg_r1", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_r2", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_r3", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_r4", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l1", 0.0, 99.0, 50.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l2", 0.0, 99.0, 50.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l3", 0.0, 99.0, 50.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l4", 0.0, 99.0, 50.0, 1.0));

    // Transpose & voice mode
    p.push(ParamSpec::discrete("transpose", -48.0, 48.0, 0.0, 1.0));
    p.push(ParamSpec::enum_("voice_mode", &["poly", "mono"]));

    // Portamento
    p.push(ParamSpec::discrete("portamento_time", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::boolean("portamento_glissando", 0.0));

    // Pitch bend
    p.push(ParamSpec::discrete("pitch_bend_range", 0.0, 12.0, 2.0, 1.0));
    p.push(ParamSpec::discrete("pitch_bend_step", 0.0, 12.0, 0.0, 1.0));

    // Controllers
    p.push(ParamSpec::discrete("mod_wheel_range", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::enum_("mod_wheel_target", CTRL_TARGETS));
    p.push(ParamSpec::discrete("foot_ctrl_range", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::enum_("foot_ctrl_target", CTRL_TARGETS));
    p.push(ParamSpec::discrete(
        "breath_ctrl_range",
        0.0,
        99.0,
        0.0,
        1.0,
    ));
    p.push(ParamSpec::enum_("breath_ctrl_target", CTRL_TARGETS));
    p.push(ParamSpec::discrete("aftertouch_range", 0.0, 99.0, 0.0, 1.0));
    p.push(ParamSpec::enum_("aftertouch_target", CTRL_TARGETS));

    p
}

/// The Dexed spec table, built once and leaked to `&'static`.
pub fn dexed_specs() -> &'static [ParamSpec] {
    static SPECS: OnceLock<Vec<ParamSpec>> = OnceLock::new();
    SPECS.get_or_init(build_specs).as_slice()
}
