// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// Browser-only prompt -> normalized-params engine.
//
// Two ways to turn a prompt into a normalized parameter set, over the same
// hand-authored anchor bank (anchors.js):
//
//   * MODEL path: embed every anchor phrase once (all-MiniLM-L6-v2 via
//     transformers.js, WASM backend), embed the user prompt, take cosine
//     similarities, softmax-blend the top-3 category recipes, then nudge params
//     by the modifier axes (signed cosine projection). This is the default.
//   * KEYWORD fallback: if transformers.js / the model can't load (offline, CDN
//     blocked), score categories + axes by keyword hits over the same bank so
//     the page still produces a real preset. The UI shows which engine ran.
//
// The output is a flat object of normalized inputs handed straight to the WASM
// build_preset() — identical semantics to the native CLI's --params-json path.

import { CATEGORY_ANCHORS, MODIFIER_AXES } from "./anchors.js";

// transformers.js is loaded lazily (see loadModel) so the keyword path never
// pays for the CDN import.
const TRANSFORMERS_URL = "https://cdn.jsdelivr.net/npm/@huggingface/transformers@4.2.0";

// ---------------------------------------------------------------------------
// Vector helpers
// ---------------------------------------------------------------------------
function dot(a, b) {
  let s = 0;
  for (let i = 0; i < a.length; i++) s += a[i] * b[i];
  return s;
}
function norm(a) {
  return Math.sqrt(dot(a, a)) || 1;
}
function cosine(a, b) {
  return dot(a, b) / (norm(a) * norm(b));
}
function meanVec(vectors) {
  const out = new Array(vectors[0].length).fill(0);
  for (const v of vectors) for (let i = 0; i < v.length; i++) out[i] += v[i];
  for (let i = 0; i < out.length; i++) out[i] /= vectors.length;
  return out;
}

// ---------------------------------------------------------------------------
// Model loading + embedding (multi-model: see models.js)
// ---------------------------------------------------------------------------
let transformersMod = null; // cached CDN import
let extractor = null; // active feature-extraction pipeline
let activeEmbeddingId = null; // registry id the extractor belongs to
let activePrefix = null; // per-model text prefix (e.g. e5's "query: ")

// Anchor embeddings are model-specific — cache per registry id.
const anchorCache = new Map(); // id -> { categoryCentroids, axisDirections }
let categoryCentroids = null; // [{ id, vec }]
let axisDirections = null; // [{ id, vec }]  (positive - negative direction)

async function importTransformers() {
  if (!transformersMod) {
    transformersMod = await import(/* @vite-ignore */ TRANSFORMERS_URL);
    const { env } = transformersMod;
    // Default the embedding path to the WASM backend (broad browser support).
    if (env && env.backends && env.backends.onnx && env.backends.onnx.wasm) {
      env.backends.onnx.wasm.proxy = false;
    }
  }
  return transformersMod;
}

function progressAdapter(onProgress) {
  return (p) => {
    if (onProgress && p && typeof p.progress === "number") {
      onProgress({ status: p.status || "loading", progress: p.progress });
    }
  };
}

/**
 * Load a feature-extraction pipeline for an embedding model from the registry
 * (models.js). Throws if the CDN import or model download fails (caught by the
 * caller, which then falls back to keyword matching).
 */
export async function loadModel(modelDef, onProgress) {
  const { pipeline } = await importTransformers();
  extractor = await pipeline("feature-extraction", modelDef.repo, {
    dtype: modelDef.dtype || "q8",
    progress_callback: progressAdapter(onProgress),
  });
  activeEmbeddingId = modelDef.id;
  activePrefix = modelDef.prefix || null;
  return extractor;
}

/** Embed a single string into a mean-pooled, L2-normalized vector. */
async function embed(text) {
  const input = activePrefix ? activePrefix + text : text;
  const out = await extractor(input, { pooling: "mean", normalize: true });
  return Array.from(out.data);
}

