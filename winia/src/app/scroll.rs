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

#[cfg(test)]
mod nested_scroll_chain_tests {
    use super::{dispatch_nested_scroll_delta};
    use crate::layout::node::LayoutNode;
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState};
    use crate::nested_scroll::{NestedScrollConnection, NestedScrollSource, ScrollDelta, ScrollVelocity};

    /// 记录器 connection：记录 on_pre_scroll/on_post_scroll 的调用顺序。
    /// pre 消费一半，post 消费全部 available——便于验证顺序与消费量。
    /// `global_log`（可选）记录**跨节点**顺序（如 "pre-R" "post-T"）——
    /// 独立 log 只能验证单节点内部顺序，无法捕获 pre-R→M→T→post-T→M→R。
    struct Recorder {
        name: String,
        log: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
        global_log: Option<std::sync::Arc<std::sync::Mutex<Vec<String>>>>,
        order: std::sync::atomic::AtomicUsize,
    }
    impl Recorder {
        fn with_global(name: &str, log: std::sync::Arc<std::sync::Mutex<Vec<String>>>, global: std::sync::Arc<std::sync::Mutex<Vec<String>>>) -> Self {
            Self { name: name.to_string(), log, global_log: Some(global), order: std::sync::atomic::AtomicUsize::new(0) }
        }
    }
    impl NestedScrollConnection for Recorder {
        fn on_pre_scroll(&self, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("pre-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("pre-{}", self.name));
            }
            ScrollDelta::new(available.x / 2.0, available.y / 2.0)
        }
        fn on_post_scroll(&self, _: ScrollDelta, available: ScrollDelta, _: NestedScrollSource) -> ScrollDelta {
            let n = self.order.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.log.lock().unwrap().push(format!("post-{}-{}", self.name, n));
            if let Some(g) = &self.global_log {
                g.lock().unwrap().push(format!("post-{}", self.name));
            }
            ScrollDelta::new(available.x, available.y)
        }
        fn on_pre_fling(&self, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
        fn on_post_fling(&self, _: ScrollVelocity, _: ScrollVelocity) -> ScrollVelocity { ScrollVelocity::default() }
    }

    #[test]
    fn delta_chain_pre_post_order_includes_target() {
        // 构造 root(connection R) → mid(connection M) → target(scroll + connection T)
        let r_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let m_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let t_log = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        // 共享全局 log：验证跨节点顺序 pre-R→M→T→post-T→M→R
        let global = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));

        let conn_r = Recorder::with_global("R", r_log.clone(), global.clone());
        let conn_m = Recorder::with_global("M", m_log.clone(), global.clone());
        let conn_t = Recorder::with_global("T", t_log.clone(), global.clone());

        let scroll = ScrollState::new();
        scroll.offset.set(0.0);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_r).size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().nested_scroll(conn_m).size(200.0, 200.0)),
            LayoutNode::leaf(Modifier::new().vertical_scroll(scroll.clone()).nested_scroll(conn_t).size(100.0, 100.0)),
        ];
        nodes[0].measured_size = Size::new(200.0, 200.0);
        nodes[1].measured_size = Size::new(200.0, 200.0);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[2].measured_size = Size::new(100.0, 100.0);
        nodes[2].position = Offset::new(0.0, 100.0);
        nodes[2].scroll_viewport_height = 100.0;
        nodes[2].scroll_content_height = 200.0; // 可滚动 100
        nodes[0].children.push(1);
        nodes[1].children.push(2);

        let density = crate::unit::Density::from_density(1.0);
        // 消费推演（dy=-20，负 delta = 内容上移 = offset 增加；每个 pre 吃一半）：
        //   R pre -10 → M pre -5 → T pre -2.5 → child 剩余 -2.5 → child 消费 2.5
        //   post 链（含 T）：M post 全吃 available → T post 全吃 → R post 全吃剩余
        let consumed = dispatch_nested_scroll_delta(
            &mut nodes, 0, 2,
            ScrollDelta::new(0.0, -20.0),
            NestedScrollSource::Wheel,
            density,
        );

        // R：pre 一次（吃 10）+ post 一次（child 消费后，吃剩余）——R 是最后 post
        assert_eq!(*r_log.lock().unwrap(), vec!["pre-R-0", "post-R-1"], "R pre 后 post");
        // M：pre 一次 + post 一次（在 T/R 之前——逆序 M→T→R）
        assert_eq!(*m_log.lock().unwrap(), vec!["pre-M-0", "post-M-1"], "M pre 后 post");
        // T：pre + post（TopAppBar 类 connection 挂在 target 上，post 必须被调用——
        // 修复前的关键回归点：排除 target 会破坏 content_offset 变色/回弹）
        assert_eq!(*t_log.lock().unwrap(), vec!["pre-T-0", "post-T-1"], "T 也参与 post（TopAppBar 依赖）");

        // consumed 应为负（负 delta 方向消费），且 child 已实际滚动 offset>0
        assert!(consumed.y < 0.0, "应沿 delta 方向消费，实际 {:?}", consumed);
        assert!(scroll.offset.get() > 0.0, "child 应实际滚动（pre 只吃一半），实际 {}", scroll.offset.get());

        // 跨节点全局顺序：pre 正序 R→M→T，post 逆序 T→M→R（含 target T）
        let g = global.lock().unwrap().clone();
        assert_eq!(g, vec!["pre-R", "pre-M", "pre-T", "post-T", "post-M", "post-R"],
            "全局顺序应为 pre-R→M→T→post-T→M→R（含 target T 参与 post）——实际 {g:?}");
    }
}

