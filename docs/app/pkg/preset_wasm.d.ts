/* tslint:disable */
/* eslint-disable */

/**
 * Build a real preset file from *normalized* inputs — the browser equivalent of
 * the CLI's `--params-json` offline path.
 *
 * `normalized_params_json` is a flat JSON object of `{ "param_name": value }`
 * where a value is either a number in `0..1` (clamped + scaled by the mapper)
 * or a string (an enum option name, or a numeric string). This is passed
 * straight through `Mapper::map_inputs` — the *exact same* mapping the native
 * app runs — and then the synth's writer, so the returned bytes are a fully
 * loadable `.fxp` / `.syx` / `.vital`. Missing parameters fall back to the
 * synth's defaults, and unknown parameter names are ignored (identical native
 * semantics).
 */
export function build_preset(synth: string, normalized_params_json: string, preset_name: string): Uint8Array;

/**
 * The canonical output file extension for a synth (no dot).
 */
export function preset_file_extension(synth: string): string;

/**
 * The generated JSON schema (as a pretty string) for a synth's parameters.
 *
 * Identical to `deepsynth-preset --schema --synth <synth>`. The browser does
 * not strictly need this to build a preset, but it lets the page introspect
 * which parameters exist / their ranges.
 */
export function schema_json(synth: string): string;

/**
 * The list of supported synths, as a JSON array of `{id, name, extension}`.
 *
 * The browser uses this to populate the synth selector.
 */
export function synths(): string;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly build_preset: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly preset_file_extension: (a: number, b: number) => [number, number, number, number];
    readonly schema_json: (a: number, b: number) => [number, number, number, number];
    readonly synths: () => [number, number];
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