/** Embed all anchor phrases for the ACTIVE model (cached per model). */
export async function embedAnchors() {
  const cached = anchorCache.get(activeEmbeddingId);
  if (cached) {
    ({ categoryCentroids, axisDirections } = cached);
    return;
  }
  categoryCentroids = [];
  for (const cat of CATEGORY_ANCHORS) {
    const phrases = [cat.phrase, ...(cat.altPhrases || [])];
    const vecs = [];
    for (const p of phrases) vecs.push(await embed(p));
    categoryCentroids.push({ id: cat.id, vec: meanVec(vecs) });
  }
  axisDirections = [];
  for (const axis of MODIFIER_AXES) {
    const pos = await embed(axis.positive);
    const neg = await embed(axis.negative);
    const dir = pos.map((v, i) => v - neg[i]);
    axisDirections.push({ id: axis.id, vec: dir });
  }
  anchorCache.set(activeEmbeddingId, { categoryCentroids, axisDirections });
}

// ---------------------------------------------------------------------------
// LLM path (WebGPU): a small instruct model fills the parameter JSON directly
// ---------------------------------------------------------------------------
let llmPipeline = null;
let activeLlmId = null;
let activeLlmDef = null;

/** Load the text-generation pipeline for an "llm"-kind registry model. */
export async function loadLLM(modelDef, onProgress) {
  const { pipeline } = await importTransformers();
  activeLlmDef = modelDef;
  llmPipeline = await pipeline("text-generation", modelDef.repo, {
    dtype: modelDef.dtype || "q4f16",
    device: modelDef.device || "webgpu",
    progress_callback: progressAdapter(onProgress),
  });
  activeLlmId = modelDef.id;
  return llmPipeline;
}

export function llmReadyFor(id) {
  return llmPipeline !== null && activeLlmId === id;
}

/**
 * Build a compact parameter brief from the synth's JSON schema so the prompt
 * stays small: numeric params as `name` (all normalized 0..1), enum params as
 * `name: choice1|choice2|...`.
 */
function schemaBrief(schemaJson) {
  const schema = JSON.parse(schemaJson);
  const numeric = [];
  const enums = [];
  const known = new Set();
  for (const [name, prop] of Object.entries(schema.properties || {})) {
    known.add(name);
    if (prop && Array.isArray(prop.enum)) {
      enums.push(`${name}: ${prop.enum.join("|")}`);
    } else {
      numeric.push(name);
    }
  }
  return { numeric, enums, known };
}

/**
 * Extract the first balanced {...} block from generated text. If generation was
 * cut off mid-object (tiny models + token caps), repair by trimming back to the
 * last complete `"key": value` pair and closing the brace.
 */
function extractJsonBlock(text) {
  const start = text.indexOf("{");
  if (start < 0) throw new Error("no JSON object in model output");
  let depth = 0;
  for (let i = start; i < text.length; i++) {
    const c = text[i];
    if (c === "{") depth++;
    else if (c === "}") {
      depth--;
      if (depth === 0) return text.slice(start, i + 1);
    }
  }
  // Truncated: keep everything up to the last comma that follows a complete
  // value, then close. Works for the flat one-level object we ask for.
  const body = text.slice(start);
  const lastComma = body.lastIndexOf(",");
  if (lastComma > 1) {
    const candidate = body.slice(0, lastComma) + "}";
    try {
      JSON.parse(candidate);
      return candidate;
    } catch (_) {
      /* fall through */
    }
  }
  throw new Error("unterminated JSON object in model output");
}

/**
 * Ask the loaded instruct model to fill the parameter JSON for a prompt.
 * Returns a raw params object (unknown keys and out-of-range values are
 * handled downstream by the WASM mapper: dropped / clamped / defaulted).
 * Throws on generation or parse failure — callers fall back to another path.
 */
