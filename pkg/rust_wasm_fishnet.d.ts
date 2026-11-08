/* tslint:disable */
/* eslint-disable */

/**
 * JS 调用入口：`generateFishnet(xmin, ymin, xmax, ymax, cellW, cellH)`
 *
 * - 成功：返回紧凑 GeoJSON 字符串（FeatureCollection，Polygon 面要素，属性含 row/col）；
 * - 失败：抛出 JS `Error`，`message` 为中文原因（四至颠倒 / 非数值 / 超上限等），
 *   前端 `catch` 后可直接展示。
 */
export function generateFishnet(xmin: number, ymin: number, xmax: number, ymax: number, cell_w: number, cell_h: number): string;

/**
 * 返回单次生成的格网数上限（前端可用来提前拦截极端参数并提示）。
 */
export function maxCells(): number;

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly generateFishnet: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number, number];
    readonly maxCells: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
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
