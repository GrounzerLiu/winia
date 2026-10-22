use super::*;

// ═══════════════════════════════════════════════════════
// LazyListPolicy — 测量策略
// ═══════════════════════════════════════════════════════

/// 懒列表测量：把已注册子节点按锚点排布，真实高度写回缓存，视口高回写。
/// 轴无关（`A: LazyAxis` 决定主轴方向——LazyColumn/LazyRow 共用）。
pub(crate) struct LazyListPolicy<A: LazyAxis> {
    pub axis: PhantomData<A>,
    pub cache: crate::runtime::state::State<ItemHeightCache>,
    pub viewport: crate::runtime::state::State<f32>,
    pub content_height: crate::runtime::state::Backchannel<f32>,
    pub fling_limit: crate::runtime::state::Backchannel<f32>,
    pub is_scroll_in_progress: crate::runtime::state::State<bool>,
    pub spacing: f32,
    pub total: usize,
    /// 主轴内容内边距 (before, after)
    pub content_padding: (f32, f32),
    /// 交叉轴内容内边距 (before, after)
    pub cross_padding: (f32, f32),
    /// 注册顺序 → 全局 index（sticky 项排在最后——画在最上层）
    pub globals: Vec<usize>,
    /// 当前数据全局 index → item key（build 期从 intervals 提取；
    /// measure 用其写 cache 的 key 视图——跨数据变化保持项身份高度）
    pub keys: Vec<u64>,
    /// 注册序中 sticky 子节点的 child 下标（全局升序）
    pub sticky_children: Vec<usize>,
    /// 钉住的 sticky header 全局 index（build 期回溯；钉住时锚点 = (pin, 0)）
    pub pin: Option<usize>,
    /// 反向布局（reverseLayout）：放置内容坐标 = content_h - 镜像位置 - 项高
    pub reverse: bool,
    /// Paging snap (from `snap_paging`): the step is written back only when every measured item
    /// measures at the viewport's main size, and `None` (no snapping) otherwise — with items of
    /// unequal size there is no snap position to speak of.
    pub snap_paging: bool,
    /// Every item fills the viewport along the main axis (`fillParentMax*`), from `fill_items`.
    pub fill_items: bool,
    /// Pin every item's main size (`PageSize.Fixed`), taking precedence over `fill_items`.
    pub item_main_size: Option<f32>,
    /// Where inside an item a snapping list rests (`SnapPosition`), from `snap_offset`.
    pub snap_offset: f32,
    /// The release threshold written into the snap record (`SnapInfo::positional_threshold`).
    pub snap_positional_threshold: f32,
    /// The settle spring written into the snap record (`SnapInfo::settle`).
    pub snap_animation_spec: crate::animation::SpringSpec,
    /// The page ceiling written into the snap record (`SnapInfo::max_pages_per_fling`).
    pub snap_max_pages: usize,
    /// Items composed beyond the viewport on each side (`beyondViewportPageCount`).
    pub beyond_bounds: usize,
    pub state: LazyListState,  // 派生锚点回写（测量后精确值）
}

impl<A: LazyAxis> std::fmt::Debug for LazyListPolicy<A> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("LazyListPolicy")
    }
}

