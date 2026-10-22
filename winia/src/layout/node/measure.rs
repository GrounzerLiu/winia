use super::*;

// ── MeasurePolicy trait ──

/// 测量和布局策略。
///
/// 类似 Compose 的 MeasurePolicy。
/// 实现此 trait 的类型定义了一个容器的布局逻辑。
pub trait MeasurePolicy: std::fmt::Debug {
    /// 测量阶段：给定约束，返回自身尺寸和子节点的放置方案。
    /// arena 版：children 是子节点索引；nodes/policies 为拆分借用（policy 在
    /// policies 池、节点在 nodes——字段级借用避免冲突）。实现内通过
    /// `measure_node(nodes, policies, c, cc)` 递归测量子节点。
    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>);

    /// 布局阶段：给定已分配的尺寸，为子节点分配位置。
    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]);

    /// Whether this policy composes content DURING measurement (`layout::subcompose`).
    ///
    /// Such a node must be re-measured whenever it is materialized: its content is composed inside
    /// the measurement, so a folded size freezes the content at the previous frame's parameters. The
    /// node's own `subcomposed` flag is only set BY a real measurement, so it cannot answer this for a
    /// node that was just rebuilt — asking the policy is what covers that case.
    fn subcomposes(&self) -> bool {
        false
    }

    // ── Intrinsic measurement (Compose's four `MeasurePolicy` intrinsic functions) ──
    //
    // Every method has a DEFAULT, exactly like Compose, and the default is Compose's own: "have
    // default implementations that make a best effort attempt to calculate the intrinsic
    // measurements by reusing the measure method". Concretely: measure the children with the
    // queried axis unbounded and the other axis fixed to the value the parent asked about, and
    // report the resulting size. That is an approximation for layouts whose geometry depends on
    // the space they are given; a policy that knows better (Row/Column, whose `weight` handling
    // needs the same arithmetic as its measure phases) overrides these.
    //
    // The children handed to that `measure` are SUBSTITUTED, exactly as Compose substitutes them:
    // each child measurement answers the corresponding intrinsic query instead of measuring
    // (`approx_measure` + `synthetic_intrinsic_measure`). That is what lets a policy with side
    // effects in its `measure` be asked an intrinsic question through a defaulted ancestor without
    // running it, and it is why Compose's animating nodes can inherit
    // `LayoutModifierNodeWithPassThroughIntrinsics`. A policy whose own `measure` has side effects
    // still overrides all four — "There should be no side-effects from implementers of
    // `maxIntrinsicWidth`" (`ui/layout/MeasurePolicy.kt:133`) — so that answering is exact and
    // never enters the approximation at all.

    /// The smallest width this content can be laid out at, given it will be `height` tall.
    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, f32::MAX, 0.0, height),
            IntrinsicQuery::MinWidth,
        )
        .width
    }

    /// The width this content takes with nothing wrapped, given it will be `height` tall.
    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, f32::MAX, 0.0, height),
            IntrinsicQuery::MaxWidth,
        )
        .width
    }

    /// The smallest height this content can be laid out at, given it will be `width` wide.
    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, width, 0.0, f32::MAX),
            IntrinsicQuery::MinHeight,
        )
        .height
    }

    /// The height this content takes with nothing wrapped, given it will be `width` wide.
    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        ctx.approx_measure(
            children,
            Constraints::new(0.0, width, 0.0, f32::MAX),
            IntrinsicQuery::MaxHeight,
        )
        .height
    }

    /// The lines this container reports, when it decides them itself — Compose's
    /// `layout(alignmentLines = …)` parameter.
    ///
    /// `None`, the default, means "inherit the children's": `measure_node_inner` merges every child's lines
    /// through each line's own merger. `Some(lines)` REPLACES that set, which is what a container whose
    /// overlay child must not move its own line needs — `BadgedBox` reports the ANCHOR's baselines so a
    /// badge's text cannot shift the line a Row aligns a whole row of icons by
    /// (`material3/Badge.kt:97-100`, "to avoid default baseline calculations from including by any badge
    /// content").
    ///
    /// Called after the children are placed, so a policy may read their positions as well as their lines.
    fn alignment_lines(
        &self,
        _nodes: &[LayoutNode],
        _children: &[usize],
    ) -> Option<Vec<(AlignmentLine, f32)>> {
        None
    }
}


// ── 递归测量引擎 ──

/// 递归测量节点（处理 modifier 中的约束并调用子节点的 measure_policy）。
///
/// arena 版：`nodes` 为节点池、`policies` 为策略池、`idx` 为当前节点索引。
/// 子节点通过 `nodes[idx].children`（索引列表）递归测量。
/// Applies layout invalidation: a DFS over the tree marks the nodes whose slot key is in
/// `dirty_keys`, and marks every ANCESTOR of those nodes.
///
/// The ancestor half is the conservative part the framework wants: a parent's placement depends on a
/// child's size, so a child that must re-measure drags its whole ancestor chain with it. The
/// DESCENDANT half is deliberately NOT here — a hit node re-measures, and that re-measure reaches its
/// children through the ordinary fold check (`cached_constraints != constraints`), so marking the
/// subtree as well only re-measures unchanged nodes.
///
/// That is not hypothetical. The walk used to pass `hit || ancestor_dirty` down to the children, which
/// marks a hit node's ENTIRE subtree, and it cost a factor of 50 on a single update: one row's change
/// in an 800-row tree whose ROOT was among the hits produced 800 subcompositions and a one-row frame
/// of 74915 µs, against ≈1600 µs with the subtree marking removed and no hit on the root
/// (`docs/benchmarks.md` has the tables).
pub(crate) fn apply_layout_dirty(nodes: &mut [LayoutNode], root_idx: usize, dirty_keys: &std::collections::HashSet<u64>) {
    pub(crate) fn walk(nodes: &mut [LayoutNode], idx: usize, dirty_keys: &std::collections::HashSet<u64>) -> bool {
        let hit = dirty_keys.contains(&nodes[idx].slot_key);
        if hit {
            nodes[idx].layout_dirty = true;
        }
        // 索引读避免 clone（layout 是热路径）；每次索引读是临时借用，不阻塞递归写
        let mut child_hit = false;
        let n = nodes[idx].children.len();
        for i in 0..n {
            let child = nodes[idx].children[i];
            if walk(nodes, child, dirty_keys) {
                child_hit = true;
            }
        }
        if child_hit {
            nodes[idx].layout_dirty = true;
        }
        hit || child_hit
    }
    walk(nodes, root_idx, dirty_keys);
}


impl FlightMeasureFrame {
    pub(crate) const IDLE: Self = Self { content: None, reported: None };
    pub(crate) fn is_idle(&self) -> bool {
        self.content.is_none() && self.reported.is_none()
    }
}

