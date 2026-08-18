# Visual Regression Matrix

Winia 的视觉回归矩阵位于 `winia/tests/visual_matrix.rs`，使用 CPU Skia raster surface 执行无窗口测试。

## 覆盖范围

矩阵固定遍历以下维度：

- 主题：Material 3 light / dark
- 布局方向：LTR / RTL
- Typography：默认 token / 自定义 `body_large`、`body_small`、`label_large`

因此每个组件都会经过 `2 × 2 × 2 = 8` 个确定性样本。当前样本组合包含：

- Chip Assist、selected Filter、disabled Input
- TextField Filled supporting、Outlined placeholder/error、disabled Filled
- enabled Elevated Button、disabled Button
- enabled Regular FloatingActionButton、disabled FAB

每个组合都检查 tagged 节点存在、有效尺寸和实际绘制像素；RTL 另有不同宽度双子节点的水平镜像断言。

## 为什么不用 golden image

仓库当前没有跨平台字体、Skia 后端和图像基线管理设施。字体抗锯齿、平台字体、阴影和动画会使逐像素 golden 在不同环境中不稳定。因此矩阵采用语义级断言：

- 固定 seed `0xff6750A4`。
- 显式使用 `with_theme_typography_and_direction`，不使用依赖操作系统的 `WiniaTheme::auto`。
- 固定 viewport `520 × 420` 和布局约束。
- 不把 ripple、动画阴影或抗锯齿边缘作为单帧颜色基线。

## 运行

```bash
cargo test -p winia --test visual_matrix
cargo test -p winia --test layout_snapshot
cargo test -p winia --test render_snapshot
```

真实窗口 UI 测试仍用于交互、焦点、滚动和重组行为，不作为这套无窗口视觉矩阵的替代品。
