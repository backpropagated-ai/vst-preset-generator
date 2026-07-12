// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Hand-authored, deterministic prompt -> parameters anchor bank.
//
// This file contains NO third-party preset content. Every recipe is written by
// hand from first-principles sound design, expressed in the same normalized
// 0..1 / enum-by-name vocabulary the native mapper consumes (see
// preset-core/src/mappers). The browser embeds each anchor phrase once with a
// small in-browser model (all-MiniLM-L6-v2), then blends the top matches for a
// user prompt. If the model can't load, a keyword fallback matches over the
// same bank.
//
// Recipe values are normalized inputs, exactly what build_preset() expects:
//   * numbers are 0..1 (the mapper clamps + scales them per the ParamSpec),
//   * strings are enum option names (e.g. "lp_ladder", "ladder", "ratio").
// Any parameter left out of a recipe uses the synth's default.

// ---------------------------------------------------------------------------
// Category anchors: one entry per sound-design archetype. `phrase` is what we
// embed; `keywords` drives the offline fallback; `recipes` gives a per-synth
// normalized parameter set. Categories cover bass (sub/acid/reese), pad
// (warm/dark/evolving), lead (pluck/saw/sync), keys/EP, organ, bells, brass,
// strings, drone and noise/fx.
// ---------------------------------------------------------------------------
export const CATEGORY_ANCHORS = [
  // ---- BASS ----------------------------------------------------------------
  {
    id: "sub_bass",
    phrase: "deep round sub bass, pure sine weight, sits under the mix",
    altPhrases: ["clean 808 style sub bass for hip hop", "heavy low end sine bass"],
    keywords: ["sub", "subbass", "deep", "low", "808", "round", "weight", "boom"],
    recipes: {
      surge: {
        osc1_type: "sine", osc1_pitch: 0.25, osc1_level: 0.9,
        osc2_level: 0.0, osc3_level: 0.0,
        filter1_type: "lp24", filter1_cutoff: 0.28, filter1_resonance: 0.05,
        amp_attack: 0.02, amp_decay: 0.5, amp_sustain: 0.85, amp_release: 0.25,
        filter_env_depth: 0.5, polyphony: 0.02, master_volume: 0.78,
      },
      vital: {
        osc_1_wave: 0.0, osc_1_level: 0.9, osc_1_tune: 0.375,
        osc_2_level: 0.0, osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.28, filter_1_resonance: 0.1,
        env_1_attack: 0.02, env_1_decay: 0.5, env_1_sustain: 0.85, env_1_release: 0.2,
        master_volume: 0.78,
      },
      dexed: {
        algorithm: 0.03, feedback: 0.0,
        op1_level: 0.99, op1_coarse: 0.032, op1_mode: "ratio",
        op2_level: 0.0, op3_level: 0.0, op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
        op1_eg_r1: 0.9, op1_eg_r4: 0.6,
      },
    },
  },
  {
    id: "acid_bass",
    phrase: "squelchy resonant acid bass, screaming 303 filter with envelope sweep",
    altPhrases: ["rubbery tb-303 acid line for techno", "resonant squelchy bassline"],
    keywords: ["acid", "303", "squelch", "resonant", "resonance", "squelchy", "tb303", "rubber"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_width: 0.35, osc1_level: 0.85, osc1_pitch: 0.5,
        osc2_level: 0.0, osc3_level: 0.0,
        filter1_type: "lp_ladder", filter1_cutoff: 0.32, filter1_resonance: 0.88,
        filter_attack: 0.0, filter_decay: 0.35, filter_sustain: 0.1, filter_release: 0.2,
        filter_env_depth: 0.85,
        amp_attack: 0.0, amp_decay: 0.4, amp_sustain: 0.6, amp_release: 0.15,
        polyphony: 0.02, portamento_time: 0.12, master_volume: 0.72,
      },
      vital: {
        osc_1_wave: 0.7, osc_1_level: 0.85,
        osc_2_level: 0.0, osc_3_level: 0.0,
        filter_1_type: "ladder", filter_1_cutoff: 0.32, filter_1_resonance: 0.85,
        filter_1_drive: 0.3,
        env_2_attack: 0.0, env_2_decay: 0.35, env_2_sustain: 0.1, env_2_release: 0.2,
        env_1_attack: 0.0, env_1_decay: 0.4, env_1_sustain: 0.6, env_1_release: 0.15,
        mod_1_source: "env_2", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.9,
        master_volume: 0.72,
      },
      dexed: {
        algorithm: 0.5, feedback: 0.85,
        op1_level: 0.99, op1_coarse: 0.032, op1_mode: "ratio",
        op2_level: 0.85, op2_coarse: 0.065,
        op3_level: 0.0, op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
        op1_eg_r2: 0.5, op1_eg_l2: 0.4,
      },
    },
  },
  {
    id: "reese_bass",
    phrase: "wide detuned reese bass, thick moving metallic drone for drum and bass",
    keywords: ["reese", "detuned", "wide", "thick", "dnb", "neuro", "growl", "moving"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_level: 0.6, osc1_pitch: 0.5,
        osc2_type: "classic", osc2_level: 0.6, osc2_pitch: 0.5,
        osc3_level: 0.0,
        unison_voices: 0.4, unison_detune: 0.35,
        filter1_type: "lp24", filter1_cutoff: 0.4, filter1_resonance: 0.3,
        amp_attack: 0.02, amp_decay: 0.5, amp_sustain: 0.9, amp_release: 0.3,
        lfo1_shape: "sine", lfo1_rate: 0.15, mod_lfo1_cutoff: 0.3,
        polyphony: 0.05, master_volume: 0.7,
      },
      vital: {
        osc_1_wave: 0.7, osc_1_level: 0.6, osc_1_unison_voices: 0.4, osc_1_unison_detune: 0.5,
        osc_2_wave: 0.7, osc_2_level: 0.6, osc_2_tune: 0.365,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.4, filter_1_resonance: 0.35,
        env_1_attack: 0.02, env_1_decay: 0.5, env_1_sustain: 0.9, env_1_release: 0.3,
        lfo_1_frequency: 0.2, mod_1_source: "lfo_1", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.3,
        master_volume: 0.7,
      },
      dexed: {
        algorithm: 0.9, feedback: 0.7,
        op1_level: 0.99, op1_coarse: 0.032, op1_detune: 0.35,
        op2_level: 0.9, op2_coarse: 0.032, op2_detune: 0.65,
        op3_level: 0.6, op3_coarse: 0.065,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- PAD -----------------------------------------------------------------
  {
    id: "warm_pad",
    phrase: "warm analog pad, soft slow attack, lush unison, gentle chorus and air",
    altPhrases: ["cozy vintage synth pad, mellow and wide", "soft warm sustained background pad"],
    keywords: ["warm", "pad", "lush", "soft", "analog", "chorus", "gentle", "cozy"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_level: 0.5, osc1_width: 0.55,
        osc2_type: "classic", osc2_level: 0.5, osc2_pitch: 0.51,
        osc3_level: 0.0,
        unison_voices: 0.35, unison_detune: 0.18,
        filter1_type: "lp24", filter1_cutoff: 0.5, filter1_resonance: 0.12,
        amp_attack: 0.55, amp_decay: 0.6, amp_sustain: 0.85, amp_release: 0.6,
        lfo1_shape: "sine", lfo1_rate: 0.1, mod_lfo1_cutoff: 0.12,
        character: "warm", drift: 0.3, master_volume: 0.68,
      },
      vital: {
        osc_1_wave: 0.5, osc_1_level: 0.5, osc_1_unison_voices: 0.35, osc_1_unison_detune: 0.3,
        osc_2_wave: 0.5, osc_2_level: 0.5, osc_2_tune: 0.512,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.5, filter_1_resonance: 0.15,
        env_1_attack: 0.55, env_1_decay: 0.6, env_1_sustain: 0.85, env_1_release: 0.6,
        chorus_mix: 0.4, chorus_depth: 0.5, reverb_mix: 0.25,
        master_volume: 0.68,
      },
      dexed: {
        algorithm: 0.97, feedback: 0.3,
        op1_level: 0.95, op1_coarse: 0.032, op1_eg_r1: 0.4, op1_eg_r4: 0.35,
        op2_level: 0.6, op2_coarse: 0.032, op2_detune: 0.6,
        op3_level: 0.5, op3_coarse: 0.065,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  {
    id: "dark_pad",
    phrase: "dark brooding pad, muffled low cutoff, ominous cinematic texture",
    keywords: ["dark", "brooding", "muffled", "ominous", "cinematic", "murky", "shadow", "sinister"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_level: 0.55, osc1_pitch: 0.4,
        osc2_type: "classic", osc2_level: 0.45, osc2_pitch: 0.49,
        osc3_level: 0.0,
        unison_voices: 0.3, unison_detune: 0.15,
        filter1_type: "lp24", filter1_cutoff: 0.28, filter1_resonance: 0.2,
        amp_attack: 0.6, amp_decay: 0.6, amp_sustain: 0.8, amp_release: 0.7,
        lfo1_shape: "sine", lfo1_rate: 0.08, mod_lfo1_cutoff: 0.15,
        character: "warm", master_volume: 0.66,
      },
      vital: {
        osc_1_wave: 0.5, osc_1_level: 0.55, osc_1_tune: 0.375,
        osc_2_wave: 0.5, osc_2_level: 0.45, osc_2_tune: 0.49,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.28, filter_1_resonance: 0.25,
        env_1_attack: 0.6, env_1_decay: 0.6, env_1_sustain: 0.8, env_1_release: 0.7,
        reverb_mix: 0.35, reverb_size: 0.7,
        master_volume: 0.66,
      },
      dexed: {
        algorithm: 0.97, feedback: 0.4,
        op1_level: 0.9, op1_coarse: 0.0, op1_eg_r1: 0.35, op1_eg_r4: 0.3,
        op2_level: 0.45, op2_coarse: 0.032,
        op3_level: 0.0, op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  {
    id: "evolving_pad",
    phrase: "ethereal evolving pad, slowly morphing shimmering wavetable motion",
    keywords: ["evolving", "ethereal", "morphing", "shimmer", "moving", "wavetable", "atmospheric", "drifting"],
    recipes: {
      surge: {
        osc1_type: "wavetable", osc1_level: 0.5,
        osc2_type: "wavetable", osc2_level: 0.4, osc2_pitch: 0.52,
        osc3_level: 0.0,
        unison_voices: 0.3, unison_detune: 0.2,
        filter1_type: "lp12", filter1_cutoff: 0.55, filter1_resonance: 0.15,
        amp_attack: 0.5, amp_decay: 0.6, amp_sustain: 0.8, amp_release: 0.65,
        lfo1_shape: "sine", lfo1_rate: 0.05, mod_lfo1_cutoff: 0.35,
        lfo2_shape: "triangle", lfo2_rate: 0.04,
        fx_send_1: 0.4, master_volume: 0.66,
      },
      vital: {
        osc_1_wave: 0.6, osc_1_level: 0.5, osc_1_unison_voices: 0.3, osc_1_unison_detune: 0.3,
        osc_2_wave: 0.4, osc_2_level: 0.4, osc_2_tune: 0.52,
        osc_3_level: 0.0,
        filter_1_type: "digital", filter_1_cutoff: 0.55, filter_1_resonance: 0.2,
        env_1_attack: 0.5, env_1_decay: 0.6, env_1_sustain: 0.8, env_1_release: 0.65,
        lfo_1_frequency: 0.1, mod_1_source: "lfo_1", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.4,
        reverb_mix: 0.4, reverb_size: 0.8, reverb_decay_time: 0.7,
        master_volume: 0.66,
      },
      dexed: {
        algorithm: 0.8, feedback: 0.5,
        op1_level: 0.9, op1_coarse: 0.032, op1_eg_r1: 0.3, op1_eg_r4: 0.3,
        op2_level: 0.6, op2_coarse: 0.097, op2_detune: 0.6,
        op3_level: 0.5, op3_coarse: 0.13,
        op4_level: 0.4, op5_level: 0.0, op6_level: 0.0,
        lfo_speed: 0.15, lfo_pmd: 0.2, lfo_wave: "sine",
      },
    },
  },
  // ---- LEAD ----------------------------------------------------------------
  {
    id: "pluck_lead",
    phrase: "bright plucky lead, short snappy attack, percussive and clean",
    keywords: ["pluck", "plucky", "snappy", "percussive", "bright", "short", "stab", "poke"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_width: 0.5, osc1_level: 0.8,
        osc2_level: 0.0, osc3_level: 0.0,
        filter1_type: "lp24", filter1_cutoff: 0.6, filter1_resonance: 0.35,
        filter_attack: 0.0, filter_decay: 0.2, filter_sustain: 0.0, filter_release: 0.15,
        filter_env_depth: 0.7,
        amp_attack: 0.0, amp_decay: 0.25, amp_sustain: 0.0, amp_release: 0.2,
        polyphony: 0.12, master_volume: 0.72,
      },
      vital: {
        osc_1_wave: 0.7, osc_1_level: 0.8,
        osc_2_level: 0.0, osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.6, filter_1_resonance: 0.35,
        env_1_attack: 0.0, env_1_decay: 0.25, env_1_sustain: 0.0, env_1_release: 0.2,
        env_2_attack: 0.0, env_2_decay: 0.2, env_2_sustain: 0.0, env_2_release: 0.15,
        mod_1_source: "env_2", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.7,
        master_volume: 0.72,
      },
      dexed: {
        algorithm: 0.5, feedback: 0.4,
        op1_level: 0.99, op1_coarse: 0.032,
        op2_level: 0.7, op2_coarse: 0.13, op2_eg_r1: 0.9, op2_eg_r2: 0.6, op2_eg_l2: 0.2,
        op3_level: 0.0, op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  {
    id: "saw_lead",
    phrase: "screaming supersaw lead, big detuned unison, anthemic and cutting",
    altPhrases: ["huge trance supersaw lead", "bright detuned saw lead for edm drops"],
    keywords: ["saw", "supersaw", "unison", "anthem", "trance", "cutting", "big", "screaming"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_width: 0.5, osc1_level: 0.75, osc1_pitch: 0.5,
        osc2_type: "classic", osc2_level: 0.55, osc2_pitch: 0.51,
        osc3_level: 0.0,
        unison_voices: 0.55, unison_detune: 0.28,
        filter1_type: "lp24", filter1_cutoff: 0.72, filter1_resonance: 0.2,
        amp_attack: 0.02, amp_decay: 0.4, amp_sustain: 0.9, amp_release: 0.25,
        character: "bright", fx_send_1: 0.3, master_volume: 0.72,
      },
      vital: {
        osc_1_wave: 0.7, osc_1_level: 0.75, osc_1_unison_voices: 0.55, osc_1_unison_detune: 0.5,
        osc_2_wave: 0.7, osc_2_level: 0.55, osc_2_tune: 0.512,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.72, filter_1_resonance: 0.2,
        env_1_attack: 0.02, env_1_decay: 0.4, env_1_sustain: 0.9, env_1_release: 0.25,
        reverb_mix: 0.2, master_volume: 0.72,
      },
      dexed: {
        algorithm: 0.6, feedback: 0.8,
        op1_level: 0.99, op1_coarse: 0.032, op1_detune: 0.4,
        op2_level: 0.8, op2_coarse: 0.032, op2_detune: 0.6,
        op3_level: 0.6, op3_coarse: 0.065,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  {
    id: "sync_lead",
    phrase: "aggressive hard sync lead, biting metallic edge, harmonic sweep",
    keywords: ["sync", "hard", "aggressive", "biting", "metallic", "harsh", "edgy", "tearing"],
    recipes: {
      surge: {
        osc1_type: "modern", osc1_level: 0.85, osc1_width: 0.4,
        osc2_level: 0.0, osc3_level: 0.0,
        filter1_type: "bp12", filter1_cutoff: 0.6, filter1_resonance: 0.45,
        filter_attack: 0.0, filter_decay: 0.35, filter_sustain: 0.3, filter_release: 0.2,
        filter_env_depth: 0.65,
        amp_attack: 0.0, amp_decay: 0.4, amp_sustain: 0.8, amp_release: 0.2,
        lfo1_shape: "triangle", lfo1_rate: 0.35, mod_lfo1_cutoff: 0.2,
        polyphony: 0.1, character: "bright", master_volume: 0.7,
      },
      vital: {
        osc_1_wave: 0.8, osc_1_level: 0.85,
        osc_2_level: 0.0, osc_3_level: 0.0,
        filter_1_type: "diode", filter_1_cutoff: 0.6, filter_1_resonance: 0.45, filter_1_drive: 0.4,
        env_1_attack: 0.0, env_1_decay: 0.4, env_1_sustain: 0.8, env_1_release: 0.2,
        distortion_type: "soft_clip", distortion_drive: 0.3, distortion_mix: 0.4,
        master_volume: 0.7,
      },
      dexed: {
        algorithm: 0.4, feedback: 0.95,
        op1_level: 0.99, op1_coarse: 0.032,
        op2_level: 0.85, op2_coarse: 0.23, op2_eg_r4: 0.3,
        op3_level: 0.6, op3_coarse: 0.42,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- KEYS / EP -----------------------------------------------------------
  {
    id: "electric_piano",
    phrase: "warm tine electric piano, bell-like Rhodes with soft bark and bloom",
    altPhrases: ["classic dx7 electric piano keys", "mellow rhodes-style ep for ballads"],
    keywords: ["rhodes", "electric piano", "ep", "tine", "keys", "wurli", "piano", "mellow"],
    recipes: {
      surge: {
        osc1_type: "sine", osc1_level: 0.7, osc1_pitch: 0.5,
        osc2_type: "sine", osc2_level: 0.3, osc2_pitch: 0.75,
        osc3_level: 0.0,
        filter1_type: "lp12", filter1_cutoff: 0.55, filter1_resonance: 0.1,
        amp_attack: 0.0, amp_decay: 0.6, amp_sustain: 0.4, amp_release: 0.4,
        mod_velocity_cutoff: 0.5, mod_velocity_amp: 0.7,
        character: "warm", master_volume: 0.7,
      },
      vital: {
        osc_1_wave: 0.0, osc_1_level: 0.7,
        osc_2_wave: 0.0, osc_2_level: 0.3, osc_2_tune: 0.625,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.55, filter_1_resonance: 0.1,
        env_1_attack: 0.0, env_1_decay: 0.6, env_1_sustain: 0.4, env_1_release: 0.4,
        chorus_mix: 0.2, master_volume: 0.7,
      },
      dexed: {
        // Classic DX7 E.PIANO 1 topology: op1 carrier, op2 modulator ~1x with a
        // fast-decaying bark, plus a bell partial. Hand-authored, not copied.
        algorithm: 0.15, feedback: 0.3,
        op1_level: 0.99, op1_coarse: 0.032, op1_eg_r1: 0.9, op1_eg_r2: 0.4, op1_eg_l2: 0.3, op1_eg_r4: 0.4,
        op2_level: 0.75, op2_coarse: 0.045, op2_eg_r1: 0.95, op2_eg_r2: 0.5, op2_eg_l2: 0.1,
        op3_level: 0.6, op3_coarse: 0.45, op3_eg_r1: 0.99, op3_eg_r2: 0.7, op3_eg_l2: 0.0,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- ORGAN ---------------------------------------------------------------
  {
    id: "organ",
    phrase: "vintage drawbar organ, additive sine harmonics, steady sustained tone",
    keywords: ["organ", "drawbar", "hammond", "b3", "church", "additive", "harmonics", "gospel"],
    recipes: {
      surge: {
        osc1_type: "sine", osc1_level: 0.6, osc1_pitch: 0.5,
        osc2_type: "sine", osc2_level: 0.45, osc2_pitch: 0.75,
        osc3_type: "sine", osc3_level: 0.35, osc3_pitch: 0.625,
        filter1_type: "lp12", filter1_cutoff: 0.7, filter1_resonance: 0.05,
        amp_attack: 0.0, amp_decay: 0.1, amp_sustain: 1.0, amp_release: 0.05,
        lfo1_shape: "sine", lfo1_rate: 0.4, mod_lfo1_pitch: 0.08,
        master_volume: 0.7,
      },
      vital: {
        osc_1_wave: 0.0, osc_1_level: 0.6,
        osc_2_wave: 0.0, osc_2_level: 0.45, osc_2_tune: 0.75,
        osc_3_wave: 0.0, osc_3_level: 0.35, osc_3_tune: 0.625,
        filter_1_type: "analog", filter_1_cutoff: 0.7, filter_1_resonance: 0.05,
        env_1_attack: 0.0, env_1_decay: 0.1, env_1_sustain: 1.0, env_1_release: 0.05,
        chorus_mix: 0.3, chorus_frequency: 0.5,
        master_volume: 0.7,
      },
      dexed: {
        algorithm: 0.97, feedback: 0.0,
        op1_level: 0.9, op1_coarse: 0.032,
        op2_level: 0.7, op2_coarse: 0.065,
        op3_level: 0.55, op3_coarse: 0.097,
        op4_level: 0.4, op4_coarse: 0.13,
        op5_level: 0.0, op6_level: 0.0,
        op1_eg_r4: 0.9, op2_eg_r4: 0.9,
      },
    },
  },
  // ---- BELLS ---------------------------------------------------------------
  {
    id: "bells",
    phrase: "glassy crystalline bell, inharmonic FM ring, sparkling long decay",
    keywords: ["bell", "bells", "glassy", "crystal", "chime", "sparkle", "glockenspiel", "tubular"],
    recipes: {
      surge: {
        osc1_type: "fm2", osc1_level: 0.75, osc1_pitch: 0.5,
        osc2_level: 0.0, osc3_level: 0.0,
        filter1_type: "hp12", filter1_cutoff: 0.35, filter1_resonance: 0.1,
        amp_attack: 0.0, amp_decay: 0.75, amp_sustain: 0.1, amp_release: 0.6,
        character: "bright", fx_send_1: 0.4, master_volume: 0.68,
      },
      vital: {
        osc_1_wave: 0.9, osc_1_level: 0.75,
        osc_2_level: 0.0, osc_3_level: 0.0,
        filter_1_type: "digital", filter_1_cutoff: 0.65, filter_1_resonance: 0.1,
        env_1_attack: 0.0, env_1_decay: 0.75, env_1_sustain: 0.1, env_1_release: 0.6,
        reverb_mix: 0.4, reverb_size: 0.7, master_volume: 0.68,
      },
      dexed: {
        // Inharmonic partials via non-integer coarse ratios -> classic FM bell.
        algorithm: 0.15, feedback: 0.2,
        op1_level: 0.99, op1_coarse: 0.032, op1_eg_r1: 0.99, op1_eg_r2: 0.35, op1_eg_l2: 0.0,
        op2_level: 0.8, op2_coarse: 0.45, op2_eg_r1: 0.99, op2_eg_r2: 0.3, op2_eg_l2: 0.0,
        op3_level: 0.6, op3_coarse: 0.71, op3_eg_r1: 0.99, op3_eg_r2: 0.25, op3_eg_l2: 0.0,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- BRASS ---------------------------------------------------------------
  {
    id: "brass",
    phrase: "bold synth brass section, punchy attack with a bite, warm and full",
    keywords: ["brass", "horn", "trumpet", "section", "fanfare", "punchy", "bold", "orchestral"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_width: 0.5, osc1_level: 0.7, osc1_pitch: 0.5,
        osc2_type: "classic", osc2_level: 0.55, osc2_pitch: 0.505,
        osc3_level: 0.0,
        unison_voices: 0.25, unison_detune: 0.12,
        filter1_type: "lp24", filter1_cutoff: 0.55, filter1_resonance: 0.2,
        filter_attack: 0.15, filter_decay: 0.4, filter_sustain: 0.6, filter_release: 0.25,
        filter_env_depth: 0.55,
        amp_attack: 0.08, amp_decay: 0.4, amp_sustain: 0.85, amp_release: 0.25,
        character: "warm", master_volume: 0.72,
      },
      vital: {
        osc_1_wave: 0.7, osc_1_level: 0.7,
        osc_2_wave: 0.7, osc_2_level: 0.55, osc_2_tune: 0.505,
        osc_3_level: 0.0,
        filter_1_type: "analog", filter_1_cutoff: 0.55, filter_1_resonance: 0.2,
        env_1_attack: 0.08, env_1_decay: 0.4, env_1_sustain: 0.85, env_1_release: 0.25,
        env_2_attack: 0.15, env_2_decay: 0.4, env_2_sustain: 0.6, env_2_release: 0.25,
        mod_1_source: "env_2", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.5,
        master_volume: 0.72,
      },
      dexed: {
        algorithm: 0.65, feedback: 0.5,
        op1_level: 0.99, op1_coarse: 0.032, op1_eg_r1: 0.6, op1_eg_r4: 0.4,
        op2_level: 0.85, op2_coarse: 0.032, op2_eg_r1: 0.55,
        op3_level: 0.6, op3_coarse: 0.065,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- STRINGS -------------------------------------------------------------
  {
    id: "strings",
    phrase: "lush ensemble strings, slow bowed swell, wide shimmering section",
    keywords: ["strings", "ensemble", "orchestral", "bowed", "swell", "violin", "cinematic", "sustained"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_width: 0.5, osc1_level: 0.55, osc1_pitch: 0.5,
        osc2_type: "classic", osc2_level: 0.5, osc2_pitch: 0.51,
        osc3_type: "classic", osc3_level: 0.4, osc3_pitch: 0.49,
        unison_voices: 0.45, unison_detune: 0.22,
        filter1_type: "lp12", filter1_cutoff: 0.55, filter1_resonance: 0.1,
        amp_attack: 0.4, amp_decay: 0.5, amp_sustain: 0.9, amp_release: 0.5,
        lfo1_shape: "sine", lfo1_rate: 0.35, mod_lfo1_pitch: 0.06,
        fx_send_1: 0.35, master_volume: 0.68,
      },
      vital: {
        osc_1_wave: 0.6, osc_1_level: 0.55, osc_1_unison_voices: 0.45, osc_1_unison_detune: 0.4,
        osc_2_wave: 0.6, osc_2_level: 0.5, osc_2_tune: 0.51,
        osc_3_wave: 0.6, osc_3_level: 0.4, osc_3_tune: 0.49,
        filter_1_type: "analog", filter_1_cutoff: 0.55, filter_1_resonance: 0.1,
        env_1_attack: 0.4, env_1_decay: 0.5, env_1_sustain: 0.9, env_1_release: 0.5,
        reverb_mix: 0.35, reverb_size: 0.7, chorus_mix: 0.3,
        master_volume: 0.68,
      },
      dexed: {
        algorithm: 0.97, feedback: 0.3,
        op1_level: 0.95, op1_coarse: 0.032, op1_eg_r1: 0.45, op1_eg_r4: 0.4,
        op2_level: 0.6, op2_coarse: 0.032, op2_detune: 0.6,
        op3_level: 0.5, op3_coarse: 0.032, op3_detune: 0.4,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
        lfo_speed: 0.35, lfo_pmd: 0.12, lfo_wave: "triangle",
      },
    },
  },
  // ---- DRONE ---------------------------------------------------------------
  {
    id: "drone",
    phrase: "dark sustained drone, static endless texture, ambient low rumble",
    keywords: ["drone", "ambient", "static", "sustained", "endless", "rumble", "texture", "underscore"],
    recipes: {
      surge: {
        osc1_type: "classic", osc1_level: 0.5, osc1_pitch: 0.4,
        osc2_type: "classic", osc2_level: 0.45, osc2_pitch: 0.401,
        osc3_type: "sine", osc3_level: 0.3, osc3_pitch: 0.25,
        unison_voices: 0.35, unison_detune: 0.14,
        filter1_type: "lp24", filter1_cutoff: 0.3, filter1_resonance: 0.15,
        amp_attack: 0.7, amp_decay: 0.5, amp_sustain: 1.0, amp_release: 0.8,
        lfo1_shape: "sine", lfo1_rate: 0.02, mod_lfo1_cutoff: 0.2,
        drift: 0.5, master_volume: 0.62,
      },
      vital: {
        osc_1_wave: 0.5, osc_1_level: 0.5, osc_1_tune: 0.375, osc_1_unison_voices: 0.35, osc_1_unison_detune: 0.25,
        osc_2_wave: 0.5, osc_2_level: 0.45, osc_2_tune: 0.377,
        osc_3_wave: 0.0, osc_3_level: 0.3, osc_3_tune: 0.25,
        filter_1_type: "analog", filter_1_cutoff: 0.3, filter_1_resonance: 0.15,
        env_1_attack: 0.7, env_1_decay: 0.5, env_1_sustain: 1.0, env_1_release: 0.8,
        lfo_1_frequency: 0.05, mod_1_source: "lfo_1", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.25,
        reverb_mix: 0.45, reverb_size: 0.9, reverb_decay_time: 0.85,
        master_volume: 0.62,
      },
      dexed: {
        algorithm: 0.97, feedback: 0.4,
        op1_level: 0.9, op1_coarse: 0.0, op1_eg_r1: 0.25, op1_eg_r4: 0.2,
        op2_level: 0.5, op2_coarse: 0.032, op2_detune: 0.55,
        op3_level: 0.35, op3_coarse: 0.032, op3_detune: 0.45,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
  // ---- NOISE / FX ----------------------------------------------------------
  {
    id: "noise_fx",
    phrase: "noisy riser sweep sound effect, filtered wind whoosh, tension builder",
    keywords: ["noise", "fx", "riser", "sweep", "whoosh", "wind", "impact", "sfx", "effect", "hiss"],
    recipes: {
      surge: {
        osc1_level: 0.0, osc2_level: 0.0, osc3_level: 0.0,
        noise_level: 0.85,
        filter1_type: "bp12", filter1_cutoff: 0.4, filter1_resonance: 0.55,
        filter_attack: 0.6, filter_decay: 0.5, filter_sustain: 0.9, filter_release: 0.4,
        filter_env_depth: 0.85,
        amp_attack: 0.5, amp_decay: 0.4, amp_sustain: 0.9, amp_release: 0.4,
        lfo1_shape: "noise", lfo1_rate: 0.5, mod_lfo1_cutoff: 0.25,
        fx_send_1: 0.4, master_volume: 0.66,
      },
      vital: {
        osc_1_level: 0.0, osc_2_level: 0.0, osc_3_level: 0.0,
        sample_level: 0.0,
        filter_1_type: "phaser", filter_1_cutoff: 0.4, filter_1_resonance: 0.55,
        env_1_attack: 0.5, env_1_decay: 0.4, env_1_sustain: 0.9, env_1_release: 0.4,
        env_2_attack: 0.6, env_2_decay: 0.5, env_2_sustain: 0.9, env_2_release: 0.4,
        mod_1_source: "env_2", mod_1_destination: "filter_1_cutoff", mod_1_amount: 0.85,
        reverb_mix: 0.4, master_volume: 0.66,
      },
      dexed: {
        // No true noise source on the DX7; approximate a bright chaotic FM wash
        // via max feedback + inharmonic ratios.
        algorithm: 0.4, feedback: 0.99,
        op1_level: 0.9, op1_coarse: 0.71, op1_eg_r1: 0.4,
        op2_level: 0.85, op2_coarse: 0.93,
        op3_level: 0.7, op3_coarse: 0.55,
        op4_level: 0.0, op5_level: 0.0, op6_level: 0.0,
      },
    },
  },
];

// ---------------------------------------------------------------------------
// Modifier axes. Each axis is a pair of opposing phrases; the user prompt's
// cosine similarity to (positive - negative) yields a signed strength in
// roughly [-1, 1], which nudges specific normalized params by `deltas` scaled by
// that strength. Deltas are per-synth; a param not present for a synth is
// skipped. Applied after the category blend, before the final 0..1 clamp.
// ---------------------------------------------------------------------------
export const MODIFIER_AXES = [
  {
    id: "brightness",
    positive: "bright airy crisp open sparkling treble sound",
    negative: "dark dull muffled muddy closed warm sound",
    posKeywords: ["bright", "airy", "crisp", "sparkle", "sparkling", "open", "shiny", "sharp", "treble"],
    negKeywords: ["dark", "dull", "muffled", "muddy", "warm", "mellow", "soft", "closed", "murky"],
    deltas: {
      surge: { filter1_cutoff: 0.28, filter2_cutoff: 0.2 },
      vital: { filter_1_cutoff: 0.28, filter_2_cutoff: 0.2 },
      dexed: { op2_level: 0.18, feedback: 0.12 },
    },
  },
  {
    id: "aggression",
    positive: "aggressive harsh distorted gritty dirty screaming sound",
    negative: "soft gentle smooth clean mellow rounded sound",
    posKeywords: ["aggressive", "harsh", "distorted", "gritty", "dirty", "screaming", "nasty", "brutal", "grinding"],
    negKeywords: ["soft", "gentle", "smooth", "clean", "mellow", "rounded", "delicate", "tender", "silky"],
    deltas: {
      surge: { filter1_resonance: 0.22, filter_feedback: 0.15, drift: 0.1 },
      vital: { filter_1_resonance: 0.2, filter_1_drive: 0.25, distortion_drive: 0.2, distortion_mix: 0.2 },
      dexed: { feedback: 0.2, op2_level: 0.12 },
    },
  },
  {
    id: "motion",
    positive: "moving evolving animated shifting modulated pulsing sound",
    negative: "static still steady flat constant unchanging sound",
    posKeywords: ["moving", "evolving", "animated", "shifting", "modulated", "pulsing", "wobbling", "morphing", "swirling"],
    negKeywords: ["static", "still", "steady", "flat", "constant", "unchanging", "fixed", "solid"],
    deltas: {
      surge: { mod_lfo1_cutoff: 0.3, lfo1_rate: 0.1 },
      vital: { mod_1_amount: 0.3, lfo_1_frequency: 0.1 },
      dexed: { lfo_pmd: 0.25, lfo_speed: 0.1 },
    },
  },
  {
    id: "width",
    positive: "wide thick fat huge lush detuned unison stereo sound",
    negative: "thin narrow small focused single mono tight sound",
    posKeywords: ["wide", "thick", "fat", "huge", "lush", "detuned", "unison", "massive", "stereo"],
    negKeywords: ["thin", "narrow", "small", "focused", "single", "mono", "tight", "pure", "simple"],
    deltas: {
      surge: { unison_voices: 0.3, unison_detune: 0.2 },
      vital: { osc_1_unison_voices: 0.3, osc_1_unison_detune: 0.25, osc_2_level: 0.15 },
      dexed: { op2_detune: 0.15, op3_level: 0.15 },
    },
  },
  {
    id: "envelope_length",
    positive: "slow soft swelling sustained long pad-like attack sound",
    negative: "fast sharp snappy plucky short percussive stab sound",
    posKeywords: ["slow", "swelling", "sustained", "long", "pad", "smooth attack", "gradual", "washy", "lingering"],
    negKeywords: ["fast", "sharp", "snappy", "plucky", "short", "percussive", "stab", "tight", "punchy"],
    deltas: {
      surge: { amp_attack: 0.4, amp_release: 0.3, filter_attack: 0.3 },
      vital: { env_1_attack: 0.4, env_1_release: 0.3, env_2_attack: 0.3 },
      dexed: { op1_eg_r1: -0.25 },
    },
  },
];
