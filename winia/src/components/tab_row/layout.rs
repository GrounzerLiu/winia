use super::*;

/// Manual, not derived: the indicator slot is a closure, which has no `Debug`.
impl std::fmt::Debug for TabRowLayoutPolicy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TabRowLayoutPolicy")
            .field("selected_tab_index", &self.selected_tab_index)
            .field("follow_content_size", &self.follow_content_size)
            .field("has_indicator_slot", &self.indicator_slot.is_some())
            .finish()
    }
}

impl MeasurePolicy for TabRowLayoutPolicy {
    /// The row can compose a caller-supplied indicator during measurement, so its node has to be
    /// re-measured rather than folded (see the trait method's contract).
    fn subcomposes(&self) -> bool {
        self.indicator_slot.is_some()
    }

    fn measure(
        &self,
        nodes: &mut Vec<LayoutNode>,
        policies: &[Box<dyn MeasurePolicy>],
        children: &[usize],
        constraints: Constraints,
    ) -> (Size, Vec<Placement>) {
        let n = children.len();
        let tab_count = if n >= 2 { n - 2 } else { 0 };
        let row_width = constraints.max_width;
        let is_rtl = self.direction == LayoutDirection::Rtl;

        // ⚠ 两段式依赖必须在 measure **最开头** get()（任何 measure_node 子调用
        // 之前）——此刻 ACTIVE_SLOT_KEY 仍是本节点（TabRow），依赖注册到 TabRow
        // 而非尾部的 divider/indicator 叶（对齐 navigation_bar.rs 注释：递归测量
        // 子节点后 key 会被改写）。动画帧 → TabRow 重测 → 指示条位置更新。
        self.offset_state.get();
        self.width_state.get();

        // 指示条高
        let indicator_h = ACTIVE_INDICATOR_HEIGHT;

        if tab_count == 0 {
            return (Size::new(row_width, 0.0), Vec::new());
        }

        let tab_width = row_width / tab_count as f32;

        // 1st pass: Compose's intrinsic fold (TabRow.kt:450-459). The row is as tall as the tallest
        // tab's MAX intrinsic height AT the width every tab gets, and a tab's content width is its
        // MAX intrinsic width AT that height — then clamped back into the slot. No tab is measured
        // here: `intrinsic_size_of` inverts each tab's own modifier chain, so a tab whose content
        // prices itself (a weighted child, a fixed-size slot) answers without a measurement pass.
        let mut tab_row_height = 0.0f32;
        let mut natural_widths = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let h = intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxHeight, tab_width,
            );
            if h > tab_row_height { tab_row_height = h; }
        }
        for i in 0..tab_count {
            natural_widths.push(intrinsic_size_of(
                nodes, policies, children[i], IntrinsicQuery::MaxWidth, tab_row_height,
            ));
        }

        // 2nd pass: tight 约束
        let mut placements = Vec::with_capacity(n);
        let mut positions = Vec::with_capacity(tab_count);
        for i in 0..tab_count {
            let tight = Constraints::new(tab_width, tab_width, tab_row_height, tab_row_height);
            let (size, _) = measure_node(nodes, policies, children[i], tight);
            let x = if is_rtl {
                row_width - (i as f32 + 1.0) * tab_width
            } else {
                i as f32 * tab_width
            };
            let content_width = (natural_widths[i].min(tab_width) - HORIZONTAL_TEXT_PADDING * 2.0)
                .max(MIN_INDICATOR_WIDTH);
            positions.push(TabPosition::new(x, tab_width, content_width));
            placements.push(Placement { size, position: Offset::new(x, 0.0) });
        }

        // 分隔线
        let (div_size, _) = measure_node(
            nodes, policies, children[n - 2],
            Constraints::new(0.0, row_width, 0.0, f32::MAX),
        );
        placements.push(Placement {
            size: Size::new(row_width, div_size.height),
            position: Offset::new(0.0, tab_row_height - div_size.height),
        });

        // 指示条
        let (target_offset, target_width) = if self.selected_tab_index < tab_count {
            let pos = &positions[self.selected_tab_index];
            let w = if self.follow_content_size { pos.content_width } else { pos.width };
            // 指示器居中于 tab slot（对齐 M3 规范——scrollable 版本也显式居中）
            (pos.left + (pos.width - w) / 2.0, w)
        } else {
            (0.0, 0.0)
        };

        if let Some(slot) = &self.indicator_slot {
            // The caller's indicator, composed HERE because this is the only place the positions exist.
            // It replaces the row's own bar (Compose's `indicator` parameter does the same), and it is
            // laid out in the row's own coordinate space, so `scope.selected_position()` is a position
            // in the row rather than an offset for the row's bar.
            //
            // BEFORE the row's own indicator bookkeeping on purpose: those two states drive a spring, and
            // skipping their `set` while still pushing the animation every frame leaves that spring
            // chasing a target it never receives — measured as one value animation alive for 312 of 356
            // loop iterations (`[anim] animating=true values=1`, id `StateId(15)`), which kept the window
            // repainting at 60 fps with nothing changing.
            let scope = TabIndicatorScope {
                positions: positions.clone(),
                selected_index: self.selected_tab_index,
            };
            // `subcompose_overlay`, not `subcompose`: an indicator is drawn ON the row, it does not
            // define the row's size (the tabs do). With the size-defining variant the row's own node
            // became the indicator's box and the whole row stopped painting.
            crate::layout::subcompose::subcompose_overlay(
                Constraints::new(0.0, row_width, 0.0, tab_row_height),
                // The scope is CLONED per call: `subcompose` runs its content as `Fn`, and each call
                // needs its own copy (a Vec of a few floats per measurement).
                |ctx| slot(ctx, scope.clone()),
            );
            // The default bar's node still exists (the children list is [tabs.., divider, indicator]),
            // so give it a ZERO area: without a placement it would keep the previous frame's geometry
            // and paint on top of the caller's indicator.
            placements.push(Placement {
                size: Size::new(0.0, 0.0),
                position: Offset::new(0.0, tab_row_height),
            });
            return (Size::new(row_width, tab_row_height), placements);
        }

        // The row's OWN indicator: initialise on the first measurement, animate afterwards.
        if !self.initialized.load(Ordering::Relaxed) {
            self.offset_state.set(target_offset);
            self.width_state.set(target_width);
            self.initialized.store(true, Ordering::Relaxed);
        } else {
            let spec = indicator_spring();
            crate::animation::push_animatable(self.offset_state.clone(), target_offset, spec.clone());
            crate::animation::push_animatable(self.width_state.clone(), target_width, spec);
        }

        // 读动画当前值用于 placement（peek 不注册依赖——依赖已在 measure 开头
        // 通过 get() 注册到 TabRow 节点；peek 只读当前值，零注册开销）
        let current_off = self.offset_state.peek();
        let current_w = self.width_state.peek();

        placements.push(Placement {
            size: Size::new(current_w, indicator_h),
            position: Offset::new(current_off, tab_row_height - indicator_h),
        });

        (Size::new(row_width, tab_row_height), placements)
    }

    fn place(&self, nodes: &mut Vec<LayoutNode>, children: &[usize], placements: &[Placement]) {
        for (index, &child) in children.iter().enumerate() {
            if let Some(p) = placements.get(index) {
                nodes[child].position = p.position;
                nodes[child].measured_size = p.size;
            }
        }
    }

    // ── Intrinsic measurement: folded from the tabs, never by running `measure` ──
    //
    // `measure` derives the indicator's target from `constraints.max_width`, starts a spring toward
    // it, and consumes the row's first-measurement flag. The trait's defaults answer an intrinsic by
    // running exactly that measure with the queried axis UNBOUNDED (`node.rs:1003-1051`; Compose's own
    // default, which is why Compose's animating nodes extend
    // `LayoutModifierNodeWithPassThroughIntrinsics`, `AnimationModifier.kt:259-280`). Measured before
    // these overrides: an ancestor asking for width intrinsics handed the row `max_width = f32::MAX`,
    // the indicator's target became `Infinity`, and the bar was placed at `NaN`.
    //
    // Both folds repeat the first pass of `measure` (`TabRow.kt:450-459`) with the indicator
    // bookkeeping left out, so the numbers are the same ones the row itself uses.

    fn min_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        tabs_intrinsic_width(ctx, children, IntrinsicQuery::MinWidth, height)
    }

    fn max_intrinsic_width(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        height: f32,
    ) -> f32 {
        tabs_intrinsic_width(ctx, children, IntrinsicQuery::MaxWidth, height)
    }

    fn min_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        tabs_intrinsic_height(ctx, children, width)
    }

    fn max_intrinsic_height(
        &self,
        ctx: &mut IntrinsicCtx<'_>,
        children: &[usize],
        width: f32,
    ) -> f32 {
        tabs_intrinsic_height(ctx, children, width)
    }
}

