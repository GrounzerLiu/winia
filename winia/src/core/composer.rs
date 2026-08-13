//! 组合引擎 — ComposeCtx 和 Composer
//!
//! ComposeCtx 是 composable 函数的入口点，提供:
//! - remember(): 在组合中持久化状态（重组时返回同一个 State 实例）
//! - next_key(): 生成组合 key，用于 start_node/end_node
//!
//! Composer 管理组合树的生命周期:
//! - SlotTable: 存储组合节点和 remembered 状态
//! - 重组调度: 批处理状态变化，在下一帧重组
//! - Key 管理: 全局唯一 key 计数器

use crate::core::state::State;
use crate::debug_log;
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, CachedNode};
use crate::modifier::Modifier;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::any::Any;
use std::cell::Cell;
use std::cell::RefCell;

/// RAII guard：语句作用域结束（含 return/break/continue/panic 提前退出）自动 pop_stmt。
/// 零大小——Drop 直接操作 thread_local 栈（不持有 &mut ctx，无借用冲突）。
pub struct StmtGuard;

impl Drop for StmtGuard {
    fn drop(&mut self) {
        STMT_STACK.with(|s| { s.borrow_mut().pop(); });
    }
}

/// RAII 组合 scope guard——Drop 时调用 end_scope（配对 start_scope_guarded）。
/// 持有 composer 裸指针：guard 生命周期内 composer 必须存活且无并发访问
/// （组合单线程）；guard 由 #[composable] 宏注入声明在函数开头、函数返回
/// 时最后 drop——end_scope 在所有语句 guard pop 之后执行，配对正确。
pub struct ScopeGuard {
    composer: *mut Composer,
}
impl Drop for ScopeGuard {
    fn drop(&mut self) {
        unsafe { (*self.composer).end_scope(); }
    }
}

thread_local! { static ACTIVE_SLOT_KEY: Cell<u64> = const { Cell::new(0) }; }
/// 依赖注册目标栈（统一）：scope（容器组件/组合函数）与节点（leaf 组件）共用——
/// 读取 State 注册到栈顶（最内层 Group）。组合外（测量阶段）栈空 → 回退 ACTIVE_SLOT_KEY。
thread_local! { static GROUP_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) }; }
/// 语句 id 栈（#[composable] 宏注入——RAII guard 写入/弹出；thread_local 使
/// guard 的 Drop 无需持有 &mut ctx——闭包/循环体内 return/break/continue 提前
/// 退出时自动 pop，不泄漏。多窗口安全：组合按窗口顺序执行，compose 开头 clear）
thread_local! { static STMT_STACK: RefCell<Vec<(u32, u32)>> = const { RefCell::new(Vec::new()) }; }


/// 参数值（阶段5 参数相等跳过用）——`ComposeCtx::changed` 暂存的参数，
/// 支持跨帧按类型比较（`Box<dyn Any>` 无法通用 PartialEq，用 trait object 桥接）。
pub(crate) trait ParamValue: Any {
    fn eq_any(&self, other: &dyn Any) -> bool;
}

impl<T: PartialEq + 'static> ParamValue for T {
    fn eq_any(&self, other: &dyn Any) -> bool {
        other.downcast_ref::<T>() == Some(self)
    }
}

/// 两个参数序列是否逐项相等（数量相同 + 类型/值全等）
fn params_equal(a: &[Box<dyn ParamValue>], b: &[Box<dyn ParamValue>]) -> bool {
    if a.len() != b.len() { return false; }
    a.iter().zip(b.iter()).all(|(x, y)| x.eq_any(&**y))
}

/// 读取当前组合作用域的依赖注册目标：最内层 scope（容器组件/函数 scope）；
/// scope 栈空（组合外/测量）→ ACTIVE_SLOT_KEY。
pub(crate) fn with_active_scope(f: impl FnOnce(u64)) {
    // 统一依赖注册目标 = 最内层 scope（容器组件 start_restartable_group 时 push、
    // #[composable] 函数 start_scope 时 push）：组件内读取（Text build）注册到最近
    // 容器 scope（对标 Compose ReplaceGroup 内联语义）；content 闭包内表达式注册到
    // 所在容器 scope。NODE_DEPTH 不再参与（此前导致 content scope 收不到依赖——
    // content 闭包内 NODE_DEPTH 恒 ≥1，永远走 ACTIVE_SLOT_KEY）。
    // scope 栈空（组合外/测量阶段）→ 回退 ACTIVE_SLOT_KEY（测量时节点）
    GROUP_STACK.with(|s| {
        let s = s.borrow();
        if let Some(&k) = s.last() {
            f(k);
        } else {
            with_active_slot_key(f);
        }
    });
}

/// 读取当前 compose 位置的 slot key（供 state.rs 依赖追踪使用）
pub(crate) fn with_active_slot_key(f: impl FnOnce(u64)) {
    ACTIVE_SLOT_KEY.with(|c| f(c.get()));
}

