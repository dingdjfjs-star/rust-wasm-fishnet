//! # GeoJSON 渔网生成器（Rust 核心算法）
//!
//! 纯逻辑库：输入四至范围与格网单元尺寸，生成矩形渔网，输出标准 GeoJSON
//! （FeatureCollection，每个格网一个 Polygon 面要素）。
//! WASM 绑定见 [`wasm_api`] 模块（仅在 wasm32 目标下编译）。
//!
//! ## 渔网口径（README 同步说明）
//! - 行列数 = `ceil((xmax - xmin) / 单元宽)` × `ceil((ymax - ymin) / 单元高)`，
//!   向上取整保证渔网**盖满**整个四至；
//! - 最后一列 / 最后一行不足一格时**截断到四至边界**（而不是超出 bbox 外），
//!   因此渔网外边界恰好等于输入的四至矩形；
//! - 要素按**先行后列**排序，行号自下而上（`row=0` 为最南一行），符合地图学 y 轴向上惯例；
//! - 每个要素的属性只含行列号：`{"row": r, "col": c}`。

use geo::{LineString, Polygon};
use geojson::{Feature, FeatureCollection, Geometry};
use serde_json::{Map, Value as JsonValue};

/// wasm-bindgen 绑定层（仅 wasm32 目标编译，见 `wasm_api.rs`）
#[cfg(target_arch = "wasm32")]
pub mod wasm_api;

/// 浏览器端单次生成的格网数上限：防止超大网格撑爆内存 / 卡死页面。
/// 20_000 个矩形对现代浏览器仍是轻松范围，超过则大概率是参数手误。
pub const MAX_CELLS: usize = 20_000;

/// 渔网生成 / 参数校验错误。中文错误信息会一路透传到前端提示。
#[derive(Debug, Clone, PartialEq)]
pub enum FishnetError {
    /// 四至非法：含 NaN / 无穷大，或范围颠倒、退化（xmin >= xmax 等）
    InvalidExtent(String),
    /// 格网尺寸非法：含 NaN / 无穷大，或 <= 0
    InvalidCellSize(String),
    /// 格网总数超过 [`MAX_CELLS`] 上限
    TooManyCells { count: usize, limit: usize },
}

impl std::fmt::Display for FishnetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidExtent(msg) => write!(
                f,
                "四至范围非法：{msg}。要求四个值均为有限数值，且 xmin < xmax、ymin < ymax（坐标本身允许负值）"
            ),
            Self::InvalidCellSize(msg) => {
                write!(f, "格网尺寸非法：{msg}。要求宽/高均为大于 0 的有限数值")
            }
            Self::TooManyCells { count, limit } => {
                write!(f, "格网总数 {count} 超过上限 {limit}，请调大格网宽/高后重试")
            }
        }
    }
}

impl std::error::Error for FishnetError {}

/// 校验四至 `[xmin, ymin, xmax, ymax]`：
/// - 四个值都必须是有限数（拒绝 NaN / 无穷大）；
/// - 必须 `xmin < xmax`、`ymin < ymax`（拒绝范围颠倒与零面积）；
/// - **坐标允许负值**：经纬度、投影坐标出现负数完全合法（西经、南半球等），
///   需要拦截的是"非数值"和"范围颠倒"，而不是负坐标本身。
pub fn validate_extent(extent: [f64; 4]) -> Result<(), FishnetError> {
    let [xmin, ymin, xmax, ymax] = extent;
    if !(xmin.is_finite() && ymin.is_finite() && xmax.is_finite() && ymax.is_finite()) {
        return Err(FishnetError::InvalidExtent(
            "含 NaN 或无穷大".to_string(),
        ));
    }
    if xmin >= xmax || ymin >= ymax {
        return Err(FishnetError::InvalidExtent(format!(
            "范围颠倒或退化（xmin={xmin}, ymin={ymin}, xmax={xmax}, ymax={ymax}）"
        )));
    }
    Ok(())
}

/// 校验格网单元宽/高：必须均为 > 0 的有限数（拒绝 0、负数、NaN、无穷大）。
pub fn validate_cell_size(cell_w: f64, cell_h: f64) -> Result<(), FishnetError> {
    if !(cell_w.is_finite() && cell_h.is_finite()) {
        return Err(FishnetError::InvalidCellSize(
            "含 NaN 或无穷大".to_string(),
        ));
    }
    if cell_w <= 0.0 || cell_h <= 0.0 {
        return Err(FishnetError::InvalidCellSize(format!(
            "宽/高必须大于 0（得到 宽={cell_w}, 高={cell_h}）"
        )));
    }
    Ok(())
}

