# GeoJSON 渔网生成器（Rust + WASM）

输入四至范围与格网单元尺寸，生成矩形渔网（Fishnet），输出标准 GeoJSON。
核心算法用 **Rust** 编写（`geo` + `geojson` crate），通过 **wasm-bindgen / wasm-pack** 编译为
WASM 模块供浏览器 JS 调用；前端是零依赖、无构建的极简 HTML + JS 页面。

> **作品集说明（为什么这么设计）**：格网剖分、参数校验、GeoJSON 序列化等空间计算**全部下沉到
> WASM**，前端只负责表单、Canvas 预览与文本展示——**前端零重计算**。相比 JS 实现，Rust 侧
> 用强类型 + 12 个单元测试锁死算法口径（浮点取整陷阱、末行末列截断、环方向等），`geo` 负责
> 几何构造与面积校验、`geojson` 负责标准序列化，两个 crate 都是 GIS 领域的事实标准；WASM
> 产物仅 115 KB，无需任何重型地图库即可在浏览器本地完成生成。

## 目录结构

```
rust-wasm-fishnet/
├── Cargo.toml               # Rust 工程配置（rlib + cdylib，wasm-pack 元数据）
├── src/
│   ├── lib.rs               # 核心：校验 + 渔网算法 + 12 个单元测试
│   └── wasm_api.rs          # wasm-bindgen 绑定层（仅 wasm32 目标编译）
├── index.html               # 前端页面（无框架/无 CDN/无地图库）
├── app.js                   # 前端薄壳：表单 → 调 WASM → Canvas 预览/复制
├── pkg/                     # wasm-pack 产物（构建后生成，git 忽略）
│   ├── rust_wasm_fishnet.js         # JS glue（ES Module）
│   ├── rust_wasm_fishnet_bg.wasm    # WASM 模块
│   └── *.d.ts                       # TypeScript 类型
└── LICENSE / README.md
```

## 渔网口径

- 行列数 = `⌈(xmax−xmin)/单元宽⌉ × ⌈(ymax−ymin)/单元高⌉`，向上取整保证**盖满**四至；
- 最后一列/最后一行不足一格时**截断到四至边界**（渔网外边界恰好等于输入 bbox，不会超出）；
- 要素按**先行后列**排序，`row=0` 为最南一行（地图学 y 轴向上惯例），属性为 `{"row": r, "col": c}`；
- 每个格网是逆时针闭合环的 Polygon（符合 GeoJSON 右手规则）；
- **坐标允许负值**（西经/南纬合法），校验只拦"非数值"与"范围颠倒"；
- 单次生成上限 `MAX_CELLS = 20000` 格，超出由 Rust 侧报中文错误，防浏览器卡死。

## 环境准备