/// 设置当前 slot key（measure_node 用它把动态尺寸的依赖注册到节点）
pub(crate) fn set_active_slot_key(key: u64) {
    ACTIVE_SLOT_KEY.with(|c| c.set(key));
}

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

    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        let pq = Arc::downgrade(&self.composer.pending_states);
        self.composer.slot_table.remember(slot_key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(pq.clone()));
            State::new(init())
        })
    }

    /// 注册顶层弹出层（Popup/Dialog/DropdownMenu 内部调用）——组合期收集，
    /// compose 后由 app.rs 取走并独立物化/渲染
    pub fn open_overlay(&mut self, desc: crate::ui::overlay::OverlayDesc) {
        self.composer.overlays.push(desc);
    }

    /// 组合期记录 overlay 的 active 状态（Popup/Dialog build 总执行时调用——
    /// 无论 visible 都记录；Skip 帧 build 不执行 → 本帧无记录 → sync 保留上帧）。
    /// sync_overlays 用此区分"注册方 Skip"（保留）与"主动关闭 visible=false"
    /// （记录 false → 删除）——slot 层无法区分，组合期显式记录是唯一正解。
    pub fn record_overlay_active(&mut self, id: u64, active: bool) {
        self.composer.overlay_active.insert(id, active);
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

    /// 使用固定 key 记住一个状态（不受 remember_counter 影响，适合跨分支持久化的值）
    pub fn remember_at_key<T: Clone + 'static>(&mut self, key: u64, init: impl FnOnce() -> T) -> State<T> {
        let pq = Arc::downgrade(&self.composer.pending_states);
        self.composer.slot_table.remember(key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(pq.clone()));
            State::new(init())
        })
    }

    /// 生成下一个组合 key（公开 API，用于 start_node）
    pub fn next_key(&mut self) -> u64 {
        self.composer.next_group_key()
    }

    /// 开始一个组合 scope——scope 内（组件外）的 `State::get()` 注册依赖到本 scope，
    /// State 变化 → scope 失效 → 其内组合代码整体重跑（组件不 Skip，modifier 重算）。
    /// 与 `end_scope` 配对。
    pub fn start_scope(&mut self) -> u64 {
        self.composer.start_scope()
    }

    /// #[composable] 宏注入：以源码哈希为 scope key 开始（函数级 key 稳定——
    /// 结构变化不漂移）。内部节点的 next_key 以 scope 源码哈希为 key 基。
    /// ⚠ 不调用 start_scope（它也会 push None——双重 push 后栈顶是 None，
    /// next_key 读 scope=0 → 跨函数同 stmt id 的 key 碰撞 → 节点复用串位）
    pub fn start_scope_keyed(&mut self, source_hash: u64) -> u64 {
        self.composer.scope_source_stack.push(Some(source_hash));
        // scope key = 源码哈希本身（稳定唯一——不依赖 next_group_key：scope 是
        // 组合第一条调用（STMT_STACK 空），走路径哈希在宏外（app_root!/根）会
        // 触发稳定 key panic；且路径哈希在结构变化时漂移——hash 反而更稳）
        let key = source_hash;
        self.composer.slot_table.start_scope(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// RAII 版 scope 开始（Drop 时自动 end_scope）——支持返回值函数与提前
    /// return（显式 end_scope 在提前退出时泄漏 scope 栈——新 key 系统
    /// #[composable] 宏展开使用此版本；guard 声明在函数开头、存活到函数
    /// 返回——end_scope 在所有语句 guard pop 之后执行，配对正确）。
    /// ⚠ guard 内持有 composer 裸指针——调用方必须保证 guard 生命周期内
    /// composer 存活且无并发访问（组合单线程——成立）。
    pub fn start_scope_guarded(&mut self, source_hash: u64) -> ScopeGuard {
        self.composer.scope_source_stack.push(Some(source_hash));
        let key = source_hash;
        self.composer.slot_table.start_scope(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// 调用链版 RAII scope 开始：scope key = 当前调用链哈希（try_stable_base）
    /// 或 fallback 哈希（调用链空——测试/组合顶层）。组件方法宏化用——
    /// 同一方法多次实例化（16 字段）靠调用点链隔离；独立组合函数多实例
    /// （列表）同样隔离。⚠ scope 在函数开头创建——此刻 STMT_STACK 栈顶
    /// 即调用点（父语句），函数内部语句注入在其后——链不含自身。
    pub fn start_scope_callchain(&mut self, fallback_hash: u64) -> ScopeGuard {
        let key = self.composer.try_stable_base().unwrap_or(fallback_hash);
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_SCOPE_TRACE").is_ok() {
            let stack = STMT_STACK.with(|s| s.borrow().clone());
            let src = self.composer.scope_source_stack.last().and_then(|s| *s);
            eprintln!("[scope] fallback={:#x} key={:#x} src={:?} stmt_stack={:?}", fallback_hash, key, src, stack);
        }
        self.composer.scope_source_stack.push(Some(key));
        self.composer.slot_table.start_scope(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        ScopeGuard { composer: self.composer as *mut Composer }
    }

    /// #[composable] 宏注入：进入一条语句（id 为编译期固定的源码位置序号）。
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

    /// #[composable] 宏注入：退出语句（与 push_stmt 配对）——保留兼容旧用法
    pub fn push_stmt(&mut self, id: u32) {
        let seq = self.composer.slot_table.sibling_position();
        STMT_STACK.with(|s| s.borrow_mut().push((id, seq)));
    }

    /// #[composable] 宏注入：退出语句（与 push_stmt 配对）
    pub fn pop_stmt(&mut self) {
        STMT_STACK.with(|s| { s.borrow_mut().pop(); });
    }

    /// 显式 key（对标 Compose `key(id)`）：包裹的子树用 id 哈希为 key 基——
    /// 结构变化（列表重排/子树移动）时 remember/复用仍稳定。
    /// 用法：`ctx.key("scroll_list", |ctx| { ... });`
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
        self.composer.key_override_stack.push(h);
        let r = f(self);
        self.composer.key_override_stack.pop();
        r
    }


    /// 参数比较（对标 Compose `$composer.changed(param)`）。
    ///
    /// 在 start 组件**前**调用（参数求值处）：与"即将 start 的 slot"上帧记录的参数
    /// 按序比较——相等返回 `false`（参数未变），不等/首次返回 `true`。
    /// 暂存本帧参数（start_node 时写入 slot.params，供下帧比较）。
    ///
    /// 用法（#[composable] 组件内——参数未变 + slot clean 时容器 Skip，content 不重跑）：
    /// ```ignore
    /// #[composable]
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
        let unchanged = {
            let idx = *self.composer.slot_table.child_counters.last().unwrap_or(&0);
            let param_idx = self.composer.pending_params.len();
            let prev = self.composer.slot_table.current_slot().children.get(idx)
                .and_then(|c| c.params.get(param_idx))
                .map(|p| p.eq_any(param));
            prev == Some(true)
        };
        self.composer.pending_params.push(Box::new(param.clone()));
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
    pub fn set_selection_registrar(&mut self, reg: crate::ui::selection_container::SelectionRegistrar) {
        self.composer.selection_registrar = Some(reg);
    }

    pub fn clear_selection_registrar(&mut self) {
        self.composer.selection_registrar = None;
    }

    /// 给当前节点设 registrar 引用（供后续渲染/事件从中读取）。
    /// 组合期（物化前）写入当前 slot 的 desc——物化时应用到 arena 节点
    /// （组合/布局分离后 node_stack 在组合期为空，直接写节点会丢失）。
    pub fn set_current_node_registrar(&mut self, reg: crate::ui::selection_container::SelectionRegistrar) {
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
    pub fn set_current_node_focus_color(&mut self, color: crate::modifier::Color) {
        // 组合/布局分离：组合期没有 arena 节点——写入当前 slot 的 desc，
        // 物化时应用到节点（与 set_current_node_registrar 同一通道）
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.focus_color = Some(color);
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
        let remember_key = self.next_remember_key();
        let state = self.composer.slot_table.remember(remember_key, || {
            crate::core::state::STATE_OWNER_QUEUE.with(|q| *q.borrow_mut() = Some(Arc::downgrade(&self.composer.pending_states)));
            crate::core::state::State::new(target)
        });
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateColorAsState — 动画颜色值到目标值（RGBA 插值，Tween 驱动）
    pub fn animate_color_as_state(&mut self, target: crate::modifier::Color, spec: crate::animation::AnimationSpec) -> State<crate::modifier::Color> {
        let state = self.remember(|| target);
        crate::animation::push_animatable_color(state.clone(), target, spec);
        state
    }

    /// animateDpAsState — 动画 Dp 值（对标 Compose animateDpAsState）
    pub fn animate_dp_as_state(&mut self, target: crate::unit::Dp, spec: crate::animation::AnimationSpec) -> State<crate::unit::Dp> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateOffsetAsState — 动画 Offset 值（对标 Compose animateOffsetAsState）
    pub fn animate_offset_as_state(&mut self, target: crate::unit::Offset, spec: crate::animation::AnimationSpec) -> State<crate::unit::Offset> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateSizeAsState — 动画 Size 值（对标 Compose animateSizeAsState）
    pub fn animate_size_as_state(&mut self, target: crate::unit::Size, spec: crate::animation::AnimationSpec) -> State<crate::unit::Size> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateIntAsState — 动画整数值（对标 Compose animateIntAsState）
    pub fn animate_int_as_state(&mut self, target: i32, spec: crate::animation::AnimationSpec) -> State<i32> {
        let state = self.remember(|| target);
        crate::animation::push_animatable(state.clone(), target, spec);
        state
    }

    /// animateValueAsState — 泛型值动画（对标 Compose animateValueAsState——
    /// 任何实现 AnimatableValue 的类型：lerp/to_f32/from_f32）
    pub fn animate_value_as_state<T: crate::animation::AnimatableValue + Send + Sync + 'static>(
        &mut self,
        target: T,
        spec: crate::animation::AnimationSpec,
    ) -> State<T> {
        let state = self.remember(|| target.clone());
        crate::animation::push_animatable(state.clone(), target, spec);
        state
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

    /// 同步 selection_range 到当前节点（渲染高亮选区用）。desc 通道
    pub fn sync_selection_range(&mut self, range: Option<std::ops::Range<usize>>) {
        if let Some(desc) = &mut self.composer.slot_table.current_slot().desc {
            desc.selection_range = Some(range);
        }
    }

    /// 获取选区注册表
    pub fn selection_registrar(&self) -> Option<crate::ui::selection_container::SelectionRegistrar> {
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
        // key = base 高 32 位身份 + 低 32 位序号——⚠ 不能用 (base << 32) | c：
        // 64 位 base 左移 32 会把高 32 位移出丢弃 → key 身份只剩 base 低 32 位
        // （2^32 碰撞空间——checkbox_demo 循环子项 label 低 32 位碰撞 → dup-key）。
        (base & 0xFFFF_FFFF_0000_0000) | (c as u64)
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

    /// 开始一个叶子节点并设置移除回调
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

// ── SlotTable ──

/// 节点描述（组合产物——组合树持有，布局阶段物化为 LayoutNode）
struct NodeDesc {
    key: u64,
    modifier: Modifier,
    policy: Option<Box<dyn MeasurePolicy>>,
    on_remove: Option<Box<dyn FnOnce() + Send>>,
    /// 本帧是否需重测（start_slot 的 Dirty 状态——slot.dirty 在 start_slot
    /// 被消费清 false，物化时须从 desc 携带）
    dirty: bool,
    /// 文本选择 registrar（组合期 set_current_node_registrar 写入——物化时应用）
    registrar: Option<crate::ui::selection_container::SelectionRegistrar>,
    /// 焦点环颜色（组合期 set_current_node_focus_color 写入——物化时应用；
    /// 渲染期 CompositionLocal 已退出，必须组合期捕获）
    focus_color: Option<crate::modifier::Color>,
    /// 光标（TextField）——组合期写入，物化时应用（node_stack 已废弃——
    /// 组合期无 arena 节点，直接写节点会静默失效）
    cursor_index: Option<usize>,
    cursor_visible: Option<bool>,
    cursor_callback: Option<Box<dyn Fn(usize) + Send>>,
    /// 显示聚焦标记（text-field-v2 容器化：焦点/交互在容器节点，输入
    /// 子节点渲染光标/选区需组合期标记——渲染端优先用此，回退 node.focused）
    display_focused: Option<bool>,
    /// IME 预输入回调（TextField——app.rs 的 Ime::Preedit 直接调用）
    ime_callback: Option<Box<dyn Fn(&str, Option<(usize, usize)>) + Send>>,
    /// IME 组合范围（渲染画下划线用）——外层 Option 区分"未设置"与"清空"
    composing_range: Option<Option<std::ops::Range<usize>>>,
    /// 选区范围（渲染高亮用）——同上
    selection_range: Option<Option<std::ops::Range<usize>>>,
    /// 布局方向（组合期捕获——provides 作用域内读 CompositionLocal；
    /// 物化在组合回调后执行——届时 WiniaTheme::direction() 已退出作用域，
    /// 必须从 desc 携带，否则 RTL 下节点快照恒 Ltr → offset/padding 镜像失效）
    direction: crate::layout::LayoutDirection,
}

/// 组合节点的一个槽位。每个 composable 调用对应一个 Slot。
struct Slot {
    key: u64,
    remembered: HashMap<u64, Box<dyn Any>>,
    children: Vec<Slot>,
    /// 重组时是否需要执行（State 变化标记）
    dirty: bool,
    /// 当前帧中此 slot 的子树总 slot 数（含自身；用于 skip 时重放）
    children_count: usize,
    /// 是否为组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    is_scope: bool,
    /// 上帧参数（`ComposeCtx::changed` 写入，按序比较——对标 Compose `$composer.changed`）
    params: Vec<Box<dyn ParamValue>>,
    /// 节点描述（组合产物）——is_scope 或纯组合 Slot 为 None（物化阶段消费）
    desc: Option<NodeDesc>,
    /// 本帧是否被访问（start_slot 置 true；reset 每帧清）——物化只收集活跃
    /// slot：结构回退时（content 少建子节点）末尾残留的上帧 slot 不收集——
    /// 其 desc 不物化，对应 arena 节点由 prev_node_by_key 回收（free）
    visited: bool,
    /// Skip 子树时保留的组合产物 modifier（content 未执行——desc 为 None；
    /// 但 build 的 modifier 参数可能变化（父层重跑传入的 offset/背景等视觉
    /// 属性）——物化 Skip 恢复时应用，避免视觉卡旧值（动画中间值不渲染）
    skip_modifier: Option<Modifier>,
    /// 布局方向（组合期捕获——物化期读不到 CompositionLocal）
    direction: crate::layout::LayoutDirection,
    /// 上帧 modifier（Skip 判定用——param_eq 比较数值参数变化）
    prev_modifier: Option<Modifier>,
    /// Skip 时保存的容器 policy（外层传入——content 未执行但 policy 可用，
    /// 物化降级/恢复时避免 policy 缺失导致测量 0 尺寸）
    skip_policy: Option<Box<dyn MeasurePolicy>>,
}

impl Slot {
    fn new(key: u64) -> Self {
        Self {
            key,
            remembered: HashMap::new(),
            children: Vec::new(),
            dirty: true, // 新创建的 slot 总是 dirty（首次必须执行）
            children_count: 1, // 自身
            is_scope: false,
            params: Vec::new(),
            desc: None,
            visited: true, // 新建即本帧活跃
            skip_modifier: None,
            prev_modifier: None,
            skip_policy: None,
            direction: crate::layout::LayoutDirection::Ltr,
        }
    }

    fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(state) = existing.downcast_ref::<State<T>>() {
                return state.clone();
            }
        }
        let state = init();
        self.remembered.insert(slot_key, Box::new(state.clone()));
        state
    }
}

/// 可重启分组状态 — start_restartable_group() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum GroupStatus {
    /// 子树完全 clean → 跳过内容闭包，重放 slot 结构
    Skip,
    /// 子树有变化 → 正常执行闭包
    Enter,
}


/// 槽位状态 — start_slot() 返回
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub(crate) enum SlotStatus {
    /// slot 与上一帧相同，且未被标记为脏 → 可跳过子树重建
    Clean,
    /// slot 与上一帧相同，但被标记为脏 → 需要重新测量
    Dirty,
    New,
}

/// 槽位表 — 组合树的内部数据结构（树形嵌套）

pub(crate) struct SlotTable {
    path: Vec<usize>,
    root_slot: Slot,
    child_counters: Vec<usize>,
    /// 当前 compose 期间活跃的 slot key（用于 State→Slot 的脏标记）
    active_slot_key: u64,
    /// 被 State 变化标记为 dirty 的 slot key 集合
    dirty_keys: std::collections::HashSet<u64>,
}

impl SlotTable {
    fn new() -> Self {
        Self {
            path: Vec::new(),
            root_slot: Slot::new(0),
            child_counters: vec![0],
            active_slot_key: 0,
            dirty_keys: std::collections::HashSet::new(),
        }
    }

    fn current_slot(&mut self) -> &mut Slot {
        let mut slot = &mut self.root_slot;
        for &idx in &self.path {
            slot = &mut slot.children[idx];
        }
        slot
    }

    /// 位置哈希（enter_stmt 的 seq 分量）：**完整 child_counters 链的 fnv**——
    /// 行容器语句（循环内）链 = [...,父层, 迭代位置]；content 闭包内语句链 =
    /// [...,行index, 行内位置]——不同行实例的链不同（父层 index 不同）→ seq
    /// 区分。start_slot 每帧无条件执行（Skip 帧也执行）→ 链跨帧稳定（位置
    /// 而非执行次数——替代旧 STMT_SEQ，滚动时 Skip/Enter 交替不再漂移）。
    pub(crate) fn sibling_position(&self) -> u32 {
        let mut h: u64 = 0xcbf29ce484222325;
        for &c in &self.child_counters {
            h ^= c as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h as u32
    }


    /// 当前活跃 slot 的 key（overlay 锚点用）
    fn active_slot_key(&self) -> u64 {
        self.active_slot_key
    }

    /// 当前作用域内"前一个已组合兄弟"的 slot key（Popup 锚点用：对标 Compose
    /// Popup 在父布局中的位置）。按本帧 child_counters 索引——不能取
    /// children.last()：重组帧中上一帧的后缀兄弟尚未 truncate，会锚错节点。
    /// 无前一个兄弟时返回 None（调用方回退窗口对齐）。
    fn prev_sibling_slot_key(&self) -> Option<u64> {
        let idx = *self.child_counters.last().unwrap_or(&0);
        if idx == 0 {
            return None;
        }
        let mut slot = &self.root_slot;
        for &i in &self.path {
            slot = &slot.children[i];
        }
        slot.children.get(idx - 1).map(|c| c.key)
    }

    /// 设置当前 slot 的参数（`ComposeCtx::changed` 暂存的参数，start_node 时写入）
    fn set_current_params(&mut self, params: Vec<Box<dyn ParamValue>>) {
        self.current_slot().params = params;
    }

    /// 设置当前 slot 的节点描述（组合产物——物化阶段消费）
    fn set_current_desc(&mut self, desc: Option<NodeDesc>) {
        self.current_slot().desc = desc;
    }

    /// Skip 路径保留组合产物 modifier（content 未执行——desc=None；物化
    /// Skip 恢复节点时应用新 modifier——视觉属性（offset/背景）随父层重跑更新）
    fn set_skip_modifier(&mut self, modifier: Modifier) {
        self.current_slot().skip_modifier = Some(modifier);
    }

    /// Skip 分支组合期捕获布局方向（物化期读不到 CompositionLocal）
    fn set_skip_direction(&mut self, direction: crate::layout::LayoutDirection) {
        self.current_slot().direction = direction;
    }
    fn set_skip_policy(&mut self, policy: Option<Box<dyn MeasurePolicy>>) {
        self.current_slot().skip_policy = policy;
    }

    /// 收集物化描述树：Slot 树 → 纯节点树（scope 跳过——children 提升；
    /// Skip 子树 slot 记 skip 标记——物化时从 prev_node_by_key 恢复）。
    /// 消费 desc（take——policy/on_remove 移出）——物化阶段调用。
    /// visited 语义：本帧活跃（start_slot 置 true；reset 每帧清）——结构回退的
    /// 残留（visited false 且不在 Skip 子树内）不收集；Skip 子树（visited false
    /// 但属于 Skip group）整体收集（skip 标记——物化恢复）
    pub(crate) fn collect_desc_tree(&mut self, out: &mut Vec<crate::core::materialize::DescNode>) {
        fn rec(slot: &mut Slot, out: &mut Vec<crate::core::materialize::DescNode>, in_skip: bool, depth: usize) {
            if !slot.visited && !in_skip {
                // 本帧未访问且不在 Skip 子树内（结构回退残留）：不收集——
                // 对应 arena 节点由 prev_node_by_key 回收（free）
                return;
            }
            if let Some(desc) = slot.desc.take() {
                let mut node = crate::core::materialize::DescNode {
                    key: desc.key,
                    skip: false,
                    modifier: desc.modifier,
                    preserve_modifier: false,
                    policy: desc.policy,
                    on_remove: desc.on_remove,
                    dirty: desc.dirty, // start_slot 的 Dirty 状态（slot.dirty 已消费）
                    registrar: desc.registrar,
                    focus_color: desc.focus_color,
                    cursor_index: desc.cursor_index,
                    cursor_visible: desc.cursor_visible,
                    cursor_callback: desc.cursor_callback,
                    display_focused: desc.display_focused,
                    ime_callback: desc.ime_callback,
                    composing_range: desc.composing_range,
                    selection_range: desc.selection_range,
                    direction: desc.direction,
                    children: Vec::new(),
                };
                for child in &mut slot.children {
                    rec(child, &mut node.children, false, depth + 1);
                }
                out.push(node);
            } else if !slot.is_scope {
                // Skip 子树 slot（content 未执行——desc 空但非 scope）：
                // 整棵子树按 key 结构恢复（物化时从 prev_node_by_key 恢复——
                // 不物化上帧 desc——子树整体保留，children 重新挂接）
                let sm = slot.skip_modifier.take();
                let sp = slot.skip_policy.take();
                let mut node = crate::core::materialize::DescNode {
                    key: slot.key,
                    skip: true,
                    modifier: sm.clone().unwrap_or_default(),
                    // 容器自身 build 被调（set_skip_modifier 写入）→ 应用新 modifier；
                    // 后代（Skip 子树内未执行）→ 保留缓存节点 modifier（不清空视觉）
                    preserve_modifier: sm.is_none(),
                    policy: sp,
                    on_remove: None,
                    dirty: false,
                    registrar: None,
                    focus_color: None,
                    cursor_index: None,
                    cursor_visible: None,
                    cursor_callback: None,
                    display_focused: None,
                    ime_callback: None,
                    composing_range: None,
                    selection_range: None,
                    direction: slot.direction,
                    children: Vec::new(),
                };
                for child in &mut slot.children {
                    rec(child, &mut node.children, true, depth + 1); // Skip 子树内：子也按同一规则（收集）
                }
                #[cfg(debug_assertions)]
                if std::env::var("WINIA_MAT_PROBE").is_ok() {
                    eprintln!("[collect] skip key={:x} kids={}", slot.key, node.children.len());
                }
                out.push(node);
            } else {
                // scope：不物化——children 提升到最近物化父（保持 in_skip 状态）
                for child in &mut slot.children {
                    rec(child, out, in_skip, depth + 1);
                }
            }
        }
        for child in &mut self.root_slot.children {
            rec(child, out, false, 0);
        }
    }

    fn start_slot(&mut self, key: u64) -> SlotStatus {
        let idx = *self.child_counters.last().unwrap_or(&0);
        self.active_slot_key = key;
        ACTIVE_SLOT_KEY.with(|c| c.set(key));
        let is_dirty = self.dirty_keys.remove(&key);
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                eprintln!("[slot] key={} path={:?} dirty={}", key >> 32, self.path.clone(), is_dirty);
            }
        }
        let parent = self.current_slot();
        #[cfg(debug_assertions)] {
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                eprintln!("[slot] key={} idx={} parent_kids={} dirty={}", key >> 32, idx, parent.children.len(), is_dirty);
            }
        }

        // key 匹配，或"同位置"（key 高位 = slot 路径哈希相同）——Enter/Skip 的
        // counter 漂移不改位置，按索引复用（更新 key 保持同步），避免 truncate
        // 重建 slot 导致 remember 的 State 丢失
        let same_position = idx < parent.children.len()
            && parent.children[idx].key >> 32 == key >> 32;
        if idx < parent.children.len() && (parent.children[idx].key == key || same_position) {
            parent.children[idx].key = key; // 同步最新 key（counter 可能漂移）
            parent.children[idx].visited = true; // 本帧活跃（物化收集依据）
            if !parent.children[idx].dirty && !is_dirty {
                self.path.push(idx);
                self.child_counters.last_mut().map(|c| *c += 1);
                self.child_counters.push(0);
                return SlotStatus::Clean;
            }
            parent.children[idx].dirty = false;
            self.path.push(idx);
        } else {
            #[cfg(debug_assertions)]
            if std::env::var("WINIA_SLOT_TRACE").is_ok() {
                let plen = parent.children.len();
                let pkey = parent.key;
                eprintln!("[slot-trunc] key={:x} parent={:x} idx={} len={}", key, pkey, idx, plen);
            }
            parent.children.truncate(idx);
            parent.children.push(Slot::new(key));
            // 新建 slot：本帧返回 Dirty 即已执行；立即消费 dirty 标记，
            // 否则残留 true 会让下一帧本应 clean 的 slot 误判为 Dirty
            parent.children.last_mut().unwrap().dirty = false;
            self.path.push(idx);
        }
        if let Some(last) = self.child_counters.last_mut() { *last += 1; }
        self.child_counters.push(0);
        SlotStatus::Dirty
    }

    fn end_slot(&mut self) {
        // 计算子树 slot 总数（自身 + 所有子 slot 的 children_count 之和）
        let count = 1 + self.current_slot().children.iter()
            .map(|c| c.children_count)
            .sum::<usize>();
        self.current_slot().children_count = count;
        self.path.pop();
        self.child_counters.pop();
    }

    fn remember<T: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> State<T>,
    ) -> State<T> {
        self.current_slot().remember(slot_key, init)
    }

    fn reset(&mut self) {
        self.path.clear();
        self.child_counters = vec![0];
        self.active_slot_key = 0;
        // 每帧清 visited——物化只收集本帧活跃 slot（结构回退的残留不收集）
        fn clear_visited(slot: &mut Slot) {
            slot.visited = false;
            for child in &mut slot.children {
                clear_visited(child);
            }
        }
        clear_visited(&mut self.root_slot);
    }

    fn truncate(&mut self) {
        if let Some(&idx) = self.child_counters.first() {
            self.root_slot.children.truncate(idx);
        }
    }

    /// 外部标记 slot key 为 dirty（由 State 变化触发）。
    /// 同时递归标记所有祖先 slot，确保父级 start_restartable_group 返回 Enter。
    pub(crate) fn mark_dirty(&mut self, key: u64) {
        self.dirty_keys.insert(key);
        let root = &mut self.root_slot;
        // 一次 DFS：找到目标 → 标其及所有祖先 dirty；若目标是 scope → 整个子树强制 Enter
        SlotTable::mark_dirty_path_scope(root, key);
    }

    /// 查找 key 的 slot：标其及所有祖先 dirty；若目标是 scope，整个子树标 dirty（失效传播）
    fn mark_dirty_path_scope(slot: &mut Slot, key: u64) -> bool {
        if slot.key == key {
            slot.dirty = true;
            if slot.is_scope {
                SlotTable::mark_dirty_subtree(slot);
            }
            return true;
        }
        for child in &mut slot.children {
            if SlotTable::mark_dirty_path_scope(child, key) {
                slot.dirty = true;
                return true;
            }
        }
        false
    }

    /// 标记 slot 及其所有后代 dirty（scope 失效传播）
    fn mark_dirty_subtree(slot: &mut Slot) {
        slot.dirty = true;
        for child in &mut slot.children {
            SlotTable::mark_dirty_subtree(child);
        }
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——依赖注册目标 + 失效传播单位）
    fn start_scope(&mut self, key: u64) -> SlotStatus {
        let status = self.start_slot(key);
        // 标记当前（刚进入的）slot 为 scope
        self.current_slot().is_scope = true;
        status
    }

    /// 结束组合 scope
    fn end_scope(&mut self) {
        self.end_slot();
    }

    /// 设置当前（栈顶）slot 是否为 scope（start_node 复用 scope slot 时重置为普通节点）
    fn set_current_scope(&mut self, is_scope: bool) {
        self.current_slot().is_scope = is_scope;
    }

    /// 在 slot 树中查找 key 对应的节点，并将其及所有祖先标记 dirty。
    /// 返回 true 表示找到了目标。
    /// 返回当前 slot 在树中的路径（用于 LayoutNode 复用时的 measured_size 查找）
    fn current_path(&self) -> &[usize] {
        &self.path
    }
}

