// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Parameter-spec framework — a faithful Rust port of the legacy Python
// `vst_mappers/base.py` (ParameterSpec + BaseMapper scaling functions).
//
// A `ParamSpec` describes one named synth parameter: how a normalized 0..1
// value maps onto the parameter's real range (linear / exponential /
// logarithmic / discrete / enum / boolean), what its default is, and — for
// enums — the ordered list of option strings.
//
// The whole point of this framework is that the *value mapping* is
// independent of the text prompt. The Claude client asks the model for a
// normalized value per parameter (0..1, or an enum string); the mapper turns
// that into the real parameter value; the writer turns real values into a
// preset file. Nothing here knows anything about prompts.

/// How a normalized `0.0..=1.0` value maps onto the real parameter range.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamType {
    /// `min + v * (max - min)`.
    Linear,
    /// Exponential curve, good for frequency-like ranges. Ported exactly from
    /// the Python `_exponential_map` (double-exp with a `curve` shaping term).
    Exponential,
    /// `10^(log10(min) + v * (log10(max) - log10(min)))` — decibel-like.
    Logarithmic,
    /// Stepped integer mapping (snapped to `snap_to`).
    Discrete,
    /// Categorical — `value` selects one of `options` by index.
    Enum,
    /// `v > 0.5`.
    Boolean,
}

/// A single parameter's specification (name, scaling, range, default, options).
#[derive(Debug, Clone)]
pub struct ParamSpec {
    pub name: &'static str,
    pub ty: ParamType,
    pub min_val: f64,
    pub max_val: f64,
    pub default: f64,
    /// Ordered enum option strings (empty for non-enum params).
    pub options: &'static [&'static str],
    /// Snap step for discrete/snapped params (`None` = no snapping).
    pub snap_to: Option<f64>,
    /// Exponential shaping term (only used by `ParamType::Exponential`).
    pub curve: f64,
    /// Optional human/LLM-facing note appended to the generated JSON-schema
    /// description (e.g. what the value physically maps to in the target synth).
    /// `None` = no extra note.
    pub note: Option<&'static str>,
}

impl ParamSpec {
    /// Linear param.
    pub const fn linear(name: &'static str, min_val: f64, max_val: f64, default: f64) -> Self {
        Self {
            name,
            ty: ParamType::Linear,
            min_val,
            max_val,
            default,
            options: &[],
            snap_to: None,
            curve: 1.0,
            note: None,
        }
    }

    /// Linear param snapped to an integer/step grid.
    pub const fn linear_snap(
        name: &'static str,
        min_val: f64,
        max_val: f64,
        default: f64,
        snap_to: f64,
    ) -> Self {
        Self {
            name,
            ty: ParamType::Linear,
            min_val,
            max_val,
            default,
            options: &[],
            snap_to: Some(snap_to),
            curve: 1.0,
            note: None,
        }
    }

    /// Exponential param (frequency-like).
    pub const fn exponential(
        name: &'static str,
        min_val: f64,
        max_val: f64,
        default: f64,
        curve: f64,
    ) -> Self {
        Self {
            name,
            ty: ParamType::Exponential,
            min_val,
            max_val,
            default,
            options: &[],
            snap_to: None,
            curve,
            note: None,
        }
    }

    /// Logarithmic param (decibel-like).
    pub const fn logarithmic(name: &'static str, min_val: f64, max_val: f64, default: f64) -> Self {
        Self {
            name,
            ty: ParamType::Logarithmic,
            min_val,
            max_val,
            default,
            options: &[],
            snap_to: None,
            curve: 1.0,
            note: None,
        }
    }

    /// Discrete/stepped integer param.
    pub const fn discrete(
        name: &'static str,
        min_val: f64,
        max_val: f64,
        default: f64,
        snap_to: f64,
    ) -> Self {
        Self {
            name,
            ty: ParamType::Discrete,
            min_val,
            max_val,
            default,
            options: &[],
            snap_to: Some(snap_to),
            curve: 1.0,
            note: None,
        }
    }

