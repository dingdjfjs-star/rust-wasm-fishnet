/**
 * 前端薄壳（app.js）
 * 职责：读取表单 → 预校验 → 调用 WASM 渔网函数 → Canvas 预览 + 展示/复制 GeoJSON。
 * 口径：参数校验的最终裁决在 Rust 侧（wasm 内），这里的预校验只是为了更快的友好反馈。
 * 空间计算（格网剖分、环构造、GeoJSON 序列化）全部在 Rust/WASM 内 —— 前端零重计算。
 */
import init, { generateFishnet, maxCells } from './pkg/rust_wasm_fishnet.js';

const $ = (id) => document.getElementById(id);
const els = {
  form: $('form'), btnGen: $('btn-gen'), btnCopy: $('btn-copy'),
  status: $('status'), out: $('output'), canvas: $('preview'),
  xmin: $('xmin'), ymin: $('ymin'), xmax: $('xmax'), ymax: $('ymax'),
  cw: $('cw'), ch: $('ch'),
};

let wasmReady = false;   // WASM 模块是否加载完成
let geojsonText = null;  // 最近一次生成的紧凑 GeoJSON 文本（一键复制的内容源）

/** 顶部状态行：msg + 着色 */
function setStatus(msg, cls = 'muted') {
  els.status.textContent = msg;
  els.status.className = cls;
}

/**
 * 预校验 + 收集参数（与 Rust 同口径，提前拦截）：
 * - 拒绝空值 / 非数字（Number 转换得 NaN）；
 * - 拒绝范围颠倒（xmin >= xmax 或 ymin >= ymax）；坐标本身允许负值；
 * - 格网宽/高必须 > 0。
 */
function readAndValidate() {
  const fields = [
    ['xmin', els.xmin], ['ymin', els.ymin], ['xmax', els.xmax], ['ymax', els.ymax],
    ['格网宽', els.cw], ['格网高', els.ch],
  ];
  const empty = fields.filter(([, el]) => el.value.trim() === '').map(([n]) => n);
  if (empty.length) return { err: `有未填写的输入：${empty.join('、')}` };

  const v = {};
  for (const [name, el] of fields) {
    const num = Number(el.value.trim());
    if (!Number.isFinite(num)) return { err: `「${name}」不是有效数字（输入为"${el.value}"）` };
    v[name] = num;
  }
  if (!(v.xmin < v.xmax) || !(v.ymin < v.ymax)) {
    return { err: '范围颠倒或退化：要求 xmin < xmax 且 ymin < ymax（坐标允许负值）' };
  }
  if (v['格网宽'] <= 0 || v['格网高'] <= 0) {
    return { err: '格网宽/高必须大于 0（拒绝 0 与负数）' };
  }
  return { extent: [v.xmin, v.ymin, v.xmax, v.ymax], cw: v['格网宽'], ch: v['格网高'] };
}

/** 点击"生成渔网"：调 WASM → 状态/输出/预览三处更新 */
function generate(ev) {
  ev.preventDefault();
  const p = readAndValidate();
  if (p.err) return setStatus(p.err, 'err');

  try {
    const t0 = performance.now();
    // 核心调用：6 个标量进，GeoJSON 字符串出；非法参数在此抛中文 Error
    const s = generateFishnet(p.extent[0], p.extent[1], p.extent[2], p.extent[3], p.cw, p.ch);
    const ms = (performance.now() - t0).toFixed(1);
    geojsonText = s;

    const fc = JSON.parse(s);
    const n = fc.features.length;
    // 由行列号反推行列数（避免再传一次返回结构）
    const rows = fc.features.reduce((m, f) => Math.max(m, f.properties.row), 0) + 1;
    const cols = fc.features.reduce((m, f) => Math.max(m, f.properties.col), 0) + 1;
    const bytes = new Blob([s]).size;
    setStatus(`生成完成：${n} 个格网（${cols} 列 × ${rows} 行），GeoJSON ${bytes.toLocaleString()} 字节，Rust 计算耗时 ${ms} ms`, 'ok');

    renderOutput(fc, s);
    drawPreview(fc, p.extent);
    els.btnCopy.disabled = false;
  } catch (e) {
    setStatus(`生成失败：${e?.message ?? e}`, 'err');
  }
}

