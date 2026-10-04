//! Scroll: the drag session, nested-scroll dispatch, and applying a delta to the tree.
//!
//! Split out of `app.rs`, which is the winit application; this is what a wheel or a drag does to
//! the node tree once the event has been routed there.

use super::*;

/// 拖拽滚动会话：slot key（跨重组稳定）+ 最近位置 + 速度样本（松手 fling 用）。
/// 双轴跟踪（LazyRow 横向拖拽/惯性用 x 轴）——样本 (t, x, y)。
pub(crate) struct DragScroll {
    pub(crate) slot: u64,
    pub(crate) last_x: f32,
    pub(crate) last_y: f32,
    pub(crate) samples: Vec<(std::time::Instant, f32, f32)>,
}

impl DragScroll {
    /// Open a session at `pos`: the deltas that follow are measured from there. The node the
    /// session scrolls is re-resolved by slot key on every move (the layout tree is rebuilt
    /// between frames, so a node index would go stale).
    pub(crate) fn new(slot: u64, pos: (f32, f32)) -> Self {
        Self {
            slot,
            last_x: pos.0,
            last_y: pos.1,
            samples: vec![(std::time::Instant::now(), pos.0, pos.1)],
        }
    }
}

impl DragScroll {
    /// 手指速度（px/s）：最近 ~200ms 窗口的最小二乘斜率。
    /// ⚠ x 轴用"距离现在的时长"（越大越早）——回归斜率符号与真实时间相反，
    /// 取负修正（实测：向上拖 100px 得 +650 而非 -650，fling 方向反了）。
    fn regression(&self, sel: fn(&(std::time::Instant, f32, f32)) -> f32) -> f32 {
        let now = std::time::Instant::now();
        let cutoff = now - std::time::Duration::from_millis(200);
        let pts: Vec<(f32, f32)> = self.samples
            .iter()
            .filter(|(t, _, _)| *t >= cutoff)
            .map(|s| (now.duration_since(s.0).as_secs_f32(), sel(s)))
            .collect();
        if pts.len() < 2 { return 0.0; }
        let n = pts.len() as f32;
        let sx: f32 = pts.iter().map(|p| p.0).sum();
        let sy: f32 = pts.iter().map(|p| p.1).sum();
        let sxy: f32 = pts.iter().map(|p| p.0 * p.1).sum();
        let sxx: f32 = pts.iter().map(|p| p.0 * p.0).sum();
        let denom = n * sxx - sx * sx;
        if denom.abs() < 1e-6 { return 0.0; }
        -(n * sxy - sx * sy) / denom
    }
    pub(crate) fn velocity_x(&self) -> f32 { self.regression(|s| s.1) }
    pub(crate) fn velocity_y(&self) -> f32 { self.regression(|s| s.2) }
}

/// Shift + 滚轮语义：把垂直滚轮 delta 转为水平滚动（保留方向，清除垂直分量）。
/// 用于兼容 LazyRow 等横向滚动容器——多数平台不会自动把 Shift+wheel 转成 dx。
pub(crate) fn scroll_delta_with_shift(dx: f32, dy: f32, shift: bool) -> (f32, f32) {
    if shift {
        (dy, 0.0)
    } else {
        (dx, dy)
    }
}

pub(crate) fn dispatch_nested_scroll_delta(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    delta: crate::nested_scroll::ScrollDelta,
    source: crate::nested_scroll::NestedScrollSource,
    density: crate::unit::Density,
) -> crate::nested_scroll::ScrollDelta {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return crate::nested_scroll::ScrollDelta::ZERO; }
    let mut remaining = delta;
    let mut total = crate::nested_scroll::ScrollDelta::ZERO;
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[pre-scroll] idx={} connection=有", idx);
            }
            let part = connection.on_pre_scroll(remaining, source).clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    let child_consumed = apply_scroll_delta_inner(nodes, target, remaining.x, remaining.y, density, false);
    total = total + child_consumed;
    remaining = remaining - child_consumed;
    // post-scroll：祖先从内到外（**含 target 自身**——TopAppBar 等 connection
    // 挂在 scroll 容器节点上，依赖 on_post_scroll 更新 content_offset 变色；
    // 排除 target 会破坏该行为，见 scaffold_demo/fixture_nested_scroll）
    for &idx in path.iter().rev() {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection.on_post_scroll(child_consumed, remaining, source).clamp_to(remaining);
            total = total + part;
            remaining = remaining - part;
        }
    }
    total
}