/// 测量计数（测试用：验证常量折叠/布局失效路径确实跳过或执行 measure）。
/// thread_local 隔离——cargo test 并行线程互不串扰（全局 Atomic 会跨测试计数破坏断言）。
#[cfg(test)]
thread_local! {
    pub(crate) static MEASURE_COUNT: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Puts back the slot key a node's measurement displaced, on the way out — including on an early
/// return or a panic, which is why it is a guard rather than a line at the end. See the arming site
/// in `measure_node_inner` for what breaks without it.
pub(crate) struct ActiveSlotKeyGuard(u64);

impl Drop for ActiveSlotKeyGuard {
    fn drop(&mut self) {
        crate::runtime::composer::set_active_slot_key(self.0);
    }
}

thread_local! {
    /// The intrinsic query an approximation is answering right now, if any — the state that turns a
    /// descendant `measure_node` into Compose's `DefaultIntrinsicMeasurable`.
    static INTRINSIC_PROBE: std::cell::Cell<Option<IntrinsicQuery>> =
        const { std::cell::Cell::new(None) };
}

pub(crate) fn intrinsic_probe_query() -> Option<IntrinsicQuery> {
    INTRINSIC_PROBE.with(|q| q.get())
}

/// Sets (and restores) the probe query for one approximation. A guard rather than two lines because
/// the policy's `measure` may panic, and a nested approximation for the other axis overwrites the
/// value — restoring the OUTER query is what keeps the enclosing fold answering its own question.
pub(crate) struct IntrinsicProbeGuard(Option<IntrinsicQuery>);

impl IntrinsicProbeGuard {
    pub(crate) fn enter(query: IntrinsicQuery) -> Self {
        Self(INTRINSIC_PROBE.with(|q| q.replace(Some(query))))
    }
}

impl Drop for IntrinsicProbeGuard {
    fn drop(&mut self) {
        INTRINSIC_PROBE.with(|q| q.set(self.0));
    }
}

/// Compose's `DefaultIntrinsicMeasurable.measure` (`ui/layout/Layout.kt:309-330`): the size a child
/// reports while an approximation is running its parent's OWN `measure`. The queried axis is the
/// child's intrinsic answer at the other axis the policy handed it, and the other axis is that
/// policy's constraint (Compose uses `LargeDimension` where it is unbounded).
///
/// Nothing is written back: no `measured_size`, no placement, no cache — the wrapper is a synthetic
/// measurable, so the frame's real measurement and, above all, any state a child's real `measure`
/// would mutate stay untouched.
pub(crate) fn synthetic_intrinsic_measure(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    query: IntrinsicQuery,
    constraints: Constraints,
) -> Size {
    if query.is_width() {
        let width = intrinsic_size_of(nodes, policies, idx, query, constraints.max_height);
        Size::new(width, constraints.max_height)
    } else {
        let height = intrinsic_size_of(nodes, policies, idx, query, constraints.max_width);
        Size::new(constraints.max_width, height)
    }
}

pub(crate) fn measure_node(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // Inside an intrinsic approximation, a child measurement IS an intrinsic query — see
    // `synthetic_intrinsic_measure`. This is what keeps a policy with side effects in its `measure`
    // (an animating container, a first-measurement flag) out of an intrinsic probe even when the
    // policy that reaches it keeps the trait's default intrinsic implementation.
    if let Some(query) = intrinsic_probe_query() {
        return (
            synthetic_intrinsic_measure(nodes, policies, idx, query, constraints),
            Vec::new(),
        );
    }
    // The fold check lives in `measure_node_inner` (it must also arm the node's slot key etc.); this
    // wrapper only adds the "which node is measuring" marker that a subcomposing policy needs, and
    // leaves folded calls with no marker at all — a policy that folds did not run, so a stale marker
    // would invite a subcomposition nobody asked for.
    //
    // A subcomposing node folds like any other here: its composition is re-run when the COMPOSITION
    // changed (`dirty` from the descriptor), and that is what re-measures it — folding an unchanged one
    // is what keeps an idle frame cheap. `layout_dirty` still forces it, which is how a state read
    // during the measurement (`first_measure`, the flight override) reaches it.
    if !nodes[idx].dirty && !nodes[idx].layout_dirty && nodes[idx].cached_constraints == Some(constraints) {
        return (nodes[idx].measured_size, Vec::new());
    }
    let displaced = crate::layout::subcompose::swap_measuring_node(Some(idx));
    let sub_before = crate::layout::subcompose::subcomposition_count();
    let out = measure_node_inner(nodes, policies, idx, constraints);
    crate::layout::subcompose::swap_measuring_node(displaced);
    // Remember whether this node's policy composed anything, so the next frame does not fold it.
    nodes[idx].subcomposed = crate::layout::subcompose::subcomposition_count() > sub_before;
    out
}

pub(crate) fn measure_node_inner(
    nodes: &mut Vec<LayoutNode>,
    policies: &[Box<dyn MeasurePolicy>],
    idx: usize,
    constraints: Constraints,
) -> (Size, Vec<Placement>) {
    // 重放 stub：clean-skip 节点无 measure_policy，绝不能重新测量
    //（无 policy 走叶子分支会返回 0，把这个 0 写进节点缓存 → 塌缩不可逆）。
    // stub 只在 slot 真正 clean（无状态变化）时出现；约束若变化，下帧该 slot dirty → Enter 正常重建。
    // 常量折叠：若节点未变脏、无布局失效且约束相同，直接复用上次结果
    //（layout_dirty：两段式依赖——布局动画值变化只重测不重组）
    // A subcomposing node is not special-cased here either — see the wrapper's note.
    if !nodes[idx].dirty && !nodes[idx].layout_dirty && nodes[idx].cached_constraints == Some(constraints) {
        return (nodes[idx].measured_size, Vec::new());
    }

    // 真正执行 measure 才计数（常量折叠命中不计——测试验证折叠路径）
    #[cfg(test)]
    MEASURE_COUNT.with(|c| c.set(c.get() + 1));

    // Set ACTIVE_SLOT_KEY = this node's slot so a State::get() inside a dynamic
    // size closure registers its dependency on THIS node (value change → node
    // dirty → recompose + re-measure).
    //
    // The key is RESTORED when this measure returns, and that half is load-bearing: this node's
    // children each arm their own key on the way in, so without a restore the key stays on whatever
    // descendant measured last, and the PARENT's own post-child reads (a policy's bookkeeping after
    // its children loop, `ResizeMode` work, a dynamic size resolved after laying children out) are
    // attributed to a descendant's slot instead. A dependency on a descendant's slot is dropped the
    // moment that descendant goes away — a lazy row recycled, an `if` branch closed — and the parent
    // silently stops tracking the state. Measured before this guard existed: a parent's post-child
    // read landed on a slot that was neither the parent's nor its last child's
    // (`test_layout_dep_of_a_parent_post_child_read_lands_on_the_parent`).
    let displaced_key = crate::runtime::composer::active_slot_key();
    crate::runtime::composer::set_active_slot_key(nodes[idx].slot_key);
    let _restore_key = ActiveSlotKeyGuard(displaced_key);

    // Flight layout contract (Compose `ResizeMode` / `PlaceHolderSize`), read
    // AFTER the fold check: a folded node must neither arm its slot key nor
    // replace its recorded layout dependencies (the fold is what keeps other
    // layout dependencies alive — see `test_layout_dep_survives_const_fold`).
    // Reading here also registers the frame state as a layout dependency, so a
    // tick re-measures this node and, through `apply_layout_dirty`, its
    // ancestors — never a recomposition.
    let flight = nodes[idx].flight_measure.clone().map(|f| f.frame.get());

    // A line belongs to the measurement that produced it: clearing here means a node that stops
    // reporting one (text that became empty, a leaf that became a container) cannot leave a stale
    // position for a Row/Column to align by. Folded nodes never reach this, so they keep theirs.
    nodes[idx].alignment_lines.clear();
    // The inset belongs to the same measurement, for the same reason: a node that stops carrying
    // `Modifier::minimum_interactive_size` — removed from the chain, or disabled with
    // `minimum_interactive_size(0.0)` — must not keep the offset from when it did, or the paint insets a box
    // that was never grown (a 40 dp container drawn at 32). The write further down only happens while the
    // element is in force, so clearing here is what makes its absence meaningful.
    nodes[idx].minimum_interactive_content = None;
    nodes[idx].minimum_interactive_painted = None;
    nodes[idx].content_shift = None;

    // 应用 modifier 中的 Layout 约束（使用查询方法）
    let mut inner_constraints = constraints;

    // Size-shaping steps come first, in CHAIN ORDER — Compose's rule, because each Compose
    // modifier is its own node that `constrain`s what the node before it produced. That is what
    // decides a conflict: `.widthIn(max = 200).widthIn(min = 300)` is 200 while the reverse is 300.
    // The walk also covers `size`/`requiredSize`/`fillMax*`, so a `wrapContent` relaxes the range
    // for whatever it wraps, wherever the chain spells it.
    //
    //   `Size` — `size`/`width`/`height` (enforceIncoming): the requested axis is clamped into the
    //   current range and then fixes both bounds. `RequiredSize` writes over the range.
    //   `Fill` — Compose's `FillNode`: a bounded maximum becomes `max * fraction` (rounded).
    //   `MinWidth`/`MaxWidth` — `widthIn(min = …)` / `widthIn(max = …)`.
    //   `SizeIn` — ONE node, both bounds against the same input range; `enforce_incoming` separates
    //   `sizeIn` from `required*In` (`Size.kt:846-888`).
    //   `DefaultMin` — `defaultMinSize`, only where the incoming minimum is 0.
    //   `Wrap` — `wrapContent*`: relax the minimum (and maximum when unbounded) and remember the
    //   range for the post-measure wrapper.
    use crate::modifier::SizeStep;
    struct WrapStep {
        align: crate::layout::ContentAlignment,
        /// Which axes the wrap node actually aligns. `wrap_content_width` fixes the horizontal
        /// alignment and leaves the vertical offset at 0 (Compose's alignmentCallback returns
        /// `IntOffset(align.align(...), 0)`); the height form mirrors it.
        width: bool,
        height: bool,
        min_width: f32,
        min_height: f32,
        max_width: f32,
        max_height: f32,
    }
    let mut wrap_steps: Vec<WrapStep> = Vec::new();
    // The constraints an `width/height(IntrinsicSize)` request saw at its chain position. A fill
    // OUTSIDE the request has already tightened them (the request honours them); a fill INSIDE has
    // not (the request decides first, the fill then fills the tight result).
    let mut intrinsic_incoming_width: Option<(f32, f32)> = None;
    let mut intrinsic_incoming_height: Option<(f32, f32)> = None;
    for step in nodes[idx].modifier.size_steps() {
        match step {
            SizeStep::Size { width, height } => {
                // Compose SizeNode (enforceIncoming): the request is clamped into the current
                // range and then fixes it.
                if let Some(v) = width {
                    let w = inner_constraints.constrain_width(v);
                    inner_constraints = inner_constraints.tighten_width(w);
                }
                if let Some(v) = height {
                    let h = inner_constraints.constrain_height(v);
                    inner_constraints = inner_constraints.tighten_height(h);
                }
            }
            SizeStep::RequiredSize { width, height } => {
                // enforceIncoming = false: the numeric axes write over the range.
                if let Some(v) = width {
                    inner_constraints.min_width = v;
                    inner_constraints.max_width = v;
                }
                if let Some(v) = height {
                    inner_constraints.min_height = v;
                    inner_constraints.max_height = v;
                }
            }
            SizeStep::Fill { width, height } => {
                // Compose FillNode: a bounded maximum becomes a fixed size at max * fraction; an
                // unbounded maximum leaves the content alone.
                if let Some(fraction) = width {
                    if inner_constraints.max_width < f32::MAX {
                        let w = (inner_constraints.max_width * fraction)
                            .round()
                            .clamp(inner_constraints.min_width, inner_constraints.max_width);
                        inner_constraints.min_width = w;
                        inner_constraints.max_width = w;
                    }
                }
                if let Some(fraction) = height {
                    if inner_constraints.max_height < f32::MAX {
                        let h = (inner_constraints.max_height * fraction)
                            .round()
                            .clamp(inner_constraints.min_height, inner_constraints.max_height);
                        inner_constraints.min_height = h;
                        inner_constraints.max_height = h;
                    }
                }
            }
            SizeStep::Intrinsic { width, height } => {
                // The request helper returns the OUTERMOST intrinsic element, so keep the FIRST
                // snapshot: a later (inner) request would otherwise be the one the clamp uses.
                if width && intrinsic_incoming_width.is_none() {
                    intrinsic_incoming_width =
                        Some((inner_constraints.min_width, inner_constraints.max_width));
                }
                if height && intrinsic_incoming_height.is_none() {
                    intrinsic_incoming_height =
                        Some((inner_constraints.min_height, inner_constraints.max_height));
                }
            }
            SizeStep::MinWidth(v) => {
                inner_constraints.min_width =
                    v.clamp(inner_constraints.min_width, inner_constraints.max_width);
            }
            SizeStep::MaxWidth(v) => {
                inner_constraints.max_width =
                    v.clamp(inner_constraints.min_width, inner_constraints.max_width);
            }
            SizeStep::MinHeight(v) => {
                inner_constraints.min_height =
                    v.clamp(inner_constraints.min_height, inner_constraints.max_height);
            }
            SizeStep::MaxHeight(v) => {
                inner_constraints.max_height =
                    v.clamp(inner_constraints.min_height, inner_constraints.max_height);
            }
            SizeStep::SizeIn {
                min_width,
                min_height,
                max_width,
                max_height,
                enforce_incoming,
            } => {
                // Compose resolves a range into a target Constraints first: a specified max is
                // itself, an unspecified one is Infinity; a specified min is clamped into [0, max]
                // (`Size.kt:808-844`).
                // A negative max is coerced to 0 first (Compose's `fastCoerceAtLeast(0)`); feeding
                // it straight into `clamp` would panic on its `min <= max` assertion during measure.
                let target_max_w = max_width.unwrap_or(f32::MAX).max(0.0);
                let target_min_w = min_width.map(|v| v.clamp(0.0, target_max_w)).unwrap_or(0.0);
                let target_max_h = max_height.unwrap_or(f32::MAX).max(0.0);
                let target_min_h = min_height.map(|v| v.clamp(0.0, target_max_h)).unwrap_or(0.0);
                let (lo_w, hi_w) = (inner_constraints.min_width, inner_constraints.max_width);
                let (lo_h, hi_h) = (inner_constraints.min_height, inner_constraints.max_height);
                if enforce_incoming {
                    // `constraints.constrain(target)`: the target range intersected with the
                    // incoming one — the incoming range always wins where it is tighter.
                    inner_constraints.min_width = target_min_w.clamp(lo_w, hi_w);
                    inner_constraints.max_width = target_max_w.clamp(lo_w, hi_w);
                    inner_constraints.min_height = target_min_h.clamp(lo_h, hi_h);
                    inner_constraints.max_height = target_max_h.clamp(lo_h, hi_h);
                } else {
                    // `required*In`: a specified bound ignores incoming; the other end still
                    // respects it (`Size.kt:854-883`).
                    inner_constraints.min_width = if min_width.is_some() {
                        target_min_w
                    } else {
                        lo_w.min(target_max_w)
                    };
                    inner_constraints.max_width = if max_width.is_some() {
                        target_max_w
                    } else {
                        hi_w.max(target_min_w)
                    };
                    inner_constraints.min_height = if min_height.is_some() {
                        target_min_h
                    } else {
                        lo_h.min(target_max_h)
                    };
                    inner_constraints.max_height = if max_height.is_some() {
                        target_max_h
                    } else {
                        hi_h.max(target_min_h)
                    };
                }
            }
            SizeStep::DefaultMin {
                min_width,
                min_height,
            } => {
                if let Some(v) = min_width {
                    if inner_constraints.min_width == 0.0 {
                        inner_constraints.min_width = v.clamp(0.0, inner_constraints.max_width);
                    }
                }
                if let Some(v) = min_height {
                    if inner_constraints.min_height == 0.0 {
                        inner_constraints.min_height = v.clamp(0.0, inner_constraints.max_height);
                    }
                }
            }
            SizeStep::Wrap {
                width,
                height,
                align,
                unbounded,
            } => {
                wrap_steps.push(WrapStep {
                    align,
                    width,
                    height,
                    min_width: inner_constraints.min_width,
                    min_height: inner_constraints.min_height,
                    max_width: inner_constraints.max_width,
                    max_height: inner_constraints.max_height,
                });
                if width {
                    inner_constraints.min_width = 0.0;
                    if unbounded {
                        inner_constraints.max_width = f32::MAX;
                    }
                }
                if height {
                    inner_constraints.min_height = 0.0;
                    if unbounded {
                        inner_constraints.max_height = f32::MAX;
                    }
                }
            }
        }
    }

    // 开放布局节点 A 型（exp/modifier-node）：在尺寸步骤之后串行变换约束。
    // 位置语义 = 链中 size/min/max/fill/wrap 之后，intrinsic/padding 之前；动态值在
    // transform 内求值（measure 期 State::get 注册布局依赖——与 Dimension::Dynamic 同机制）。
    // Arc 克隆出链表避免借用冲突（nodes[idx] 不可变借用与后续可变写冲突）。
    // P2-2：无 node 时早退（全树每节点每次 measure 省一次 collect 分配）。
    let layout_transforms: Vec<std::sync::Arc<dyn crate::modifier::LayoutModifierNode>> =
        if nodes[idx].modifier.has_layout_nodes() {
            nodes[idx].modifier.layout_nodes().cloned().collect()
        } else {
            Vec::new()
        };
    for t in &layout_transforms {
        inner_constraints = t.transform(inner_constraints);
    }

    // Step 1 (a fixed `Size`) used to be applied here as well, reading `Modifier::fixed_size()`.
    // It was redundant — `resolved_size` above already turns every `Static(Fixed)`/`Static(Dp)`/
    // `Static(Px)` axis into a number and tightens with it, `fixed_size` only reports the subset
    // where BOTH axes are static, and merging is a superset of "the first element with both static"
    // — but it was not harmless: it re-wrote the RAW request after the clamp, so a `.size(100, 600)`
    // in a 470 dp slot came out at 600 again and undid the alignment with Compose. Removed rather
    // than duplicated so there is one place that decides a node's size from its modifier.

    // 1.5 固有尺寸请求（`Modifier.width/height(IntrinsicSize)`）——该轴取内容自己的固有测量。
    // Compose 由 `IntrinsicWidthNode.calculateContentConstraints` 做这件事：先算出内容在该轴上的
    // 固有尺寸，把约束收紧到它；`enforceIncoming`（`size/width/height` 形式）为真时 incoming 仍
    // 是最终夹取者，`requiredWidth/Height(IntrinsicSize)` 为假（忽略 incoming，同 requiredSize）。
    // 位置必须在 padding offset **之前**：管线是"先定节点尺寸，再把 padding 偏移进内层"，
    // 所以这里收紧的是节点自身（含 padding）的尺寸。
    if let Some((size, enforce_incoming)) = nodes[idx].modifier.intrinsic_width_request() {
        // The constraints the request saw at ITS chain position decide the clamp: a fill outside
        // has already tightened them (Compose's IntrinsicWidthNode measures under those), while a
        // fill inside has not (FillNode forwards the intrinsic answer, so the request decides
        // first and the inner fill then fills the tight result).
        let other = inner_constraints.max_height;
        let mut w = intrinsic_size_of(nodes, policies, idx, IntrinsicQuery::for_width(size), other);
        if enforce_incoming {
            w = match intrinsic_incoming_width {
                Some((lo, hi)) => w.clamp(lo, hi),
                None => inner_constraints.constrain_width(w),
            };
        }
        inner_constraints = inner_constraints.tighten_width(w);
    }
    if let Some((size, enforce_incoming)) = nodes[idx].modifier.intrinsic_height_request() {
        let other = inner_constraints.max_width;
        let mut h = intrinsic_size_of(nodes, policies, idx, IntrinsicQuery::for_height(size), other);
        if enforce_incoming {
            h = match intrinsic_incoming_height {
                Some((lo, hi)) => h.clamp(lo, hi),
                None => inner_constraints.constrain_height(h),
            };
        }
        inner_constraints = inner_constraints.tighten_height(h);
    }

    // 2. 应用 padding
    let (mut pad_start, mut pad_end) = nodes[idx].modifier.get_padding_horizontal();
    let (mut pad_top, mut pad_bottom) = nodes[idx].modifier.get_padding_vertical();
    // Window-inset padding is resolved HERE, at measure time: the controller's state read registers a
    // layout dependency (so an inset change re-measures just these nodes), and the insets it applies
    // are consumed for the subtree. Consume-only elements mark them without padding.
    let mut inset_consumed = crate::insets::consumed();
    for (kind, controller) in nodes[idx].modifier.inset_padding_steps() {
        let window = controller.state().get();
        let value = crate::insets::remaining(kind, window, inset_consumed);
        pad_start += value.left;
        pad_end += value.right;
        pad_top += value.top;
        pad_bottom += value.bottom;
        inset_consumed = crate::insets::consume(kind, window, inset_consumed);
    }
    for (kind, controller) in nodes[idx].modifier.inset_consume_steps() {
        let window = controller.state().get();
        inset_consumed = crate::insets::consume(kind, window, inset_consumed);
    }
    let _inset_guard = crate::insets::ConsumeScope::enter(inset_consumed);
    let pad_x = pad_start + pad_end;
    let pad_y = pad_top + pad_bottom;
    if pad_x > 0.0 || pad_y > 0.0 {
        inner_constraints = inner_constraints.offset(pad_x, pad_y);
    }

    // 2b. `Modifier::paddingFrom` relaxes the MINIMUM on its line's axis before the content is
    // measured, the way Compose measures its child with `constraints.copy(minHeight = 0)`
    // (`foundation/layout/AlignmentLine.kt:311-315`). Without it a min height handed down from above
    // is already inside the content's own size, and the padding is then added on top of it — measured:
    // a text under `min_height = 100` with `padding_from_baseline(Some(40))` came out 122.73 tall
    // instead of 100, because the 100 was counted as the content.
    //
    // The minimum is not dropped: the padding step below puts it back, and the content still sits at
    // `before` inside the taller box.
    let mut pad_from_min_width = inner_constraints.min_width;
    let mut pad_from_min_height = inner_constraints.min_height;
    for (line, _, _) in nodes[idx].modifier.get_padding_from() {
        if line.horizontal {
            pad_from_min_height = pad_from_min_height.max(inner_constraints.min_height);
            inner_constraints.min_height = 0.0;
        } else {
            pad_from_min_width = pad_from_min_width.max(inner_constraints.min_width);
            inner_constraints.min_width = 0.0;
        }
    }

    // 3. Save the viewport before the scroll step rewrites the maximum. `fillMax*` has already
    // been applied in the size walk above, at its chain position (`FillNode`, `Size.kt:713-746`).
    let viewport_height = inner_constraints.max_height;
    let viewport_width = inner_constraints.max_width;

    // 4. 检查 scroll 修饰符——给子节点无限约束
    if nodes[idx].modifier.vertical_scroll_state().is_some() {
        // scroll 容器自身填 viewport（fill_max_height 在无限 max 时跳过，这里补上）
        if nodes[idx].modifier.is_fill_max_height() && inner_constraints.max_height >= f32::MAX {
            inner_constraints.min_height = viewport_height;
        }
        // 保存 viewport 高度供滚动 clamping 使用
        nodes[idx].scroll_viewport_height = viewport_height;
        // lazy 列表：内容总高 State → 节点字段（apply_scroll_delta 用；在测量前读，
        // 值是上一帧测量回写的——首帧 0 退化到节点自身高度）
        if let Some(ch) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_height = ch.get();
            nodes[idx].scroll_reverse = nodes[idx].modifier.is_lazy_scroll_reverse();
        }
        // ⚠ lazy 容器**不**改写成无界：policy 自己显式控制子约束（child_constraints
        // 高度 ∞），保留有限 max_height 让 policy 拿到真实视口高——内部 clamp 的
        // max_offset = content_h - vh 才正确（实测：回退 vh=600 > 真实视口 ~500，
        // 跳末尾滚过头，Item 999 被推出视口底部）。普通 scroll 容器仍改写（子内容
        // wrap）。
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_height = f32::MAX;
        }
    }
    if nodes[idx].modifier.horizontal_scroll_state().is_some() {
        // scroll 容器自身填 viewport（fill_max_width 在无限 max 时跳过，这里补上）
        if nodes[idx].modifier.is_fill_max_width() && inner_constraints.max_width >= f32::MAX {
            inner_constraints.min_width = viewport_width;
        }
        // 保存 viewport 宽度供滚动 clamping 使用
        nodes[idx].scroll_viewport_width = viewport_width;
        // 水平反向滚动（RTL：render 平移镜像——offset 0 显示内容末端，对齐垂直 reverseLayout；
        // LazyRow 的 reverse_layout 打 lazy_scroll_reverse 标记（与垂直同构），
        // 此处同样认领——否则水平反向有 placement 镜像、无 render 镜像，
        // offset 两端语义不一致。非懒水平容器无 LazyScroll 元素，|| 安全。
        nodes[idx].scroll_reverse = nodes[idx].modifier.is_horizontal_scroll_reverse()
            || nodes[idx].modifier.is_lazy_scroll_reverse();
        // lazy 横向列表：内容总宽 State → 节点字段（与垂直同语义）
        if let Some(cw) = nodes[idx].modifier.lazy_scroll_content_height() {
            nodes[idx].scroll_content_width = cw.get();
        }
        // ⚠ lazy 容器不改写成无界：保留有限 max_width 让 policy 拿真实视口宽
        if nodes[idx].modifier.lazy_scroll_content_height().is_none() {
            inner_constraints.max_width = f32::MAX;
        }
    }

    // 实际测量
    // RemeasureToBounds: the child layout is re-measured with FIXED constraints
    // of the animated bounds, so content reflows (text rewraps, rows resize)
    // instead of being graphically scaled. Applied last — Compose's "animated
    // fixed constraints" override the incoming and modifier constraints alike.
    if let Some(fm) = flight {
        if let Some(c) = fm.content {
            inner_constraints = Constraints::new(c.width, c.width, c.height, c.height);
            // A scroll container reads its viewport from the constraints it was
            // measured with, so a re-measured one must re-derive them — render
            // clips and hit clamping both use these, and they would otherwise
            // stay on the natural size while the box animates.
            if nodes[idx].modifier.vertical_scroll_state().is_some() {
                nodes[idx].scroll_viewport_height = c.height;
            }
            if nodes[idx].modifier.horizontal_scroll_state().is_some() {
                nodes[idx].scroll_viewport_width = c.width;
            }
        }
    }

    let mut result = if let Some(pidx) = nodes[idx].measure_policy {
        // 先拷贝子节点索引（policy.measure 会可变借用整个 nodes，不能持有 nodes[idx] 借用）
        //
        // The ADOPTED subcomposition child is left out: this policy did not create it (a subcomposing
        // component appends it during adoption), and a policy that counts its children read one too
        // many. Measured on `TabRow`: `tab_count = children.len() - 2` became 4 for a three-tab row, so
        // each tab measured 488/4 = 122 instead of 488/3 ≈ 162.7, and the indicator was placed with the
        // four-tab geometry (`node_pos=(287.5,44)` where the three-tab one wants 389.2).
        let mut children = nodes[idx].children.clone();
        if let Some(sub) = nodes[idx].subcomposed_child {
            children.retain(|&c| c != sub);
        }
        let (size, placements) = policies[pidx].measure(nodes, policies, &children, inner_constraints);
        // 测量后同步 lazy 内容总高/宽到节点字段（render 的 reverse translate 与
        // apply_scroll_delta 依赖；measure_node 顶部读的是上一帧值——首帧为 0
        // 会让 reverseLayout 首帧平移错误，内容整体被推出视口）
        let content_h_state = nodes[idx].modifier.lazy_scroll_content_height().map(|s| s.get());
        if let Some(v) = content_h_state {
            nodes[idx].scroll_content_height = v;
            nodes[idx].scroll_content_width = v;
        }
        // apply positions
        policies[pidx].place(nodes, &children, &placements);
        // ── Alignment-line inheritance ──
        //
        // A container reports the lines its children report, shifted into its own coordinates, with
        // each line's own merger combining them — Compose's rule (`ui/layout/AlignmentLine.kt:60-66`:
        // the position of a line within a layout is the merger applied over the children's values).
        // This is what lets a Row align by a baseline that lives inside a child: without it only a
        // DIRECT child's own line is visible, so `align_by_baseline` on a Column wrapping text found
        // nothing and fell back to the child's top edge.
        //
        // Done here rather than in every policy because this is the one point that has both the
        // children's lines (set during their own measure) and their positions (just placed), and a
        // policy does not even know its own node index.
        //
        // A policy may decide the set itself — Compose's `layout(alignmentLines = …)`: `Some` replaces the
        // inherited lines entirely (that is how an overlay child is kept out of its host's baseline), and
        // `None` leaves the merge below in charge.
        // A leaf's own line is untouched: it has no children, so the merge reports nothing for it.
        if let Some(lines) = policies[pidx].alignment_lines(nodes, &children) {
            for (line, value) in lines {
                nodes[idx].set_alignment_line(line, value);
            }
        } else {
            for child in &children {
                // Only the lines the child actually reports; a child that reports none contributes none.
                // `alignment_lines` is a small Vec (usually empty), so this is a cheap nested loop.
                let child_position = nodes[*child].position;
                let child_lines = nodes[*child].alignment_lines.clone();
                for (line, value) in child_lines {
                    // A horizontal line runs ACROSS the horizontal axis, so its position is a vertical
                    // offset from this node's top; a vertical line's position is horizontal.
                    let absolute = if line.horizontal {
                        child_position.y + value
                    } else {
                        child_position.x + value
                    };
                    // MERGE, not overwrite: two children reporting the same line combine through the
                    // line's own merger (`::min` for the first baseline of a block, `::max` for the
                    // last), which is what Compose's `merge` does.
                    let merged = match nodes[idx].alignment_line(line) {
                        Some(previous) => line.merge(previous, absolute),
                        None => absolute,
                    };
                    nodes[idx].set_alignment_line(line, merged);
                }
            }
        }
        // Record whether any child asks for a paint order of its own (`Modifier::z_index`). The
        // renderer and the hit test consult this before doing any ordering work, so the common case
        // (nobody sets a z) pays nothing for the feature existing.
        nodes[idx].children_have_z = children
            .iter()
            .any(|&c| nodes[c].modifier.get_z_index().is_some());
        // apply padding offset（RTL：start 在右——子靠右偏移）
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        let left_offset = if rtl { pad_end } else { pad_start };
        if left_offset > 0.0 || pad_top > 0.0 {
            for &c in &children {
                nodes[c].position.x += left_offset;
                nodes[c].position.y += pad_top;
            }
        }
        // apply per-child offset modifier（普通 offset 在 RTL 下 x 镜像——
        // 对标 Compose；absolute_offset 豁免镜像）
        let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
        for &c in &children {
            if let Some((ox, oy)) = nodes[c].modifier.get_offset() {
                nodes[c].position.x += if rtl { -ox } else { ox };
                nodes[c].position.y += oy;
            }
            if let Some((ax, ay)) = nodes[c].modifier.get_absolute_offset() {
                nodes[c].position.x += ax;
                nodes[c].position.y += ay;
            }
        }
        // 非 lazy 垂直滚动容器：内容总高 = 子节点底部最大值（含底部 padding）——
        // 供 apply_scroll_delta 的 max_offset 与 fling 极限。修复 fill 容器场景：
        // 自身高度 = 视口 → 原 measured_size 高度算 max_offset 恒 0（无法滚动）
        if nodes[idx].modifier.vertical_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_h = 0.0f32;
            for &c in &children {
                let b = nodes[c].position.y + nodes[c].measured_size.height;
                if b > content_h { content_h = b; }
            }
            if content_h > 0.0 {
                content_h += pad_bottom;
                nodes[idx].scroll_content_height = content_h;
                // The scroll range ends when the CONTENT's bottom meets the container's own bottom, so
                // the container's height is what belongs here — not the padding-deducted viewport
                // constraint. `content_h` counts the content from the container's top edge (its children
                // sit below `pad_top`), so mixing the two double-counted the vertical padding and let a
                // menu scroll 2*padding past its end: measured with a 112x424 menu holding 20 items,
                // `content_h` was 976 against a viewport of 424-16 = 408, giving an offset of 568 where
                // 552 is the end — 16px of dead space under the last item (24 in total with its own 8dp
                // padding).
                let node_h = (size.height + pad_y).min(viewport_height + pad_y);
                let max_off = (content_h - node_h).max(0.0);
                if let Some(ss) = nodes[idx].modifier.vertical_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        // 非 lazy 水平滚动容器：内容总宽 = 子节点右侧最大值（含尾部 padding）
        if nodes[idx].modifier.horizontal_scroll_state().is_some()
            && nodes[idx].modifier.lazy_scroll_content_height().is_none()
        {
            let mut content_w = 0.0f32;
            for &c in &children {
                let right = nodes[c].position.x + nodes[c].measured_size.width;
                if right > content_w { content_w = right; }
            }
            if content_w > 0.0 {
                content_w += pad_end;
                nodes[idx].scroll_content_width = content_w;
                // Same shape as the vertical case above: the range ends at the container's own edge.
                let node_w = (size.width + pad_x).min(viewport_width + pad_x);
                let max_off = (content_w - node_w).max(0.0);
                if let Some(ss) = nodes[idx].modifier.horizontal_scroll_state() {
                    ss.fling_limit.set(max_off);
                }
            }
        }
        let mut outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        // scroll 容器自身尺寸 clamp 回视口——第 4 步为子节点把约束改成了无界
        // （max=f32::MAX），若不做 clamp，自身高度/宽度会被内容撑开（如
        // `.height(180.0)` 的滚动列表实测 991 高）。viewport_* 保存于第 3 步
        // （padding 已扣、size 约束已收窄），故完整视口高 = viewport + pad。
        // lazy 容器不改写约束（见第 4 步注释），其 policy 已按有限视口测量，
        // 此处 clamp 对它们是无操作（内容本来就 ≤ 视口）。若父约束无限
        // （viewport=f32::MAX），min 无效果，仍按内容撑开（语义：无固定高度
        // 的滚动容器随内容增长）。
        if nodes[idx].modifier.vertical_scroll_state().is_some() && viewport_height < f32::MAX {
            outer_size.height = outer_size.height.min(viewport_height + pad_y);
        }
        if nodes[idx].modifier.horizontal_scroll_state().is_some() && viewport_width < f32::MAX {
            outer_size.width = outer_size.width.min(viewport_width + pad_x);
        }
        nodes[idx].measured_size = outer_size;
        (outer_size, placements)
    } else {
        // 叶子节点：使用 ContentMeasurer 或默认逻辑
        let size = if nodes[idx].has_text_content {
            // 合并的 measure + cache（避免重复创建 Paragraph）
            // 使用父约束的 max_width 作为排版宽度，确保文本在可用空间内自动换行。
            // 对于可滚动容器，inner_constraints.max_width 已被设为 f32::MAX。
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_text(&nodes[idx], layout_width);
            // A text leaf is what makes `Modifier::align_by_baseline` useful: it reports its text
            // baselines, measured from the node's own top, exactly as Compose's text does. Taken from
            // the paragraph just cached, so they are the baselines of the layout the size came from.
            //
            // Both of Compose's baselines: `FirstBaseline` is the paragraph's own
            // `alphabetic_baseline`, and `LastBaseline` is the last line's (`::max`, the line to align
            // by when the bottom of a wrapped block is what should line up).
            let (first, last) = {
                let paragraph = nodes[idx].cached_paragraph.borrow();
                match paragraph.as_ref() {
                    Some(para) => {
                        let first = para.alphabetic_baseline() as f32;
                        let last = para
                            .get_line_metrics()
                            .last()
                            .map(|m| m.baseline as f32)
                            .unwrap_or(first);
                        (Some(first), Some(last))
                    }
                    None => (None, None),
                }
            };
            if let Some(first) = first {
                nodes[idx].set_alignment_line(crate::layout::AlignmentLine::FIRST_BASELINE, first);
            }
            if let Some(last) = last {
                nodes[idx].set_alignment_line(crate::layout::AlignmentLine::LAST_BASELINE, last);
            }
            // 支持文本（TextField supporting——渲染画在容器底部外 4dp，
            // 高度 +20 预留，防与下方元素重叠）
            let supporting_h = supporting_text_height(&nodes[idx]);
            // 用约束 clamping 最终尺寸（fill_max_width 时约束收紧，文本应填满可用宽度）。
            // ⚠ 高度 = paragraph 实际高度（含自动折行）+ supporting——不得按
            // 显式换行数近似（折行文本高度会裁剪）；min_height 提升兜底占位
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height + supporting_h),
            )
        } else if nodes[idx].has_richtext_content {
            let layout_width = inner_constraints.max_width;
            let text_size = measure_and_cache_richtext(&nodes[idx], layout_width);
            Size::new(
                inner_constraints.constrain_width(text_size.width),
                inner_constraints.constrain_height(text_size.height),
            )
        } else if nodes[idx].has_image_content {
            // Image 叶子：固有尺寸（位图像素 / SVG viewBox），约束钳制
            let (iw, ih) = nodes[idx].modifier.image_intrinsic_size().unwrap_or((0.0, 0.0));
            Size::new(
                inner_constraints.constrain_width(iw),
                inner_constraints.constrain_height(ih),
            )
        } else {
            // 普通叶子节点
            let w = inner_constraints.constrain_width(
                if inner_constraints.has_fixed_width() {
                    inner_constraints.min_width
                } else {
                    0.0
                },
            );
            let h = inner_constraints.constrain_height(
                if inner_constraints.has_fixed_height() {
                    inner_constraints.min_height
                } else {
                    0.0
                },
            );
            Size::new(w, h)
        };

        nodes[idx].measured_size = size;
        // 叶子尺寸回加 padding（容器路径 1279 同语义——叶子此前丢失：
        // 测量尺寸不含内边距 → Text+padding 高度塌陷、内容贴边）
        let outer_size = Size::new(size.width + pad_x, size.height + pad_y);
        nodes[idx].measured_size = outer_size;
        (outer_size, Vec::new())
    };

    // ── `Modifier::wrapContent*` ──
    //
    // The content was measured without the incoming minimum (and, when unbounded, without the
    // maximum). Compose's `WrapContentNode` then reports the content coerced back into the incoming
    // range and places it inside that box by `align` (`Size.kt:1024-1061`). Steps apply from the
    // innermost out — the last one recorded wraps everything measured before it.
    for wrap in wrap_steps.iter().rev() {
        let content = result.0;
        let wrapper = Size::new(
            content.width.clamp(wrap.min_width, wrap.max_width),
            content.height.clamp(wrap.min_height, wrap.max_height),
        );
        let (dx, dy) = wrap_alignment_offset(
            wrap.align,
            content,
            wrapper,
            nodes[idx].layout_direction,
            wrap.width,
            wrap.height,
        );
        if dx != 0.0 || dy != 0.0 {
            if nodes[idx].children.is_empty() {
                let (pad_s, pad_t, pad_e, pad_b) = nodes[idx].modifier.get_padding_sides();
                let rtl = nodes[idx].layout_direction == LayoutDirection::Rtl;
                let left = if rtl { pad_e } else { pad_s };
                nodes[idx].content_box_override = Some((
                    left + dx,
                    pad_t + dy,
                    (content.width - pad_s - pad_e).max(0.0),
                    (content.height - pad_t - pad_b).max(0.0),
                ));
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.x += dx;
                    nodes[c].position.y += dy;
                }
            }
            for (line, value) in nodes[idx].alignment_lines.iter_mut() {
                *value += if line.horizontal { dy } else { dx };
            }
        }
        result.0 = wrapper;
        nodes[idx].measured_size = wrapper;
    }

    // ── Caller-declared alignment lines (`Modifier::alignment_line`) ──
    //
    // Published BEFORE the `paddingFrom` step below, because that step measures against the lines a
    // node reports — including one declared here. Compose declares them as the layout is produced
    // (`layout(w, h, alignmentLines = mapOf(…))`); a `MeasurePolicy` in winia is never told its own
    // node index, so the declaration travels on the modifier and is evaluated here, where the
    // measured size exists.
    for (line, value_of) in nodes[idx].modifier.get_alignment_line_values() {
        let value = value_of(result.0);
        nodes[idx].set_alignment_line(line, value);
    }

    // ── `Modifier::paddingFrom` ──
    //
    // Runs here, after the content is measured, because it is the content's own alignment line that
    // says how much padding the node needs — Compose's `alignmentLineOffsetMeasure`
    // (`foundation/layout/AlignmentLine.kt:304-358`):
    //
    //   linePosition = the content's line (0 when it reports none)
    //   paddingBefore = (before - line).coerceIn(0, axisMax - axis)          // 0 when unspecified
    //   paddingAfter  = (after - axis + line).coerceIn(0, axisMax - axis - paddingBefore)
    //   size on the line's axis = max(paddingBefore + axis + paddingAfter, min)
    //   content placed at paddingBefore when `before` is given, else at size - paddingAfter - axis
    //
    // `before` therefore wins when the two cannot both fit, and the padding never pushes the node past
    // the incoming maximum on that axis. The other axis is untouched.
    //
    // Two elements can be in force at once (`padding_from_baseline(top, bottom)` is one for the first
    // baseline and one for the last), and each sees what the one before it did — the same nesting
    // Compose gets from stacking two layout nodes. So the running size, the content's box and the
    // node's own lines all move along with the padding, which is what lets the second element read a
    // line the first has already shifted, and an outer `align_by` see the padded position.
    let mut leaf_box: Option<(f32, f32, f32, f32)> = None;
    for (line, before, after) in nodes[idx].modifier.get_padding_from() {
        let horizontal = line.horizontal;
        let size = result.0;
        let axis = if horizontal { size.height } else { size.width };
        let axis_max = if horizontal { inner_constraints.max_height } else { inner_constraints.max_width };
        // The minimum the padding must honour — the one from BEFORE the relaxation above, since the
        // content was measured without it.
        let min_axis = if horizontal { pad_from_min_height } else { pad_from_min_width };
        let line_position = nodes[idx].alignment_line(line).unwrap_or(0.0);
        let padding_before = (before.unwrap_or(0.0) - line_position).clamp(0.0, (axis_max - axis).max(0.0));
        let padding_after = (after.unwrap_or(0.0) - axis + line_position)
            .clamp(0.0, (axis_max - axis - padding_before).max(0.0));
        let grown = (padding_before + axis + padding_after).max(min_axis).max(axis);
        let placed_at = if before.is_some() {
            padding_before
        } else {
            grown - padding_after - axis
        };

        // The content's box inside this node, tracked across the elements. A leaf's content is
        // whatever it draws itself, inset by its `padding`; a container's is its children, which move.
        if leaf_box.is_none() && nodes[idx].children.is_empty() {
            let (pad_s, pad_t, pad_e, pad_b) = nodes[idx].modifier.get_padding_sides();
            leaf_box = Some((
                if nodes[idx].layout_direction == LayoutDirection::Rtl { pad_e } else { pad_s },
                pad_t,
                (size.width - pad_s - pad_e).max(0.0),
                (size.height - pad_t - pad_b).max(0.0),
            ));
        }
        if horizontal {
            result.0 = Size::new(size.width, grown);
            if let Some((ix, iy, iw, ih)) = leaf_box.as_mut() {
                let _ = (ix, iw, ih);
                *iy += placed_at;
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.y += placed_at;
                }
            }
            for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                if l.horizontal {
                    *value += placed_at;
                }
            }
        } else {
            result.0 = Size::new(grown, size.height);
            if let Some((ix, _iy, _iw, _ih)) = leaf_box.as_mut() {
                *ix += placed_at;
            } else {
                let kids = nodes[idx].children.clone();
                for c in kids {
                    nodes[c].position.x += placed_at;
                }
            }
            for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                if !l.horizontal {
                    *value += placed_at;
                }
            }
        }
        nodes[idx].measured_size = result.0;
    }
    if leaf_box.is_some() {
        // The last element wins: it is the outermost, so its box is where the content ended up.
        let (ix, iy, iw, ih) = leaf_box.unwrap();
        nodes[idx].content_box_override = Some((ix, iy, iw, ih));
    }

    // ── `Modifier::minimum_interactive_size` ──
    //
    // Compose's `MinimumInteractiveModifierNode.measure` (`material3/InteractiveComponentSize.kt:98-140`):
    // the box is `max(content, sizePx)` on BOTH axes, the content is placed CENTRED inside it, and two
    // lines are published — where the VISUAL content begins — so a parent can align the small component
    // by its real edge instead of by its 48 dp touch target. `sizePx <= 0` means enforcement is off
    // (Compose's "unspecified or 0.dp"), and then nothing changes at all.
    //
    // Runs after `paddingFrom` because it wraps whatever is inside it, which is the padded content by
    // then — the same order the chain of two Compose nodes would give.
    if let Some(size_px) = nodes[idx].modifier.get_minimum_interactive_size() {
        if size_px > 0.0 {
            let content = result.0;
            let width = content.width.max(size_px);
            let height = content.height.max(size_px);
            let left = ((size_px - content.width) / 2.0).max(0.0);
            let top = ((size_px - content.height) / 2.0).max(0.0);
            nodes[idx].set_alignment_line(AlignmentLine::minimum_interactive_left(), left);
            nodes[idx].set_alignment_line(AlignmentLine::minimum_interactive_top(), top);
            // The paint reads THIS field, not the lines just above: a container merges its children's
            // alignment lines (see the field's comment), so an ancestor reading the line would inset its
            // own background. Only the node the modifier belongs to ever sets this.
            //
            // The CONTENT's own size, which is what the component declared — NOT `content`, which is
            // the MEASURED box and a parent may have forced it wider or narrower (a ButtonGroup
            // squeezing an item forces the node, and then `content` is the forced size, which painted
            // an icon button as a 48 x 40 OVAL). The declared size is the inner container: read the
            // chain's own `size(...)` when it is fixed, and fall back to the measured box when it is
            // not (a text-sized button has no fixed size and is not what this insets for).
            // Only a component with a DECLARED fixed size paints a box of its own inside the touch
            // target — an IconButton declares `size(40, 40)` and the 48 dp target is padding around
            // it. A component without one (a text Button) has no box to inset: its container IS the
            // node, so publishing a content box here would paint it without its padding and, worse,
            // freeze it: the row can widen the node, and a box derived from the content would not
            // follow (measured: section 4's first item grew 75 -> 86 and its container did not move).
            // Only a FULLY declared pair is a box this can paint: `Modifier::width()`/`height()`
            // fill the other axis with `Dimension::Auto`, `fixed_size()` matches any static pair,
            // and `to_logical_px()` answers 0.0 for `Auto` (and for `Fill`) — which would publish a
            // content box with a zero side, and a zero-height `paint_rect` paints nothing.
            let declared = nodes[idx]
                .modifier
                .fixed_size()
                .map(|(w, h)| (w.to_logical_px(), h.to_logical_px()));
            if let Some((content_w, content_h)) = declared {
                nodes[idx].minimum_interactive_content = Some((content_w, content_h));
            }
            // `minimum_interactive_painted` and `content_shift` are NOT cleared here: the clear at
            // the top of this measurement covers them, and it has to run for a node that carries no
            // minimum-interactive element at all. Clearing a second time here only read as if this
            // block owned it.
            {
                let cx = (width - content.width) / 2.0;
                let cy = (height - content.height) / 2.0;
                if let Some((ix, iy, _iw, _ih)) = leaf_box.as_mut() {
                    *ix += cx;
                    *iy += cy;
                    nodes[idx].content_box_override = Some((*ix, *iy, content.width, content.height));
                } else {
                    let kids = nodes[idx].children.clone();
                    for c in kids {
                        nodes[c].position.x += cx;
                        nodes[c].position.y += cy;
                    }
                }
                // The lines published above are the VISUAL content's position inside the bigger box, so
                // they do not move with the centring — but the lines inherited from the children do, and
                // so does a line a caller declared with `Modifier::alignment_line`, whose closure saw the
                // pre-enlargement size. Shifting them keeps both in the new coordinates, which is what lets
                // an outer `align_by` land on the visual edge.
                for (l, value) in nodes[idx].alignment_lines.iter_mut() {
                    let line = *l;
                    if line == AlignmentLine::minimum_interactive_left()
                        || line == AlignmentLine::minimum_interactive_top()
                    {
                        continue;
                    }
                    *value += if line.horizontal { cy } else { cx };
                }
                result.0 = Size::new(width, height);
                nodes[idx].measured_size = result.0;
            }
        }
    }

    // aspectRatio：测量后按 inner_constraints（含 size/required 链内收紧）
    // 推导节点尺寸（对标 Compose——aspect 的 incoming = 链中 aspect 位置的
    // 约束；固定一轴推另一轴 + clamp；内容按 inner_constraints 已排版，
    // 节点尺寸可能大于内容——留白正常）
    if let Some((ratio, match_height_first)) = nodes[idx].modifier.aspect_ratio_constraint() {
        let (min_w, max_w) = (inner_constraints.min_width, inner_constraints.max_width);
        let (min_h, max_h) = (inner_constraints.min_height, inner_constraints.max_height);
        let (w, h) = if match_height_first {
            if max_h < f32::MAX {
                let w = max_h * ratio;
                if w <= max_w { (w, max_h) } else { (max_w, max_w / ratio) }
            } else if max_w < f32::MAX {
                (max_w, max_w / ratio)
            } else {
                (result.0.width, result.0.height)
            }
        } else if max_w < f32::MAX {
            let h = max_w / ratio;
            if h <= max_h { (max_w, h) } else { (max_h * ratio, max_h) }
        } else if max_h < f32::MAX {
            (max_h * ratio, max_h)
        } else {
            (result.0.width, result.0.height)
        };
        let w = w.clamp(min_w, max_w);
        let h = h.clamp(min_h, max_h);
        result.0 = Size::new(w, h);
        nodes[idx].measured_size = result.0;
    }

    // 标记测量完成，缓存约束供下帧复用
    nodes[idx].dirty = false;
    nodes[idx].layout_dirty = false;
    nodes[idx].cached_constraints = Some(constraints);
    // 尺寸上报（对标 Compose onSizeChanged）——最终尺寸定型后回调
    // （常量折叠早退路径不经过这里——尺寸未变无需上报；元素内再去重）
    let (rw, rh) = (nodes[idx].measured_size.width, nodes[idx].measured_size.height);
    nodes[idx].modifier.report_measured_size(rw, rh);

    // Flight layout contract (Compose `PlaceHolderSize`): `reported` is the size
    // the PARENT observes — `AnimatedSize` hands it the animated size so siblings
    // reflow, `ContentSize`/`JumpCut` keep the natural size so the surrounding
    // layout holds still. This node's OWN box (paint, clip, hit, text rects) must
    // keep the size the content was really measured at, so it lives in
    // `flight_content_size` instead: parents legitimately re-clobber
    // `measured_size` with the size they placed us at (`place()`).
    nodes[idx].flight_content_size = if flight.is_some() { Some(result.0) } else { None };
    if let Some(reported) = flight.and_then(|f| f.reported) {
        nodes[idx].measured_size = reported;
        result.0 = reported;
    }
    result
}