export async function paramsFromPromptLLM(synth, prompt, schemaJson) {
  if (!llmPipeline) throw new Error("LLM not loaded");
  const { numeric, enums, known } = schemaBrief(schemaJson);
  const messages = [
    {
      role: "system",
      content:
        "You are a synthesizer sound designer. Reply with ONE JSON object only — " +
        "no prose, no markdown. Every numeric value must be normalized between 0.0 and 1.0. " +
        "Only use the parameter names given. Set ONLY the 8 to 20 parameters that matter " +
        "most for the requested sound; omit everything you would leave at its default.\n" +
        'Example (a bright pluck): {"osc1_type": "classic", "osc1_level": 0.85, ' +
        '"filter1_cutoff": 0.7, "filter1_resonance": 0.3, "amp_attack": 0.02, ' +
        '"amp_decay": 0.25, "amp_sustain": 0.1, "amp_release": 0.2}',
    },
    {
      role: "user",
      content:
        `Design a ${synth} patch for: "${prompt}".\n` +
        `Numeric parameters (0..1): ${numeric.join(", ")}.\n` +
        (enums.length ? `Choice parameters: ${enums.join("; ")}.\n` : "") +
        "JSON:",
    },
  ];
  const out = await llmPipeline(messages, {
    max_new_tokens: 1200,
    do_sample: false,
    // Repetition penalty is per-model (models.js): JSON is full of repeated
    // quote/comma tokens, so a penalty strong enough to stop a tiny model's
    // key-suffix loop makes a larger model abandon JSON syntax entirely.
    repetition_penalty: (activeLlmDef && activeLlmDef.repetitionPenalty) || 1.0,
    return_full_text: false,
  });
  let text = "";
  const gen = out && out[0] && out[0].generated_text;
  if (typeof gen === "string") {
    text = gen;
  } else if (Array.isArray(gen)) {
    const last = gen[gen.length - 1];
    text = (last && last.content) || "";
  }
  let block;
  try {
    block = extractJsonBlock(text);
  } catch (e) {
    // Surface a snippet for diagnostics (console only, never the UI).
    console.warn("LLM raw output (first 300 chars):", text.slice(0, 300));
    throw e;
  }
  const raw = JSON.parse(block);
  if (typeof raw !== "object" || raw === null || Array.isArray(raw)) {
    throw new Error("model output is not a JSON object");
  }
  // Keep ONLY schema-known parameter names (hallucinated keys would be dropped
  // by the WASM mapper anyway, but they must not reach the UI either), clamp
  // numerics, and reject degenerate outputs so the caller falls back.
  const params = {};
  let numericCount = 0;
  let nonZero = 0;
  for (const [k, v] of Object.entries(raw)) {
    if (!known.has(k)) continue;
    if (typeof v === "number" && isFinite(v)) {
      params[k] = Math.min(1, Math.max(0, v));
      numericCount += 1;
      if (params[k] > 1e-6) nonZero += 1;
    } else if (typeof v === "string") {
      params[k] = v;
    }
  }
  if (Object.keys(params).length < 4) {
    throw new Error("model output had too few valid parameters");
  }
  if (numericCount > 0 && nonZero === 0) {
    throw new Error("model output was degenerate (all numeric values zero)");
  }
  return params;
}

// ---------------------------------------------------------------------------
// Blending: category recipes + modifier axes -> normalized params
// ---------------------------------------------------------------------------

/** Softmax over an array with a temperature (lower = sharper). */
function softmax(xs, temp) {
  const t = temp || 0.15;
  const scaled = xs.map((x) => x / t);
  const m = Math.max(...scaled);
  const exps = scaled.map((x) => Math.exp(x - m));
  const sum = exps.reduce((a, b) => a + b, 0) || 1;
  return exps.map((e) => e / sum);
}

/**
 * Blend the top-3 category recipes for a synth by softmax weight, then apply
 * modifier-axis offsets, then clamp to 0..1. `catScores` is [{id, score}] over
 * all categories (cosine sims or keyword scores); `axisStrengths` is
 * {axisId: signedStrength}. Returns a flat normalized-params object.
 */
