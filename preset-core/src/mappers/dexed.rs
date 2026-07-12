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
        p.push(
            ParamSpec::discrete(leak(format!("{pre}level")), 0.0, 99.0, 99.0, 1.0).with_note(
                "Operator output level 0..99. For a CARRIER this is loudness (90..99 typical); \
                 for a MODULATOR this is timbre/brightness (0 = pure sine from its carrier, \
                 50..75 = mellow, 75..90 = bright, >90 = harsh/metallic).",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}coarse")), 0.0, 31.0, 1.0, 1.0).with_note(
                "Coarse frequency ratio 0..31 (ratio mode): 0 = 0.5x, 1 = 1x (fundamental), \
                 2 = 2x (octave up), 3 = 3x, ... Non-integer-related modulator ratios sound \
                 inharmonic/bell-like; low integer ratios sound harmonic.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}fine")), 0.0, 99.0, 0.0, 1.0).with_note(
                "Fine ratio 0..99: multiplies the coarse ratio by 1 + fine/100 \
                 (e.g. coarse 1 + fine 41 = ratio 1.41, inharmonic).",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}detune")), -7.0, 7.0, 0.0, 1.0).with_note(
                "Slight detune -7..+7 for beating/chorusing between operators; 0 = none.",
            ),
        );
        // envelope rates R1-R4
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_r1")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG attack RATE 0..99 (99 = instant, lower = slower). Keys/plucks want 95..99; \
                 pads/strings want 30..60.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_r2")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG rate from L1 to L2 (first decay). 99 = instant.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_r3")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG rate from L2 to L3 (approach to sustain). 99 = instant.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_r4")), 0.0, 99.0, 0.0, 1.0).with_note(
                "EG release RATE toward L4 after key-off. 99 = instant cutoff, 50..70 = \
                 natural release, <30 = long tail. 0 keeps sounding — avoid 0 unless a drone \
                 is wanted.",
            ),
        );
        // envelope levels L1-L4
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_l1")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG peak level after attack (0..99). Usually 99.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_l2")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG level after first decay (0..99).",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_l3")), 0.0, 99.0, 99.0, 1.0).with_note(
                "EG SUSTAIN level while the key is held (0..99). Plucks/keys decay to a low \
                 L3 (0..40); pads/organs hold a high L3 (80..99). On modulators a lower L3 \
                 darkens the tone over the note.",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}eg_l4")), 0.0, 99.0, 0.0, 1.0).with_note(
                "EG final level after release. Almost always 0 (silence); >0 keeps the \
                 operator sounding forever.",
            ),
        );
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
        p.push(
            ParamSpec::discrete(leak(format!("{pre}rate_scale")), 0.0, 7.0, 0.0, 1.0).with_note(
                "Keyboard rate scaling 0..7: higher = envelopes get faster toward high notes \
                 (natural for pianos/plucks, use 1..3).",
            ),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}amp_mod_sens")), 0.0, 3.0, 0.0, 1.0)
                .with_note("How much the LFO's amplitude modulation affects this operator (0..3)."),
        );
        p.push(
            ParamSpec::discrete(leak(format!("{pre}vel_sens")), 0.0, 7.0, 0.0, 1.0).with_note(
                "Velocity sensitivity 0..7. On carriers: louder when hit harder. On \
                 modulators: brighter when hit harder (very expressive, 2..5 typical for \
                 keys/EPs).",
            ),
        );
        // frequency mode
        p.push(
            ParamSpec::enum_(leak(format!("{pre}mode")), &["ratio", "fixed"]).with_note(
                "ratio = frequency tracks the key (musical, the usual choice); fixed = \
                 constant Hz regardless of key (for percussion/noise/drone components).",
            ),
        );
        p.push(
            ParamSpec::logarithmic(leak(format!("{pre}freq_fixed")), 1.0, 9772.0, 440.0)
                .with_note("Fixed-mode frequency in Hz; ONLY used when mode = fixed."),
        );
    }

    // Global
    p.push(
        ParamSpec::discrete("algorithm", 1.0, 32.0, 1.0, 1.0).with_note(
            "DX7 algorithm 1..32 — THE most important choice: it fixes which operators are \
             carriers (audible) vs modulators (shape timbre). Guide: 1..4 = two stacks \
             (rich, EPs/basses; carriers 1,3); 5..6 = three 2-op pairs (bells, EPs; \
             carriers 1,3,5); 7..11 = one thick stack + a pair; 16..18 = single carrier \
             (op1) driven by many modulators (solo leads); 32 = all six carriers in \
             parallel (additive/organ, no FM). Operators listed as carriers MUST keep \
             level >= 90 to be heard.",
        ),
    );
    p.push(
        ParamSpec::discrete("feedback", 0.0, 7.0, 0.0, 1.0).with_note(
            "Feedback 0..7 on the algorithm's loop operator: adds sawtooth-like buzz/edge. \
             0 = pure, 5..7 = gritty/brassy.",
        ),
    );
    p.push(ParamSpec::boolean("osc_sync", 0.0)
        .with_note("Restart all operator phases on each key-on (tighter, punchier attacks)."));

    // LFO
    p.push(ParamSpec::discrete("lfo_speed", 0.0, 99.0, 35.0, 1.0)
        .with_note("LFO speed 0..99 (35 ≈ 4..5 Hz vibrato range)."));
    p.push(ParamSpec::discrete("lfo_delay", 0.0, 99.0, 0.0, 1.0)
        .with_note("Delay before the LFO fades in after key-on (0 = immediate)."));
    p.push(ParamSpec::discrete("lfo_pmd", 0.0, 99.0, 0.0, 1.0)
        .with_note("LFO pitch-mod depth 0..99: vibrato amount (10..30 subtle, >50 dramatic)."));
    p.push(ParamSpec::discrete("lfo_amd", 0.0, 99.0, 0.0, 1.0)
        .with_note("LFO amplitude-mod depth 0..99: tremolo amount."));
    p.push(ParamSpec::boolean("lfo_sync", 0.0)
        .with_note("Restart the LFO phase on each key-on."));
    p.push(ParamSpec::enum_("lfo_wave", LFO_WAVES));
    p.push(
        ParamSpec::discrete("lfo_pitch_mod_sens", 0.0, 7.0, 3.0, 1.0)
            .with_note("Pitch-mod sensitivity 0..7 scaling lfo_pmd (0 disables vibrato)."),
    );

    // Pitch envelope
    p.push(ParamSpec::discrete("pitch_eg_r1", 0.0, 99.0, 99.0, 1.0)
        .with_note("Pitch EG rate 1 (99 = instant). Levels 50 = no pitch change."));
    p.push(ParamSpec::discrete("pitch_eg_r2", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_r3", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_r4", 0.0, 99.0, 99.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l1", 0.0, 99.0, 50.0, 1.0).with_note(
        "Pitch EG level 1: 50 = no pitch offset, <50 = down, >50 = up. Keep ALL pitch EG \
         levels at 50 unless a pitch sweep/drop effect is wanted.",
    ));
    p.push(ParamSpec::discrete("pitch_eg_l2", 0.0, 99.0, 50.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l3", 0.0, 99.0, 50.0, 1.0));
    p.push(ParamSpec::discrete("pitch_eg_l4", 0.0, 99.0, 50.0, 1.0));

    // Transpose & voice mode
    p.push(ParamSpec::discrete("transpose", -24.0, 24.0, 0.0, 1.0)
        .with_note("Semitones -24..+24 (0 = middle C standard)."));
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