/// 构建普通文本段落（测量与绘制共用——单一事实来源）。
///
/// 从 TextStyle 参数构造 skia Paragraph（含 max_lines/ellipsis/justify/字重/倾斜），
/// 并按 soft_wrap 决定布局宽度。测量期（node.rs）与绘制兜底（render.rs）都调此函数，
/// 避免两处独立构造导致样式不一致。
pub(crate) fn build_plain_paragraph(
    content: &str,
    font_size: f32,
    color: &crate::graphics::Color,
    font_weight: crate::text::FontWeight,
    font_style: crate::text::FontSlant,
    max_lines: usize,
    align: crate::text::TextAlign,
    overflow: crate::text::TextOverflow,
    soft_wrap: bool,
    letter_spacing: f32,
    line_height: Option<f32>,
    max_width: f32,
) -> crate::text::Paragraph {
    use skia_safe::textlayout::ParagraphStyle;

    let mut para_style = ParagraphStyle::new();

    // max_lines：限制行数
    if max_lines < usize::MAX {
        para_style.set_max_lines(max_lines);
    }

    // ellipsis overflow：超出时显示省略号
    if overflow == crate::text::TextOverflow::Ellipsis {
        para_style.set_ellipsis("\u{2026}");
    }

    // justify alignment
    if align == crate::text::TextAlign::Justify {
        para_style.set_text_align(skia_safe::textlayout::TextAlign::Justify);
    }

    // soft_wrap=false: 无限宽度排版，不换行
    let layout_width = if soft_wrap { max_width } else { f32::MAX };

    let mut text_style = skia_safe::textlayout::TextStyle::new();
    text_style.set_font_size(font_size);
    // IMPORTANT: 设置文字颜色（Skia TextStyle 默认白色，不设的话画在白色背景上不可见）
    text_style.set_color(skia_safe::Color::from_argb(color.a, color.r, color.g, color.b));
    // 字间距（对标 Compose TextStyle.letterSpacing——逻辑像素）
    if letter_spacing != 0.0 {
        text_style.set_letter_spacing(letter_spacing);
    }
    // 行高（对标 Compose TextStyle.lineHeight——固定 px；skia 是倍数语义，
    // set_height(multiplier) + set_height_override(true)——override 才强制
    // 行高生效（与 RichText 路径 node.rs:1284 一致，否则被字体默认行高覆盖））
    if let Some(lh) = line_height {
        if lh > 0.0 && font_size > 0.0 {
            text_style.set_height(lh / font_size);
            text_style.set_height_override(true);
        }
    }
    // 设置字重和倾斜
    if font_weight != crate::text::FontWeight::NORMAL || font_style != crate::text::FontSlant::Upright {
        use skia_safe::FontStyle;
        use crate::text::FontSlant;
        let slant = match font_style {
            FontSlant::Upright => skia_safe::font_style::Slant::Upright,
            FontSlant::Italic => skia_safe::font_style::Slant::Italic,
            FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
        };
        text_style.set_font_style(FontStyle::new(font_weight.value().into(), 5.into(), slant));
    }
    let fc = crate::text::font::get_font_collection();
    let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);
    builder.push_style(&text_style);
    builder.add_text(content);
    let mut para = builder.build();
    para.layout(layout_width);
    para
}