pub(crate) fn dispatch_nested_scroll_fling(
    nodes: &mut [LayoutNode],
    root: usize,
    target: usize,
    velocity: crate::nested_scroll::ScrollVelocity,
) -> crate::nested_scroll::ScrollVelocity {
    fn path_to(nodes: &[LayoutNode], current: usize, target: usize, path: &mut Vec<usize>) -> bool {
        path.push(current);
        if current == target { return true; }
        for &child in &nodes[current].children {
            if path_to(nodes, child, target, path) { return true; }
        }
        path.pop();
        false
    }
    let mut path = Vec::new();
    if !path_to(nodes, root, target, &mut path) { return velocity; }

    let mut remaining = velocity;
    let mut consumed = crate::nested_scroll::ScrollVelocity::default();
    // pre-fling：祖先（含 target）先消费一部分速度——正序 path，target 自身
    // connection 也参与 pre（对标 Compose：目标自身的 connection 参与 pre）
    for &idx in &path {
        if let Some(connection) = nodes[idx].modifier.nested_scroll_connection() {
            let part = connection.on_pre_fling(remaining).clamp_to(remaining);
            consumed.x += part.x;
            consumed.y += part.y;
            remaining.x -= part.x;
            remaining.y -= part.y;
        }
    }
    // child fling：剩余速度交给目标滚动节点；撞边界时把瞬时剩余速度交给
    // post-fling 链。post 链 = path 逆序（**含 target 自身**——TopAppBar 等
    // connection 挂在 scroll 容器节点上，依赖 on_post_fling 弹回/复位；
    // 排除 target 会破坏该行为，见 scaffold_demo/fixture_nested_scroll）。
    let child_velocity = remaining;
    // child 实际消费量 = 起始速度 − 边界剩余速度（在 boundary 回调内计算——
    // fling_with_boundary 回调传入的是撞边界时的瞬时剩余速度）。
    // ⚠ 不能传起始速度：on_post_fling 的 consumed_by_child 语义是"child 实际
    // 消费了多少"，TopAppBar 依赖它做回弹幅度（review C1）。
    let post_connections: Vec<std::sync::Arc<dyn crate::nested_scroll::NestedScrollConnection>> =
        path.iter().rev()
            .filter_map(|&idx| nodes[idx].modifier.nested_scroll_connection())
            .collect();
    let child_started = {
        let node = &nodes[target];
        // The 50 px/s floor below is a DEACAY floor — it stops a release that is really just the tail
        // of a drag from banking momentum. A paging list must ignore it: its whole motion is the snap
        // spring, so a slow drag past halfway that is let go with the finger nearly still has to still
        // advance a page. Skipping the fling there left the list resting between two pages for good,
        // because nothing else ever snaps it back. Compose has no such floor — `Scrollable.kt:857-881`
        // calls `performFling` on every release.
        if let Some(ss) = node.modifier.vertical_scroll_state() {
            if child_velocity.y.abs() >= 50.0 || ss.snaps() {
                let post_connections = post_connections.clone();
                ss.fling_with_boundary(child_velocity.y, move |remaining_velocity| {
                    // child 实际消费 = 起始 − 边界剩余（剩余为 0 时全消费）
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: 0.0,
                        y: child_velocity.y - remaining_velocity,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: 0.0, y: remaining_velocity };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.y -= crate::nested_scroll::ScrollVelocity { x: 0.0, y: part.y }.clamp_to(available).y;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else if let Some(ss) = node.modifier.horizontal_scroll_state() {
            if child_velocity.x.abs() >= 50.0 || ss.snaps() {
                let post_connections = post_connections.clone();
                // ⚠ reverse（RTL）滚动：fling 速度方向与手势 delta 同需镜像
                //（apply_scroll_delta 已镜像 delta，此处镜像速度保持一致）
                let fling_vx = if node.scroll_reverse { -child_velocity.x } else { child_velocity.x };
                ss.fling_with_boundary(fling_vx, move |remaining_velocity| {
                    // child 实际消费 = 起始 − 边界剩余
                    let consumed_by_child = crate::nested_scroll::ScrollVelocity {
                        x: child_velocity.x - remaining_velocity,
                        y: 0.0,
                    };
                    let mut available = crate::nested_scroll::ScrollVelocity { x: remaining_velocity, y: 0.0 };
                    for connection in &post_connections {
                        let part = connection.on_post_fling(consumed_by_child, available);
                        available.x -= crate::nested_scroll::ScrollVelocity { x: part.x, y: 0.0 }.clamp_to(available).x;
                    }
                });
                true
            } else { ss.is_scroll_in_progress.set(false); false }
        } else { false }
    };
    if child_started {
        consumed.x += child_velocity.x;
        consumed.y += child_velocity.y;
    }
    consumed
}

pub(crate) fn find_scroll_target(nodes: &[LayoutNode], idx: usize, dx: f32, dy: f32) -> Option<usize> {
    for &child in nodes[idx].children.iter().rev() {
        if let Some(target) = find_scroll_target(nodes, child, dx, dy) { return Some(target); }
    }
    if (dy != 0.0 && nodes[idx].modifier.vertical_scroll_state().is_some())
        || (dx != 0.0 && nodes[idx].modifier.horizontal_scroll_state().is_some()) {
        Some(idx)
    } else { None }
}

#[allow(dead_code)] // the tests in this file call it
pub(crate) fn apply_scroll_delta(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density) -> crate::nested_scroll::ScrollDelta {
    apply_scroll_delta_inner(nodes, idx, dx, dy, density, true)
}

/// `apply_scroll_delta` 实现。`recursive=true` 时，若节点自身未消费（已到
/// 边界），回退递归子节点（旧行为——wheel/拖拽的 fallback 语义，让内层
/// 子 scroll 消费）。`recursive=false` 时**只滚目标自身**——`dispatch_nested
/// _scroll_delta` 的显式 target 语义：target 滚不动应留给 post 链的祖先
/// connection 处理，**不能**偷偷滚子节点（否则"在外层顶部向下拖"会错误地
/// 滚动内层子列表——用户报告的 bug：外层已到顶，delta 递归到内层）。
fn apply_scroll_delta_inner(nodes: &mut [LayoutNode], idx: usize, dx: f32, dy: f32, density: crate::unit::Density, recursive: bool) -> crate::nested_scroll::ScrollDelta {
    let mut consumed = crate::nested_scroll::ScrollDelta::ZERO;
    {
        let node = &nodes[idx];
        #[cfg(debug_assertions)]
        if drag_trace_enabled() {
            eprintln!("[apply-scroll] idx={} dy={} vp_h={} content_h={} has_vscroll={}",
                idx, dy, node.scroll_viewport_height, node.scroll_content_height,
                node.modifier.vertical_scroll_state().is_some());
        }
        if dy != 0.0 {
            if let Some(state) = node.modifier.vertical_scroll_state() {
            // 手动输入接管：取消进行中的 fling + 结束滚动中标记（拖拽路径随后置回）
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            #[cfg(debug_assertions)]
            if drag_trace_enabled() {
                eprintln!("[apply-scroll] → state.offset 应用前 = {}", current);
            }
            // 滚动极限 = 内容总高度 - 可视区域高度
            // The visible size is the node's OWN measured height — what the user sees. It used to prefer
            // `scroll_viewport_height`, which is the padding-DEDUCTED measure constraint: for a menu
            // (a 112x424 container holding 20 items with 8dp of padding) that gave a range of
            // 976 - 408 = 568 where the end is 976 - 424 = 552, so the content scrolled 16px past its end
            // and the last item sat in dead space (24px with its own padding). Falls back to the viewport
            // while the node has not been measured yet, and to 0 (no movement) when neither is known.
            let visible_h = if node.measured_size.height > 0.0 {
                node.measured_size.height
            } else if node.scroll_viewport_height > 0.0 {
                node.scroll_viewport_height
            } else {
                node.modifier.fixed_size()
                    .and_then(|(_, h)| {
                        use crate::layout::Dimension;
                        match h {
                            Dimension::Fixed(h) | Dimension::Dp(crate::unit::Dp(h)) => Some(h),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            // 内容总高：lazy 列表（scroll_content_height > 0）用其真实内容高；
            // 否则用节点自身高度（普通 scroll 容器）
            let content_h = if node.scroll_content_height > 0.0 {
                node.scroll_content_height
            } else {
                node.measured_size.height
            };
            let max_offset = (content_h - visible_h).max(0.0);
            let new = (current - dy).clamp(0.0, max_offset);
            state.offset.set(new);
            consumed.y = current - new;
            }
        }
        // 水平滚动（LazyRow/横向 scroll 容器）——与垂直对称：dx 正 = 内容左移
        if dx != 0.0 {
        if let Some(state) = node.modifier.horizontal_scroll_state() {
            crate::animation::cancel_animation(&state.offset);
            state.is_scroll_in_progress.set(false);
            let current = state.offset.get();
            // Same as the vertical case above: the container's own measured width is what the range ends
            // against, with the viewport only as a not-measured-yet fallback.
            let visible_w = if node.measured_size.width > 0.0 {
                node.measured_size.width
            } else if node.scroll_viewport_width > 0.0 {
                node.scroll_viewport_width
            } else {
                node.modifier.fixed_size()
                    .and_then(|(w, _)| {
                        use crate::layout::Dimension;
                        match w {
                            Dimension::Fixed(w) | Dimension::Dp(crate::unit::Dp(w)) => Some(w),
                            Dimension::Px(p) => Some(p.to_logical(density)),
                            _ => None,
                        }
                    })
                    .unwrap_or(0.0)
            };
            let content_w = if node.scroll_content_width > 0.0 {
                node.scroll_content_width
            } else {
                node.measured_size.width
            };
            let max_offset = (content_w - visible_w).max(0.0);
            // ⚠ reverse（RTL）滚动：render 端 offset 语义被镜像（offset 0 = 内容末端），
            // 手势 delta 方向也需镜像——否则拖动方向反（用户实测 bug）。垂直同理见上。
            let new = if node.scroll_reverse {
                (current + dx).clamp(0.0, max_offset)
            } else {
                (current - dx).clamp(0.0, max_offset)
            };
            state.offset.set(new);
            consumed.x = current - new;
        }
        }
    }
    if consumed.x != 0.0 || consumed.y != 0.0 { return consumed; }
    // 自身未消费：recursive 模式回退递归子节点（旧 fallback 语义）；
    // 非 recursive（dispatch 路径）不递归——target 滚不动交给 post 链
    if !recursive { return consumed; }
    // 子节点（clone 索引后递归，避免与 nodes 的可变借用冲突）
    let children: Vec<usize> = nodes[idx].children.clone();
    for c in children {
        let child = apply_scroll_delta_inner(nodes, c, dx, dy, density, true);
        if child.x != 0.0 || child.y != 0.0 { return child; }
    }
    consumed
}
