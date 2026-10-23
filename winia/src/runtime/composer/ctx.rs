use super::*;

// ── ComposeCtx ──

/// 组合上下文。
///
/// 传递给每个 composable 函数，用于:
/// - `remember()`: 在组合中持久化状态
/// - `next_key()`: 生成唯一的组合 key
///
/// 注意: ComposeCtx 不实现 Clone —— 组合树是严格树形遍历。
pub struct ComposeCtx<'a> {
    composer: &'a mut Composer,
}

impl<'a> ComposeCtx<'a> {
    pub(crate) fn new(composer: &'a mut Composer) -> Self {
        Self {
            composer,
        }
    }

    /// The frame clock of this composition's window.
    ///
    /// Prefer this over the process-wide `with_frame_nanos`: with more than one window the global
    /// clock wakes a waiter on ANY window's frame, while this one wakes it only on its own.
    pub fn frame_clock(&self) -> crate::effect::FrameClock {
        crate::effect::FrameClock::for_window(self.composer.window_id)
    }

    /// Return the Window lifecycle context owned by this Composer.
    pub(crate) fn window_lifecycle(&self) -> crate::runtime::lifecycle::LifecycleState {
        self.composer.lifecycle.clone()
    }


    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember(slot_key, || State::new(init()))
    }

    /// Run `f` when the slot being composed RIGHT NOW is dropped — a lifecycle hook that costs no
    /// layout node.
    ///
    /// This is what effects use for their cleanup. The alternative the framework had first was an
    /// implicit leaf node carrying an `on_remove`: it worked, but the node was a real child of the
    /// enclosing container, so an effect inside a `Column(spacing = 10)` pushed its next sibling
    /// down by one spacing slot (measured: y 29 -> 39). Compose has the same shape of hook — a
    /// `RememberObserver` lives in the slot table and never materializes.
    ///
    /// Registering again at the same call site REPLACES the previous callback rather than stacking,
    /// which is what a recomposition needs.
    pub fn remember_on_forget(&mut self, f: impl FnOnce() + Send + 'static) {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_on_forget(slot_key, Box::new(f));
    }

    /// Tie a motion to the CURRENT slot: when the slot is forgotten — the branch changed shape, the
    /// component left the tree — the motion is removed from the engine.
    ///
    /// Compose expresses this with `DisposableEffect`. Without it a slot-owned animation keeps
    /// ticking, and keeps writing to a state nobody reads, until the whole `Composer` is dropped:
    /// a conditionally composed `animateFloatAsState` is the common case, and it used to outlive
    /// its composable by however long the window stayed open.
    ///
    /// The registration is unconditional. The key is `mix_key(statement, n)`, where `n` counts the
    /// `remember` calls made from that statement: registering only on the first pass would renumber
    /// every LATER `own_motion` of the same scope, leaving stale callbacks in the slot's forget map at
    /// keys nothing ever revisits. Re-registering is cheap and REPLACES the callback at the same key
    /// (see `remember_on_forget`).
    pub(crate) fn own_motion(&mut self, state_id: crate::runtime::state::StateId) {
        self.composer.animation_state_ids.insert(state_id);
        self.remember_on_forget(move || crate::animation::remove_animation_by_state(state_id));
    }

    /// Remember a callable whose placed state and nodes move between hosts without changing identity.
    /// The callable is initialized once, like `remember { movableContentOf { ... } }`; read dynamic
    /// values through State or an explicit latest-value channel rather than replacing its captures.
    ///
    /// An invocation conservatively enters its child scopes so current payloads and placement locals
    /// are applied. A completed composition with no placement disposes its stored state and nodes.
    ///
    /// Current winia restriction: a handle has one placement per composition, and movable invocation
    /// cannot be nested. Violations are rejected explicitly. Compose permits multiple independent
    /// placements of a callable; use separate handles for simultaneous copies in this minimal API.
    pub fn remember_movable_content(
        &mut self,
        content: impl Fn(&mut ComposeCtx) + Send + Sync + 'static,
    ) -> crate::runtime::movable::MovableContent {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::movable::MovableContent::new(content)
        })
    }

    /// Compose this content here.
    /// Hand the slot table over to movable content #id — see
    /// [`MovableContent::compose`](crate::runtime::movable::MovableContent::compose).
    pub(crate) fn begin_movable(&mut self, id: u64) {
        self.composer.slot_table.validate_movable(id);
        let key = self.next_key();
        let caller_active = self.composer.slot_table.active_slot_key;
        let caller_group = self.composer.current_group_key;
        self.composer.slot_table.begin_movable(key, id);
        self.composer.movable_saved_skip_depth = self.composer.group_skip_stack.len();
        self.composer.movable_saved_path_counters = std::mem::take(&mut self.composer.path_counters);
        self.composer.movable_saved_remember_counters = std::mem::take(&mut self.composer.remember_path_counters);
        self.composer.movable_saved_params = std::mem::take(&mut self.composer.pending_params);
        self.composer.movable_saved_pending_next = std::mem::replace(&mut self.composer.pending_next, 0);
        self.composer.movable_saved_group_key = caller_group;
        self.composer.movable_saved_active = caller_active;
        // A placement owns its identities and dependency targets; inherited CompositionLocals remain
        // available, but caller statement/explicit-key/group stacks must not enter the stored tree.
        self.composer.movable_saved_scopes = std::mem::take(&mut self.composer.scope_source_stack);
        self.composer.movable_saved_stmts = STMT_STACK.with(|s| std::mem::take(&mut *s.borrow_mut()));
        self.composer.movable_saved_keys = std::mem::take(&mut self.composer.key_override_stack);
        self.composer.movable_saved_groups = GROUP_STACK.with(|s| std::mem::take(&mut *s.borrow_mut()));
        let root = self.composer.slot_table.current_slot().key;
        self.composer.slot_table.active_slot_key = root;
        ACTIVE_SLOT_KEY.with(|s| s.set(root));
        self.composer.entered_compose_keys.insert(root);
        GROUP_STACK.with(|s| s.borrow_mut().push(root));
    }

    /// Restore caller stacks even if the caller catches a panic and continues this composition.
    pub(crate) fn abort_movable(&mut self) {
        self.composer.slot_table.abort_movable();
        self.restore_movable_context();
    }

    /// Give the slot table and invocation context back to the caller.
    pub(crate) fn end_movable(&mut self) {
        self.composer.slot_table.end_movable();
        self.restore_movable_context();
    }

    fn restore_movable_context(&mut self) {
        self.composer.scope_source_stack = std::mem::take(&mut self.composer.movable_saved_scopes);
        self.composer.key_override_stack = std::mem::take(&mut self.composer.movable_saved_keys);
        STMT_STACK.with(|s| *s.borrow_mut() = std::mem::take(&mut self.composer.movable_saved_stmts));
        GROUP_STACK.with(|s| *s.borrow_mut() = std::mem::take(&mut self.composer.movable_saved_groups));
        self.composer.path_counters = std::mem::take(&mut self.composer.movable_saved_path_counters);
        self.composer.remember_path_counters = std::mem::take(&mut self.composer.movable_saved_remember_counters);
        self.composer.pending_params = std::mem::take(&mut self.composer.movable_saved_params);
        self.composer.pending_next = self.composer.movable_saved_pending_next;
        self.composer.group_skip_stack.truncate(self.composer.movable_saved_skip_depth);
        self.composer.current_group_key = self.composer.movable_saved_group_key;
        self.composer.slot_table.active_slot_key = self.composer.movable_saved_active;
        ACTIVE_SLOT_KEY.with(|s| s.set(self.composer.movable_saved_active));
    }

    /// Remember a write-back channel: same slot stability as `remember`, but
    /// the stored value never notifies (Backchannel write). For measure/layout
    /// write-back and cross-frame staging (see docs/state-handles.md).
    pub fn remember_backchannel<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Backchannel<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Backchannel::new(init())
        })
    }

    /// Remember an animation-tick handle: recompose without waking the event
    /// loop (see docs/state-handles.md).
    pub fn remember_animating<T: Clone + PartialEq + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(init())
        })
    }

    /// Remember a draw-layer handle: writes land without recomposition
    /// (see docs/state-handles.md).
    pub fn remember_visual<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::runtime::state::Visual<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Visual::new(init())
        })
    }

    /// 注册顶层弹出层（Popup/Dialog/DropdownMenu 内部调用）——组合期收集，
    /// compose 后由 app.rs 取走并独立物化/渲染
    pub fn open_overlay(&mut self, mut desc: crate::overlay::OverlayDesc) {
        // ⚠ 捕获 CompositionLocal 快照（主树 provides 内——Theme 等）——
        // overlay 独立 Composer 在 provides 弹栈后 recompose，读不到主树
        // 隐式上下文；快照重放让 overlay 继承主树主题/方向/排版。
        desc.local_snapshot = crate::runtime::composition_local::capture();
        self.composer.overlays.push(desc);
    }

    /// 组合期记录 overlay 的 active 状态（Popup/Dialog build 总执行时调用——
    /// 无论 visible 都记录；Skip 帧 build 不执行 → 本帧无记录 → sync 保留上帧）。
    /// sync_overlays 用此区分"注册方 Skip"（保留）与"主动关闭 visible=false"
    /// （记录 false → 删除）——slot 层无法区分，组合期显式记录是唯一正解。
    pub fn record_overlay_active(&mut self, id: u64, active: bool) {
        self.composer.overlay_active.insert(id, active);
    }

    /// Remember a write-back channel under an EXPLICIT key.
    ///
    /// The sibling of [`ComposeCtx::remember_at_key`] for state a provider has to keep per frame about
    /// ITSELF: no notification (the composer is already running), and no stable-call-site lookup — that
    /// lookup panics outside a `#[composable]`/keyed context, which a provider cannot require, since
    /// `WiniaTheme::provide_resolved` also runs from a window's per-frame wrapper.
    ///
    /// The caller owns key uniqueness: use [`crate::runtime::composer::mix_key`] over the position's
    /// identity so two providers in one composer never share a slot.
    pub fn remember_backchannel_at_key<T: Clone + 'static>(
        &mut self,
        key: u64,
        init: impl FnOnce() -> crate::runtime::state::Backchannel<T>,
    ) -> crate::runtime::state::Backchannel<T> {
        self.composer.slot_table.remember_handle(key, init)
    }

    /// A key for "the composition position we are at right now", mixed with `namespace`.
    ///
    /// A provider needs a per-SITE key: two `WiniaTheme::with_*` calls in one composer must not share
    /// the memory of what they last provided, or each would see the other's value as a change and dirty
    /// its subtree every frame. The position is the current group (a container's or scope's key, which
    /// is statement-derived and therefore stable) or, before any group, the slot path.
    pub fn position_key(&self, namespace: u64) -> u64 {
        let base = match GROUP_STACK.with(|s| s.borrow().last().copied()) {
            Some(key) => key,
            None => {
                // No group yet (a window's content wrapper is the common case): hash the slot path.
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in self.composer.slot_table.current_path() {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
        };
        mix_key(base, namespace)
    }

    /// Invalidate everything composed INSIDE this position — what a `CompositionLocal` provider calls
    /// when the value it is about to provide differs from the previous frame's.
    ///
    /// Why a provider has to do this itself: `provides` only changes what the NEXT reader reads. A
    /// reader re-reads a local when its own group runs again, and a group is re-entered only when it is
    /// dirty. A container that declares nothing about itself (`start_restartable_group` with an
    /// unchanged modifier and no `ctx.changed`) is therefore Skipped and keeps the value it composed
    /// with — the layout, padding and colors it built from the old one. Marking the inner slots makes
    /// those readers run again.
    ///
    /// Call it only when the value actually changed: it costs a re-entry of the whole subtree.
    pub fn mark_subtree_dirty(&mut self) {
        self.composer.mark_inner_slots_dirty();
    }

    /// 当前组合节点的 slot_key（DropdownMenu 锚点用）
    pub fn composer_slot_key(&self) -> u64 {
        self.composer.slot_table.active_slot_key()
    }

    /// 当前作用域内最后一个已组合兄弟的 slot_key（Popup 锚点用）——
    /// 紧跟最后组合的兄弟之后，等价于 Compose Popup 零尺寸占位节点在父布局中的位置。
    /// 无兄弟时返回 None（Popup 回退窗口对齐）。
    pub fn prev_sibling_slot_key(&self) -> Option<u64> {
        self.composer.slot_table.prev_sibling_slot_key()
    }

    /// Remember a plain handle (no `State` wrapper): the slot stores `T` itself.
    ///
    /// `remember` always allocates a `State<T>` — a `StateInner` plus a `StateSignal` — even when
    /// the caller only wants somewhere to keep a handle, which for an effect is every instance.
    /// This is the same storage the typed helpers below use; it is crate-private because exposing
    /// it would hand callers a slot they can never invalidate, which is a decision the framework
    /// has not made yet (`docs/state-handles.md` §7.5).
    pub(crate) fn remember_handle<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> T {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, init)
    }

    /// Queue `effect` to run once the composition being built has committed.
    ///
    /// This is Compose's `SideEffect`: it runs at the end of this `compose()`, after the slot tree
    /// is materialized and after the dependency frame closed. Two consequences: it sees the tree
    /// this frame actually built (not a half-built one), and a `State` it writes survives the pass
    /// that is ending rather than being swallowed by it.
    ///
    /// The write lands on the NEXT PASS, not the next frame: `recompose_until_stable` runs that pass
    /// before layout, so the value is on screen in the same frame.
    ///
    /// No cleanup and no rollback — a composition that panics before the drain drops the callback
    /// without running it. Use it to publish to something outside the composition (a platform
    /// window, a store, an analytics sink), not to mutate composition state.
    pub fn side_effect(&mut self, effect: impl FnOnce() + Send + 'static) {
        self.composer.post_compose.push(Box::new(effect));
    }

    /// 使用固定 key 记住一个状态（不受 remember_counter 影响，适合跨分支持久化的值）
    pub fn remember_at_key<T: Clone + 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        self.composer.slot_table.remember(key, || State::new(init()))
    }

    /// 生成下一个组合 key（公开 API，用于 start_node）
    pub fn next_key(&mut self) -> u64 {
        self.composer.next_group_key()
    }

    /// Begins a composition scope: a `State::get()` inside it (a component body, outside any container
    /// group) registers its dependency on this scope, and a change re-enters the scope's body together
    /// with its DIRECT children — deeper descendants skip unless their own dependencies fired. That one
    /// level exists because winia cannot compare a value captured by a closure the way Compose compares
    /// a lambda parameter (see `mark_dirty_path_scope`); a component that needs its whole subtree
    /// re-entered says so with `mark_subtree_dirty()`. Pair with `end_scope`.
    pub fn start_scope(&mut self) -> u64 {
        self.composer.start_scope()
    }

    /// `#[composable]` 宏注入：以源码哈希为 scope key 开始（函数级 key 稳定——
    /// 结构变化不漂移）。内部节点的 next_key 以 scope 源码哈希为 key 基。
    /// ⚠ 不调用 start_scope（它也会 push None——双重 push 后栈顶是 None，
    /// next_key 读 scope=0 → 跨函数同 stmt id 的 key 碰撞 → 节点复用串位）
    pub fn start_scope_keyed(&mut self, source_hash: u64) -> u64 {
        self.composer.scope_source_stack.push(Some(source_hash));
        // scope key = 源码哈希本身（稳定唯一——不依赖 next_group_key：scope 是
        // 组合第一条调用（STMT_STACK 空），走路径哈希在宏外（app_root!/根）会
        // 触发稳定 key panic；且路径哈希在结构变化时漂移——hash 反而更稳）
        let key = self.composer.namespace_key_base(source_hash);
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// RAII 版 scope 开始（Drop 时自动 end_scope）——支持返回值函数与提前
    /// return（显式 end_scope 在提前退出时泄漏 scope 栈——新 key 系统
    /// `#[composable]` 宏展开使用此版本；guard 声明在函数开头、存活到函数
    /// 返回——end_scope 在所有语句 guard pop 之后执行，配对正确）。
    /// ⚠ guard 内持有 composer 裸指针——调用方必须保证 guard 生命周期内
    /// composer 存活且无并发访问（组合单线程——成立）。
    pub fn start_scope_guarded(&mut self, source_hash: u64) -> ScopeGuard {
        self.composer.scope_source_stack.push(Some(source_hash));
        let key = self.composer.namespace_key_base(source_hash);
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// 调用链版 RAII scope 开始：scope key = 当前调用链哈希（try_stable_base）
    /// 或 fallback 哈希（调用链空——测试/组合顶层）。组件方法宏化用——
    /// 同一方法多次实例化（16 字段）靠调用点链隔离；独立组合函数多实例
    /// （列表）同样隔离。⚠ scope 在函数开头创建——此刻 STMT_STACK 栈顶
    /// 即调用点（父语句），函数内部语句注入在其后——链不含自身。
    pub fn start_scope_callchain(&mut self, fallback_hash: u64) -> ScopeGuard {
        let raw_placement = self.composer.slot_table.movable_composing.is_some()
            && self.composer.key_override_stack.is_empty()
            && STMT_STACK.with(|s| s.borrow().is_empty());
        let key = if raw_placement {
            // A raw placement is a stable source for node ordinals, not for function identity.
            // Different composable functions must retain their own compiler fallback hashes.
            let base = self.composer.namespace_key_base(fallback_hash);
            let ordinal = self.composer.path_counters.entry(base).or_insert(0);
            let key = mix_key(base, *ordinal as u64);
            *ordinal += 1;
            key
        } else {
            self.composer.try_stable_base().unwrap_or_else(|| self.composer.namespace_key_base(fallback_hash))
        };
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_SCOPE_TRACE").is_ok() {
            let stack = STMT_STACK.with(|s| s.borrow().clone());
            let src = self.composer.scope_source_stack.last().and_then(|s| *s);
            eprintln!("[scope] fallback={:#x} key={:#x} src={:?} stmt_stack={:?}", fallback_hash, key, src, stack);
        }
        self.composer.scope_source_stack.push(Some(key));
        self.composer.slot_table.start_scope(key);
        self.composer.entered_compose_keys.insert(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// `#[composable]` 宏注入：进入一条语句（id 为编译期固定的源码位置序号）。
    /// 返回 RAII guard——语句块结束时 drop 自动 pop_stmt：闭包体/循环体内的
    /// `return`/`break`/`continue`/`panic!` 提前退出也不会泄漏 stmt 栈
    /// （显式 push/pop 在提前退出时栈会永久错位——后续语句 key 静默漂移）。
    ///
    /// seq（迭代位置）解析：**max(自身执行计数, 栈顶外层语句的 seq)**——
    /// ① for 体语句每次迭代都执行：自身计数 = 迭代位置（1..30）✓；② content
    /// 闭包内的语句只在容器 Enter 时执行（行 Skip 时 content 不跑）——自身计数
    /// 会漂移（首帧 seq=30，滚动后首次执行 seq=1）→ key 碰撞（text29 撞 text0）
    /// → 槽树 truncate 重建 → 内容丢失——继承外层行语句的迭代位置（max 兜底）。
    /// 已知限制：嵌套循环（for i { for j { … } }）内层语句取 max(内层次数, 外层
    /// 位置)——内层迭代与外层位置可能混淆，需显式 key（文档化）。
    pub fn enter_stmt(&mut self, id: u32) -> StmtGuard {
        // seq = 完整 child_counters 链哈希（位置而非执行次数）——start_slot 每帧
        // 无条件执行（Skip 帧也执行）→ 链跨帧稳定。替代旧 STMT_SEQ 执行计数
        // （每 compose 清空 + Skip 帧不执行 → 滚动时计数漂移 → seq≠迭代位置
        // → key 漂移 → dup-key panic——animation_demo 滚动卡死根因）。
        // 链含父层 index：content 闭包内语句不同行实例链不同 → seq 区分
        // （旧 last() 恒 0 + max(outer) 全继承行容器 seq → 行间冲突）。
        let seq = self.composer.slot_table.sibling_position();
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_STMT_TRACE").is_ok() {
            eprintln!("[stmt] compose={} id={} seq={}", self.composer.compose_count, id, seq);
        }
        STMT_STACK.with(|s| s.borrow_mut().push((id, seq)));
        StmtGuard
    }

    /// `#[composable]` 宏注入：退出语句（与 push_stmt 配对）——保留兼容旧用法
    pub fn push_stmt(&mut self, id: u32) {
        let seq = self.composer.slot_table.sibling_position();
        STMT_STACK.with(|s| s.borrow_mut().push((id, seq)));
    }

    /// `#[composable]` 宏注入：退出语句（与 push_stmt 配对）
    pub fn pop_stmt(&mut self) {
        STMT_STACK.with(|s| { s.borrow_mut().pop(); });
    }

    /// 显式 key（对标 Compose `key(id)`）：包裹的子树用 id 哈希为 key 基——
    /// 结构变化（列表重排/子树移动）时 remember/复用仍稳定。
    /// 用法：`ctx.key("scroll_list", |ctx| { ... });`
    ///
    /// 重排这条直到 `start_slot` 加上按 key 的查找才真正成立：在那之前，一个 key 换了位置的子节点
    /// 会被当成新节点、从该索引截断重建（`a_keyed_sibling_that_moves_keeps_its_slots` 是那条测量，
    /// 去掉查找它就会数出 4 个槽而不是 2 个）。
    /// 显式 key 作用域（对标 Compose `key(key1, content)`）：`id` 参与内部所有
    /// 语句/组件的 key 基——`id` 变化 → 内部槽 key 变化 → 旧子树重建（结构切换）；
    /// `id` 稳定 → 跨重组 key 稳定（remember State 保留）。
    /// 支持任意 `Hash` 值：`key("section", ...)`、`key(i, ...)`（循环迭代变量——
    /// 嵌套循环内层迭代位置的显式正解，语句级 seq 只携带外层位置）、
    /// `key((row, col), ...)`。
    pub fn key<R>(&mut self, id: impl std::hash::Hash, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        id.hash(&mut hasher);
        let h = hasher.finish();
        let depth = self.composer.key_override_stack.len();
        self.composer.key_override_stack.push(h);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(self)));
        self.composer.key_override_stack.truncate(depth);
        match result {
            Ok(value) => value,
            Err(panic) => std::panic::resume_unwind(panic),
        }
    }

    /// Compose `count` children under stable `(prefix, index)` keys.
    ///
    /// This is the loop form of [`Self::key`], for the common case where a list is generated
    /// from an index range. The prefix identifies the list and the index identifies the item, so
    /// inserting or reordering items keeps each child's remembered slots instead of assigning them
    /// by position.
    pub fn keyed_range(
        &mut self,
        prefix: impl std::hash::Hash + Clone,
        count: usize,
        content: impl FnMut(&mut Self, usize),
    ) {
        self.keyed_for_each(0..count, move |index| (prefix.clone(), *index), content);
    }

    /// Compose an iterator under keys supplied by `key`.
    ///
    /// This is the general form of [`Self::keyed_range`] for lists that are not an index range.
    /// `key` is evaluated once per item and should identify the ITEM, not its current position;
    /// that is what makes reordering preserve the child's remembered slots. The iterator is
    /// consumed in order, so a `Vec` can be reversed between frames without changing the keys.
    pub fn keyed_for_each<T, K>(
        &mut self,
        items: impl IntoIterator<Item = T>,
        mut key: impl FnMut(&T) -> K,
        mut content: impl FnMut(&mut Self, T),
    ) where
        K: std::hash::Hash,
    {
        for item in items {
            let item_key = key(&item);
            self.key(item_key, |ctx| content(ctx, item));
        }
    }


    /// 参数比较（对标 Compose `$composer.changed(param)`）。
    ///
    /// 在 start 组件**前**调用（参数求值处）：与"即将 start 的 slot"上帧记录的参数
    /// 按序比较——相等返回 `false`（参数未变），不等/首次返回 `true`。
    /// 暂存本帧参数（start_node 时写入 slot.params，供下帧比较）。
    ///
    /// 用法（`#[composable]` 组件内——参数未变 + slot clean 时容器 Skip，content 不重跑）：
    /// ```ignore
    /// `#[composable]`
    /// fn card(ctx: &mut ComposeCtx, title: &str) {
    ///     let _title_changed = ctx.changed(&title.to_string());  // 参数声明（start 容器前）
    ///     Column::new()
    ///         .modifier(Modifier::new().padding(8.0))
    ///         .build(ctx, |ctx| {
    ///             Text::new(title).build(ctx);
    ///             // title 未变 + slot clean → Column Skip（content 不执行）
    ///         });
    /// }
    /// ```
    pub fn changed<T: PartialEq + Clone + 'static>(&mut self, param: &T) -> bool {
        // 与"即将 start 的 child slot"的上帧 params 按序比较
        let param_idx = self.composer.pending_next;
        let unchanged = {
            let idx = *self.composer.slot_table.child_counters.last().unwrap_or(&0);
            let prev = self.composer.slot_table.current_slot().children.get(idx)
                .and_then(|c| c.params.get(param_idx))
                .map(|p| p.eq_any(param));
            prev == Some(true)
        };
        // Write into the buffer at this position, REUSING the box already there when it can hold this
        // type. The buffer is the previous frame's parameter vector (the slot hands it back at
        // `commit_pending_params`), so in a list — where every row declares the same parameter types —
        // the same handful of allocations circulates down the rows instead of one box per row per
        // frame. A slot holding a different type is dropped and re-allocated, which is also the path
        // a first frame takes.
        let reused = match self.composer.pending_params.get_mut(param_idx) {
            Some(existing) => existing.set_from_any(param),
            None => false,
        };
        if !reused {
            self.composer.pending_params.truncate(param_idx);
            self.composer.pending_params.push(Box::new(param.clone()));
            #[cfg(test)]
            param_alloc::note();
        }
        self.composer.pending_next = param_idx + 1;
        !unchanged
    }

    /// 结束组合 scope（与 start_scope 配对）
    pub fn end_scope(&mut self) {
        self.composer.end_scope();
    }

    /// 获取当前正在构建的节点 ID（用于注册选中、焦点等外部状态）
    pub fn current_node_id(&self) -> Option<u64> {
        self.composer.current_node_id()
    }

    pub fn layout_root(&self) -> Option<&crate::layout::node::LayoutNode> {
        self.composer.layout_root()
    }

    /// 设置当前选区注册表（由 SelectionContainer::build 调用）
    pub fn set_selection_registrar(&mut self, reg: crate::text::selection::SelectionRegistrar) {
        self.composer.selection_registrar = Some(reg);
    }

    pub fn clear_selection_registrar(&mut self) {
        self.composer.selection_registrar = None;
    }

    /// 给当前节点设 registrar 引用（供后续渲染/事件从中读取）。
    /// 组合期（物化前）写入当前 slot 的 desc——物化时应用到 arena 节点
    /// （组合/布局分离后 node_stack 在组合期为空，直接写节点会丢失）。
    pub fn set_current_node_registrar(&mut self, reg: crate::text::selection::SelectionRegistrar) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.registrar = Some(reg);
        }
    }

    /// 设置当前节点的光标位置和可见性，同时设置光标回调。
    /// ⚠ 组合/布局分离：组合期没有 arena 节点（node_stack 已废弃）——
    /// 写入当前 slot 的 desc，物化时应用到节点（与 focus_color 同一通道）。
    pub fn set_current_node_cursor_and_callback(
        &mut self,
        cursor_index: usize,
        visible: bool,
        callback: Box<dyn Fn(usize) + Send>,
    ) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.cursor_index = Some(cursor_index);
            desc.cursor_visible = Some(visible);
            desc.cursor_callback = Some(callback);
        }
    }

    /// 设置当前节点的焦点环颜色（组合期调用——主题色在此捕获；
    /// 渲染期 CompositionLocal 已退出，不能读主题）
    pub fn set_current_node_focus_color(&mut self, color: crate::graphics::Color) {
        // 组合/布局分离：组合期没有 arena 节点——写入当前 slot 的 desc，
        // 物化时应用到节点（与 set_current_node_registrar 同一通道）
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.focus_color = Some(color);
        }
    }

    /// 设置当前节点的 IME 组合下划线颜色（组合期调用——主题 primary 在此捕获；
    /// 渲染期 CompositionLocal 已退出，不能读主题，Phase 4.2）
    pub fn set_current_node_composing_color(&mut self, color: crate::graphics::Color) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.composing_color = Some(color);
        }
    }

    /// 设置当前节点的显示聚焦标记（text-field-v2 容器化：焦点在容器，
    /// 输入子节点用此标记渲染光标/选区）
    pub fn set_current_node_display_focused(&mut self, focused: bool) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.display_focused = Some(focused);
        }
    }

    /// animateFloatAsState — 动画浮点值到目标值
    pub fn animate_float_as_state(&mut self, target: f32, spec: crate::animation::AnimationSpec) -> State<f32> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// Compose's `Modifier.animateContentSize`, as a composition-owned modifier helper.
    ///
    /// Apply it to the node whose measured size should spring toward the size its content
    /// measured. The animation State cannot live in a pure `Modifier`, so this method creates the
    /// two handles during composition and returns the modifier to attach.
    pub fn animate_content_size(
        &mut self,
        spec: impl Into<crate::animation::AnimationSpec>,
    ) -> crate::modifier::Modifier {
        // `next_remember_key` is the stable statement key; using it explicitly lets two calls in
        // one group keep separate state while remaining stable across recomposition. The target
        // uses a mixed sibling key so it can never collide with another call's value slot.
        let key = self.next_remember_key();
        let value: crate::runtime::state::State<crate::unit::Size> =
            self.remember_at_key(key, || crate::unit::Size::ZERO);
        let target: crate::runtime::state::Backchannel<Option<crate::unit::Size>> =
            self.remember_backchannel_at_key(crate::runtime::composer::mix_key(key, 1), || {
                crate::runtime::state::Backchannel::new(None)
            });
        crate::modifier::Modifier::new()
            .animate_content_size(value, target, spec.into())
            .clip(crate::graphics::Shape::Rectangle)
    }

    /// `animateFloatAsState(targetValue, animationSpec, label, finishedListener)` — the completion
    /// callback Compose's overload carries, fired once when a motion this call registered finishes.
    ///
    /// Prefer this over remembering a state and driving it with the free `push_animatable_with_done`:
    /// the state here is created and owned by the slot, so no second mechanism can land on it and make
    /// the dedup swallow the callback. An interrupted motion does not report (the retarget replaces its
    /// callback, as Compose's `animateTo` replaces its block).
    pub fn animate_float_as_state_with_done(
        &mut self,
        target: f32,
        spec: crate::animation::AnimationSpec,
        done: impl FnOnce() + Send + 'static,
    ) -> State<f32> {
        self.animate_value_as_state_with_done(target, spec, done).into_state()
    }

    /// `animateValueAsState` + the completion callback (Compose's `finishedListener`); any
    /// `AnimatableValue` works. See `animate_float_as_state_with_done` for the semantics.
    pub fn animate_value_as_state_with_done<
        T: Clone + PartialEq + crate::animation::AnimatableValue + Send + Sync + 'static,
    >(
        &mut self,
        target: T,
        spec: crate::animation::AnimationSpec,
        done: impl FnOnce() + Send + 'static,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target.clone())
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_with_done(
            State::from_raw(handle.as_raw().clone()),
            target,
            spec,
            done,
        );
        handle
    }

    /// animateColorAsState — 动画颜色值到目标值（RGBA 插值，Tween 驱动）
    pub fn animate_color_as_state(&mut self, target: crate::graphics::Color, spec: crate::animation::AnimationSpec) -> State<crate::graphics::Color> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target)
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_color(
            crate::runtime::state::State::from_raw(handle.as_raw().clone()),
            target,
            spec,
        );
        handle.into_state()
    }

    /// animateDpAsState — 动画 Dp 值（对标 Compose animateDpAsState）
    pub fn animate_dp_as_state(&mut self, target: crate::unit::Dp, spec: crate::animation::AnimationSpec) -> State<crate::unit::Dp> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateOffsetAsState — 动画 Offset 值（对标 Compose animateOffsetAsState）
    pub fn animate_offset_as_state(&mut self, target: crate::unit::Offset, spec: crate::animation::AnimationSpec) -> State<crate::unit::Offset> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateSizeAsState — 动画 Size 值（对标 Compose animateSizeAsState）
    pub fn animate_size_as_state(&mut self, target: crate::unit::Size, spec: crate::animation::AnimationSpec) -> State<crate::unit::Size> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateIntAsState — 动画整数值（对标 Compose animateIntAsState）
    pub fn animate_int_as_state(&mut self, target: i32, spec: crate::animation::AnimationSpec) -> State<i32> {
        self.animate_value_as_state(target, spec).into_state()
    }

    /// animateValueAsState — 泛型值动画（对标 Compose animateValueAsState——
    /// 任何实现 AnimatableValue 的类型：lerp/to_f32/from_f32）
    pub fn animate_value_as_state<T: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        target: T,
        spec: crate::animation::AnimationSpec,
    ) -> crate::runtime::state::Animating<T> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::runtime::state::Animating::new(target.clone())
        });
        self.own_motion(handle.state_id());
        crate::animation::push_animatable_handle(handle.clone(), target, spec);
        handle
    }

    /// 设置当前节点的 IME 预输入回调
    /// 设置当前节点的 IME 预输入回调（app.rs 的 Ime::Preedit 直接调用）。
    /// desc 通道（组合期无 arena 节点）
    pub fn set_current_node_ime_callback(&mut self, callback: Box<dyn Fn(&str, Option<(usize, usize)>) + Send>) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.ime_callback = Some(callback);
        }
    }

    /// 同步 composing_range 到当前节点（渲染画下划线用）。desc 通道——
    /// 外层 Option 区分"未设置"（非 TextField）与"清空"（组合结束）
    pub fn sync_composing_range(&mut self, range: Option<std::ops::Range<usize>>) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.composing_range = Some(range);
        }
    }

    /// 获取选区注册表
    pub fn selection_registrar(&self) -> Option<crate::text::selection::SelectionRegistrar> {
        self.composer.selection_registrar.clone()
    }

    /// 为 remember 调用生成位置 key。
    ///
    /// 位置 key 编码方式: 基于 slot 树路径（结构稳定——不随 Enter/Skip 的
    /// next_key 序列漂移，保证同一组合位置跨重组复用同一 State）。
    fn next_remember_key(&mut self) -> u64 {
        // key 基与 next_group_key 一致：显式 key() > 语句 id（源码位置）。
        // remember 的 State 跨帧稳定依赖 key 稳定——结构变化时语句 id 不动 → State 保留。
        // 无稳定源 → panic（fail-fast）。
        let base = match self.composer.try_stable_base() {
            Some(b) => b,
            None if cfg!(test) => {
                // 测试路径：路径哈希 fallback（同 next_group_key——测试自控结构）
                let path = self.composer.slot_table.current_path().to_vec();
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in &path {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            None => self.composer.panic_no_stable_key("remember"),
        };
        let counter = self.composer.remember_path_counters.entry(base).or_insert(0);
        let c = *counter;
        *counter += 1;
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_REMEMBER_TRACE").is_ok() {
            eprintln!(
                "[remember] compose={} base={:#x} n={} key={:#x} stmt={:?}",
                self.composer.compose_count,
                base,
                c,
                crate::runtime::composer::mix_key(base, c as u64),
                STMT_STACK.with(|s| s.borrow().last().copied())
            );
        }
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵。⚠ 不能用
        // (base << 32) | c（左移丢弃 base 高 32 位）或 base 高 32 位 | c
        // （丢弃 base 低 32 位 → 身份只剩 2^32 空间——checkbox 循环子项碰撞）。
        crate::runtime::composer::mix_key(base, c as u64)
    }

    /// 开始一个布局节点（叶子组件如 Text 使用）
    pub fn start_leaf(&mut self, key: u64, modifier: Modifier) {
        self.composer.start_node(key, modifier, None, None);
    }

    /// 开始一个容器节点（布局组件如 Button/Column 使用）
    pub fn start_container(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) {
        self.composer
            .start_node(key, modifier, Some(Box::new(policy)), None);
    }

    /// Start a leaf node that carries an `on_remove` callback — one that needs a NODE. For a cleanup
    /// with no layout meaning, use [`crate::effect::on_forget`] instead.
    ///
    /// The distinction is not cosmetic. A node created here is a real child of the enclosing
    /// container, so it takes part in layout and consumes a spacing slot: an effect hung on a leaf
    /// between two rows of a `Column(spacing = 10)` pushed the second row from y=29 to y=39. That is
    /// exactly why `effect::on_forget` exists — it keeps the callback in the slot and materializes
    /// nothing (`docs/effects.md` §2).
    ///
    /// Reach for this only when the callback belongs to a node's actual lifetime — `Window` and
    /// `remember_infinite_transition` both used it for a while and both were migrated away, which is
    /// a fair signal that "I have a cleanup callback" is not a reason to use it.
    pub fn start_leaf_with_remove(
        &mut self,
        key: u64,
        modifier: Modifier,
        on_remove: Box<dyn FnOnce() + Send>,
    ) {
        self.composer.start_node(key, modifier, None, Some(on_remove));
    }

    /// 结束当前节点
    pub fn end_node(&mut self) {
        self.composer.end_node();
    }

    /// 开始一个可重启的组合分组（容器节点）。
    /// 返回 Enter（正常执行闭包）或 Skip（跳过内容，从缓存重放子树）。
    pub fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: impl MeasurePolicy + 'static,
    ) -> GroupStatus {
        self.composer
            .start_restartable_group(key, modifier, Some(Box::new(policy)), None)
    }

    /// 结束一个可重启分组。
    /// 在 Enter 模式下等同于 end_node()；
    /// 在 Skip 模式下重放子 slot 结构并创建 stub LayoutNode。
    pub fn end_restartable_group(&mut self) {
        self.composer.end_restartable_group();
    }
}