/// Text leaf: build the paragraph laid out at `max_width`, WITHOUT touching the render cache.
///
/// `measure_and_cache_text` and the intrinsic probes both go through this. A probe must not leave
/// a paragraph laid out at its own width in `cached_paragraph` — the renderer reads that cache,
/// and a probe runs with the intrinsic query's width, not the frame's.
pub(crate) fn build_text_paragraph(node: &LayoutNode, max_width: f32) -> Option<crate::text::Paragraph> {
    for el in node.modifier.elements() {
        if let ModifierElement::TextContent {
            content, font_size, color, font_weight, font_style, max_lines, align, overflow, soft_wrap,
            letter_spacing, line_height,
        } = el {
            return Some(build_plain_paragraph(
                content.as_str(),
                *font_size,
                color,
                *font_weight,
                *font_style,
                *max_lines,
                *align,
                *overflow,
                *soft_wrap,
                *letter_spacing,
                *line_height,
                max_width,
            ));
        }
    }
    None
}

/// Height reserved below a text leaf that carries TextField supporting text (the supporting line
/// is drawn 4dp below the container and is 16dp tall). Shared by the measure pass and the
/// intrinsic probes so both reserve the same box.
pub(crate) fn supporting_text_height(node: &LayoutNode) -> f32 {
    if node
        .modifier
        .elements()
        .iter()
        .any(|el| matches!(el, ModifierElement::TextFieldVisual { supporting: Some(_), .. }))
    {
        4.0 + 16.0
    } else {
        0.0
    }
}