1. **Rust**（含 cargo）：[rustup](https://rustup.rs) 安装；本机若装不了 MSVC，可用
   `x86_64-pc-windows-gnu` 宿主（自带链接器，免 Visual Studio）：
   ```bat
   rustup-init.exe -y --default-host x86_64-pc-windows-gnu --profile minimal
   ```
2. **wasm32 编译目标**：
   ```bat
   rustup target add wasm32-unknown-unknown
   ```
3. **wasm-pack**（任选其一）：
   ```bat
   npm i -g wasm-pack          :: npm 通道（国内建议 --registry=https://registry.npmmirror.com）
   cargo install wasm-pack     :: 源码编译通道
   ```

## 编译与测试

```bat
:: 单元测试（12 项：整除/截断/负坐标/浮点陷阱/非法参数/超上限/GeoJSON 结构/环方向）
cargo test

:: 编译 WASM（web 目标，产物在 pkg/）
wasm-pack build --target web --release
```

## 本地启动

**不能直接双击 `file://` 打开**：ES Module 与 `.wasm` 的获取受浏览器同源策略限制，
`file://` 下会被 CORS 拦截。必须用任意静态文件服务器从工程根目录起服：

```bat
:: 方式一（有 Node）：任意静态服务器，端口随意（示例 8970）
npx serve . -l 8970

:: 方式二（有 Python）
python -m http.server 8970

:: 然后浏览器打开
http://127.0.0.1:8970/
```

## 测试示例

**浏览器端**：默认表单就是一组示例——四至 `[113, 28, 114, 29]`（约宁德福鼎一带 1° 见方）、
格网 `0.25 × 0.25`（度）→ 点"生成渔网"：

- 状态行显示 `16 个格网（4 列 × 4 行），GeoJSON 2,729 字节，Rust 计算耗时 <1 ms`；
- Canvas 预览出 4×4 网格，四角标注坐标；
- 输出区展示美化 GeoJSON，"一键复制"得到紧凑完整文本。

**非法输入对照表**（前后端双重校验，Rust 侧为最终裁决）：

| 输入 | 提示 |
|---|---|
| `xmin=115, xmax=114`（颠倒） | 范围颠倒或退化：要求 xmin < xmax 且 ymin < ymax（坐标允许负值） |
| `xmin=abc` / 留空 | 「xmin」不是有效数字 / 有未填写的输入 |
| `格网宽=0` 或负数 | 格网宽/高必须大于 0（拒绝 0 与负数） |
| `格网宽=格网高=0.001`（100 万格） | 格网总数 1000000 超过上限 20000，请调大格网宽/高后重试 |

**JS 直接调用 WASM**（无需页面）：

```js
import init, { generateFishnet } from './pkg/rust_wasm_fishnet.js';
await init();
const geojson = generateFishnet(113, 28, 114, 29, 0.25, 0.25); // 字符串
// 非法参数抛 Error，message 为中文，可直接展示给用户
```

## 部署方式

纯静态站点，GitHub Pages / 任意静态托管均可：

1. `wasm-pack build --target web --release` 后，把 **工程根目录整体**（含 `pkg/`）上传；
2. GitHub Pages 直接推仓库、开启 Pages 即可（无需特殊构建；纯静态站建议加 `.nojekyll`）；
3. 托管服务若允许配置 MIME，请将 `.wasm` 映射为 `application/wasm`（多数平台默认正确；
   即使不配，wasm-bindgen 的加载器也会自动退回非流式实例化，功能不受影响，只是略慢）。

## 二次开发

- 改算法/口径：`src/lib.rs`（口径说明在文件头注释）→ `cargo test` 全绿 → `wasm-pack build` → 刷新页面；
- 改上限：`src/lib.rs` 中 `MAX_CELLS`；
- 前端展示逻辑在 `app.js`，样式内联在 `index.html`，两者无需构建。

## 编译排坑清单（本项目实战记录）

1. **gnu 工具链 `ld` 不认中文路径**：项目放在含中文的目录（如 `E:\zcode\探索前沿\...`）时，
   Windows gnu 工具链自带的 ld 以 ANSI 方式解读 UTF-8 路径，链接期报
   `ld: cannot find *.o: No such file or directory`（文件其实都在）。
   解法：设 `set CARGO_TARGET_DIR=E:\cargo-target\rust-wasm-fishnet`（纯 ASCII 路径）再构建；
   或把项目移到纯 ASCII 路径。MSVC 工具链无此问题。
2. **wasm-pack 从 GitHub 拉 binaryen 失败**：release 构建收尾时 wasm-pack 会下载 wasm-opt
   （binaryen），github.com 直连被阻的环境报
   `failed to download from https://github.com/WebAssembly/binaryen/...`。
   解法：在 `Cargo.toml` 加
   ```toml
   [package.metadata.wasm-pack.profile.release]
   wasm-opt = false
   ```
   体积略增（本例 wasm 115 KB），可接受；有条件也可走代理让它下载。
3. **glue 文件名是下划线**：crate 名 `rust-wasm-fishnet`，wasm-pack 产物却是
   `rust_wasm_fishnet.js`（连字符转下划线），前端 `import` 必须按下划线写，否则模块 404。
4. **`file://` 打不开 WASM 页**：ES Module + wasm 获取受 CORS 限制，必须本地静态服务器
   （见上文），双击 html 只会看到"WASM 加载失败"。
5. **剪贴板权限**：`navigator.clipboard` 仅在安全上下文（https / localhost）可用；
   页面已内置 `execCommand` 兜底。无头/自动化浏览器可能直接拒绝剪贴板权限，此时按提示手动
   全选复制即可。
6. **rustup 安装被墙/慢**：`static.rust-lang.org` 可直连时无需镜像；如需镜像可用
   `RUSTUP_DIST_SERVER` / `RUSTUP_UPDATE_ROOT` 指向国内镜像站后再跑 rustup-init。