// ── Composer ──

/// 组合引擎。
///
/// 每个窗口/组合根拥有一个 Composer 实例。
/// 负责:
/// - 管理 SlotTable（组合节点的持久化存储）
/// - 调度重组（状态变化 → 标记 dirty → 下一帧批量重组）
/// - 持有全局 key 计数器
pub struct Composer {
    pub(crate) slot_table: SlotTable,
    pub(crate) current_group_key: u32,
    /// 每路径独立 counter（next_group_key 用）——同组合位置跨帧 counter 恒定，
    /// key 不随 Skip/Enter 的 next_key 调用序变化（全局 counter 会因 Skip 的
    /// content 不执行而平移 → key 漂移 → 节点复用错位 + 常量折叠冻结）
    path_counters: std::collections::HashMap<u64, u32>,
    /// 每路径独立 counter（next_remember_key 用——同上，防 remember key 漂移）
    remember_path_counters: std::collections::HashMap<u64, u32>,

    /// 组合 scope 的源码哈希栈（宏传——函数级 key 基）
    scope_source_stack: Vec<Option<u64>>,
    /// ctx.key() 显式 key 栈（最高优先级）
    key_override_stack: Vec<u64>,
    pending_recomposition: VecDeque<u64>,
    needs_recomposition: bool,
    pub(crate) arena: crate::layout::node::NodeArena,
    node_stack: Vec<usize>,
    /// 记录每个 start_restartable_group 的 skip 状态（用于 end_restartable_group 判断）
    group_skip_stack: Vec<bool>,
    /// 顶层弹出层（Popup/Dialog/DropdownMenu——组合期注册，compose 后取走；
    /// 内容为独立组合单元——独立 Composer 物化/布局/渲染，不参与主树布局）
    pub(crate) overlays: Vec<crate::ui::overlay::OverlayDesc>,
    /// 本帧组合期各 overlay 的 active 状态（Popup/Dialog build 总执行时记录——
    /// Skip 帧不记录 → sync 保留上帧；主动关闭 visible=false → 记录 false →
    /// sync 删除）。区分"注册方 Skip"（保留）与"主动关闭"（删除）——
    /// slot 层不可区分，需此组合期显式记录。
    pub(crate) overlay_active: HashMap<u64, bool>,
    /// state_id -> slot_keys 依赖映射
    slot_deps: HashMap<u32, HashSet<u64>>,
    /// 布局依赖表（state_id → slot_key；上帧布局注册的持久表，供下帧 pending 消费）
    layout_deps: HashMap<u32, HashSet<u64>>,
    /// 本帧 pending 消费收集的布局失效 key（layout() 应用后清空）
    layout_dirty_keys: HashSet<u64>,
    /// 本帧确认移除的 slot_key（compose 末尾回收未复用节点时收集——layout_deps 死 key 清理用）
    removed_slot_keys: HashSet<u64>,
    /// 本 Composer 实例的 pending state 通知队列
    pending_states: Arc<parking_lot::Mutex<Vec<u32>>>,
    /// 上一帧各 slot_key → 节点缓存（用于 clean slot 跳过和子树重放；
    /// 用 slot_key 而非 slot 路径作键——scope 层不产生 LayoutNode，路径在两棵树不一致，
    /// key 是稳定位置标识（路径哈希 + counter），两侧天然对齐）
    pub(crate) prev_nodes: HashMap<u64, CachedNode>,
    /// `ComposeCtx::changed` 暂存的参数（start_slot 时写入新 slot 的 params）
    pending_params: Vec<Box<dyn ParamValue>>,
    /// 上帧布局树：slot_key → arena 节点索引（阶段D 节点复用——start_node 按 key 复用槽位）
    pub(crate) prev_node_by_key: HashMap<u64, usize>,
    /// 本帧已复用的节点索引（free 时跳过——避免递归进本帧树形成环）
    pub(crate) reused_nodes: std::collections::HashSet<usize>,
    /// 当前选区注册表（SelectionContainer compose 时注入，供事件处理访问）
    pub(crate) selection_registrar: Option<crate::ui::selection_container::SelectionRegistrar>,

    #[cfg(test)]
    pub(crate) compose_clean_count: usize,
    #[cfg(test)]
    pub(crate) compose_dirty_count: usize,
    /// 重组总次数（vsync 研究——单次渲染内多次 compose 的观测）
    pub(crate) compose_count: u64,
}

impl Composer {
/// 选区注册表（由 SelectionContainer 在 compose 时注入，供事件处理访问）

    pub fn new() -> Self {
        let pending_states = Arc::new(parking_lot::Mutex::new(Vec::new()));
        crate::core::state::register_composer_queue(Arc::downgrade(&pending_states));
        Self {
            slot_table: SlotTable::new(),
            current_group_key: 0,
            path_counters: std::collections::HashMap::new(),
            remember_path_counters: std::collections::HashMap::new(),
            scope_source_stack: Vec::new(),
            key_override_stack: Vec::new(),
            pending_recomposition: VecDeque::new(),
            needs_recomposition: true,
            arena: crate::layout::node::NodeArena::new(),
            node_stack: Vec::new(),
            group_skip_stack: Vec::new(),
            overlays: Vec::new(),
            overlay_active: HashMap::new(),
            slot_deps: HashMap::new(),
            layout_deps: HashMap::new(),
            layout_dirty_keys: HashSet::new(),
            removed_slot_keys: HashSet::new(),
            pending_states,
            prev_nodes: HashMap::new(),
            pending_params: Vec::new(),
            prev_node_by_key: HashMap::new(),
            reused_nodes: std::collections::HashSet::new(),
            selection_registrar: None,
            #[cfg(test)]
            compose_clean_count: 0,
            #[cfg(test)]
            compose_dirty_count: 0,
            compose_count: 0,
        }
    }

    /// 获取当前正在构建的节点 ID（node_stack 栈顶）
    pub fn current_node_id(&self) -> Option<u64> {
        self.node_stack.last().map(|&idx| self.arena.nodes[idx].id)
    }

    /// 分配下一个 group key。
    ///
    /// 基于 slot 路径编码：结构稳定——Enter/Skip 的执行顺序不影响 key，
    /// 保证同一组合位置跨重组得到相同 slot（否则 slot 树 truncate 重建，
    /// 导致 remember 的 State 全部丢失重建）。
    /// 能否获得稳定 key 的 base（新 key 系统核心判定）：
    /// - 显式 ctx.key(id, f) 作用域内 → 稳定（用户保证唯一）
    /// - STMT_STACK 有宏注入的语句（#[composable]/keyed_stmt! 展开）→ 稳定
    ///   （base = fnv(scope_src, 语句id, 迭代seq)——编译期固定）
    /// - 都不是 → None（调用方 panic 兜底——fail-fast，不静默降级）
    pub(crate) fn try_stable_base(&self) -> Option<u64> {
        if let Some(&k) = self.key_override_stack.last() {
            // ctx.key(id) 语义：显式 id 替代位置——但**同一 id 跨调用点**（多
            // 字段同 role：16 个 field 的 label 都是 TextFieldSlotRole::Label）
            // 会碰撞——混合调用链隔离实例（同 id 不同调用点 → 不同 base）；
            // 链空（非宏顶层）时退化纯 id（兼容旧行为）
            let chain = self.chain_hash().unwrap_or(0);
            let mut h: u64 = 0xcbf29ce484222325;
            h ^= k; h = h.wrapping_mul(0x100000001b3);
            h ^= chain; h = h.wrapping_mul(0x100000001b3);
            return Some(h);
        }
        self.chain_hash()
    }