/// The spec a `Tab` animates its content colour with, following Compose's `TabTransition`
/// (`material3/Tab.kt:270-291`): a `updateTransition(selected)` whose `animateColor` picks
/// `DefaultEffects` when `false isTransitioningTo true` — i.e. while the tab is ARRIVING at the
/// selected colour — and `FastEffects` otherwise, so leaving the selected state is the quicker move.
/// (`Tab.kt` has no `animateColorAsState`; it is this transition that carries the direction.)
///
/// Kept as a function so both directions are assertable: a change back to one spring for both, or to a
/// tween, fails a test rather than only changing how a tab feels.
pub(crate) fn tab_colour_spec(arriving: bool) -> crate::animation::SpringSpec {
    if arriving {
        crate::animation::SpringSpec::effects_default()
    } else {
        crate::animation::SpringSpec::effects_fast()
    }
}

/// TabRow children are `[tabs.., divider, indicator]`; the divider and the indicator are placed
/// inside the row's height, so neither prices it.
pub(crate) fn tab_count_of(children: &[usize]) -> usize {
    children.len().saturating_sub(2)
}

/// A fixed `TabRow` gives every tab an equal slot (`tab_width = row_width / tab_count`), so the row's
/// natural width is `tab_count` slots as wide as the widest tab's content wants to be.
pub(crate) fn tabs_intrinsic_width(
    ctx: &mut IntrinsicCtx<'_>,
    children: &[usize],
    query: IntrinsicQuery,
    height: f32,
) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    let widest = (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], query, height))
        .fold(0.0, f32::max);
    widest * tab_count as f32
}

/// The row is as tall as the TALLEST tab's max intrinsic height AT the width every tab gets — the
/// measure's own first pass.
pub(crate) fn tabs_intrinsic_height(ctx: &mut IntrinsicCtx<'_>, children: &[usize], width: f32) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    let tab_width = if width >= f32::MAX {
        f32::MAX
    } else {
        width / tab_count as f32
    };
    (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], IntrinsicQuery::MaxHeight, tab_width))
        .fold(0.0, f32::max)
}

/// A scrollable strip asks every tab at `Constraints.Infinity` — width and height — so its height is
/// the tallest tab's max intrinsic height there.
pub(crate) fn strip_intrinsic_height(ctx: &mut IntrinsicCtx<'_>, children: &[usize]) -> f32 {
    let tab_count = tab_count_of(children);
    if tab_count == 0 {
        return 0.0;
    }
    (0..tab_count)
        .map(|i| ctx.child_intrinsic(children[i], IntrinsicQuery::MaxHeight, f32::MAX))
        .fold(0.0, f32::max)
}