/// 生成渔网（结构化 [`FeatureCollection`]，便于测试与复用）。
pub fn fishnet_fc(
    extent: [f64; 4],
    cell_w: f64,
    cell_h: f64,
) -> Result<FeatureCollection, FishnetError> {
    validate_extent(extent)?;
    validate_cell_size(cell_w, cell_h)?;

    let [xmin, ymin, xmax, ymax] = extent;
    let dx = xmax - xmin;
    let dy = ymax - ymin;

    // 行列数向上取整，保证盖满四至。
    // 浮点除法可能"向上取整过头"（如 0.7/0.1 得到 7.000000000000001），
    // 若最后一格的起点已经越过 xmax/ymax，说明多算了一列/行，回退一格。
    let mut ncols = (dx / cell_w).ceil() as usize;
    let mut nrows = (dy / cell_h).ceil() as usize;
    while ncols > 1 && xmin + (ncols as f64 - 1.0) * cell_w >= xmax {
        ncols -= 1;
    }
    while nrows > 1 && ymin + (nrows as f64 - 1.0) * cell_h >= ymax {
        nrows -= 1;
    }

    let count = ncols * nrows;
    if count > MAX_CELLS {
        return Err(FishnetError::TooManyCells {
            count,
            limit: MAX_CELLS,
        });
    }

    // 先行后列（row 外层、col 内层），row=0 为最南一行
    let mut features = Vec::with_capacity(count);
    for row in 0..nrows {
        let y0 = ymin + row as f64 * cell_h;
        // 末行截断到 ymax，其余行高度恰为 cell_h
        let y1 = if row == nrows - 1 { ymax } else { y0 + cell_h };
        for col in 0..ncols {
            let x0 = xmin + col as f64 * cell_w;
            // 末列截断到 xmax，其余列宽度恰为 cell_w
            let x1 = if col == ncols - 1 { xmax } else { x0 + cell_w };

            let mut props = Map::new();
            props.insert("row".to_string(), JsonValue::from(row));
            props.insert("col".to_string(), JsonValue::from(col));

            features.push(Feature {
                geometry: Some(Geometry::from(&cell_polygon(x0, y0, x1, y1))),
                properties: Some(props),
                ..Default::default()
            });
        }
    }

    Ok(FeatureCollection {
        features,
        bbox: None,
        foreign_members: None,
    })
}

/// 一步到位：参数校验 + 生成 + 序列化为紧凑 GeoJSON 字符串。
/// 这是 WASM 暴露给 JS 的核心入口（见 [`wasm_api`]）。
pub fn fishnet_geojson(
    extent: [f64; 4],
    cell_w: f64,
    cell_h: f64,
) -> Result<String, FishnetError> {
    let fc = fishnet_fc(extent, cell_w, cell_h)?;
    // geojson 结构体仅含字符串/数值/数组，序列化不会失败；万一失败属于库 bug，直接 panic 暴露
    Ok(serde_json::to_string(&fc).expect("GeoJSON 序列化不可能失败"))
}