export function blendRecipes(synth, catScores, axisStrengths) {
  // Top-3 categories by score.
  const ranked = [...catScores].sort((a, b) => b.score - a.score);
  const top = ranked.slice(0, 3);
  const weights = softmax(top.map((t) => t.score));

  // Weighted blend of numeric params; enum params take the top category's value.
  const numeric = {}; // name -> weighted sum
  const enums = {}; // name -> value from the highest-weight recipe that sets it
  const enumSeen = {}; // name -> weight of the recipe that set it (to keep the top)

  top.forEach((entry, idx) => {
    const cat = CATEGORY_ANCHORS.find((c) => c.id === entry.id);
    const recipe = (cat && cat.recipes && cat.recipes[synth]) || {};
    const w = weights[idx];
    for (const [name, value] of Object.entries(recipe)) {
      if (typeof value === "number") {
        numeric[name] = (numeric[name] || 0) + w * value;
      } else {
        // string (enum) — keep the value from the highest-weighted recipe.
        if (enumSeen[name] === undefined || w > enumSeen[name]) {
          enums[name] = value;
          enumSeen[name] = w;
        }
      }
    }
  });

  // Apply modifier axes to numeric params only.
  for (const axis of MODIFIER_AXES) {
    const strength = axisStrengths[axis.id] || 0;
    if (Math.abs(strength) < 1e-3) continue;
    const deltas = axis.deltas[synth] || {};
    for (const [name, delta] of Object.entries(deltas)) {
      // Only nudge params the blend actually set, so an axis never invents a
      // control the chosen sound doesn't use.
      if (numeric[name] !== undefined) {
        numeric[name] += strength * delta;
      }
    }
  }

  // Clamp numeric to 0..1 and merge in enums.
  const params = {};
  for (const [name, v] of Object.entries(numeric)) {
    params[name] = Math.min(1, Math.max(0, v));
  }
  for (const [name, v] of Object.entries(enums)) {
    params[name] = v;
  }
  return params;
}

// ---------------------------------------------------------------------------
// MODEL path
// ---------------------------------------------------------------------------
/** Debug info from the most recent model-path run (used by tests/UI). */
export let lastModelDebug = null;

export async function paramsFromPromptModel(synth, prompt) {
  const p = await embed(prompt);
  const catScores = categoryCentroids.map((c) => ({ id: c.id, score: cosine(p, c.vec) }));
  const axisStrengths = {};
  for (const axis of axisDirections) {
    // Projection of the (unit) prompt onto the (non-unit) axis direction,
    // scaled to a usable range. clamp to [-1, 1].
    const proj = dot(p, axis.vec) / (norm(axis.vec) || 1);
    axisStrengths[axis.id] = Math.min(1, Math.max(-1, proj * 2.2));
  }
  lastModelDebug = {
    model: activeEmbeddingId,
    topCategories: [...catScores].sort((a, b) => b.score - a.score).slice(0, 3),
  };
  return blendRecipes(synth, catScores, axisStrengths);
}

// ---------------------------------------------------------------------------
// KEYWORD fallback path
// ---------------------------------------------------------------------------
function countHits(text, keywords) {
  let n = 0;
  for (const kw of keywords) {
    if (text.includes(kw)) n += 1;
  }
  return n;
}

export function paramsFromPromptKeyword(synth, prompt) {
  const text = " " + prompt.toLowerCase() + " ";
  const catScores = CATEGORY_ANCHORS.map((c) => ({
    id: c.id,
    score: countHits(text, c.keywords),
  }));
  // If nothing matched at all, gently bias toward a neutral warm pad so we still
  // emit a musical, non-default patch.
  const anyHit = catScores.some((c) => c.score > 0);
  if (!anyHit) {
    const warm = catScores.find((c) => c.id === "warm_pad");
    if (warm) warm.score = 1;
  }
  const axisStrengths = {};
  for (const axis of MODIFIER_AXES) {
    const pos = countHits(text, axis.posKeywords);
    const neg = countHits(text, axis.negKeywords);
    // Signed, saturating strength from keyword balance.
    axisStrengths[axis.id] = Math.max(-1, Math.min(1, (pos - neg) * 0.5));
  }
  return blendRecipes(synth, catScores, axisStrengths);
}