/// 合并的文本测量 + Paragraph 缓存。
///
/// 从 TextContent modifier 中提取所有参数（font_size、max_lines、overflow、align），
/// 在 ParagraphStyle 上正确设置后一次创建 Paragraph，测量尺寸并缓存供渲染复用。
/// 消除旧代码中 `measurer.measure()` + `cache_text_paragraph()` 重复创建的开销。
pub(crate) fn measure_and_cache_text(node: &LayoutNode, max_width: f32) -> Size {
    let Some(para) = build_text_paragraph(node, max_width) else {
        return Size::ZERO;
    };

    let size = Size::new(
        para.max_intrinsic_width().ceil().min(max_width),
        para.height().ceil(),
    );

    *node.cached_paragraph.borrow_mut() = Some(para);
    size
}

/// 富文本测量 + 缓存（含内联 drawable）。
///
/// 从 RichTextContent modifier 中提取内容文本、内联元素列表和每段样式，
/// 使用 Skia ParagraphBuilder 构建带 U+FFFC 占位符的段落，
/// 对每个片段应用对应的样式后缓存 Paragraph 和 drawables 供渲染复用。
pub(crate) fn measure_and_cache_richtext(node: &LayoutNode, max_width: f32) -> Size {
    use skia_safe::textlayout::{ParagraphStyle, PlaceholderAlignment, TextBaseline};
    
    
    let fc = crate::text::font::get_font_collection();

    for el in node.modifier.elements() {
        if let ModifierElement::RichTextContent { content, drawables, drawable_ranges, spans } = el {
            let para_style = ParagraphStyle::new();
            let mut builder = crate::text::ParagraphBuilder::new(&para_style, &fc);

            // 按 span 迭代 + drawable 范围
            let chars: Vec<char> = content.chars().collect();
            let mut di = 0usize;
            let mut span_idx = 0usize;
            let total = chars.len();
            let mut ci = 0usize;
            while ci < total {
                // 是否在 drawable 范围内
                if di < drawable_ranges.len() && ci >= drawable_ranges[di].start && ci < drawable_ranges[di].end {
                    // 占位符（只对范围起点做一次 add_placeholder，跳过剩余字符）
                    if ci == drawable_ranges[di].start {
                        if let Some(s) = spans.iter().find(|s| s.start <= ci && s.end > ci) {
                            builder.push_style(&to_sktextstyle(s));
                        }
                        let d = drawables[di].clone();
                        builder.add_placeholder(" ", d, &Some((PlaceholderAlignment::Bottom, TextBaseline::Alphabetic, 0.0)));
                        if let Some(_s) = spans.iter().find(|s| s.start <= ci && s.end > ci) { builder.pop(); }
                    }
                    ci += 1;
                    if ci >= drawable_ranges[di].end { di += 1; }
                    continue;
                }

                // 文本：先查 span
                while span_idx < spans.len() && spans[span_idx].end <= ci { span_idx += 1; }
                if span_idx < spans.len() && spans[span_idx].start <= ci && ci < spans[span_idx].end {
                    let run_end = spans[span_idx].end.min(total);
                    let text: String = chars[ci..run_end].iter().collect();
                    if !text.is_empty() {
                        builder.push_style(&to_sktextstyle(&spans[span_idx]));
                        builder.add_text(&text);
                        builder.pop();
                    }
                    ci = run_end;
                    continue;
                }

                // 无样式文本
                builder.add_text(&chars[ci].to_string());
                ci += 1;
            }

            let mut para = builder.build();
            para.layout(max_width);
            let size = Size::new(
                para.max_intrinsic_width().ceil().min(max_width),
                para.height().ceil(),
            );
            *node.cached_paragraph.borrow_mut() = Some(para);
            return size;
        }
    }
    Size::ZERO
}

