# Debug Server 使用指南

`debug-server` feature 提供运行时调试通道：**注入输入事件、读取布局树、抓取帧像素**——
UI 自动化测试与交互问题排查的基础设施。随 `run_app!` 自动启动（app.rs 的
`debug::start_stdin_channel()` / `start_ws_server()`）。

## 启用方式

```bash
# 运行任何 example / 应用时加上 feature
cargo run -p winia --example navigation_rail_demo --features debug-server

# 端口通过环境变量隔离（并行测试/多实例必备），默认 9998
WINIA_DEBUG_PORT=10077 cargo run --features debug-server ...
```

启用后同时开启两个通道：

| 通道 | 地址 | 适用场景 |
|------|------|---------|
| WebSocket | `ws://127.0.0.1:$WINIA_DEBUG_PORT` | 脚本驱动（Python + websockets） |
| stdin | 进程标准输入，每行一条命令 | 手动验证：`echo 'c 190 130' \| ./app` |

## 命令参考

WebSocket 文本命令（空格分隔参数）；stdin 同协议，响应走 stdout（`t` 带 `TREE:` 前缀）。

| 命令 | 作用 | WS 响应 |
|------|------|--------|
| `c <x> <y>` | 模拟点击（内部拆 down+up） | `ok click` |
| `d` / `m` / `u` `<x> <y>` | 指针按下 / 移动 / 释放（手势、拖拽序列） | `ok down/move/up` |
| `k <key...>` | 键盘事件（如 `k Return`） | `ok key` |
| `s <dy>` 或 `s <dx> <dy>` | 滚轮（双参形式测横向滚动） | `ok scroll` |
| `r` | 请求截图：置标志 + 唤醒循环，下一帧渲染后像素可取 | `screenshot <W>x<H>` |
| `p` | 取最近一帧像素——**二进制帧** = 8 字节头（W、H 各 u32 LE）+ RGBA | binary / `no frame` |
| `t` | 读全部窗口布局树 JSON | JSON 字符串 |
| `q` | 强制退出应用（测试收尾兜底） | （进程退出） |

⚠ **坐标均为逻辑像素**（非物理像素；HiDPI 下物理 = 逻辑 × scale_factor，
demo 截图 1350x720 = 窗口 900x480 × 1.5）。

## 布局树 JSON

`t` 返回所有窗口及其顶层弹出层：

```json
[
  {"window": 7408332, "root": [ ...主树节点... ]},
  {"window": 7408332, "overlay": 0, "id": 1, "root": [ ...弹出层节点... ]}
]
```

- 主条目：`window` = winit window id，无 `overlay` 字段
- 弹出层条目（Popup/Dialog/DropdownMenu/ModalRail 等）：`overlay` = z 序索引
  （0 最底），`id` = OverlayDesc 稳定 id。**每帧整体替换**——弹出层关闭后
  条目自动消失，可据此断言「浮层是否真正关闭」（见下方排查案例）
- 无弹出层时输出与旧版完全一致（既有解析脚本兼容）
- 树在每帧渲染后更新（RedrawRequested → build_tree_json）；渲染停止则树冻结

### 节点结构

```json
{"pos": [86,48], "size": [48,48], "mod": "size(Fixed(48.0),Fixed(48.0))|bg(<dynamic>)|click",
 "tag": null, "focused": false, "children": [...]}
```

- `mod`：modifier 摘要（click/focus/hover/ripple/text(...)、pad(...)、size(...)；
  动态闭包显示 `<dynamic>`）
- ⚠ **`pos` 是相对父节点的坐标**！求屏幕绝对位置必须沿根到该节点路径累加：

```python
def walk(node, ox=0.0, oy=0.0, out=None):
    if isinstance(node, list):          # root 本身是数组
        for n in node: walk(n, ox, oy, out)
        return out
    x, y = node.get("pos", [0, 0])
    ax, ay = ox + x, oy + y             # 累加父链偏移 → 绝对坐标
    out.append((ax, ay, node))
    for c in node.get("children", []):
        walk(c, ax, ay, out)
    return out
```

## 截图像素格式

`p` 的二进制负载：

```
[0..4)   width  u32 LE（物理像素）
[4..8)   height u32 LE
[8..]    像素   skia N32 premul —— 内存序 BGRA
```

Python 解码（PIL 按 RGBA 读入会 R/B 互换；亮度/alpha 分析不受影响，色彩分析需换序）：

```python
w, h = struct.unpack("<II", resp[:8])
img = Image.frombytes("RGBA", (w, h), resp[8:])
img.save("frame.png")
```

## 典型排查工作流

以「ModalWideNavigationRail 关闭后遮罩残留」为例（tools/ws_modal_seq.py，
2026-08 实战定位 `visible` 缺响应式依赖导致 overlay 永不移除；该组件后已移除，
排查方法论仍适用于一切基于 overlay 的浮层组件）：

1. 启动 demo（带 feature + 独立端口），WS 带重试连接
2. `t` 找目标文本节点 → 累加得绝对坐标 → `d`+`u` 点击
3. **树条目存在性做状态断言**：开 → 有 overlay 条目；关 → 条目应消失
4. 像素反推视觉量：空白区基线色 vs 当前色比值 = 遮罩 alpha
   （实测开启态 0.625 = 内建模态遮罩 0.431 + 组件 scrim 0.32 叠加）
5. 时间线采样（每 ~150ms 一轮 t+p）区分「慢收敛」vs「永久卡死」

### 经验教训（踩过的坑）

- **`p` 只在有截图请求后有数据**——纯 `p` 轮询拿到陈旧帧或 `no frame`
- **`r` 会唤醒事件循环**（wake + request_redraw），观测动画节奏时会扰动时序；
  `t` 不唤醒。判断「卡死」用 `t`，看画面用 `r`+`p`
- Windows 上重编译前先 `taskkill //F //IM <demo>.exe`——残留进程锁 exe 导致
  link 失败（exit 1104），此时跑的是**旧二进制**，结论全部作废
- 树 pos 忘记累加父链 → 点击落空且无报错（打在错误组件上）
- `WINIA_RECOMPOSE_TRACE=1` 输出每帧重组决策日志（stderr），配合时间线定位
  「哪一帧之后不再重组」

## 测试集成

```bash
cargo test --test ui_test --features debug-server
```

- 并行隔离：每个测试实例设不同 `WINIA_DEBUG_PORT`
- 收尾统一发 `q` 强制退出（正常关闭路径之外的安全兜底）
- stdin 树断言按 stdout 的 `TREE:` 前缀过滤（其他输出可能污染管道）

## 现成驱动脚本

`tools/` 目录不进版本库（本地工作区工具），以下为仓库开发者常用的本地脚本，
克隆后需自行照上文协议实现（核心模式已在上文给出）：

| 脚本 | 用途 |
|------|------|
| ws_drive.py | 通用：启动→连 WS→抓树→点击 |
| ws_screenshot_png.py | 前后对比截图存 PNG |
| ws_modal_debug.py | modal 开关状态机断言（overlay 树条目） |
| ws_modal_seq.py | 逐帧时间线（alpha 演化 + overlay 存在性） |