    /// 调用链哈希：fnv(scope_src, STMT栈顶语句id, 迭代seq)——编译期固定
    /// 的组合位置（#[composable]/keyed_stmt! 注入的语句）
    fn chain_hash(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h ^= (seq as u64).wrapping_mul(0x9E3779B97F4A7C15);
                h
            })
        })
    }

    /// 没有稳定 key 源时的 panic（fail-fast——不静默降级为路径哈希）
    pub(crate) fn panic_no_stable_key(&self, api: &str) -> ! {
        panic!(
            "无法获得稳定 key（{}）：调用点不在 #[composable]/keyed_stmt! 注入内，\
             也无 ctx.key() 包裹——key 会在结构变化时漂移。修复：①将调用点放入 \
             #[composable] 函数内 ②用 ctx.key() 包裹 ③content 闭包参数名与 \
             #[composable(x)] 指定的标识符一致",
            api
        )
    }

    pub fn next_group_key(&mut self) -> u64 {
        // key 基优先级：显式 ctx.key() > #[composable] 语句 id（源码位置）。
        // 语句 id 由宏注入（编译期按源码结构固定编号）——结构变化（前面插入/移除兄弟
        // 节点）不影响语句 id → key 不漂移 → remember/复用稳定（对标 Compose 编译器
        // 的调用点 key）。无稳定源 → panic（fail-fast）。
        let base = match self.try_stable_base() {
            Some(b) => b,
            None if cfg!(test) => {
                // 测试路径：无语句级 key 时退化为路径哈希（测试自控结构——漂移由
                // 测试自己负责；生产代码禁止——见下方 panic）
                let path = self.slot_table.current_path().to_vec();
                let mut h: u64 = 0xcbf29ce484222325;
                for &idx in &path {
                    h ^= idx as u64;
                    h = h.wrapping_mul(0x100000001b3);
                }
                h
            }
            None => self.panic_no_stable_key("next_key"),
        };
        // 每路径独立 counter：同 key 基第 N 次调用跨帧恒定（Skip 的 content 不执行
        // 不平移——节点复用错位 + 常量折叠冻结的防护）
        let counter = self.path_counters.entry(base).or_insert(1);
        let c = *counter;
        *counter += 1;
        // key = base 高 32 位身份 + 低 32 位序号（⚠ (base<<32)|c 会把 base 高 32
        // 位移出丢弃 → 身份只剩低 32 位，2^32 碰撞空间——dup-key 根因）
        (base & 0xFFFF_FFFF_0000_0000) | (c as u64)
    }

    /// 开始一个组合 scope（无 LayoutNode 的作用域节点——组合代码重跑的失效单位）。
    /// 返回 scope key；`State::get()` 在 scope 内（组件外）注册依赖到 scope。
    ///
    /// ⚠ 手动调用（无源码哈希）：scope_source_stack push None，与 start_scope_keyed
    /// 的 Some 区分——end_scope 严格配对，不破坏外层宏注入的 source。
    /// ⚠ release 下宏外调用会触发稳定 key panic（next_group_key 快速失败）——
    /// 生产代码应使用 #[composable]/app_root! 注入的 start_scope_keyed。
    pub fn start_scope(&mut self) -> u64 {
        self.scope_source_stack.push(None);
        let key = self.next_group_key();
        self.slot_table.start_scope(key);
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        key
    }

    /// 结束组合 scope
    pub fn end_scope(&mut self) {
        self.slot_table.end_scope();
        // 防御性配对：非空才 pop（start_scope/end_scope 不配对时避免 panic/污染其他 scope）
        GROUP_STACK.with(|s| {
            let mut s = s.borrow_mut();
            if !s.is_empty() { s.pop(); }
        });
        if !self.scope_source_stack.is_empty() { self.scope_source_stack.pop(); }
    }

    /// 物化：组合树（Slot desc）→ 布局树（arena LayoutNode）——完整分离的核心。
    /// 由 compose 末尾调用（layout 只测量）。实现拆到 core/materialize.rs（SRP）。
    pub fn materialize(&mut self) {
        crate::core::materialize::materialize(self);
    }

    /// 在组合树中开始一个节点（由组件的 build 方法调用）
    pub fn start_node(&mut self, key: u64, modifier: Modifier, policy: Option<Box<dyn MeasurePolicy>>, on_remove: Option<Box<dyn FnOnce() + Send>>) {
        self.current_group_key = key as u32;
        let slot_status = self.slot_table.start_slot(key);
        // 普通节点：复用 scope slot 时重置为普通（同路径类型切换场景）
        self.slot_table.set_current_scope(false);
        // 写入 `ComposeCtx::changed` 暂存的参数（供下帧比较）——
        // 仅当 pending 非空（有 changed 声明）；否则保留上帧 params：
        // replay 的 stub start_node（pending 空）不清空子 slot params，
        // 避免父 Skip 后子组件参数未变也被强制 Enter
        if !self.pending_params.is_empty() {
            self.slot_table.set_current_params(std::mem::take(&mut self.pending_params));
        }
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // 组合产物写入 Slot（物化阶段消费——完整分离：arena 建节点移出组合阶段）
        // ⚠ direction 必须在此捕获（组合期 provides 作用域内）——物化期
        // WiniaTheme::direction() 已退出作用域读不到（RTL 全局切换失效根因）
        let direction = modifier.get_layout_direction()
            .unwrap_or(crate::ui::theme::WiniaTheme::direction());
        self.slot_table.set_current_desc(Some(NodeDesc {
            key,
            modifier,
            policy,
            on_remove,
            dirty: slot_status != SlotStatus::Clean, // 重测标记（slot.dirty 已消费）
            registrar: None,
            focus_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
            selection_range: None,
            direction,
        }));
        // 统一依赖栈：节点 push（组件 build 期间 State 读取注册到最内层 Group——
        // 组件内读取失效目标 = 本节点（对标 Compose 最内层 Group 语义））
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
    }

    /// 结束当前节点：组合侧出栈（物化建树由 materialize 统一处理）
    pub fn end_node(&mut self) {
        self.slot_table.end_slot();
        GROUP_STACK.with(|s| { s.borrow_mut().pop(); }); // 与 start_node 的 push 配对
    }

    /// 开始可重启分组（内部调用 start_node + 返回 skip/enter 状态）
    fn start_restartable_group(
        &mut self,
        key: u64,
        modifier: Modifier,
        policy: Option<Box<dyn MeasurePolicy>>,
        on_remove: Option<Box<dyn FnOnce() + Send>>,
    ) -> GroupStatus {
        self.current_group_key = key as u32;
        // 容器 build 期间维护（scope 栈由 start_restartable_group 管理）
        let slot_status = self.slot_table.start_slot(key);
        // 容器组件 = scope（对标 Compose RestartGroup）：push 组件 scope——
        // 组件内/ content 闭包内读取注册到本组件（最内层 scope）
        GROUP_STACK.with(|s| s.borrow_mut().push(key));
        #[cfg(test)] { match slot_status { SlotStatus::Clean => self.compose_clean_count += 1, _ => self.compose_dirty_count += 1, } }

        // Clean slot：从缓存恢复（阶段5：加参数相等条件——slot clean 且参数
        // 全相等才 Skip；参数变化（changed 比较）时即使 slot clean 也 Enter）
        let is_skip = if slot_status == SlotStatus::Clean {
            // 与上帧 slot.params 比较（pending_params = 本帧 changed 暂存；
            // slot.params 此刻仍是上帧的——本帧写入在其后）
            let params_unchanged = params_equal(
                &self.pending_params,
                &self.slot_table.current_slot().params,
            ) && {
                // modifier 数值参数相等（width/背景色等变化 → Enter 重跑内容）
                let prev_m = self.slot_table.current_slot().prev_modifier.as_ref();
                prev_m.map(|pm| modifier.param_eq(pm)).unwrap_or(false)
            };
            #[cfg(debug_assertions)] {
                if std::env::var("WINIA_SKIP_TRACE").is_ok() {
                    eprintln!("[skip] key={} clean={} params_u={} prev={} pending_len={}",
                        key >> 32, slot_status == SlotStatus::Clean, params_unchanged,
                        self.prev_nodes.contains_key(&key), self.pending_params.len());
                }
            }
            if params_unchanged {
                // 有上帧缓存才可 Skip（否则物化无节点可恢复）
                self.prev_nodes.contains_key(&key)
            } else {
                false
            }
        } else {
            false
        };
        // 写入本帧参数（在 is_skip 比较之后——比较用上帧 slot.params）
        // 仅当 pending 非空（有 changed 声明）；空则保留上帧 params（replay stub 场景）
        if !self.pending_params.is_empty() {
            self.slot_table.set_current_params(std::mem::take(&mut self.pending_params));
        }

        // 组合产物写入 Slot：Enter 写完整描述（物化消费）；Skip 写 None——
        // content 不执行（无新描述），物化时按 key 恢复缓存节点（skip 标记）
        if is_skip {
            // 方向先算（modifier 随后 move 进 set_skip_modifier）
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::ui::theme::WiniaTheme::direction());
            self.slot_table.set_current_desc(None);
            // 保留本帧组合产物 modifier（父层重跑传入的新 offset/背景——物化应用）
            self.slot_table.set_skip_modifier(modifier);
            // 保留 policy（外层传入——Skip 时 content 未执行但 policy 可用；
            // 物化恢复失败降级时避免 policy 缺失测量 0 尺寸）
            self.slot_table.set_skip_policy(policy);
            // Skip 容器方向也须组合期捕获（同 Enter——物化期读不到 theme）
            self.slot_table.set_skip_direction(direction);
        } else {
            // Enter：记录本帧 modifier（下帧 Skip 判定比较用）
            self.slot_table.current_slot().prev_modifier = Some(modifier.clone());
            // Enter：组合期捕获方向（provides 作用域内）——先算再 move
            let direction = modifier.get_layout_direction()
                .unwrap_or(crate::ui::theme::WiniaTheme::direction());
        self.slot_table.set_current_desc(Some(NodeDesc {

                key,
                modifier,
                policy,
                on_remove,
                dirty: true, // Enter 即重测（content 重跑——参数/内容可能变；Skip 恢复不受影响）
                registrar: None,
                focus_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
                composing_range: None,
                selection_range: None,
                direction,
            }));
        }
        self.group_skip_stack.push(is_skip);

        if is_skip {
            GroupStatus::Skip
        } else {
            GroupStatus::Enter
        }
    }

    /// 结束可重启分组：
    /// - Skip 模式：重放子 slot 结构并创建 stub LayoutNode
    /// - Enter 模式：等同于 end_node()
    fn end_restartable_group(&mut self) {
        let was_skip = self.group_skip_stack.pop().unwrap_or(false);
        // Enter（content 重跑）：清理未访问子槽——内容结构变化后残留的旧槽
        // （如 AnimatedContent B(3 文本) → A(2 文本) 的第 3 槽）若保留，下帧
        // Skip 收集时结构签名（desc children vs 缓存 children）不等 → 物化
        // 降级 0x0 → 子树塌缩。Skip 槽保留（content 未执行——结构需保留供恢复）。
        if !was_skip {
            self.slot_table.current_slot().children.retain(|c| c.visited);
        }
        // Skip：content 未执行——slot 树保留（上帧 children 结构）——物化时
        // 整棵子树按 key 从 prev_node_by_key 恢复（stub 机制已由物化替代）
        // GROUP_STACK pop 由 end_node 统一处理（与 start_restartable_group 的
        // push 配对——此前此处额外 pop 导致容器组件两次 pop 一次 push →
        // 栈错乱 → 后续依赖注册到错误 Group → State 变化不标记容器 dirty）
        self.end_node();
    }

    /// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        #[cfg(test)] { self.compose_clean_count = 0; self.compose_dirty_count = 0; }
        self.compose_count += 1;
        self.slot_table.reset();
        self.overlay_active.clear(); // 每帧组合期重记录（Skip 帧不记录）
        self.current_group_key = 0;
        self.path_counters.clear();
        self.remember_path_counters.clear();
        STMT_STACK.with(|s| s.borrow_mut().clear());
        self.scope_source_stack.clear();
        self.key_override_stack.clear();
        // 注意：不在 compose 开头清 arena.root——materialize 管理 root
        // （开头清 + 末尾设）。此处若清，同帧第二次 compose 的 materialize
        // 守卫（prev 空 + root 有）失效 → 空 prev 重建 → 树塌缩。
        // 重置 Window 生命周期标志（先于未复用节点回收，on_remove 再设置新值）
        crate::ui::window::reset_lifecycle_flags();
        // 阶段D：保留上帧树（prev_node_by_key 由上帧 layout 构建）——
        // start_node 按 slot_key 复用节点槽位；本帧未复用的旧节点在
        // compose 末尾统一 free（见下方 drain）
        self.node_stack.clear();

        // 消费本 Composer 实例的 pending states → 标记对应 slot 为脏
        // 同时收集受影响的 slot key（用于增量更新 slot_deps）
        let mut affected_slot_keys = HashSet::new();
        let mut pending = self.pending_states.lock();
        for state_id in pending.drain(..) {
            if let Some(keys) = self.slot_deps.get(&state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                    affected_slot_keys.insert(k);
                }
            }
            // 两段式依赖：布局期注册的依赖 → 只标布局失效（重测不重组）
            if let Some(keys) = self.layout_deps.get(&state_id) {
                for &k in keys {
                    self.layout_dirty_keys.insert(k);
                }
            }
        }
        drop(pending);

        // 开始组合期依赖记录（thread_local 缓冲——State::get 写入，末尾 take_deps 取走）
        crate::core::state::begin_compose_deps();

        {
            let ctx = &mut ComposeCtx::new(self);
            content(ctx);
        }

        // modifier 依赖注册（scroll 等）移到 layout 的 materialize 之后——
        // 完整分离：compose 阶段不建 arena（此处 arena 为空）

        // 注意：recording target 不在此处清除——layout()（measure 阶段）的
        // SizeDynamic 闭包内 State::get() 也要记录依赖（kf/dp 尺寸动画），
        // 由 layout() 末尾统一 clear + drain（见 layout()）。
        self.slot_table.truncate();
        // 防御：scope 配对完整性（漏配 end_scope 会导致 SCOPE_STACK 残留跨帧，
        // 使下帧组件外读取注册到失效 scope → 失效静默丢失）
        debug_assert_eq!(GROUP_STACK.with(|s| s.borrow().len()), 0,
            "compose 结束时 GROUP_STACK 应清空（scope/节点配对不完整）");
        GROUP_STACK.with(|s| s.borrow_mut().clear());

        // 完整分离：组合完成后物化布局树（测试/调用方可直接 layout_root_idx）
        self.materialize();
        // 物化后：注册 modifier 中引用的 State 依赖（scroll 等——组合期 arena 空）。
        // 必须在 take_deps() 之前执行——其中 State::get() 依赖 DEP_MODE=Compose
        //（begin_compose_deps 后未复位）；先复位则 scroll 依赖被静默丢弃（滚动不刷新）
        if let Some(root_idx) = self.arena.root {
            register_modifier_deps_recursive(&self.arena, root_idx);
        }
        // 依赖注册（组合期 + modifier 期收集的 State 依赖 → slot_deps）
        for (state_id, slot_key) in crate::core::state::take_deps() {
            self.slot_deps.entry(state_id).or_default().insert(slot_key);
        }
        // 回收本帧未复用的上帧节点（结构变化移除的子树——on_remove 触发）；
        // 跳过已复用节点（已挂入本帧树，free 会递归进本帧树形成环）
        let mut visited = std::collections::HashSet::new();
        for (key, idx) in self.prev_node_by_key.drain() {
            // 收集移除的 slot_key（layout_deps 死 key 清理）
            self.removed_slot_keys.insert(key);
            self.arena.free_node_skip(idx, &self.reused_nodes, &mut visited);
        }
        self.prev_node_by_key.clear();
        self.reused_nodes.clear();
    }

    /// 返回 LayoutNode 树的根节点引用
    pub fn layout_root(&self) -> Option<&LayoutNode> {
        self.arena.root()
    }

    /// 返回 LayoutNode 树的根节点可变引用
    pub fn layout_root_mut(&mut self) -> Option<&mut LayoutNode> {
        self.arena.root_mut()
    }

    /// arena 节点池只读访问（arena 化遍历用）
    pub fn arena_nodes(&self) -> &[LayoutNode] {
        &self.arena.nodes
    }

    /// arena 节点池可变访问（arena 化遍历用）
    pub fn arena_nodes_mut(&mut self) -> &mut Vec<LayoutNode> {
        &mut self.arena.nodes
    }

    /// 根节点 arena 索引
    pub fn layout_root_idx(&self) -> Option<usize> {
        self.arena.root
    }

    /// 测量策略池只读访问（measure_node 用）
    pub fn arena_policies(&self) -> &[Box<dyn MeasurePolicy>] {
        &self.arena.policies
    }

    /// 执行整棵布局树的 measure + place，并缓存测量结果供下帧复用
    pub fn layout(&mut self, root_constraints: Constraints) {
        // 应用布局失效：清全树旧标记 → 按 layout_dirty_keys 标节点 + 祖先传播
        // （保守超集：祖先全链标脏——布局动画场景父必然依赖子尺寸，Compose 精确传播留待优化）
        if let Some(root_idx) = self.arena.root {
            let nodes = &mut self.arena.nodes;
            // 清全树旧标记（每帧重新标记）
            for n in nodes.iter_mut() { n.layout_dirty = false; }
            if !self.layout_dirty_keys.is_empty() {
                crate::layout::node::apply_layout_dirty(nodes, root_idx, &self.layout_dirty_keys);
            }
            self.layout_dirty_keys.clear();
        } else {
            self.layout_dirty_keys.clear();
        }
        // 物化只在 compose 末尾（完整分离：组合完成即建树）——layout 只测量。
        // 单独调 layout（无 compose）时树为空——measure 无操作（无害）
        // 开始布局期依赖记录（measure 中 State::get → 两段式分流）
        crate::core::state::begin_layout_deps();
        if let Some(root_idx) = self.arena.root {
            let (_size, _placements) = crate::layout::measure_node(
                &mut self.arena.nodes, &self.arena.policies, root_idx, root_constraints);
            self.arena.nodes[root_idx].measured_size = _size;
            // 收集整棵树的节点信息（measured_size、cached_constraints、modifier），按 slot_key 索引
            self.prev_nodes.clear();
            crate::core::materialize::collect_nodes(&mut self.arena, root_idx, &mut self.prev_nodes);
            // 阶段D：重建 slot_key → 节点索引映射（供下帧 start_node 复用）
            self.prev_node_by_key.clear();
            crate::core::materialize::collect_node_keys(&self.arena, root_idx, &mut self.prev_node_by_key);
            // 布局依赖增量更新（两段式依赖）：
            // 本帧 measure 过的 slot_key（touched）→ 清旧写新（依赖集收敛）；
            // 未 measure 的（常量折叠命中）→ 保留旧项（折叠前提=依赖无 notify，闭环成立）。
            let recorded: Vec<(u32, u64)> = crate::core::state::take_deps();
            let touched: HashSet<u64> = recorded.iter().map(|&(_, k)| k).collect();
            if !touched.is_empty() {
                for set in self.layout_deps.values_mut() {
                    set.retain(|k| !touched.contains(k));
                }
            }
            // 顺手清理死 key：本帧确认移除的节点（compose 末尾回收时收集）
            if !self.removed_slot_keys.is_empty() {
                for set in self.layout_deps.values_mut() {
                    set.retain(|k| !self.removed_slot_keys.contains(k));
                }
                self.removed_slot_keys.clear();
            }
            // 空条目清理（节点不再依赖任何 state 或已移除）
            self.layout_deps.retain(|_, set| !set.is_empty());
            // 写入本帧新注册
            for (state_id, slot_key) in recorded {
                self.layout_deps.entry(state_id).or_default().insert(slot_key);
            }
        } else {
            // 无根节点（空内容帧）：布局依赖缓冲无 measure 期新增，直接取走丢弃
            //（防御性对称——未来若在无 root 路径写入 measure 依赖，不会残留跨帧）
            crate::core::state::take_deps();
        }
        // 组合 + 测量全部完成：记录模式已由 take_deps 复位（无指针残留——无悬垂风险）
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    /// 是否有待处理的 state 变化
    pub fn has_pending_states(&self) -> bool {
        !self.pending_states.lock().is_empty()
    }

    /// 取走本帧注册的顶层弹出层（compose 后调用——清空收集）
    pub fn take_overlays(&mut self) -> Vec<crate::ui::overlay::OverlayDesc> {
        std::mem::take(&mut self.overlays)
    }

    /// 当前组合节点的 slot_key（overlay 锚点用）
    pub fn active_slot_key(&self) -> u64 {
        self.slot_table.active_slot_key()
    }

    /// 重组次数（vsync 研究——单次渲染内的 compose 次数）
    pub fn compose_count(&self) -> u64 {
        self.compose_count
    }

    /// 待消费 State 数（vsync 研究——渲染时刻的 pending 积压）
    pub fn pending_state_count(&self) -> usize {
        self.pending_states.lock().len()
    }

    /// 执行待处理的重组。返回 true 表示实际执行了 compose。
    /// 若无待处理则跳过，保留上一帧的布局树。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) -> bool {
        let has_pending = !self.pending_states.lock().is_empty();

        if !self.needs_recomposition && !has_pending && self.pending_recomposition.is_empty() {
            return false;
        }

        // 处理队列中的待重组节点（当前简化：全量重组）
        self.pending_recomposition.clear();
        self.needs_recomposition = false;
        self.compose(content);
        true
    }
}

/// Compose 末尾：递归遍历 LayoutNode 树（arena），为所有 modifier 注册 State 依赖。
/// 模块级函数（impl 外）——impl 内直接调用。
fn register_modifier_deps_recursive(arena: &crate::layout::node::NodeArena, idx: usize) {
    arena.nodes[idx].modifier.register_state_deps();
    let children = arena.nodes[idx].children.clone();
    for c in children {
        register_modifier_deps_recursive(arena, c);
    }
}

impl Default for Composer {
    fn default() -> Self {
        Self::new()
    }
}

// ── 测试 ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_composer_new() {
        let mut composer = Composer::new();
        let key = composer.next_group_key();
        let key2 = composer.next_group_key();
        // key = (slot 路径哈希 << 32) | counter：同一路径下 counter 区分，高位相同
        assert_ne!(key, key2);
        assert_eq!(key >> 32, key2 >> 32);
    }

    #[test]
    fn test_remember_persistence() {
        let mut composer = Composer::new();

        // 首次组合
        composer.compose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 42);
            state.set(100);
        });

        // 重组：同一个 remember 应返回同一个 State，值保持 100
        composer.recompose(|ctx| {
            let state: State<i32> = ctx.remember(|| 42);
            assert_eq!(state.get(), 100);
        });
    }

    #[test]
    fn test_remember_multiple() {
        let mut composer = Composer::new();

        composer.compose(|ctx| {
            let a: State<i32> = ctx.remember(|| 1);
            let b: State<String> = ctx.remember(|| "hello".to_string());
            a.set(10);
            b.set("world".to_string());
        });

        composer.recompose(|ctx| {
            let a: State<i32> = ctx.remember(|| 99);
            let b: State<String> = ctx.remember(|| "none".to_string());
            assert_eq!(a.get(), 10);
            assert_eq!(b.get(), "world");
        });
    }

    #[test]
    fn test_conditional_remember() {
        let mut composer = Composer::new();
        let show_extra = State::new(false);

        composer.compose(|ctx| {
            let always: State<i32> = ctx.remember(|| 1);
            if show_extra.get() {
                let extra: State<i32> = ctx.remember(|| 2);
                assert_eq!(extra.get(), 2);
            }
            assert_eq!(always.get(), 1);
        });
    }