    /// Enum/categorical param.
    pub const fn enum_(name: &'static str, options: &'static [&'static str]) -> Self {
        Self {
            name,
            ty: ParamType::Enum,
            min_val: 0.0,
            max_val: 1.0,
            default: 0.0,
            options,
            snap_to: None,
            curve: 1.0,
            note: None,
        }
    }

    /// Boolean param.
    pub const fn boolean(name: &'static str, default: f64) -> Self {
        Self {
            name,
            ty: ParamType::Boolean,
            min_val: 0.0,
            max_val: 1.0,
            default,
            options: &[],
            snap_to: None,
            curve: 1.0,
            note: None,
        }
    }

    /// Attach a description note (appended to the generated JSON schema). Meant
    /// to be chained after a constructor in a `const` param table.
    pub const fn with_note(mut self, note: &'static str) -> Self {
        self.note = Some(note);
        self
    }

    /// The mapped default value (as a `ParamValue`).
    pub fn default_value(&self) -> ParamValue {
        match self.ty {
            ParamType::Enum => {
                ParamValue::Enum(self.options.first().copied().unwrap_or("").to_string())
            }
            ParamType::Boolean => ParamValue::Bool(self.default > 0.5),
            _ => ParamValue::Num(self.default),
        }
    }
}

/// A mapped parameter value: a number, an enum-string, or a boolean.
///
/// This mirrors the Python mapper output (`Union[float, int, str, bool]`) — the
/// writers pattern-match on it to serialize the right byte/XML/JSON form.
#[derive(Debug, Clone, PartialEq)]
pub enum ParamValue {
    Num(f64),
    Enum(String),
    Bool(bool),
}

impl ParamValue {
    /// Numeric view (`Bool` → 0/1, `Enum` → 0). Used by generic writers.
    pub fn as_f64(&self) -> f64 {
        match self {
            ParamValue::Num(n) => *n,
            ParamValue::Bool(b) => {
                if *b {
                    1.0
                } else {
                    0.0
                }
            }
            ParamValue::Enum(_) => 0.0,
        }
    }
}

/// Clamp to `[lo, hi]`.
#[inline]
pub fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

/// Map a normalized `0..1` value through a `ParamSpec` into a real value.
///
/// This is a faithful port of `BaseMapper._map_parameter` in the legacy Python.
pub fn map_normalized(value: f64, spec: &ParamSpec) -> ParamValue {
    let value = clamp(value, 0.0, 1.0);

    match spec.ty {
        ParamType::Linear => {
            let mut m = linear_map(value, spec.min_val, spec.max_val);
            if let Some(step) = spec.snap_to {
                m = (m / step).round() * step;
            }
            ParamValue::Num(m)
        }
        ParamType::Exponential => {
            let mut m = exponential_map(value, spec.min_val, spec.max_val, spec.curve);
            if let Some(step) = spec.snap_to {
                m = (m / step).round() * step;
            }
            ParamValue::Num(m)
        }
        ParamType::Logarithmic => {
            let mut m = logarithmic_map(value, spec.min_val, spec.max_val);
            if let Some(step) = spec.snap_to {
                m = (m / step).round() * step;
            }
            ParamValue::Num(m)
        }
        ParamType::Discrete => {
            let m = discrete_map(value, spec.min_val, spec.max_val, spec.snap_to);
            ParamValue::Num(m as f64)
        }
        ParamType::Enum => ParamValue::Enum(enum_map(value, spec.options).to_string()),
        ParamType::Boolean => ParamValue::Bool(value > 0.5),
    }
}

/// `min + v * (max - min)`.
#[inline]
pub fn linear_map(value: f64, min_val: f64, max_val: f64) -> f64 {
    min_val + value * (max_val - min_val)
}

/// Exponential mapping (faithful port of Python `_exponential_map`).
///
/// `exp_value = (e^(v*curve) - 1) / (e^curve - 1)`
/// `result    = min * e^(exp_value * ln(max/min))`
pub fn exponential_map(value: f64, min_val: f64, max_val: f64, curve: f64) -> f64 {
    // Python: min_val * exp(exp_value * log(max/min)); undefined when min <= 0.
    // The legacy Surge/Vital tables always keep exponential min > 0, but guard
    // against a zero min the same way the log map does, to avoid NaN.
    let min_val = if min_val <= 0.0 { 0.001 } else { min_val };
    let exp_value = (f64::exp(value * curve) - 1.0) / (f64::exp(curve) - 1.0);
    min_val * f64::exp(exp_value * f64::ln(max_val / min_val))
}

/// Logarithmic mapping (faithful port of Python `_logarithmic_map`).
pub fn logarithmic_map(value: f64, min_val: f64, max_val: f64) -> f64 {
    let min_val = if min_val <= 0.0 { 0.001 } else { min_val };
    let log_min = min_val.log10();
    let log_max = max_val.log10();
    let log_value = log_min + value * (log_max - log_min);
    10f64.powf(log_value)
}

/// Discrete/stepped mapping (faithful port of Python `_discrete_map`).
pub fn discrete_map(value: f64, min_val: f64, max_val: f64, step: Option<f64>) -> i64 {
    let continuous = min_val + value * (max_val - min_val);
    match step {
        Some(s) if s != 0.0 => ((continuous / s).round() * s) as i64,
        _ => continuous.round() as i64,
    }
}

/// Map to an enumerated option string (faithful port of Python `_enum_map`).
pub fn enum_map(value: f64, options: &[&'static str]) -> &'static str {
    if options.is_empty() {
        return "";
    }
    let mut index = (value * options.len() as f64) as usize;
    if index >= options.len() {
        index = options.len() - 1;
    }
    options[index]
}

#[cfg(test)]
mod tests {
    use super::*;

    // Values verified against the Python `base.py` scaling functions.

    #[test]
    fn linear_endpoints_and_mid() {
        assert_eq!(linear_map(0.0, -48.0, 48.0), -48.0);
        assert_eq!(linear_map(1.0, -48.0, 48.0), 48.0);
        assert_eq!(linear_map(0.5, -48.0, 48.0), 0.0);
    }

    #[test]
    fn exponential_matches_python_curve() {
        // exp_value = (e^(v*curve)-1)/(e^curve-1); result = min*e^(exp_value*ln(max/min))
        // At v=0 → min; at v=1 → max.
        let lo = exponential_map(0.0, 20.0, 20000.0, 3.0);
        let hi = exponential_map(1.0, 20.0, 20000.0, 3.0);
        assert!((lo - 20.0).abs() < 1e-6, "lo={lo}");
        assert!((hi - 20000.0).abs() < 1e-3, "hi={hi}");
        // Spot-check the mid: recomputed by hand from the Python formula.
        let mid = exponential_map(0.5, 20.0, 20000.0, 3.0);
        // exp_value(0.5) = (e^1.5 - 1)/(e^3 - 1) = 3.4816.../19.0855... = 0.182422
        // result = 20 * e^(0.182422 * ln(1000)) = 20 * e^(0.182422*6.907755) = 20 * e^1.26014
        //        = 20 * 3.52565 = 70.513
        assert!((mid - 70.513).abs() < 0.1, "mid={mid}");
    }

    #[test]
    fn logarithmic_endpoints() {
        let lo = logarithmic_map(0.0, 0.1, 2.0);
        let hi = logarithmic_map(1.0, 0.1, 2.0);
        assert!((lo - 0.1).abs() < 1e-9, "lo={lo}");
        assert!((hi - 2.0).abs() < 1e-9, "hi={hi}");
    }

    #[test]
    fn discrete_snaps_to_integers() {
        // range 1..64, step 1, v=0 -> 1, v=1 -> 64, v~0.5 -> 32 or 33
        assert_eq!(discrete_map(0.0, 1.0, 64.0, Some(1.0)), 1);
        assert_eq!(discrete_map(1.0, 1.0, 64.0, Some(1.0)), 64);
    }

    #[test]
    fn enum_index_selection() {
        let opts = &["a", "b", "c", "d"];
        assert_eq!(enum_map(0.0, opts), "a");
        assert_eq!(enum_map(0.99, opts), "d");
        // Python: index = int(0.5 * 4) = 2 -> "c"
        assert_eq!(enum_map(0.5, opts), "c");
        // Clamp at top.
        assert_eq!(enum_map(1.0, opts), "d");
    }

    #[test]
    fn map_normalized_clamps_input() {
        let spec = ParamSpec::linear("x", 0.0, 10.0, 5.0);
        assert_eq!(map_normalized(-1.0, &spec), ParamValue::Num(0.0));
        assert_eq!(map_normalized(2.0, &spec), ParamValue::Num(10.0));
    }

    #[test]
    fn boolean_threshold() {
        let spec = ParamSpec::boolean("b", 0.0);
        assert_eq!(map_normalized(0.4, &spec), ParamValue::Bool(false));
        assert_eq!(map_normalized(0.6, &spec), ParamValue::Bool(true));
    }
}