/// 将 RichSpanStyle 转为 Skia TextStyle
pub(crate) fn to_sktextstyle(s: &RichSpanStyle) -> SkTextStyle {
    let mut ts = SkTextStyle::new();
    ts.set_font_size(s.font_size);
    ts.set_color(skia_safe::Color::from_argb(s.color.a, s.color.r, s.color.g, s.color.b));

    // 字重/字型/字宽
    let slant = match s.font_style {
        FontSlant::Upright => skia_safe::font_style::Slant::Upright,
        FontSlant::Italic => skia_safe::font_style::Slant::Italic,
        FontSlant::Oblique => skia_safe::font_style::Slant::Oblique,
    };
    ts.set_font_style(SkFontStyle::new(s.font_weight.value().into(), s.font_width.into(), slant));

    // 装饰线
    let mut deco: skia_safe::textlayout::TextDecoration = skia_safe::textlayout::TextDecoration::default();
    if s.underline { deco |= skia_safe::textlayout::TextDecoration::UNDERLINE; }
    if s.overline { deco |= skia_safe::textlayout::TextDecoration::OVERLINE; }
    if s.strikethrough { deco |= skia_safe::textlayout::TextDecoration::LINE_THROUGH; }
    ts.set_decoration_type(deco);
    if let Some(c) = &s.decoration_color {
        ts.set_decoration_color(skia_safe::Color::from_argb(c.a, c.r, c.g, c.b));
    }
    if let Some(st) = s.decoration_style {
        use crate::text::DecoStyle;
        let sk = match st {
            DecoStyle::Solid => skia_safe::textlayout::TextDecorationStyle::Solid,
            DecoStyle::Double => skia_safe::textlayout::TextDecorationStyle::Double,
            DecoStyle::Dotted => skia_safe::textlayout::TextDecorationStyle::Dotted,
            DecoStyle::Dashed => skia_safe::textlayout::TextDecorationStyle::Dashed,
            DecoStyle::Wavy => skia_safe::textlayout::TextDecorationStyle::Wavy,
        };
        ts.set_decoration_style(sk);
    }
    if let Some(m) = s.decoration_mode {
        use crate::text::DecoMode;
        let sk = match m {
            DecoMode::Gaps => skia_safe::textlayout::TextDecorationMode::Gaps,
            DecoMode::Through => skia_safe::textlayout::TextDecorationMode::Through,
        };
        ts.set_decoration_mode(sk);
    }

    // 基线偏移（shift = font_size * multiplier）
    if s.baseline_shift != 0.0 {
        ts.set_baseline_shift(s.baseline_shift * s.font_size);
    }

    // 间距
    if s.letter_spacing != 0.0 { ts.set_letter_spacing(s.letter_spacing); }
    if s.word_spacing != 0.0 { ts.set_word_spacing(s.word_spacing); }
    if s.height_multiple != 0.0 { ts.set_height(s.height_multiple); ts.set_height_override(true); }
    if s.half_leading { ts.set_half_leading(true); }

    // 字体族
    if !s.font_families.is_empty() {
        ts.set_font_families(&s.font_families.iter().map(|s| s.as_str()).collect::<Vec<_>>());
    }

    // 渲染精度
    if let Some(e) = s.font_edging {
        use crate::text::FontEdge;
        let sk = match e {
            FontEdge::Alias => skia_safe::font::Edging::Alias,
            FontEdge::AntiAlias => skia_safe::font::Edging::AntiAlias,
            FontEdge::SubpixelAntiAlias => skia_safe::font::Edging::SubpixelAntiAlias,
        };
        ts.set_font_edging(sk);
    }
    if let Some(h) = s.font_hinting {
        use crate::text::FontHint;
        let sk = match h {
            FontHint::None => skia_safe::FontHinting::None,
            FontHint::Slight => skia_safe::FontHinting::Slight,
            FontHint::Normal => skia_safe::FontHinting::Normal,
            FontHint::Full => skia_safe::FontHinting::Full,
        };
        ts.set_font_hinting(sk);
    }
    if s.subpixel { ts.set_subpixel(true); }

    // 前景/背景
    if let Some(fg) = &s.foreground_color {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(fg.a, fg.r, fg.g, fg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_foreground_paint(&paint);
    }
    if let Some(bg) = &s.background {
        let mut paint = skia_safe::Paint::default();
        paint.set_color(skia_safe::Color::from_argb(bg.a, bg.r, bg.g, bg.b));
        paint.set_style(skia_safe::paint::Style::Fill);
        ts.set_background_paint(&paint);
    }

    // locale
    if let Some(loc) = &s.locale {
        ts.set_locale(loc);
    }

    ts
}

// ── 主轴间距计算（Column/Row 共用）──

/// 计算 Arrangement 的间距分配：返回 (元素间额外间距, 首/尾 leading offset)。
/// Column 垂直主轴 / Row 水平主轴——由调用方（measure_flex）确定主轴分量。
pub fn compute_spacing(
    arrangement: crate::layout::Arrangement,
    remaining: f32,
    gap_count: usize,
) -> (f32, f32) {
    match arrangement {
        crate::layout::Arrangement::Start => (0.0, 0.0),
        crate::layout::Arrangement::End => (0.0, remaining),
        crate::layout::Arrangement::Center => (0.0, remaining / 2.0),
        crate::layout::Arrangement::SpaceBetween => {
            if gap_count > 0 {
                (remaining / gap_count as f32, 0.0)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceAround => {
            if gap_count > 0 {
                let space = remaining / (gap_count + 1) as f32;
                (space, space / 2.0) // (元素间间距, 首/尾边缘间距 = 一半)
            } else {
                (0.0, remaining / 2.0)
            }
        }
        crate::layout::Arrangement::SpaceEvenly => {
            if gap_count > 0 {
                // n children leave n+1 equal gaps: one before the first child and one after the
                // last, on top of the n-1 between pairs. Dividing by `gap_count + 1` left the
                // trailing gap out — measured on the docked picker's month list, whose four 36 dp
                // rows came out at leading 47.75 / pitch 47.75 / trailing 0 in a 335 dp panel.
                let space = remaining / (gap_count + 2) as f32;
                (space, space) // 元素间与边缘间距相等
            } else {
                (0.0, remaining / 2.0)
            }
        }
    }
}