#[cfg(test)]
mod release_velocity_floor_tests {
    use super::{dispatch_nested_scroll_fling};
    use crate::layout::node::LayoutNode;
    use crate::unit::{Offset, Size};
    use crate::modifier::{Modifier, ScrollState, SnapSpec};
    use crate::nested_scroll::ScrollVelocity;

    const STEP: f32 = 336.0;
    /// Below the 50 px/s floor the call site uses for the DECAY, and below the snap's own 400 dp/s
    /// threshold too — so the snap that runs here is the "settle on the nearer page" branch, which is
    /// the one a slow drag-and-release needs.
    const GENTLE: f32 = 12.0;

    /// A root with one scrollable child, mid-page so the snap has somewhere to go.
    ///
    /// The offset matters: at an exact page boundary the snap target equals the current offset and the
    /// spring is never registered (`push_animatable_with_velocity_and_done` short-circuits on
    /// `peek() == target`), which would make a `has_animation` assertion vacuous.
    fn tree(snap: Option<SnapSpec>, offset: f32) -> (Vec<LayoutNode>, ScrollState) {
        let scroll = ScrollState::new();
        scroll.offset.set(offset);
        scroll.fling_limit.set(STEP * 9.0);
        scroll.snap.set(snap);
        let mut nodes = vec![
            LayoutNode::leaf(Modifier::new().size(400.0, 600.0)),
            LayoutNode::leaf(
                Modifier::new()
                    .vertical_scroll(scroll.clone())
                    .size(400.0, STEP),
            ),
        ];
        nodes[0].measured_size = Size::new(400.0, 600.0);
        nodes[1].measured_size = Size::new(400.0, STEP);
        nodes[1].position = Offset::new(0.0, 0.0);
        nodes[1].scroll_viewport_height = STEP;
        nodes[1].scroll_content_height = STEP * 10.0;
        nodes[0].children.push(1);
        (nodes, scroll)
    }

    fn snap_spec() -> SnapSpec {
        SnapSpec { step: STEP, min_fling_velocity: 400.0 }
    }

    /// A paged list released below the 50 px/s floor still flings — because its whole motion is the
    /// snap spring, and the floor was written for a decay it does not run.
    ///
    /// This is the half no unit test reached before: `ScrollState::fling` enters `fling_with_boundary`
    /// directly and never crosses this call site, so the existing
    /// `a_paged_list_settles_even_when_it_is_released_at_rest` is green whether or not the bypass here
    /// exists. Without `|| ss.snaps()`, the release below is dropped before any fling starts and the
    /// list comes to rest between two pages with nothing left to snap it back.
    #[test]
    fn a_paged_list_flings_below_the_decay_floor() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        // Mid-page, so the nearer-page snap target is a real move.
        let (mut nodes, scroll) = tree(Some(snap_spec()), STEP * 0.6);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a paging list released at {GENTLE} px/s must still run its snap spring — the 50 px/s \
             floor is the decay's, and this list has no decay phase"
        );

        // And it is the SNAP that ran, not a decay: one page, on the boundary.
        let mut frames = 0;
        while crate::animation::has_animation_for_state(scroll.offset.state_id()) && frames < 400 {
            crate::animation::update_animations();
            std::thread::sleep(std::time::Duration::from_millis(10));
            frames += 1;
        }
        let end = scroll.offset.get();
        assert!(
            (end / STEP - (end / STEP).round()).abs() < 1e-3,
            "the gentle release settled at {end}, which is not a page boundary — a decay would stop \
             wherever the friction ran out"
        );
        assert!(
            end >= STEP,
            "and it advanced to the nearer page ahead ({end} should be at least one page in)"
        );
    }

    /// The control, and the reason the test above is about the exception rather than about the number:
    /// the SAME velocity on a container with no snap spec is still dropped by the floor, so a plain
    /// scroll view does not bank a fling out of the tail of a slow drag.
    #[test]
    fn an_ordinary_container_keeps_the_decay_floor() {
        let _g = crate::animation::tests::TEST_SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        crate::animation::clear_all_animations();

        let (mut nodes, scroll) = tree(None, STEP * 0.6);
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: GENTLE });

        assert!(
            !crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "without a snap spec a {GENTLE} px/s release must start nothing — the 50 px/s floor still \
             guards the decay, which is what it was written for"
        );
        assert_eq!(scroll.offset.get(), STEP * 0.6, "and the offset is untouched");

        // The floor is a floor, not a wall: the same container released fast does fling.
        dispatch_nested_scroll_fling(&mut nodes, 0, 1, ScrollVelocity { x: 0.0, y: 900.0 });
        assert!(
            crate::animation::has_animation_for_state(scroll.offset.state_id()),
            "a fast release on an ordinary container still flings"
        );
    }
}