impl<A: LazyAxis> crate::layout::node::MeasurePolicy for LazyListPolicy<A> {
    fn measure(
        &self,
        nodes: &mut Vec<crate::layout::node::LayoutNode>,
        policies: &[Box<dyn crate::layout::node::MeasurePolicy>],
        children: &[usize],
        constraints: crate::layout::constraints::Constraints,
    ) -> (crate::unit::Size, Vec<crate::layout::node::Placement>) {
        // 视口主轴尺寸：有限约束直接用（回写缓存）；无界（父内容驱动——Column 无
        // 固定高度时给子节点 f32::MAX；⚠ is_finite() 对 f32::MAX 也返回 true，
        // 必须用框架惯例 `< f32::MAX` 判定）回退缓存值，避免视口
        // 无限膨胀（实测：f32::MAX 视口会让 clamp 把 offset 清零）
        let main_max = A::main_max(&constraints);
        let vh = if main_max < f32::MAX && main_max > 0.0 {
            // 非 silent set：viewport 变化（resize）→ 通知 build 重组，用新视口
            // 补组合可见项（方向一）。值稳定时 PartialEq 去重不通知（无振荡）。
            self.viewport.set(main_max);
            main_max
        } else {
            self.viewport.get()
        };

        // Measure each child: the cross axis inherits the parent's max less the cross-axis
        // `contentPadding` (Compose: an item's usable width is the viewport minus that padding), and
        // the main axis is unbounded so the item wraps to its content — passing the parent's max
        // (= the viewport) there would inflate every item to fill it. `fill_items` opts back into
        // exactly that inflation, which is what `fillParentMaxWidth` asks for and what makes a snap
        // position an item boundary.
        let (cb, ca) = self.cross_padding;
        let (pad_before, pad_after) = self.content_padding;
        let mut child_constraints = A::child_constraints(constraints);
        let cross_max = A::cross_max(&child_constraints);
        child_constraints = A::with_cross_max(child_constraints, (cross_max - cb - ca).max(0.0));
        // A pager's Fill pages occupy the usable viewport between the main-axis content padding,
        // matching Compose's `mainAxisAvailableSize`. Ordinary lazy items remain wrap-content.
        let uniform_page_size = if let Some(size) = self.item_main_size {
            Some(size)
        } else if self.fill_items && vh > 0.0 {
            Some((vh - pad_before - pad_after).max(0.0))
        } else {
            None
        };
        if let Some(size) = uniform_page_size {
            child_constraints = A::with_main_exact(child_constraints, size);
        }
        let mut placements = Vec::with_capacity(children.len());
        let mut measured: Vec<(f32, f32)> = Vec::with_capacity(children.len());
        for &c in children.iter() {
            let (size, _) = crate::layout::node::measure_node(nodes, policies, c, child_constraints);
            measured.push((A::main_size(size), A::cross_size(size)));
        }

        // 全局 index 映射：注册顺序从 start 到 end，start = first_index - 预取数
        // 但 build 只给了 policy first_index（锚点）——子节点全局序需要从 placement 反推：
        // 实际实现：build 把 (start, end) 传给 policy，这里按序写回
        let mut cache = self.cache.get();
        // `fill_items` pins every item to the viewport, so unmeasured items can be priced exactly. It
        // has to be declared, or the flat estimate is used and every position outside the measured
        // window — and so every fling that leaves it — is computed from the wrong size.
        let uniform = uniform_page_size;
        if cache.uniform != uniform {
            cache.set_uniform(uniform);
        }
        for (i, (h, _)) in measured.iter().enumerate() {
            let global = self.globals[i];
            cache.record(global, *h);
            // 同步写 key 视图（keys[global] = 该全局项的 item key）——跨数据
            // 变化保持"项身份 → 高度"，rebase 据此迁移到正确的 index
            if let Some(&k) = self.keys.get(global) {
                cache.record_keyed(k, *h);
            }
        }
        // 内容总高：注册项实测 + 未注册项预估（供 apply_scroll_delta 算 max_offset）；
        // 含 主轴 contentPadding（对齐 Compose：max_offset = before + 内容 + after - 视口）
        // The snap offset is a PLACEMENT shift (Compose shifts the page, not the scroll target).
        // Price spacing only BETWEEN items, then add enough trailing room for the final snap position
        // to reach the same visual alignment as every earlier page.
        let snap_shift = self.snap_offset.max(0.0);
        let mut content_h = pad_before + pad_after + snap_shift;
        for g in 0..self.total {
            // `coverage_height`, like `prefix_height` and the window walk: `max_off` clamps an offset that
            // `prefix_height` produced, so the two have to price unmeasured items the same way or the end
            // of the list disagrees with itself by a little on every frame.
            content_h += cache.coverage_height(g);
            if g + 1 < self.total {
                content_h += self.spacing;
            }
        }
        if self.snap_paging {
            if let Some(page_size) = uniform_page_size.filter(|size| *size > 0.0) {
                let trailing_snap_room =
                    (vh - (pad_before + pad_after + snap_shift + page_size)).max(0.0);
                content_h += trailing_snap_room;
            }
        }
        self.content_height.set(content_h);

        // 越界 clamp：scroll_to_item 无 total 信息，程序化滚动超出内容边界时
        // 在这里收回到末尾（对齐 Compose：scroll position 在 measure 期 clamp）
        let max_off = (content_h - vh).max(0.0);

        // 程序化跳转：消费 jump_request（放在 max_off 之后——动画目标需要 clamp）。
        // 像素 offset 用**写回后的缓存**推导（实测项真实高度 + 未测项预估）——
        // 与锚点解析/放置共用同一 prefix_height，round-trip 精确：跳 500 就是
        // 500，不会因预估 vs 实测高度差累积漂移（实测：旧实现 500×48 vs 47.5
        // → 落到 505）。reverseLayout 下公式相同（offset = before + prefix + 偏移：
        // 语义变为项**底**贴视口底——镜像坐标中与正向同构）。
        if let Some((req_idx, req_off, animate)) = self.state.jump_request.get() {
            let idx = req_idx.min(self.total.saturating_sub(1));
            let target = pad_before + prefix_height(&cache, idx, self.spacing) + req_off;
            self.state.jump_request.set(None);
            if animate {
                // 动画滚动（对齐 Compose animateScrollToItem——spring 收敛）；
                // push_animatable 内部处理同 state 动画替换（retarget 继承速度）
                //
                // The flag goes UP for the length of the spring, not down. It is the same state the
                // drag and fling paths write (`build` hands this clone to the list's ScrollState), and
                // Compose's guard on a programmatic jump — `!monthsListState.isScrollInProgress` before
                // `scrollToItem` (`DatePicker.kt:1548-1553`) — depends on it being set while this runs.
                // Clearing it here left that guard inert for the month arrows: an outside change landing
                // mid-animation would `scroll_to_item` and `cancel_animation` straight through the motion
                // the user just asked for.
                //
                // The variant whose early exits all report completion is deliberate: the plain
                // `push_animatable_with_done` has TWO silent early exits — already at the target
                // (`animation.rs:475-480`), and same-state-same-target dedup when the identical motion is
                // already running (`animation.rs:491-494`) — and neither fires `done`, which would leave
                // the flag set with no animation to clear it. The velocity variant reports completion in
                // both (`animation.rs:195-198`, `:212-215`). Velocity 0 is right for a jump — nothing was
                // flung.
                let in_progress = self.is_scroll_in_progress.clone();
                in_progress.set(true);
                crate::animation::push_animatable_with_velocity_and_done(
                    self.state.offset.clone(),
                    target.clamp(0.0, max_off),
                    crate::animation::AnimationSpec::Spring(crate::animation::SpringSpec::default()),
                    0.0,
                    move || {
                        in_progress.set(false);
                    },
                );
            } else {
                // 立即跳转：取消进行中的 fling + 结束滚动中标记
                crate::animation::cancel_animation(&self.state.offset);
                self.is_scroll_in_progress.set(false);
                self.state.offset.set(target);
            }
        }
        // fling 极限回写（输入路径 ScrollState::fling + 程序化 LazyListState::fling）
        self.fling_limit.set(max_off);
        self.state.fling_limit.set(max_off);
        // The viewport is what a pager divides a pixel offset by; write it beside the limit.
        if self.state.viewport_main.peek() != vh {
            self.state.viewport_main.set(vh);
        }
        // Paging snap step, written back so a finished fling knows where to land. Only given when
        // every measured item is exactly the viewport's main size: `measured` covers every item in
        // this frame's composition window, so unequal items are caught and snapping turns itself off
        // rather than snapping to a step that means nothing. Fewer than two items is no snapping
        // either — there is nothing to page to, and nothing should move.
        // A paged list rests on an item boundary; a FIXED-size page rests wherever its size says, and
        // the spacing between items is part of the distance to the next one.
        let page_size = uniform_page_size.unwrap_or(vh);
        let snap = if self.snap_paging
            && self.total >= 2
            && uniform_page_size.is_some()
            && page_size > 0.0
            && measured.iter().all(|(size, _)| (size - page_size).abs() < 0.5)
        {
            Some(crate::fling::SnapInfo {
                step: page_size + self.spacing,
                // Page k is placed after `pad_before`, but its scroll snap position is `k * step`;
                // the leading padding belongs to layout, not to the scroll state's snap anchor.
                anchor: 0.0,
                positional_threshold: self.snap_positional_threshold,
                settle: self.snap_animation_spec.clone(),
                max_pages_per_fling: self.snap_max_pages,
                // Compose's `MinFlingVelocityDp = 400.dp`, in px at the density this list lays out
                // under — below it a fling settles back on the page it started from.
                min_fling_velocity: 400.0 * crate::runtime::density::current_density().density,
            })
        } else {
            None
        };
        if self.state.snap.peek() != snap {
            self.state.snap.set(snap);
        }
        let clamped = self.state.offset.get().clamp(0.0, max_off);
        if clamped != self.state.offset.get() {
            self.state.offset.set(clamped);
        }

        // 精确锚点回写：用真实高度（cache 已写回）反推 first_visible_index/offset——
        // build 期用的是预估高度，这里给出精确值（对齐 Compose 从 measure result 更新）
        let offset = self.state.offset.get();
        // 钉住时锚点 = (pin, 0)（对齐 Compose：firstVisibleItemIndex 就是钉住的
        // header）；否则自然锚点（像素反推）
        let (real_first, real_off) = match self.pin {
            Some(p) if offset >= pad_before + prefix_height(&cache, p, self.spacing) => (p, 0.0),
            _ => anchor_from_offset(&cache, offset, self.spacing, pad_before),
        };
        self.state.first_visible_index.set(real_first);
        self.state.first_visible_offset.set(real_off);

        // Same-frame window coverage. `build` picked the window from the viewport the PREVIOUS measure
        // wrote, and THIS measure just learned the real one — on the frame the viewport grows, that
        // window is short and the new bottom has nothing in it. No measurement can conjure the missing
        // items (they were never composed), so ask the frame for one more compose instead of drawing an
        // empty region and converging on the next frame (`docs/lazy-column.md`).
        //
        // The condition is "the window I was given does not cover what the real viewport needs", which
        // is exactly the case that leaves a hole on screen. It is deliberately NOT "the viewport
        // changed": a frame where the viewport is unchanged and the window still covers asks for
        // nothing, so a scrolling frame stays at one compose+layout pass.
        {
            let (need_start, need_end) =
                visible_range(&cache, real_first, real_off, vh, self.spacing, self.total, self.beyond_bounds);
            // `globals` is the composed window in registration order (sticky items are appended last,
            // so min/max — not first/last — give the range).
            let composed_start = self.globals.iter().copied().min().unwrap_or(usize::MAX);
            let composed_end = self.globals.iter().copied().max().map_or(0, |m| m + 1);
            if need_end > composed_end || need_start < composed_start {
                crate::runtime::composer::request_compose_after_layout();
            }
        }

        // 锚点排布：**内容坐标**（主轴 = 项在内容中的累计位置，不含 offset）——
        // 滚动由框架 scroll translate(-offset) 处理。若 placement 也含 offset 会
        // 双重偏移（实测：滚动 3000 后内容完全滚出视口）。
        // 内容起点 = before 内容内边距（对齐 Compose contentPadding）。
        // sticky pass（对齐 Compose LazyListMeasure）：
        //   正向 final(i) = max(C(i) - offset, before)   —— 滚过顶钉在 before 处
        //   反向 final(prev) = min(final(prev), final(next) - h(prev))
        //     —— 下一个 header 距顶 h(prev) 内时前一个开始滑出
        // 放置内容坐标 = final + offset；普通项保持自然位置（累计）
        let mut fin: std::collections::HashMap<usize, f32> = std::collections::HashMap::new();
        for &ci in &self.sticky_children {
            let v = pad_before + prefix_height(&cache, self.globals[ci], self.spacing) - offset;
            fin.insert(ci, v.max(pad_before));
        }
        for k in (1..self.sticky_children.len()).rev() {
            let prev = self.sticky_children[k - 1];
            let next = self.sticky_children[k];
            let nf = fin[&next];
            let pf = fin[&prev];
            let h_prev = measured[prev].0;
            fin.insert(prev, pf.min(nf - h_prev));
        }
        let mut main_pos = 0.0f32;
        let mut main_init = false;
        for (i, _c) in children.iter().enumerate() {
            let (h, w) = measured[i];
            let pos = match fin.get(&i) {
                Some(f) => *f + offset,
                None => {
                    if !main_init {
                        main_pos = pad_before + prefix_height(&cache, self.globals[i], self.spacing);
                        main_init = true;
                    }
                    // The snap offset moves every item forward, which is what makes a page smaller
                    // than the viewport rest where `SnapPosition` asks (Compose shifts the page in
                    // layout, it does not skew the snap positions).
                    let p = main_pos + snap_shift;
                    main_pos += h + self.spacing;
                    p
                }
            };
            // 反向布局：镜像坐标（从底向上累计）→ 内容坐标 = content_h - pos - h
            // （index 0 在内容底部；锚点/可见范围/pin 回溯在镜像坐标中公式不变）
            let content_pos = if self.reverse { content_h - pos - h } else { pos };
            placements.push(crate::layout::node::Placement {
                // 交叉轴从 cross_before 处开始（contentPadding 交叉轴语义）
                position: A::point(content_pos, cb),
                size: A::size(h, w),
            });
        }

        self.cache.set(cache);
        (A::size(vh, A::cross_max(&constraints)), placements)
    }

    fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], placements: &[crate::layout::node::Placement]) {
        /// Whether any node in this item's subtree asks for placement animation (`animateItem`).
        fn subtree_has_animate_item(nodes: &[crate::layout::node::LayoutNode], idx: usize) -> bool {
            if nodes[idx].modifier.has_animate_item() {
                return true;
            }
            nodes[idx]
                .children
                .iter()
                .any(|&c| subtree_has_animate_item(nodes, c))
        }
        // Placement animations (Compose's `animateItem`): an item whose key occupied a different
        // main-axis slot on the PREVIOUS frame starts at the old slot and springs to the new one.
        // Items that were away (not placed last frame) reset instead of springing from a stale slot.
        // The map is keyed by the item key, so the animation survives the node recycling the lazy
        // window does.
        let mut anims = self
            .state
            .item_animations
            .lock()
            .expect("lazy item animations");
        anims.frame += 1;
        let frame = anims.frame;
        for (i, &c) in children.iter().enumerate() {
            // The marker sits on the item's ROOT content, and the lazy list registers a wrapper
            // group node per item — so the animation belongs to the wrapper and the marker is looked
            // for in its subtree.
            let animate = subtree_has_animate_item(nodes, c);
            nodes[c].position = placements[i].position;
            nodes[c].measured_size = placements[i].size;
            nodes[c].item_anim = None;
            if !animate {
                continue;
            }
            let global = self.globals.get(i).copied().unwrap_or(i);
            let key = self.keys.get(global).copied().unwrap_or(global as u64);
            let main = A::main_size(crate::unit::Size::new(
                nodes[c].position.x,
                nodes[c].position.y,
            ));
            let item = anims.items.entry(key).or_insert_with(|| ItemAnimation {
                offset: crate::runtime::state::State::new(0.0),
                last_main: main,
                last_frame: frame,
            });
            if item.last_frame + 1 == frame {
                let delta = item.last_main - main;
                if delta.abs() > 0.5 {
                    crate::animation::cancel_animation(&item.offset);
                    item.offset.set(delta);
                    crate::animation::push_animatable_restart(
                        item.offset.clone(),
                        0.0,
                        crate::animation::AnimationSpec::Spring(
                            crate::animation::SpringSpec::default(),
                        ),
                    );
                }
            } else {
                // Away for a frame (or new): no stale spring.
                item.offset.set(0.0);
            }
            item.last_main = main;
            item.last_frame = frame;
            nodes[c].item_anim = Some((item.offset.clone(), A::VERTICAL));
        }
        retain_recent_items(&mut anims, frame);
    }
}

