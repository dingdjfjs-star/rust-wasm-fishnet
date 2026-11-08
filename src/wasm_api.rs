//! wasm-bindgen 绑定层：把核心库暴露给 JavaScript。
//!
//! 仅在 `wasm32` 目标下编译（见 `lib.rs` 中的 `#[cfg(target_arch = "wasm32")]`），
//! 原生 `cargo test` 不受影响。核心校验与空间计算全部在 Rust 侧完成，
//! JS 只负责传参与展示——前端零重计算。

use wasm_bindgen::prelude::*;

use crate::{fishnet_geojson, MAX_CELLS};

/// JS 调用入口：`generateFishnet(xmin, ymin, xmax, ymax, cellW, cellH)`
///
/// - 成功：返回紧凑 GeoJSON 字符串（FeatureCollection，Polygon 面要素，属性含 row/col）；
/// - 失败：抛出 JS `Error`，`message` 为中文原因（四至颠倒 / 非数值 / 超上限等），
///   前端 `catch` 后可直接展示。
#[wasm_bindgen(js_name = generateFishnet)]
pub fn generate_fishnet(
    xmin: f64,
    ymin: f64,
    xmax: f64,
    ymax: f64,
    cell_w: f64,
    cell_h: f64,
) -> Result<String, JsError> {
    fishnet_geojson([xmin, ymin, xmax, ymax], cell_w, cell_h)
        .map_err(|e| JsError::new(&e.to_string()))
}

/// 返回单次生成的格网数上限（前端可用来提前拦截极端参数并提示）。
#[wasm_bindgen(js_name = maxCells)]
pub fn max_cells() -> usize {
    MAX_CELLS
}