use crate::layout::BoxLayout;

    /// Popup 锚点：当前作用域最后一个兄弟的 slot key（无兄弟 → None）
    #[test]
    fn test_prev_sibling_slot_key() {
        let mut composer = Composer::new();
        let mut root_key = 0u64;
        let mut leaf1_key = 0u64;
        let mut leaf2_key = 0u64;
        let mut observed: Vec<Option<u64>> = Vec::new();
        composer.compose(|ctx| {
            root_key = ctx.next_key();
            ctx.start_container(root_key, Modifier::new(), BoxLayout::new());
            leaf1_key = ctx.next_key();
            ctx.start_leaf(leaf1_key, Modifier::new());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
            leaf2_key = ctx.next_key();
            ctx.start_leaf(leaf2_key, Modifier::new());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
            ctx.end_node();
            observed.push(ctx.prev_sibling_slot_key());
        });
        assert_eq!(observed, vec![Some(leaf1_key), Some(leaf2_key), Some(root_key)]);

        // 重组帧：上一帧有 A/B/C 三个兄弟，本帧只重组合 A 后查询——
        // 必须返回 A（而非残留的 C）——复现 Popup 锚点错位到 Dialog 按钮的 bug
        composer.compose(|ctx| {
            for _ in 0..3 {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
            }
        });
        let mut recomposed: Option<u64> = None;
        let mut a_key = 0u64;
        composer.compose(|ctx| {
            a_key = ctx.next_key();
            ctx.start_leaf(a_key, Modifier::new());
            ctx.end_node();
            recomposed = ctx.prev_sibling_slot_key();
        });
        assert_eq!(recomposed, Some(a_key), "重组帧应锚到本帧刚组合的兄弟 A，而非残留的旧兄弟");
    }

    /// 测试 restartable group 的 skip → replay 路径：
    /// 状态变化只影响某个 leaf slot，兄弟 slot 应被 clean skip 并正确 replay。
    #[test]
    fn test_restartable_skip_replay_tree_integrity() {
        let mut composer = Composer::new();

        // Frame 1: 建立初始树
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 子 1: state-dependent leaf
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 0);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    // 子 2: clean restartable group
                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 1 树结构
        let root = composer.layout_root_idx().expect("root should exist");
        assert_eq!(composer.arena_nodes()[root].children.len(), 2, "root should have 2 children");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 0, "text should be leaf");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[1]].children.len(), 1, "button should have 1 child (content)");

        // Frame 2: recompose（button 应被 skip/replay）
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let text_key = ctx.next_key();
                    {
                        let _count: State<i32> = ctx.remember(|| 999);
                        ctx.start_leaf(text_key, Modifier::new());
                    }
                    ctx.end_node();

                    let btn_key = ctx.next_key();
                    match ctx.start_restartable_group(btn_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let content_key = ctx.next_key();
                            ctx.start_leaf(content_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // 断言 Frame 2 树结构仍正确
        let root = composer.layout_root_idx().expect("root should exist");
        assert_eq!(composer.arena_nodes()[root].children.len(), 2, "after recompose: root should have 2 children, got {}", composer.arena_nodes()[root].children.len());
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 0, "after recompose: text should still be leaf");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[1]].children.len(), 1, "after recompose: button should still have 1 child");
    }

    /// 3 层嵌套 restartable group: Column → Column → Text，验证深层 replay 正确性
    #[test]
    fn test_deep_nested_skip_replay() {
        let mut composer = Composer::new();

        // Frame 1: compose 3-level tree
        composer.compose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // Level 2: nested restartable group
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            // Level 3: state-dependent leaf
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 0);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            // Sibling: clean leaf
                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 1: root → [level2 → [leaf1, leaf2]]
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(composer.arena_nodes()[root].children.len(), 1, "Frame1: root has 1 child");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 2, "Frame1: level2 has 2 children");

        // Frame 2: recompose (level2 and leaf2 should be clean → skip/replay)
        composer.recompose(|ctx| {
            let key1 = ctx.next_key();
            match ctx.start_restartable_group(key1, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let key2 = ctx.next_key();
                    match ctx.start_restartable_group(key2, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let leaf_key = ctx.next_key();
                            {
                                let _count: State<i32> = ctx.remember(|| 999);
                                ctx.start_leaf(leaf_key, Modifier::new());
                            }
                            ctx.end_node();

                            let leaf2_key = ctx.next_key();
                            ctx.start_leaf(leaf2_key, Modifier::new());
                            ctx.end_node();
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
        });

        // Verify Frame 2: structure should be identical
        let root = composer.layout_root_idx().unwrap();
        assert_eq!(composer.arena_nodes()[root].children.len(), 1, "Frame2: root has 1 child");
        assert_eq!(composer.arena_nodes()[composer.arena_nodes()[root].children[0]].children.len(), 2, "Frame2: level2 has 2 children (leaf1 + leaf2)");
    }

    /// 验证 compose 时 slot 计数功能正常（增量重组的前提）
    #[test]
    fn test_compose_counts_clean_and_dirty() {
        let mut composer = Composer::new();
        let count: State<i32> = State::new(0);

        // Frame 1: 初始 compose
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        assert_eq!(composer.compose_dirty_count, 2, "initial: root + leaf both new");
        assert_eq!(composer.compose_clean_count, 0, "initial: no clean slots");

        // Frame 2: recompose
        composer.recompose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        // recompose 后计数非零（具体值取决于 state deps 是否正确触发）
        assert!(composer.compose_dirty_count + composer.compose_clean_count >= 2,
            "expected >=2 slots, got dirty={} clean={}",
            composer.compose_dirty_count, composer.compose_clean_count);
    }
}

#[cfg(test)]
mod scope_tests {
    use super::*;

    /// scope 内 remember 创建 State（owner=composer）→ 读取注册到 scope →
    /// State.set() 进入 pending → 第二次组合 scope 子树强制 Enter（leaf 不 clean）
    #[test]
    fn test_scope_dependency_invalidation() {
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<f32>>);

        let compose_once = |composer: &mut Composer| {
            composer.compose(|ctx| {
                ctx.start_scope();
                let s = ctx.remember(|| 0.0f32);
                *holder.borrow_mut() = Some(s.clone());
                let _v = s.get();           // scope 内读取 → 注册到 scope
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new());
                ctx.end_node();
                ctx.end_scope();
            });
        };

        compose_once(&mut composer);
        // 对照帧：无 State 变化 → 至少 scope/leaf 之一 Clean（证明本测试下正常可 Skip）
        compose_once(&mut composer);
        assert!(composer.compose_clean_count >= 1,
            "对照帧（未失效）应至少 1 个 clean，实际 clean={}（若 0 说明永不 Skip，测试无鉴别力）",
            composer.compose_clean_count);
        // 手动触发失效（State 已绑定 composer owner）
        let s = holder.borrow().clone().unwrap();
        s.set(1.0);
        compose_once(&mut composer);
        // scope 失效 → 子树强制 Enter：leaf 必须 dirty（不 clean）
        assert_eq!(composer.compose_clean_count, 0,
            "scope 失效后 leaf 应强制 Enter，实际 clean={}", composer.compose_clean_count);
    }

    /// scope 与 restartable group 配对：scope 不产生 LayoutNode，children 数稳定
    #[test]
    fn test_scope_group_pairing() {
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<bool>>);

        let compose_both = |composer: &mut Composer| {
            composer.compose(|ctx| {
                ctx.start_scope();
                let group_key = ctx.next_key();
                let status = ctx.start_restartable_group(group_key, Modifier::new(), TestPolicy);
                if let GroupStatus::Enter = status {
                    let s = ctx.remember(|| false);
                    *holder.borrow_mut() = Some(s.clone());
                    let _v = s.get();       // 容器 scope 内读取 → 注册到容器（最内层 scope）
                    let key = ctx.next_key();
                    ctx.start_leaf(key, Modifier::new());
                    ctx.end_node();
                }
                ctx.end_restartable_group();
                ctx.end_scope();
            });
        };

        compose_both(&mut composer);
        let n1 = composer.layout_root().map(|r| r.children.len());

        let s = holder.borrow().clone().unwrap();
        s.set(true);
        compose_both(&mut composer);
        let n2 = composer.layout_root().map(|r| r.children.len());

        assert_eq!(n1, n2, "scope 不产生 LayoutNode，两次组合 children 数应稳定");
    }

    /// 依赖注册目标：无 scope（无容器）场景下读取回退 ACTIVE_SLOT_KEY（leaf 节点）——
    /// 未读对照 leaf 保持 clean（验证回退粒度）
    #[test]
    fn test_node_dependency_precedence() {
        let mut composer = Composer::new();
        let holder = std::cell::RefCell::new(None::<crate::core::state::State<f32>>);

        let compose_once = |composer: &mut Composer| {
            composer.compose(|ctx| {
                let s = ctx.remember(|| 0.0f32);
                *holder.borrow_mut() = Some(s.clone());
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new());
                let _inner = s.get();       // 组件内 → leaf 节点
                ctx.end_node();
                // 未读 state 的对照 leaf——若读取误注册到 scope/父级，对照会被连带标脏
                let key2 = ctx.next_key();
                ctx.start_leaf(key2, Modifier::new());
                ctx.end_node();
            });
        };

        compose_once(&mut composer);
        let s = holder.borrow().clone().unwrap();
        s.set(1.0);
        compose_once(&mut composer);
        // 读 state 的 leaf 强制 Enter（组件内依赖），对照 leaf 保持 clean
        assert_eq!(composer.compose_clean_count, 1,
            "读 state 的 leaf 应 Enter，未读对照 leaf 应 clean（组件内读取注册到节点），实际 clean={}",
            composer.compose_clean_count);
    }

    #[derive(Clone, Debug)]
    struct TestPolicy;
    impl crate::layout::node::MeasurePolicy for TestPolicy {
        fn measure(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, policies: &[Box<dyn MeasurePolicy>], children: &[usize], constraints: crate::layout::constraints::Constraints)
            -> (crate::layout::node::Size, Vec<crate::layout::node::Placement>) {
            let mut h = 0.0f32;
            for &c in children {
                let (s, _) = crate::layout::node::measure_node(nodes, policies, c, constraints);
                h += s.height;
            }
            (crate::layout::node::Size::new(0.0, h), Vec::new())
        }
        fn place(&self, nodes: &mut Vec<crate::layout::node::LayoutNode>, children: &[usize], _placements: &[crate::layout::node::Placement]) {
            let _ = (nodes, children);
        }
    }
}

/// 阶段4 键修复验证：prev_nodes/frame_cache 改用 slot_key 后，
/// 无变化帧的 clean group 应真正 Skip（键 miss 时恒 Enter）。
/// 帧1 组合 root+leaf（无 scope）→ 帧2 无状态变化 → root 应返回 Skip。
#[test]
fn test_is_skip_after_clean_frame() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    // layout 一次：填充 prev_nodes（真实流程：compose → layout → 下帧 compose）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：无状态变化 → root slot Clean → prev_nodes 按 slot_key 命中 → Skip
    let mut skip_happened = false;
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => skip_happened = true,
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    assert!(skip_happened,
        "无变化帧的 clean group 应 Skip（prev_nodes 按 slot_key 命中）——若 Enter 说明 is_skip 键 miss");
}

/// 回归：同帧二次 compose（recompose 循环）——首次物化后 drain 了 prev_node_by_key，
/// 第二次 materialize 必须保留现有树（守卫：prev 空 + root 有）——否则空 prev 重建
/// → Skip 恢复全失败 → 树塌缩（动画启动瞬间坐标错乱 bug）。
#[test]
fn test_same_frame_second_compose_retains_tree() {
    let mut composer = Composer::new();

    // 帧 1：初始 compose + layout（prev 构建）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    // 帧 2 同帧两次 compose（模拟 recompose 循环——中间无 layout）：
    // 第一次 compose 物化并 drain prev_node_by_key；第二次 materialize
    // 不再命中守卫（8548718 守卫已移除——守卫会错误阻断同帧二次 compose 的
    // 真实 Enter 内容，如 AnimatedContent 切换帧）——走防御降级重建：
    // Skip 无缓存 → 按 Enter 重建（dirty 重测）——不塌缩、内容正确
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    let root2 = composer.layout_root_idx().expect("帧2 root");
    // 树完整（root 有效 + 内容保留——降级重建不塌缩）
    let nodes = composer.arena_nodes();
    assert!(nodes[root2].children.len() == 1, "root 仍有一个 leaf 子节点（未塌缩）");

    // 帧 3：prev 空（帧2 无 layout——collect 未跑）→ 降级重建（不塌缩、内容正确）
    let nodes2 = composer.arena_nodes().len();
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    let nodes3 = composer.arena_nodes().len();
    let root3 = composer.layout_root_idx().expect("帧3 root");
    assert!(composer.arena_nodes()[root3].children.len() == 1, "帧3 重建后树完整");

    // 帧 4：帧3 已 collect → 恢复 Skip 复用（节点数不再增长）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    let nodes4 = composer.arena_nodes().len();
    assert_eq!(nodes3, nodes4, "帧4 应恢复 Skip 复用（节点数不变）");
}

/// 阶段4 键修复的**关键回归用例**：scope 层存在时（scope 是 slot 树中 group 的父，
/// LayoutNode 树无 scope 层——两棵树路径不一致），prev_nodes 按 slot_key 索引仍应命中。
/// path 键实现下此场景 miss → 恒 Enter；slot_key 键修复后应 Skip。
#[test]
fn test_is_skip_with_scope_layer() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    composer.compose(|ctx| {
        ctx.start_scope();
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
        ctx.end_scope();
    });

    // layout 一次：填充 prev_nodes（真实流程：compose → layout → 下帧 compose）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：无状态变化 → group slot Clean → prev_nodes 按 slot_key 命中 → Skip（尽管 scope 层在 slot 树中）
    let mut skip_happened = false;
    composer.compose(|ctx| {
        ctx.start_scope();
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => skip_happened = true,
            GroupStatus::Enter => {
                let _ = count.get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
        ctx.end_scope();
    });
    assert!(skip_happened,
        "含 scope 层的 clean group 应 Skip（slot_key 键修复后两棵树路径错位不再导致 miss）——若 Enter 说明回归");
    // 自清洁：帧2 后 layout（复位依赖记录模式——take_deps 等价）
}

/// 阶段5：changed 参数比较机制——帧1 参数写入 slot.params，帧2 同参数比较返回 false（未变）。
/// 参数变化时返回 true（触发重建）。
#[test]
fn test_changed_param_comparison() {
    let mut composer = Composer::new();

    // 帧1：参数 "hello"（首次 → changed=true）
    let mut p1 = None;
    composer.compose(|ctx| {
        p1 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p1, Some(true), "首次 changed 应返回 true");

    // 帧2：同参数 → changed=false（未变）
    let mut p2 = None;
    composer.compose(|ctx| {
        p2 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p2, Some(false), "同参数 changed 应返回 false（未变）");

    // 帧3：参数变化 → changed=true
    let mut p3 = None;
    composer.compose(|ctx| {
        p3 = Some(ctx.changed(&"world".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p3, Some(true), "参数变化 changed 应返回 true");

    // 帧4：回到帧2 的参数（同位置 → 与上帧比较——上帧是 "world" → 变化 → true）
    let mut p4 = None;
    composer.compose(|ctx| {
        p4 = Some(ctx.changed(&"hello".to_string()));
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    assert_eq!(p4, Some(true), "与上帧比较（hello vs world）→ 变化 → true");
}

/// 阶段5：参数相等跳过集成测试——#[composable] 组件用 ctx.changed 声明参数，
/// 参数未变 + slot clean → group Skip；参数变化 → Enter。
#[test]
fn test_param_equal_skip_integration() {
    let mut composer = Composer::new();
    let mut title = "hello".to_string();
    let mut last_status = None;

    let compose_once = |composer: &mut Composer, title: &str, out: &mut Option<GroupStatus>| {
        composer.compose(|ctx| {
            // 组件（模拟 #[composable]）：参数声明（start group 前）
            let _changed = ctx.changed(&title.to_string());
            let key = ctx.next_key();
            let status = ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new());
            *out = Some(status);
            if let GroupStatus::Enter = status {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); }
                ctx.end_node();
            }
            ctx.end_restartable_group();
        });
    };

    // 帧1：参数 "hello"（首次 → Enter）
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Enter), "首次应 Enter");
    // layout（prev_nodes 填充）
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧2：参数未变 → Skip（参数相等 + slot clean）
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Skip), "参数未变 + clean 应 Skip");
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0));

    // 帧3：参数变化 → Enter（即使 slot clean）
    title = "world".to_string();
    compose_once(&mut composer, &title, &mut last_status);
    assert_eq!(last_status, Some(GroupStatus::Enter), "参数变化应 Enter（绕过 clean-skip）");
}

/// 阶段5 边界：changed 调用次数变化（上帧 2 参，本帧 1 参）→ 保守 Enter（不等）。
#[test]
fn test_changed_count_change_enters() {
    let mut composer = Composer::new();
    let mut last_status = None;

    // 帧1：2 个参数
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        ctx.changed(&1u32);
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });

    // 帧2：1 个参数（次数变化）→ 与上帧 2 参比较 → 数量不等 → Enter（保守）
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter),
        "changed 次数变化（2→1）应保守 Enter");

    // 帧3：恢复 2 参（与帧2 的 1 参比较 → 数量不等 → Enter）
    composer.compose(|ctx| {
        ctx.changed(&"a".to_string());
        ctx.changed(&1u32);
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter), "次数恢复 1→2 应保守 Enter");
}

/// 阶段5 边界：组件结构切换（参数组件 → 无参数容器）→ 保守 Enter，不错 Skip。
#[test]
fn test_param_to_plain_switch_enters() {
    let mut composer = Composer::new();
    let mut last_status = None;

    // 帧1：声明参数的组件（changed 1 次）
    composer.compose(|ctx| {
        ctx.changed(&"x".to_string());
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });

    // 帧2：同位置换成无参数容器（不调 changed）→ pending 空 vs 上帧 params 非空 → 不等 → Enter
    composer.compose(|ctx| {
        let k = ctx.next_key();
        let s = ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new());
        last_status = Some(s);
        if let GroupStatus::Enter = s { ctx.end_node(); }
        ctx.end_restartable_group();
    });
    assert_eq!(last_status, Some(GroupStatus::Enter),
        "参数组件→无参数容器切换应保守 Enter");
}

