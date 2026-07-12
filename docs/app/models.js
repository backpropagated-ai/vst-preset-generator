// SPDX-License-Identifier: GPL-3.0-only
// Copyright (c) 2026 DeepSynth AI
//
// On-device model registry for the browser generator. All models stream from
// the Hugging Face CDN at runtime (none are bundled); licenses are recorded in
// LICENSES.md. Sizes are the quantized ONNX weights actually downloaded.
//
// Two kinds:
//   * "embedding" — feature-extraction models driving the anchor-blend path.
//     `prefix`, when set, is prepended to BOTH prompts and anchor phrases
//     (symmetric text-similarity usage, per the e5 model card).
//   * "llm" — a small instruct model (WebGPU only) that fills the parameter
//     JSON directly from the schema instead of blending anchors.

export const MODEL_REGISTRY = [
  {
    id: "minilm",
    kind: "embedding",
    repo: "Xenova/all-MiniLM-L6-v2",
    label: "MiniLM-L6-v2 · 23 MB — fast (default)",
    shortName: "all-MiniLM-L6-v2",
    sizeMB: 23,
    prefix: null,
    dtype: "q8",
  },
  {
    id: "bge",
    kind: "embedding",
    repo: "Xenova/bge-small-en-v1.5",
    label: "bge-small-en-v1.5 · 32 MB — better English matching",
    shortName: "bge-small-en-v1.5",
    sizeMB: 32,
    prefix: null,
    dtype: "q8",
  },
  {
    id: "e5",
    kind: "embedding",
    repo: "Xenova/multilingual-e5-small",
    label: "multilingual-e5-small · 113 MB — multilingual prompts (한국어, 日本語, …)",
    shortName: "multilingual-e5-small",
    sizeMB: 113,
    // e5 models are trained with instruction prefixes; for symmetric
    // short-text similarity the model card prescribes "query: " on both sides.
    prefix: "query: ",
    dtype: "q8",
  },
  {
    id: "smollm",
    kind: "llm",
    repo: "HuggingFaceTB/SmolLM2-360M-Instruct",
    label: "SmolLM2-360M-Instruct · 260 MB — smallest LLM, may fall back (WebGPU)",
    shortName: "SmolLM2-360M-Instruct",
    sizeMB: 260,
    dtype: "q4f16",
    device: "webgpu",
    // Tiny model loops key suffixes under greedy decoding without this.
    repetitionPenalty: 1.3,
  },
  {
    id: "gemma1b",
    kind: "llm",
    repo: "onnx-community/gemma-3-1b-it-ONNX",
    label: "Gemma-3-1B-it · 0.8 GB — best direct generation (WebGPU)",
    shortName: "Gemma-3-1B-it",
    sizeMB: 780,
    dtype: "q4f16",
    device: "webgpu",
    repetitionPenalty: 1.1,
  },
];

export const DEFAULT_MODEL_ID = "minilm";

export function modelById(id) {
  return MODEL_REGISTRY.find((m) => m.id === id) || MODEL_REGISTRY[0];
}

export function webgpuAvailable() {
  return typeof navigator !== "undefined" && !!navigator.gpu;
}