/** GeoJSON 文本展示：格网少时美化缩进，格网多时直接展示紧凑文本（复制内容不受影响） */
function renderOutput(fc, s) {
  els.out.textContent = fc.features.length <= 1000
    ? JSON.stringify(fc, null, 2)
    : `${s}\n\n（格网较多，此处仅显示紧凑 JSON；点"一键复制"得到完整文本）`;
}

/**
 * Canvas 预览：自绘等比缩放，地理坐标 y 向上 → 画布 y 向下翻转。
 * 所有格网合并进一个 Path2D 批量描边，格网很多时也不逐格操作。
 */
function drawPreview(fc, [xmin, ymin, xmax, ymax]) {
  const cv = els.canvas;
  const ctx = cv.getContext('2d');
  const W = cv.width, H = cv.height, pad = 40;
  ctx.clearRect(0, 0, W, H);

  const scale = Math.min((W - pad * 2) / (xmax - xmin), (H - pad * 2) / (ymax - ymin));
  const px = (x) => pad + (x - xmin) * scale;
  const py = (y) => H - pad - (y - ymin) * scale;

  const path = new Path2D();
  for (const f of fc.features) {
    const ring = f.geometry.coordinates[0]; // 外环 5 点（首尾闭合）
    path.moveTo(px(ring[0][0]), py(ring[0][1]));
    for (let i = 1; i < ring.length; i++) path.lineTo(px(ring[i][0]), py(ring[i][1]));
    path.closePath();
  }
  if (fc.features.length <= 3000) { ctx.fillStyle = 'rgba(37,99,235,.08)'; ctx.fill(path); }
  ctx.strokeStyle = '#2563eb'; ctx.lineWidth = 1; ctx.stroke(path);

  // 四至外框 + 四角坐标标注
  ctx.strokeStyle = '#334155'; ctx.lineWidth = 2;
  ctx.strokeRect(px(xmin), py(ymax), (xmax - xmin) * scale, (ymax - ymin) * scale);
  ctx.fillStyle = '#64748b'; ctx.font = '12px Consolas, monospace';
  ctx.textAlign = 'center';
  ctx.fillText(String(xmin), px(xmin), H - pad + 16);
  ctx.fillText(String(xmax), px(xmax), H - pad + 16);
  ctx.textAlign = 'right';
  ctx.fillText(String(ymax), pad - 6, py(ymax) + 4);
  ctx.fillText(String(ymin), pad - 6, py(ymin) + 4);
}

/** 一键复制：clipboard API 优先，非安全上下文退回 execCommand 兜底 */
async function copyGeojson() {
  if (!geojsonText) return;
  const flash = (msg) => {
    const old = els.btnCopy.textContent;
    els.btnCopy.textContent = msg;
    setTimeout(() => { els.btnCopy.textContent = old; }, 1500);
  };
  try {
    await navigator.clipboard.writeText(geojsonText);
    flash('已复制');
  } catch {
    const ta = document.createElement('textarea');
    ta.value = geojsonText;
    ta.style.position = 'fixed'; ta.style.opacity = '0';
    document.body.appendChild(ta);
    ta.select();
    const ok = document.execCommand('copy'); // 兼容 http:// 局域网访问等非安全上下文
    ta.remove();
    flash(ok ? '已复制' : '复制失败，请手动全选');
  }
}

/** 入口：先加载 WASM，成功后放开生成按钮；加载失败给出 file:// 提示 */
async function main() {
  try {
    await init(); // 默认拉取 ./pkg/rust_wasm_fishnet_bg.wasm
    wasmReady = true;
    els.btnGen.disabled = false;
    els.btnGen.textContent = '生成渔网';
    setStatus(`WASM 就绪（单次格网上限 ${maxCells().toLocaleString()}），点击"生成渔网"试试`, 'ok');
  } catch (e) {
    setStatus(`WASM 加载失败：${e?.message ?? e}。若是直接双击 file:// 打开本页，请改用本地静态服务器启动（见 README）`, 'err');
  }
}

els.form.addEventListener('submit', (ev) => { if (wasmReady) generate(ev); });
els.btnCopy.addEventListener('click', copyGeojson);
main();