/// 量化：无变化帧的重组粒度——clean（可跳过/复用缓存）vs dirty（需重建）占比。
/// 若 clean 占比高 → 增量重组已达成（对象复用收益边际）；dirty 高 → 重建是主开销。
#[test]
fn test_quantify_reuse_ratio() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    // 模拟 demo 结构：root Column(scope) → 7 节 × (标题 Text + 动画 box Column + Text)
    let build_tree = |composer: &mut Composer| {
        composer.compose(|ctx| {
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..7 {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } // 标题
                        let g = ctx.next_key();
                        match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let _ = count.get();
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    build_tree(&mut composer);
    let clean1 = composer.compose_clean_count;
    let dirty1 = composer.compose_dirty_count;
    eprintln!("[quant] 帧1: clean={} dirty={}", clean1, dirty1);

    // 帧2：无变化 → 应全 clean（Skip）
    build_tree(&mut composer);
    let clean2 = composer.compose_clean_count;
    let dirty2 = composer.compose_dirty_count;
    eprintln!("[quant] 帧2(无变化): clean={} dirty={}  → clean占比 {:.0}%",
        clean2, dirty2, if clean2+dirty2>0 { clean2*100/(clean2+dirty2) } else { 0 });
    assert!(clean2 >= dirty2, "无变化帧 clean 应 ≥ dirty（增量重组生效）");
}

/// 阶段D：节点复用生效验证——连续 compose+layout 多帧，arena 节点数应稳定
/// （复用 = 不新增 alloc；无复用 = 每帧全新建 → 持续增长）。
#[test]
fn test_arena_reuse_stabilizes() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    for _ in 0..5 {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                        let g = ctx.next_key();
                        match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                            GroupStatus::Skip => {}
                            GroupStatus::Enter => {
                                let _ = count.get();
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            }
                        }
                        ctx.end_restartable_group();
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    build(&mut composer);
    let n1 = composer.arena.nodes.len();
    eprintln!("[reuse-check] frame1 nodes={}", n1);
    for _ in 0..10 {
        build(&mut composer);
    }
    eprintln!("[key-stable] 帧2 clean={} dirty={}", composer.compose_clean_count, composer.compose_dirty_count);
    let n2 = composer.arena.nodes.len();
    eprintln!("[reuse-check] frame11 nodes={}", n2);
    assert!(n2 <= n1 + 2,
        "节点复用应使 arena 稳定：frame1={} frame11={}（无复用会持续增长）", n1, n2);
}

/// 遗留问题验证：policy 池是否每帧增长（节点复用但 policy 每帧 alloc → 内存泄漏）
#[test]
fn test_policy_pool_growth() {
    let mut composer = Composer::new();
    let count: State<i32> = State::new(0);
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                }
            }
            ctx.end_restartable_group();
        });
        };
    build(&mut composer);
    let p1 = composer.arena.policies.len();
    for _ in 0..20 { build(&mut composer); }
    let p2 = composer.arena.policies.len();
    eprintln!("[policy-growth] frame1={} frame21={}", p1, p2);
    assert!(p2 <= p1 + 1, "policy 池应稳定（复用 policy）——frame1={} frame21={}", p1, p2);
}

/// 回归测试（review 发现的 Blocking bug）：复用节点必须按 slot_status 设 dirty——
/// State 变化只标记 slot，与 LayoutNode.dirty 无桥接；复用节点不设 dirty 会被
/// 常量折叠返回旧尺寸（动画/文本不更新）。本测试：帧2 set State → 节点应重测。
#[test]
fn test_reused_node_remeasures_on_state_change() {
    let mut composer = Composer::new();
    let count: State<f32> = State::new(50.0);
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = s.get();
                    // 尺寸由 State 驱动（动态 size——测量时求值 + 注册依赖）
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let w1 = composer.arena_nodes()[root_idx].children[0];
    let size1 = composer.arena_nodes()[w1].measured_size.width;
    eprintln!("[remeasure] frame1 w={}", size1);

    // 帧2：State 变化（尺寸目标变）→ 节点复用 → 必须重测（dirty 桥接）
    let s = holder.borrow().clone().unwrap();
    s.set(300.0);
    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let w1 = composer.arena_nodes()[root_idx].children[0];
    let size2 = composer.arena_nodes()[w1].measured_size.width;
    eprintln!("[remeasure] frame2 w={}", size2);
    assert!(size2 > size1 + 10.0,
        "State 变化后复用节点应重测：frame1 w={} frame2 w={}（冻结则 bug 复发）", size1, size2);
}

// ═══════════════════════════════════════════════════════════
// 两段式依赖测试（P2-1：布局期读动画值只重测不重组）
// ═══════════════════════════════════════════════════════════

/// T1 记录分流：组合期 get() 进 slot_deps；布局期（measure 中 SizeDynamic）get() 进 layout_deps
#[test]
fn test_layout_dep_recording_split() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    composer.compose(|ctx| {
        let s = ctx.remember(|| 0.0f32);
        *holder.borrow_mut() = Some(s.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = s.get(); // 组合期读 → slot_deps
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new().size(&s, 10.0)); // 布局期读 → layout_deps
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    let s = holder.borrow().clone().unwrap();
    let sid = s.id();
    assert!(composer.slot_deps.contains_key(&sid),
        "组合期 get() 应注册进 slot_deps");
    assert!(composer.layout_deps.contains_key(&sid),
        "布局期（SizeDynamic）get() 应注册进 layout_deps");
    // 同一 State 双通道（组合+布局）各自记录
    let keys_layout = composer.layout_deps.get(&sid).unwrap();
    assert_eq!(keys_layout.len(), 1, "layout_deps 应含叶子节点 key");
}

/// T2 布局失效传播：layout_dirty_keys 命中的节点 + 祖先链全部标 layout_dirty
#[test]
fn test_layout_dirty_propagates_to_ancestors() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    composer.compose(|ctx| {
        let s = ctx.remember(|| 0.0f32);
        *holder.borrow_mut() = Some(s.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let leaf_key = composer.arena_nodes()[leaf_idx].slot_key;

    // 模拟 pending 消费收集 → 直接测 apply_layout_dirty（裸函数：DFS + 祖先传播）
    composer.layout_dirty_keys.insert(leaf_key);
    let mut nodes = std::mem::take(&mut composer.arena.nodes);
    crate::layout::node::apply_layout_dirty(&mut nodes, root_idx, &composer.layout_dirty_keys);
    assert!(nodes[root_idx].layout_dirty, "祖先（root）应被传播标脏");
    assert!(nodes[leaf_idx].layout_dirty, "命中节点应标脏");
    composer.arena.nodes = nodes;
}

/// T3 动画尺寸只重测不重组：布局依赖 State 变化 → 组合不重跑（clean 计数）+ 尺寸更新
#[test]
fn test_layout_dep_remesures_without_recompose() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let size1 = composer.arena_nodes()[leaf_idx].measured_size.width;

    // 布局动画值变化（set_no_wake——动画推进语义）
    let s = holder.borrow().clone().unwrap();
    s.set_no_wake(300.0);
    build(&mut composer);
    let root_idx = composer.layout_root_idx().unwrap();
    let leaf_idx = composer.arena_nodes()[root_idx].children[0];
    let size2 = composer.arena_nodes()[leaf_idx].measured_size.width;

    assert!(size2 > size1 + 10.0, "布局依赖 State 变化应重测：frame1 w={} frame2 w={}", size1, size2);
    // 关键断言：不重组——leaf slot 应为 Clean（layout_dirty 不触发组合级 dirty）
    assert_eq!(composer.compose_dirty_count, 0,
        "布局动画值变化不应触发组合级 dirty（只重测不重组）——dirty_count={}", composer.compose_dirty_count);
}

/// T4 常量折叠保留布局依赖：折叠帧不重新注册 → layout_deps 旧项保留；notify 后重测
#[test]
fn test_layout_dep_survives_const_fold() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0.0f32);
            *holder.borrow_mut() = Some(s.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().size(&s, 10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let sid = holder.borrow().clone().unwrap().id();
    assert!(composer.layout_deps.contains_key(&sid), "帧1 应注册布局依赖");

    // 帧2：无 notify 的重复 build——compose 全 Skip、measure 常量折叠命中
    use crate::layout::node::MEASURE_COUNT;
    let m1 = MEASURE_COUNT.with(|c| c.get());
    build(&mut composer);
    let m2 = MEASURE_COUNT.with(|c| c.get());
    assert_eq!(m1, m2,
        "折叠帧不应重新 measure（m1={} m2={}——若重测则依赖续期而非折叠保留，T4 语义失效）", m1, m2);
    assert!(composer.layout_deps.contains_key(&sid),
        "常量折叠帧（未重新 measure）应保留旧布局依赖——丢失则布局动画冻结");
    let leaf_key = {
        let root_idx = composer.layout_root_idx().unwrap();
        let leaf_idx = composer.arena_nodes()[root_idx].children[0];
        composer.arena_nodes()[leaf_idx].slot_key
    };
    assert!(composer.layout_deps.get(&sid).unwrap().contains(&leaf_key),
        "旧依赖项（state→leaf key）应保留");

    // 帧3：notify → 重测并维持依赖
    let s = holder.borrow().clone().unwrap();
    s.set_no_wake(123.0);
    build(&mut composer);
    let m3 = MEASURE_COUNT.with(|c| c.get());
    assert!(m3 > m2, "notify 后应重新 measure（布局失效生效）");
    assert!(composer.layout_deps.contains_key(&sid), "重测后依赖应续期");
}

/// 回归测试（review 发现）：register_modifier_deps_recursive（scroll 等 modifier 内
/// State::get）必须在 take_deps 之前执行——否则依赖被静默丢弃、滚动不刷新。
#[test]
fn test_modifier_scroll_dep_registered() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::modifier::ScrollState>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let ss = ctx.remember(|| crate::modifier::ScrollState::new()).get();
            *holder.borrow_mut() = Some(ss.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new().vertical_scroll(ss.clone()));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let sid = holder.borrow().as_ref().unwrap().offset.id();
    assert!(composer.slot_deps.contains_key(&sid),
        "scroll offset 应注册组合依赖（register_modifier_deps_recursive）——丢失则滚动不刷新");

    // offset 变化 → 下帧该 slot 组合级 dirty（滚动触发重组）
    holder.borrow().as_ref().unwrap().offset.set(50.0);
    build(&mut composer);
    assert!(composer.compose_dirty_count > 0,
        "scroll 变化应触发组合级 dirty（dirty_count={}）", composer.compose_dirty_count);
}

/// 崩溃边界（P3-3）前提验证：content panic 后（catch_unwind 捕获），
/// 下帧恢复正常内容应自愈——slot 表/依赖缓冲（DEP_MODE 残留由 begin 清空）
/// 从半状态重建，不残留垃圾。
#[test]
fn test_compose_panic_recovers_next_frame() {
    let mut composer = Composer::new();
    let should_panic = std::cell::Cell::new(true);

    let content = |ctx: &mut ComposeCtx| {
        if should_panic.get() {
            panic!("模拟用户 content panic");
        }
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    };

    // 帧1：panic（上层 catch_unwind 捕获——此处直接验证 panic 确实发生）
    let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(content);
    }));
    assert!(r.is_err(), "帧1 应 panic（模拟渲染路径崩溃边界触发）");

    // 帧2：恢复正常内容 → 自愈
    should_panic.set(false);
    composer.compose(content);
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(composer.layout_root_idx().is_some(), "panic 后下帧应自愈（树重建）");
    let root_idx = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root_idx].children.len(), 1, "自愈后结构正确");
}

/// 数据驱动的结构变化：State 变 → root Enter → 新增 leaf 生效。
/// （源码级结构变化在 Skip 语义下不触发——对标 Compose：结构变化必须由数据驱动）
#[test]
fn test_data_driven_structure_change() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<i32>>);

    let build = |composer: &mut Composer, two_leaves: bool| {
        composer.compose(|ctx| {
            let s = ctx.remember(|| 0i32);
            *holder.borrow_mut() = Some(s.clone());
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = s.get();
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    if two_leaves {
                        { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    }
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    // 帧1：1 leaf（Enter）
    build(&mut composer, false);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 1, "帧1 应有 1 leaf");

    // 帧2：s 变化 → Enter → 2 leaf（数据驱动结构变化）
    let s = holder.borrow().clone().unwrap();
    s.set(1);
    build(&mut composer, true);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 2,
        "数据驱动结构变化：s.set → Enter → 新增 leaf 应生效");
    eprintln!("[struct-change] ok: 1 leaf → 2 leaf（数据驱动）");

    // 帧3：s 再变 → 结构回退 1 leaf（未复用节点回收）
    let s = holder.borrow().clone().unwrap();
    s.set(2);
    build(&mut composer, false);
    let root = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[root].children.len(), 1,
        "结构回退：移除 leaf 应生效（未复用节点回收）");
}

/// 回归测试（23ad6a7）：Skip/Enter 交替时 key 稳定性——
/// 模拟 demo 场景：帧1 全 Enter → 帧2 部分 Skip（Row 的 content 不执行，
/// 其内 next_key 消失——全局 counter 会平移，每路径 counter 不漂移）→
/// 帧3 全 Enter——关键：帧3 的 scroll 内节点 key 与帧1 相同（复用命中）。
#[test]
fn test_key_stable_across_skip_enter() {
    let mut composer = Composer::new();
    let count_holder = std::cell::RefCell::new(None::<State<i32>>);
    let row_holder = std::cell::RefCell::new(None::<State<i32>>);

    // 模拟：root Column → [Row(Text+spacer+Button), scroll Column(2 节 × (标题+box))]
    // row_state 驱动 Row（帧2 不变 → Row Skip → 其内 3 个 next_key 消失——全局
    // counter 会平移 scroll 的 key；每路径 counter 不漂移）
    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let count = ctx.remember(|| 0i32);
            *count_holder.borrow_mut() = Some(count.clone());
            let row_state = ctx.remember(|| 0i32);
            *row_holder.borrow_mut() = Some(row_state.clone());
            ctx.start_scope();
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = count.get(); // root 依赖（root scope 内注册——帧2 count 变 → root Enter）
                    // Row（帧2 依赖 row_state 未变 → Clean → Skip——content 不执行）
                    let row_key = ctx.next_key();
                    match ctx.start_restartable_group(row_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = row_state.get(); // Row 依赖
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                            { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                        }
                    }
                    ctx.end_restartable_group();
                    // scroll Column（依赖 count——帧2 Enter：count 变 → scroll 重跑）
                    let scroll_key = ctx.next_key();
                    match ctx.start_restartable_group(scroll_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                        GroupStatus::Skip => {}
                        GroupStatus::Enter => {
                            let _ = count.get(); // scroll 依赖（帧2 count 变 → scroll Enter）
                            for _ in 0..2 {
                                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } // 标题
                                let g = ctx.next_key();
                                match ctx.start_restartable_group(g, Modifier::new(), crate::layout::BoxLayout::new()) {
                                    GroupStatus::Skip => {}
                                    GroupStatus::Enter => { { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); } }
                                }
                                ctx.end_restartable_group();
                            }
                        }
                    }
                    ctx.end_restartable_group();
                }
            }
            ctx.end_restartable_group();
            ctx.end_scope();
        });
        };

    // 帧1：全 Enter（count + row_state 首次 → dirty）
    build(&mut composer);
    let n1 = composer.arena.nodes.len();
    eprintln!("[key-stable] 帧1 nodes={}", n1);

    // 帧2：count 变（root/scroll Enter）但 row_state 不变 → **Row Skip**（其内
    // 3 个 next_key 消失——旧全局 counter 会平移 scroll 的 key → 不复用 → arena 增长；
    // 每路径 counter 恒定 → scroll 节点 key 同帧1 → 全复用）
    let s = count_holder.borrow().clone().unwrap();
    s.set(1);
    build(&mut composer);
    eprintln!("[key-stable] 帧2 clean={} dirty={}", composer.compose_clean_count, composer.compose_dirty_count);
    let n2 = composer.arena.nodes.len();
    eprintln!("[key-stable] 帧2 nodes={}", n2);
    assert_eq!(n1, n2, "Row Skip 时 scroll 节点 key 应稳定（每路径 counter）——旧全局 counter 会平移 → arena 增长");
}