/// 构造单个格网矩形：逆时针外环（符合 GeoJSON 右手规则），首尾点闭合，无内环。
fn cell_polygon(x0: f64, y0: f64, x1: f64, y1: f64) -> Polygon<f64> {
    Polygon::new(
        LineString::from(vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]),
        vec![], // 无内环（洞）
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use geo::Area; // 用 geo 的面积算法验证环方向（体现 geo crate 的实际用途）
    use geojson::Value as GeoJsonValue;

    /// 取要素第一个外环坐标
    fn outer_ring(f: &Feature) -> &[Vec<f64>] {
        match &f.geometry.as_ref().unwrap().value {
            GeoJsonValue::Polygon(rings) => rings[0].as_slice(),
            other => panic!("应为 Polygon，实际 {other:?}"),
        }
    }

    /// 由环坐标求 bbox (minx, miny, maxx, maxy)
    fn ring_bbox(ring: &[Vec<f64>]) -> [f64; 4] {
        // min 类坐标从 +MAX 向下收紧，max 类坐标从 -MAX（f64::MIN）向上收紧
        let mut bbox = [f64::MAX, f64::MAX, f64::MIN, f64::MIN];
        for p in ring {
            bbox[0] = bbox[0].min(p[0]);
            bbox[1] = bbox[1].min(p[1]);
            bbox[2] = bbox[2].max(p[0]);
            bbox[3] = bbox[3].max(p[1]);
        }
        bbox
    }

    #[test]
    fn 整除时每格恰好等于单元尺寸() {
        let fc = fishnet_fc([0.0, 0.0, 10.0, 10.0], 2.0, 2.0).unwrap();
        assert_eq!(fc.features.len(), 25); // 5 列 × 5 行

        // 西南角第一格：row=0, col=0，坐标 (0,0)-(2,2)
        let first = &fc.features[0];
        let props = first.properties.as_ref().unwrap();
        assert_eq!(props["row"], serde_json::json!(0));
        assert_eq!(props["col"], serde_json::json!(0));
        assert_eq!(ring_bbox(outer_ring(first)), [0.0, 0.0, 2.0, 2.0]);

        // 东北角最后一格：(8,8)-(10,10)
        let last = fc.features.last().unwrap();
        let props = last.properties.as_ref().unwrap();
        assert_eq!(props["row"], serde_json::json!(4));
        assert_eq!(props["col"], serde_json::json!(4));
        assert_eq!(ring_bbox(outer_ring(last)), [8.0, 8.0, 10.0, 10.0]);
    }

    #[test]
    fn 不足一格时末行末列截断到边界() {
        // 10/3 = 3.33 → 4 列 × 4 行，末列宽 1、末行高 1
        let fc = fishnet_fc([0.0, 0.0, 10.0, 10.0], 3.0, 3.0).unwrap();
        assert_eq!(fc.features.len(), 16);

        // 内部格宽高恰为 3
        assert_eq!(ring_bbox(outer_ring(&fc.features[0])), [0.0, 0.0, 3.0, 3.0]);
        // 末格截断到 (10,10)
        let last = fc.features.last().unwrap();
        assert_eq!(ring_bbox(outer_ring(last)), [9.0, 9.0, 10.0, 10.0]);

        // 所有格都必须落在四至之内，且面积为正
        for f in &fc.features {
            let b = ring_bbox(outer_ring(f));
            assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= 10.0 && b[3] <= 10.0);
            assert!(b[2] > b[0] && b[3] > b[1], "不允许退化格：{b:?}");
        }
    }

    #[test]
    fn 四至允许负坐标() {
        // 西经/南纬为负是合法地理坐标，不应被拒
        let fc = fishnet_fc([-10.0, -10.0, 0.0, 0.0], 2.0, 2.0).unwrap();
        assert_eq!(fc.features.len(), 25);
        assert_eq!(ring_bbox(outer_ring(&fc.features[0])), [-10.0, -10.0, -8.0, -8.0]);
    }

    #[test]
    fn 经纬度示例_113到114_28到29_0点25度格网() {
        // README 中的浏览器示例输入；0.25 是 2 的负幂，除法精确
        let fc = fishnet_fc([113.0, 28.0, 114.0, 29.0], 0.25, 0.25).unwrap();
        assert_eq!(fc.features.len(), 16); // 4 列 × 4 行
        // 整体外包恰好等于四至
        let last = fc.features.last().unwrap();
        assert_eq!(ring_bbox(outer_ring(last)), [113.75, 28.75, 114.0, 29.0]);
    }

    #[test]
    fn 范围颠倒或退化被拒绝() {
        assert!(matches!(
            fishnet_fc([10.0, 10.0, 0.0, 0.0], 1.0, 1.0),
            Err(FishnetError::InvalidExtent(_))
        ));
        assert!(matches!(
            fishnet_fc([5.0, 0.0, 5.0, 10.0], 1.0, 1.0),
            Err(FishnetError::InvalidExtent(_))
        ));
        assert!(matches!(
            fishnet_fc([0.0, 20.0, 10.0, 10.0], 1.0, 1.0),
            Err(FishnetError::InvalidExtent(_))
        ));
    }

    #[test]
    fn 非有限数值被拒绝() {
        let nan = f64::NAN;
        let inf = f64::INFINITY;
        for bad in [[nan, 0.0, 1.0, 1.0], [0.0, 0.0, inf, 1.0], [0.0, nan, 1.0, 1.0]] {
            assert!(matches!(
                fishnet_fc(bad, 1.0, 1.0),
                Err(FishnetError::InvalidExtent(_))
            ));
        }
        assert!(matches!(
            fishnet_fc([0.0, 0.0, 1.0, 1.0], nan, 1.0),
            Err(FishnetError::InvalidCellSize(_))
        ));
        assert!(matches!(
            fishnet_fc([0.0, 0.0, 1.0, 1.0], 0.5, -1.0),
            Err(FishnetError::InvalidCellSize(_))
        ));
    }

    #[test]
    fn 格网尺寸为零或负数被拒绝() {
        assert!(matches!(
            fishnet_fc([0.0, 0.0, 1.0, 1.0], 0.0, 1.0),
            Err(FishnetError::InvalidCellSize(_))
        ));
        assert!(matches!(
            fishnet_fc([0.0, 0.0, 1.0, 1.0], 0.0, 0.0),
            Err(FishnetError::InvalidCellSize(_))
        ));
    }

    #[test]
    fn 恰好两千格允许_超过上限拒绝() {
        // 200 列 × 100 行 = 20000 格，恰好等于上限，应放行
        let fc = fishnet_fc([0.0, 0.0, 200.0, 100.0], 1.0, 1.0).unwrap();
        assert_eq!(fc.features.len(), MAX_CELLS);

        // 20100 格 → 超上限
        let err = fishnet_fc([0.0, 0.0, 201.0, 100.0], 1.0, 1.0).unwrap_err();
        assert!(matches!(err, FishnetError::TooManyCells { count: 20100, limit: MAX_CELLS }));

        // 中文错误信息可读
        assert!(err.to_string().contains("超过上限"));
    }

    #[test]
    fn 浮点陷阱_末列不会越过四至() {
        // 0.7/0.1 的浮点结果在 7.0 边缘抖动，历史实现容易多算一列生成"倒置多边形"
        let fc = fishnet_fc([0.0, 0.0, 0.7, 0.3], 0.1, 0.1).unwrap();
        assert_eq!(fc.features.len(), 7 * 3); // 修正后应为 7 列 × 3 行
        let mut maxx = f64::MIN;
        for f in &fc.features {
            let b = ring_bbox(outer_ring(f));
            assert!(b[2] > b[0] && b[3] > b[1], "出现退化/倒置格：{b:?}");
            assert!(b[0] >= 0.0 && b[1] >= 0.0 && b[2] <= 0.7 + 1e-9 && b[3] <= 0.3 + 1e-9);
            maxx = maxx.max(b[2]);
        }
        // 最东边界恰好是 xmax
        assert!((maxx - 0.7).abs() < 1e-9);
    }

    #[test]
    #[allow(non_snake_case)] // 允许测试名中出现 GeoJSON 大写
    fn 输出为标准GeoJSON且要素按先行后列排序() {
        let s = fishnet_geojson([0.0, 0.0, 4.0, 2.0], 2.0, 1.0).unwrap();
        let v: serde_json::Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["type"], "FeatureCollection");
        let feats = v["features"].as_array().unwrap();
        assert_eq!(feats.len(), 4); // 2 列 × 2 行

        for f in feats {
            assert_eq!(f["type"], "Feature");
            assert_eq!(f["geometry"]["type"], "Polygon");
            let ring = f["geometry"]["coordinates"][0].as_array().unwrap();
            assert_eq!(ring.len(), 5, "矩形外环必须闭合为 5 点");
            assert_eq!(ring[0], ring[4], "首尾点必须相同");
        }
        // 先行后列：第 2 个要素是 row=0, col=1
        assert_eq!(feats[1]["properties"]["row"], 0);
        assert_eq!(feats[1]["properties"]["col"], 1);
        // 第 3 个要素进入第二行
        assert_eq!(feats[2]["properties"]["row"], 1);
        assert_eq!(feats[2]["properties"]["col"], 0);
    }

    #[test]
    fn 环方向为逆时针_符合右手规则() {
        let fc = fishnet_fc([0.0, 0.0, 2.0, 2.0], 1.0, 1.0).unwrap();
        for f in &fc.features {
            let ring = outer_ring(f);
            let pts: Vec<(f64, f64)> = ring.iter().map(|p| (p[0], p[1])).collect();
            let poly = Polygon::new(LineString::from(pts), vec![]);
            // geo::Area：逆时针外环有向面积为正
            assert!(poly.signed_area() > 0.0);
        }
    }

    #[test]
    fn 单格渔网退化为一个恰好填满四至的矩形() {
        // 格网尺寸大于四至时，至少生成 1 格并截断为整个 bbox
        let fc = fishnet_fc([1.0, 2.0, 1.5, 2.4], 10.0, 10.0).unwrap();
        assert_eq!(fc.features.len(), 1);
        assert_eq!(ring_bbox(outer_ring(&fc.features[0])), [1.0, 2.0, 1.5, 2.4]);
    }
}