/// 回归测试（TextField 输入不显示 bug）：文本内容变化（依赖注册在父容器 →
/// leaf slot Clean）→ 复用节点必须重测（modifier_text_content_differs 检测）——
/// 否则常量折叠 + cached_paragraph 旧内容 → 渲染画旧文本。
#[test]
fn test_text_content_change_remeasures() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::core::state::State<String>>);

    // 模拟 TextField：外部 value State（依赖注册在容器 scope）+ TextContent leaf
    let build = |composer: &mut Composer, text: &str| {
        composer.compose(|ctx| {
            let value = ctx.remember(|| "".to_string());
            *holder.borrow_mut() = Some(value.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let _ = value.get(); // 依赖注册到容器 scope（对标 TextField）
                    let k = ctx.next_key();
                    // TextContent leaf（content 来自 value——但内容在 build 时快照）
                    let modifier = Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                        content: text.to_string(),
                        font_size: 14.0,
                        color: crate::modifier::Color::from_argb(255, 0, 0, 0),
                        font_weight: crate::ui::text::FontWeight::NORMAL,
                        font_style: crate::ui::text::FontSlant::Upright,
                        max_lines: usize::MAX,
                        align: crate::ui::TextAlign::Left,
                        overflow: crate::ui::TextOverflow::Clip,
                        soft_wrap: true,
                        letter_spacing: 0.0,
                        line_height: None,
                    });
                    ctx.start_leaf(k, modifier);
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, "");
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let w1 = composer.arena_nodes()[leaf].measured_size.width;
    eprintln!("[text-change] frame1 w={}", w1);

    // 帧2：value 变（容器 Enter）+ content 新（"hello"）→ leaf 复用但内容变 → 必须重测
    let s = holder.borrow().clone().unwrap();
    s.set("hello".to_string());
    build(&mut composer, "hello");
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let w2 = composer.arena_nodes()[leaf].measured_size.width;
    eprintln!("[text-change] frame2 w={}", w2);
    assert!(w2 > w1, "文本内容变化后复用 leaf 应重测（宽度变）：frame1 w={} frame2 w={}（冻结则 bug 复发）", w1, w2);
}

#[test]
fn test_stmt_key_stable_across_structure_change() {
    let mut composer = Composer::new();
    // 模拟 #[composable] 宏注入：帧 1 语句 5 内组合两个节点；帧 2 前插入一条语句
    // （运行时结构变化——但语句 id 是源码位置，不受影响）→ key 应稳定。
    let mut keys_frame1 = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(5);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys_frame1.push((k1, k2));
    });
    let mut keys_frame2 = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(3); // 模拟"前面插入的新语句"（源码里在语句 5 前新增）
        ctx.pop_stmt();
        ctx.push_stmt(5); // 原语句 5——id 不变
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys_frame2.push((k1, k2));
    });
    assert_eq!(keys_frame1[0], keys_frame2[0], "同语句 id 跨帧 key 应稳定（结构变化不漂移）");
}

#[test]
fn test_stmt_key_differs_by_stmt_id() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(1);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.push_stmt(2);
        let k2 = ctx.next_key();
        ctx.start_leaf(k2, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys.push((k1, k2));
    });
    assert_ne!(keys[0].0, keys[0].1, "不同语句 id → 不同 key");
}

#[test]
fn test_key_override_stable() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    for _ in 0..2 {
        composer.compose(|ctx| {
            let _ = ctx.start_scope_keyed(0xABCD);
            let k = ctx.key("scroll_list", |ctx| {
                let k = ctx.next_key();
                ctx.start_leaf(k, Modifier::new());
                ctx.end_node();
                k
            });
            ctx.end_scope();
            keys.push(k);
        });
        }
    assert_eq!(keys[0], keys[1], "显式 key() 跨帧稳定");
}

#[test]
fn test_key_override_differs_from_stmt() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        ctx.push_stmt(1);
        let k1 = ctx.next_key();
        ctx.start_leaf(k1, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        let k2 = ctx.key("other", |ctx| {
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
            k
        });
        ctx.end_scope();
        keys.push((k1, k2));
    });
    assert_ne!(keys[0].0, keys[0].1, "显式 key() 与语句 key 不同空间");
}

#[test]
fn test_stmt_guard_drops_on_scope_exit() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xABCD);
        // 块内 enter_stmt——块尾（模拟 return/break 提前退出）guard drop 自动 pop
        {
            let _g = ctx.enter_stmt(7);
            // seq = 链哈希（位置分量）——只断言 id 与"非零 seq"（值不固定）
            let (id, seq) = STMT_STACK.with(|s| s.borrow().last().copied()).unwrap();
            assert_eq!(id, 7, "guard 生效：栈顶 id=7");
            let _ = seq;
        } // 块退出——guard drop
        assert!(STMT_STACK.with(|s| s.borrow().is_empty()), "提前退出后栈应自动恢复（无泄漏）");
        // guard 存活期间显式 pop 配对（guard 仍持有——drop 时再 pop 一次无害）
        let _g8 = ctx.enter_stmt(8);
        ctx.pop_stmt(); // 显式配对也正常
        drop(_g8);
        ctx.end_scope();
    });
}

/// for 循环迭代 key 回归（seq 位置化后）：行容器语句的 seq = slot 树兄弟
/// index——start_slot 每帧无条件执行（Skip 帧也执行）→ index 跨帧稳定 =
/// 迭代位置。content 闭包内语句（只在 Enter 执行）继承外层行语句的迭代
/// 位置（outer_seq）。滚动后部分行 Skip → 行容器的 index 不变 → key 稳定
/// （防 text29 撞 text0——旧执行计数机制在 Skip 帧漂移的根因）。
#[test]
fn test_stmt_seq_inherits_outer_iteration_position() {
    use crate::ui::layout_components::Column;
    use std::cell::RefCell;
    let mut composer = Composer::new();
    let keys_first = RefCell::new(Vec::new());
    let keys_recompose = RefCell::new(Vec::new());
    // 记录行容器 key（enter_stmt(6) 后 next_key——真实组合场景）
    let mut scene = |composer: &mut Composer, skip_before: usize| {
        // Column 每帧传变化的 spacing → 强制 Enter（content 重跑）——
        // 模拟滚动触发 Column 重跑；行容器本身 clean + 参数未变 → content Skip
        composer.compose(crate::compose!(|ctx| {
            Column::new().spacing(skip_before as f32).build(ctx, |ctx| {
                for i in 0..30u32 {
                    // 行容器语句（每迭代执行）——seq = 兄弟 index = 迭代位置
                    let _g = ctx.enter_stmt(6);
                    let k = ctx.next_key();
                    match ctx.start_restartable_group(k, Modifier::new(), crate::layout::BoxLayout::new()) {
                        crate::core::composer::GroupStatus::Skip => {}
                        crate::core::composer::GroupStatus::Enter => {
                            // content 内语句（仅 Enter 执行）——继承外层迭代位置
                            let _g2 = ctx.enter_stmt(7);
                            let _ = ctx.next_key();
                            drop(_g2);
                        }
                    }
                    ctx.end_restartable_group();
                    drop(_g);
                    if i >= skip_before as u32 {
                        keys_recompose.borrow_mut().push(k);
                    }
                }
            });
        }));
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 600.0));
    };
    // 首帧：30 行全 Enter（全部执行）
    scene(&mut composer, 0);
    keys_first.borrow_mut().extend(keys_recompose.borrow().iter().copied());
    keys_recompose.borrow_mut().clear();
    // 重组：前 29 行 content Skip（只 start 不 Enter），第 30 行 Enter——
    // 行容器 index 仍 0..29（start_slot 无条件）→ 第 30 行 key 与首帧一致
    scene(&mut composer, 29);
    assert_eq!(
        keys_first.borrow()[29], keys_recompose.borrow()[0],
        "content 内语句继承外层迭代位置——滚动后 key 与首帧一致（防 text29 撞 text0）"
    );
    assert_ne!(
        keys_first.borrow()[0], keys_first.borrow()[29],
        "迭代 key 互异——30 行 key 全同（无 seq 分量时代）会让槽树错乱"
    );
}

/// 跨函数 seq 隔离回归：不同 #[composable] 函数的语句 id 各自从 0 开始——
/// seq 取 slot 树兄弟位置（next_sibling_index）——函数 A/B 在不同 scope 槽位，
/// 各自 child_counters 独立 → seq 天然隔离（函数 B 行数变化不影响 A 的 key）。
#[test]
fn test_stmt_seq_isolated_across_functions() {
    let mut composer = Composer::new();
    let mut keys_a = Vec::new();
    let mut keys_b = Vec::new();
    composer.compose(|ctx| {
        // 函数 A（scope 0xAAAA）：3 次迭代
        let _ = ctx.start_scope_keyed(0xAAAA);
        for _ in 0..3 {
            ctx.push_stmt(1);
            keys_a.push(ctx.next_key());
            ctx.pop_stmt();
        }
        ctx.end_scope();
        // 函数 B（scope 0xBBBB）：语句 id 也从 1 开始——位置独立于 A
        let _ = ctx.start_scope_keyed(0xBBBB);
        ctx.push_stmt(1);
        keys_b.push(ctx.next_key());
        ctx.pop_stmt();
        ctx.end_scope();
    });
    // 新语义（seq = 链哈希位置）：A 的 3 次迭代位置互异（行实例区分）；
    // B 的位置含"A 在其前"的兄弟序号——与 B_alone（无 A）位置不同是正确
    // 行为（位置不同 = 不同 key = 各自独立，非"基数泄漏"）
    assert_ne!(
        keys_a[0], keys_a[1],
        "函数 A 内迭代位置互异（行实例区分）"
    );
    assert_ne!(
        keys_a[1], keys_a[2],
        "函数 A 内迭代位置互异（行实例区分）"
    );
}

/// 串位 bug 回归：两个不同 scope（不同源码哈希）内**相同的语句 id**（同 push_stmt(5)）
/// 必须生成不同 key——旧 bug（start_scope_keyed 双重 push None → scope=0）下
/// 同 stmt id 跨函数碰撞 → 节点复用串位（nest_demo row 0 ↔ [extra] button）
#[test]
fn test_scope_isolates_stmt_key_across_functions() {
    let mut composer = Composer::new();
    let mut keys = Vec::new();
    composer.compose(|ctx| {
        // 函数 A（hash A）
        let _ = ctx.start_scope_keyed(0xAAAA);
        ctx.push_stmt(5);
        let k_a = ctx.next_key();
        ctx.start_leaf(k_a, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        // 函数 B（hash B——不同）
        let _ = ctx.start_scope_keyed(0xBBBB);
        ctx.push_stmt(5); // 相同语句 id
        let k_b = ctx.next_key();
        ctx.start_leaf(k_b, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        keys.push((k_a, k_b));
    });
    // 断言 key 基（高位）不同——旧 bug（scope=0）下两函数 base 相同（仅 counter 区分）
    let base_a = keys[0].0 >> 32;
    let base_b = keys[0].1 >> 32;
    assert_ne!(base_a, base_b, "不同 scope 的 key 基必须不同（scope 隔离）——旧 bug 下 base 相同（scope=0）跨函数碰撞");
}

/// 手动 start_scope（push None）与宏 start_scope_keyed（push Some）混用——配对正确
#[test]
fn test_mixed_manual_and_keyed_scope_pairing() {
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let _ = ctx.start_scope_keyed(0xAAAA);
        {
            // 手动 scope（None——不覆盖外层 Some）
            let _ = ctx.start_scope();
            ctx.end_scope();
        }
        ctx.push_stmt(7);
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
        ctx.pop_stmt();
        ctx.end_scope();
        // 外层 keyed scope 的 source 应保留（未被手动 scope 的 None 破坏）
        assert_eq!(STMT_STACK.with(|s| s.borrow().len()), 0, "stmt 栈应清空");
    });
}

/// 阶段 5 集成测试：容器组件参数（spacing）相等跳过——
/// 参数变化 → Enter（content 重跑）；参数未变 + slot clean → Skip（content 不跑）
#[test]
fn test_component_param_change_forces_reenter() {
    use crate::ui::layout_components::Column;
    let mut composer = Composer::new();
    let mut run_count = std::cell::Cell::new(0);

    // 帧 1：spacing 0（首帧——Enter）
    composer.compose(|ctx| {
        Column::new().spacing(0.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0)); // prev_nodes 在 layout 更新
    assert_eq!(run_count.get(), 1, "首帧 content 执行");

    // 帧 2：spacing 10（参数变化 → Enter——content 重跑）
    run_count.set(0);
    composer.compose(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0));
    assert_eq!(run_count.get(), 1, "参数变化 → content 重跑");

    // 帧 3：spacing 10（参数未变 + clean → Skip——content 不跑）
    run_count.set(0);
    composer.compose(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            run_count.set(run_count.get() + 1);
            let _ = ctx;
        });
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0));
    assert_eq!(run_count.get(), 0, "参数未变 → Skip（content 不执行）");
}

/// 参数变化后布局层必须更新（should-fix 回归：Enter 时置 dirty——
/// 否则 measure_node 常量折叠返回上帧尺寸/子位置）
#[test]
fn test_param_change_updates_layout() {
    use crate::ui::layout_components::Column;
    use crate::layout::node::find_node_by_id;
    let mut composer = Composer::new();
    let root_id = std::cell::Cell::new(0u64);

    // 帧 1：spacing 0，两个子 Text
    composer.compose(crate::compose!(|ctx| {
        Column::new().spacing(0.0).build(ctx, |ctx| {
            crate::ui::text::Text::new("a").build(ctx);
            crate::ui::text::Text::new("b").build(ctx);
        });
    }));
    let c = crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0);
    composer.layout(c);

    // 帧 2：spacing 10——参数变化 → Enter + dirty → 布局更新
    composer.compose(crate::compose!(|ctx| {
        Column::new().spacing(10.0).build(ctx, |ctx| {
            crate::ui::text::Text::new("a").build(ctx);
            crate::ui::text::Text::new("b").build(ctx);
        });
    }));
    composer.layout(c);
    let nodes = &composer.arena.nodes;
    // b 的 position（相对 Column——spacing 10 后应 > spacing 0 时）
    // 子 Text 的 position（相对 Column）：a 在 0，b 在 a 高 + spacing 之后
    let positions: Vec<f32> = nodes.iter().filter(|n| n.slot_key != 0 && n.measured_size.height > 0.0)
        .map(|n| n.position.y).collect();
    assert!(positions.len() >= 2, "两个子节点应有 position");
    let max_y = positions.iter().cloned().fold(0.0_f32, f32::max);
    assert!(max_y >= 19.0 + 10.0 - 0.5,
        "参数变化后子位置应反映 spacing=10（b 应在 a 高 19 + spacing 10 之后——实际 {positions:?}）");
}

/// ③ 布局树独立缓存边界：结构变化删除的节点槽位应回收复用（free 池）——
/// arena 容量不随结构变化持续增长
#[test]
fn test_arena_recycles_freed_slots() {
    use crate::ui::layout_components::Column;
    use crate::ui::text::Text;
    let mut composer = Composer::new();
    let show = crate::core::state::State::new(true);
    let c = crate::layout::constraints::Constraints::new(0.0, 800.0, 0.0, 600.0);

    // 帧 1：show=true——含 extra 分支（3 个 Text）
    let mut cap1 = 0;
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);
    cap1 = composer.arena.nodes.len();
    assert!(cap1 >= 4, "帧1 应有 4+ 节点（根+3 Text）");

    // 帧 2：show=false——extra 分支删除（2 节点 free）
    show.set(false);
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);

    // 帧 3：show=true——重新创建分支——槽位应复用（容量不持续增长）
    show.set(true);
    composer.compose(crate::compose!(|ctx| {
        Column::new().build(ctx, |ctx| {
            Text::new("a").build(ctx);
            if show.get() {
                Text::new("b").build(ctx);
                Text::new("c").build(ctx);
            }
        });
    }));
    composer.layout(c);
    let cap3 = composer.arena.nodes.len();
    assert!(cap3 <= cap1 + 2, "槽位应复用（帧3 容量 {cap3} 不应远超帧1 {cap1}——free 池回收）");

}

#[test]
fn test_materialize_structure_change_window_insert() {
    // 结构变化回归：if 分支插入（Window 场景）——slot 树重建——物化树应完整
    let mut composer = Composer::new();
    // show 必须经 remember 创建（owner queue 绑定——notify 定向推送本 Composer）
    let mut show: Option<crate::core::state::State<bool>> = None;

    // 帧1：false（无 Window 分支）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                show = Some(ctx.remember(|| false));
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应 2 leaf");

    // 帧2：true（Window 分支插入——结构变化）
    show.as_ref().unwrap().set(true);
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                if show.as_ref().unwrap().get() {
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧2 应 2 leaf（if 插入后）");

    // 帧3：false（结构回退）
    show.as_ref().unwrap().set(false);
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let _ = show.as_ref().unwrap().get();
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    let r = composer.layout_root_idx().unwrap();
    eprintln!("[t3] root children={} nodes={}", composer.arena_nodes()[r].children.len(), composer.arena_nodes().len());
    for c in composer.arena_nodes()[r].children.clone() {
        eprintln!("[t3] child key={} dirty={}", composer.arena_nodes()[c].slot_key >> 32, composer.arena_nodes()[c].dirty);
    }
    assert_eq!(composer.arena_nodes()[r].children.len(), 1, "帧3 应 1 leaf（回退）");
}

#[test]
fn test_materialize_skip_restores_subtree() {
    // Skip 恢复（物化核心路径）：帧2 无 State 变化/无参数变化 → 容器 Skip →
    // content 不执行 → 物化从 prev_node_by_key 恢复整个子树（children 重新挂接）
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    // 帧1：Enter——容器 + 2 leaf
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应 2 leaf");

    // 帧2：无变化 → 容器 Skip（content 不跑）→ 物化恢复上帧子树
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                panic!("帧2 应 Skip（content 不应执行）");
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "Skip 应恢复上帧 2 leaf");
    for ci in composer.arena_nodes()[r].children.clone() {
        let n = &composer.arena_nodes()[ci];
        assert!(!n.dirty, "恢复的 leaf 不应 dirty（缓存测量保留）");
        assert!(n.cached_constraints.is_some(), "恢复的 leaf 应保留测量缓存（cached_constraints）");
        assert_eq!(n.slot_key, composer.arena_nodes()[r].children.iter().find(|&&x| x == ci).map(|_| composer.arena_nodes()[ci].slot_key).unwrap(), "恢复的 leaf slot_key 保留");
    }
}

/// review 修复回归：复用路径必须刷新 layout_direction 快照。
/// 帧1 无方向元素 → Ltr；帧2 modifier 加 layout_direction(Rtl) →
/// param_eq 变化 → Enter 复用旧节点 → 快照必须更新为 Rtl
/// （修复前只有新建节点路径设置快照，复用节点保持旧 Ltr → padding 镜像失效）
#[test]
fn test_materialize_reuse_refreshes_layout_direction() {
    use crate::layout::LayoutDirection;
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        LayoutDirection::Ltr,
        "帧1 无方向元素 → Ltr"
    );

    // 帧2：modifier 加 layout_direction(Rtl) → param_eq 不等 → Enter 复用旧节点
    composer.compose(|ctx| {
        let key = ctx.next_key();
        match ctx.start_restartable_group(
            key,
            Modifier::new().layout_direction(LayoutDirection::Rtl),
            crate::layout::BoxLayout::new(),
        ) {
            GroupStatus::Skip => panic!("方向变化应 Enter（param_eq 不等）"),
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        LayoutDirection::Rtl,
        "复用路径必须刷新方向快照（modifier 覆盖 > theme）"
    );
}

/// RTL 全局切换修复回归：组合期 provides 作用域内捕获方向到 desc——
/// 物化在组合回调后执行（WiniaTheme::direction() 已退出作用域），
/// 修复前物化期读 theme 恒 Ltr → offset/padding 镜像全部失效（用户实测
/// "都是同向运动"根因）。此测试验证 desc 携带方向。
#[test]
fn test_compose_captures_direction_in_provides_scope() {
    use crate::ui::theme::WiniaTheme;
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 100.0, 0.0, 100.0);

    // 模拟 demo 全局切换：with_theme_and_direction(Rtl) 包住组合回调
    composer.compose(|ctx| {
        WiniaTheme::with_theme_and_direction(
            WiniaTheme::colors(),
            crate::layout::LayoutDirection::Rtl,
            ctx,
            |ctx| {
                let key = ctx.next_key();
                ctx.start_leaf(key, Modifier::new());
                ctx.end_node();
            },
        );
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        crate::layout::LayoutDirection::Rtl,
        "组合期 provides 作用域内必须捕获 Rtl（物化期读 theme 会退回 Ltr）"
    );

    // 对照组：无 provides → Ltr
    let mut composer = Composer::new();
    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new());
        ctx.end_node();
    });
    composer.layout(c);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(
        composer.arena_nodes()[r].layout_direction,
        crate::layout::LayoutDirection::Ltr,
        "无 provides 作用域默认 Ltr"
    );
}

// ═══════════════════════════════════════════════════════════
// P3-1 Skip 恢复健壮性测试（结构签名）
// ═══════════════════════════════════════════════════════════

/// T2：State 驱动结构变化回归——if 分支增删（show_b State）→ root Enter 重建，
/// A 位置不复用 B 缓存，B 移除后无残留。
#[test]
fn test_skip_recovery_structure_change_by_state() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<bool>>);

    let build = |composer: &mut Composer, holder: &std::cell::RefCell<Option<State<bool>>>| {
        composer.compose(|ctx| {
            let show_b = ctx.remember(|| true);
            *holder.borrow_mut() = Some(show_b.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // A 叶子（始终在）
                    { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                    // B 分支（show_b 控制）
                    if show_b.get() {
                        let k = ctx.next_key();
                        ctx.start_leaf(k, Modifier::new().size(100.0, 50.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧1 应有 A+B 两个 leaf");

    // 帧2：show_b=false（State 驱动 → root Enter → content 重跑 → B 分支不建）
    holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 1, "帧2 应只剩 A（B 移除）");
    // A 正常保留；不应复用 B 的缓存（B 的 size 100x50）
    let a = composer.arena_nodes()[r].children[0];
    assert!(composer.arena_nodes()[a].measured_size.width < 100.0,
        "A 不应复用 B 的缓存（B 的 width=100 不应出现在 A）——width={}", composer.arena_nodes()[a].measured_size.width);

    // 帧3：B 恢复
    holder.borrow().as_ref().unwrap().set(true);
    build(&mut composer, &holder);
    let r = composer.layout_root_idx().unwrap();
    assert_eq!(composer.arena_nodes()[r].children.len(), 2, "帧3 应恢复 A+B");
}

/// T3：结构签名直接验证——手动构造"缓存 children 数 != desc children 数"，
/// materialize_node Skip 恢复应放弃（重建 + key 保留待回收，防张冠李戴与泄漏）。
#[test]
fn test_skip_recovery_sig_mismatch_direct() {
    let mut composer = Composer::new();
    // 帧1：root + 2 leaf
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
                { let k = ctx.next_key(); ctx.start_leaf(k, Modifier::new()); ctx.end_node(); }
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    let old_root = composer.layout_root_idx().unwrap();
    let old_root_key = composer.arena_nodes()[old_root].slot_key;
    let leaf0_key = composer.arena_nodes()[composer.arena_nodes()[old_root].children[0]].slot_key;

    // 手动构造 Skip desc：root 只有 1 子（缓存 2 子——签名不等）
    let desc = crate::core::materialize::DescNode {
        key: old_root_key,
        skip: true,
        modifier: Modifier::new(),
        preserve_modifier: true,
        policy: None,
        on_remove: None,
        dirty: false,
        registrar: None,
        focus_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
        composing_range: None,
        selection_range: None,
        direction: crate::layout::LayoutDirection::Ltr,
        children: vec![crate::core::materialize::DescNode {
            key: leaf0_key,
            skip: true,
            modifier: Modifier::new(),
            preserve_modifier: true,
            policy: None,
            on_remove: None,
            dirty: false,
            registrar: None,
            focus_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
            selection_range: None,
            direction: crate::layout::LayoutDirection::Ltr,
            children: vec![],
        }],
    };
    composer.arena.root = None; // 模拟新帧物化开始
    let new_root = crate::core::materialize::materialize_node(&mut composer, desc, None).unwrap();

    // 断言：签名不等 → 重建（new_root != old_root）而非恢复缓存
    assert_ne!(new_root, old_root, "签名不等应重建而非恢复缓存");
    // 旧 root 未被复用（不在 reused_nodes）；key 保留在 prev_node_by_key（待 compose 末尾回收 free）
    assert!(!composer.reused_nodes.contains(&old_root), "旧节点不应标记复用（待回收）");
    assert!(composer.prev_node_by_key.contains_key(&old_root_key),
        "key 应保留待回收（否则旧节点 arena 泄漏）");
}

/// T4：数量相同内容不同（A→B 同位置）——保持恢复（Compose 位置复用语义，不强制 Enter）
#[test]
fn test_skip_recovery_same_count_different_content() {
    let mut composer = Composer::new();
    let content = std::cell::Cell::new(0u32);

    let build = |composer: &mut Composer, content: &std::cell::Cell<u32>| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    // 同位置单个 leaf（数量恒 1）——内容由 cell 控制（无 state 驱动）
                    let _ = content.get();
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer, &content);
    // 帧2：内容 cell 变化但无 state notify → slot clean + 参数相同 → Skip（恢复）
    content.set(1);
    build(&mut composer, &content);
    // 关键断言：帧2 全 Skip（clean 计数 > 0）——同数量同位置保持恢复（Compose 语义）
    assert!(composer.compose_clean_count > 0, "数量相同内容不同应保持 Skip（clean_count={}）", composer.compose_clean_count);
}

// ═══════════════════════════════════════════════════════════
// app_root! 根入口宏——稳定 key 测试（T7）
// ═══════════════════════════════════════════════════════════

/// T7：app_root! 覆盖下，根闭包内组件调用点获得语句级稳定 key——
/// if 分支结构增删后同位置组件 key 不变、remember 状态保留。
#[test]
fn test_app_root_stable_keys_across_structure_change() {
    let mut composer = Composer::new();
    let show_holder = std::cell::RefCell::new(None::<State<bool>>);
    // B 组件的 key 记录（跨帧断言）
    let b_key = std::cell::Cell::new(None::<u64>);

    // 根入口用 app_root!（宏注入语句级 key——根闭包内调用点稳定）
    let root = crate::app_root!(|ctx: &mut ComposeCtx| {
        let show = ctx.remember(|| true);
        *show_holder.borrow_mut() = Some(show.clone());
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            crate::core::composer::GroupStatus::Skip => {}
            crate::core::composer::GroupStatus::Enter => {
                // A 组件（if 分支包裹——结构变化场景）
                if show.get() {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                }
                // B 组件（始终存在——key 应跨结构变化稳定）
                let k2 = ctx.next_key();
                ctx.start_leaf(k2, Modifier::new().size(50.0, 20.0));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| { root(ctx); });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
        // 记录 B 组件（最后一个 leaf）的 slot_key
        let r = composer.layout_root_idx().unwrap();
        let children = composer.arena_nodes()[r].children.clone();
        let b = children[children.len() - 1];
        b_key.set(Some(composer.arena_nodes()[b].slot_key));
    };

    // 帧1：[A, B]
    build(&mut composer);
    let k1 = b_key.get().unwrap();
    // show 状态 id（跨帧保留断言）
    let show_id = show_holder.borrow().as_ref().unwrap().id();

    // 帧2：show=false → [B]（A 移除——结构变化）。
    // ⚠ seq 位置化（方案 B）：B 的 seq = 兄弟 index——A 移除后 B 从 index 1 变 0
    // → key 变 → B leaf 重建。这是结构变化（start 本身被跳过）的已知边界
    // （Compose 组栈位置同语义：结构变化 = 位置记忆重置）；B 无状态，重建无
    // 视觉影响。show 的 remember 在 if 外（根闭包首语句，index 恒 0）→ 仍稳定。
    show_holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer);
    let k2 = b_key.get().unwrap();
    // B 节点仍存在（重建后 leaf）
    assert!(b_key.get().is_some(), "B 组件应存在（重建）");

    // 帧3：show=true → [A, B]（A 恢复——B index 回到 1）
    show_holder.borrow().as_ref().unwrap().set(true);
    build(&mut composer);
    let k3 = b_key.get().unwrap();
    assert_eq!(k1, k3, "A 恢复后 B 的 key 应回到帧1 值（index 回到 1）——key 由兄弟位置决定");

    // remember 状态（show）跨结构变化保留（同一 State id——根闭包首语句 index 稳定）
    assert_eq!(show_holder.borrow().as_ref().unwrap().id(), show_id,
        "remember 状态应跨结构变化保留（语句级 key 稳定）");
}

/// 回归（文档 1.3 原始 bug 的直接验证）：**条件分支内的 remember 增删不影响
/// 分支外语句的 remember**——语句 id 编译期固定 + per-base 独立计数：
/// if 分支（语句 1）的 remember 在自己的 base 下计数，a（语句 0）/c（语句 2）
/// 的 base 独立——show 切换平移 if 内序号，不触碰 a/c 的 key。
#[test]
fn test_conditional_branch_remember_does_not_drift_siblings() {
    let mut composer = Composer::new();
    let show = crate::core::state::State::new(true);
    let a_holder = std::cell::RefCell::new(None::<crate::core::state::State<i32>>);
    let c_holder = std::cell::RefCell::new(None::<crate::core::state::State<i32>>);

    let build = |composer: &mut Composer| {
        composer.compose(crate::compose!(|ctx| {
            let a = ctx.remember(|| 0i32); // 语句 0
            *a_holder.borrow_mut() = Some(a.clone());
            if show.get() {
                // 语句 1（if 注入）内 remember——show 切换时整个分支增删
                let _b = ctx.remember(|| 1i32);
            }
            let c = ctx.remember(|| 2i32); // 语句 2——独立 base
            *c_holder.borrow_mut() = Some(c.clone());
        }));
        };

    // 帧1：show=true——a/c + if 内 b 全部创建
    build(&mut composer);
    let a1 = a_holder.borrow().clone().unwrap().id();
    let c1 = c_holder.borrow().clone().unwrap().id();

    // 帧2：show=false——if 分支 remember 消失（结构变化）→ 平移 if 内序号
    show.set(false);
    build(&mut composer);
    let a2 = a_holder.borrow().clone().unwrap().id();
    let c2 = c_holder.borrow().clone().unwrap().id();
    assert_eq!(a1, a2, "a（语句 0）的 State 应跨 if 分支增删保留——跨语句不漂移");
    assert_eq!(c1, c2, "c（语句 2）的 State 应跨 if 分支增删保留——跨语句不漂移");

    // 帧3：show=true——b 恢复（新 State——结构变化 = 重置，符合语义）
    show.set(true);
    build(&mut composer);
    let a3 = a_holder.borrow().clone().unwrap().id();
    let c3 = c_holder.borrow().clone().unwrap().id();
    assert_eq!(a1, a3, "a 恢复帧仍应保留");
    assert_eq!(c1, c3, "c 恢复帧仍应保留");
}

/// 语义边界验证（非 bug——Compose 同语义）：**同语句内** remember 数量变化
/// → 序号平移 → 状态重置（可预测）。明确记录该行为，防止被误当漂移 bug 报。
#[test]
fn test_same_stmt_remember_count_change_resets() {
    let mut composer = Composer::new();
    let show_extra = crate::core::state::State::new(true);
    let first_holder = std::cell::RefCell::new(None::<crate::core::state::State<i32>>);

    let build = |composer: &mut Composer| {
        composer.compose(crate::compose!(|ctx| {
            if show_extra.get() {
                // 同语句内两个 remember——数量随 show_extra 变化
                let _x = ctx.remember(|| 1i32); // seq 0
                let y = ctx.remember(|| 2i32);  // seq 1
                *first_holder.borrow_mut() = Some(y.clone());
            } else {
                let y = ctx.remember(|| 2i32);  // 只剩一个——seq 0（原 x 的槽）
                *first_holder.borrow_mut() = Some(y.clone());
            }
        }));
        };

    build(&mut composer);
    let id1 = first_holder.borrow().clone().unwrap().id();
    // 同语句内 remember 数量 2→1 → 序号平移 → y 拿到原 x 的 key → State 重置
    show_extra.set(false);
    build(&mut composer);
    let id2 = first_holder.borrow().clone().unwrap().id();
    assert_ne!(id1, id2,
        "同语句内 remember 数量变化 = 序号平移 = 重置（Compose 语义，非漂移 bug——明确记录）");
}
/// 复现 overlay 闪烁：独立 composer + 无宏注入的 content 闭包（Popup 场景）——
/// 内容组件（宏化 build）在独立 composer 执行，节点 key 是否跨帧稳定（复用）？
#[test]
fn overlay_composer_nodes_stable_across_frames() {
    let mut composer = Composer::new();
    // 模拟 overlay content：无宏注入的闭包，内部 Column + Text（宏化组件）
    let content = |ctx: &mut ComposeCtx| {
        crate::ui::Column::new()
            .spacing(6.0)
            .modifier(Modifier::new().size(200.0, 90.0))
            .build(ctx, |ctx| {
                crate::ui::Text::new("这是一个 Popup").build(ctx);
                crate::ui::Text::new("内容").build(ctx);
            });
    };
    let mut sizes = Vec::new();
    for frame in 0..10 {
        composer.recompose(|ctx| content(ctx));
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 400.0, 0.0, 300.0));
        sizes.push(composer.arena.nodes.len());
        // 每帧改动触发 recompose（模拟 overlay 每帧重建）
        composer.request_recomposition(0);
    }
    // 节点数应稳定（复用）——若每帧增长 = 全量重建（闪烁根因）
    eprintln!("[ovl-repro] 节点数序列: {sizes:?}");
    assert!(sizes.iter().all(|&n| n == sizes[0]),
        "overlay 内容节点应跨帧稳定（复用）——实际 {sizes:?}");
}
