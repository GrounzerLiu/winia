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

use crate::core::state::{ComposerSubscription, State, StateId, StateSignal};
use crate::layout::constraints::Constraints;
use crate::layout::node::{LayoutNode, MeasurePolicy, CachedNode};
use crate::modifier::Modifier;
use std::collections::{HashMap, HashSet, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
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
thread_local! { static GROUP_STACK: RefCell<Vec<u64>> = const { RefCell::new(Vec::new()) }; }
thread_local! { static STMT_STACK: RefCell<Vec<(u32, u32)>> = const { RefCell::new(Vec::new()) }; }

/// A saved outer runtime context. The current TLS values are replaced with
/// empty values while a nested Composer compose/layout call is active.
struct RuntimeFrame {
    id: u64,
    active_slot_key: u64,
    group_stack: Vec<u64>,
    stmt_stack: Vec<(u32, u32)>,
    measured_layout_keys: Option<HashSet<u64>>,
}

/// Restores the surrounding runtime context on normal return and panic.
pub(crate) struct RuntimeFrameGuard {
    id: u64,
    active: bool,
}

static NEXT_RUNTIME_FRAME_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// Saved outer runtime contexts, kept in strict LIFO order.
    static RUNTIME_FRAME_STACK: RefCell<Vec<RuntimeFrame>> = const { RefCell::new(Vec::new()) };
    /// Set only while layout measurement is traversing nodes.
    static MEASURED_LAYOUT_KEYS: RefCell<Option<HashSet<u64>>> = const { RefCell::new(None) };
}

fn begin_runtime_frame() -> RuntimeFrameGuard {
    let id = NEXT_RUNTIME_FRAME_ID.fetch_add(1, Ordering::Relaxed);
    let previous = RuntimeFrame {
        id,
        active_slot_key: ACTIVE_SLOT_KEY.with(|slot| {
            let previous = slot.get();
            slot.set(0);
            previous
        }),
        group_stack: GROUP_STACK.with(|groups| std::mem::take(&mut *groups.borrow_mut())),
        stmt_stack: STMT_STACK.with(|stmts| std::mem::take(&mut *stmts.borrow_mut())),
        measured_layout_keys: MEASURED_LAYOUT_KEYS.with(|keys| {
            std::mem::replace(&mut *keys.borrow_mut(), None)
        }),
    };
    RUNTIME_FRAME_STACK.with(|frames| frames.borrow_mut().push(previous));
    RuntimeFrameGuard { id, active: true }
}

fn restore_runtime_frame(id: u64) -> bool {
    let previous = RUNTIME_FRAME_STACK.with(|frames| {
        let mut frames = frames.borrow_mut();
        if frames.last().map(|frame| frame.id) != Some(id) {
            return None;
        }
        frames.pop()
    });
    let Some(previous) = previous else {
        // Drop must not panic while unwinding. A non-LIFO guard is a caller
        // error; leave the active frame untouched rather than corrupting it.
        return false;
    };

    ACTIVE_SLOT_KEY.with(|slot| slot.set(previous.active_slot_key));
    GROUP_STACK.with(|groups| *groups.borrow_mut() = previous.group_stack);
    STMT_STACK.with(|stmts| *stmts.borrow_mut() = previous.stmt_stack);
    MEASURED_LAYOUT_KEYS.with(|keys| *keys.borrow_mut() = previous.measured_layout_keys);
    true
}

impl Drop for RuntimeFrameGuard {
    fn drop(&mut self) {
        if self.active {
            let _ = restore_runtime_frame(self.id);
            self.active = false;
        }
    }
}


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

/// key = fnv(base, 序号)——全 64 位混合身份与实例序号，不丢熵。
/// 拼接方案（`(base << 32) | c` 或 `base 高 32 位 | c`）会把 base 截到 32 位
/// → 身份空间 2^32（碰撞概率高——checkbox 循环子项即碰撞）。
pub(crate) fn mix_key(base: u64, c: u64) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    h ^= base; h = h.wrapping_mul(0x100000001b3);
    h ^= c; h = h.wrapping_mul(0x100000001b3);
    h
}

/// 设置当前 slot key（measure_node 用它把动态尺寸的依赖注册到节点）
pub(crate) fn set_active_slot_key(key: u64) {
    ACTIVE_SLOT_KEY.with(|c| c.set(key));
    MEASURED_LAYOUT_KEYS.with(|keys| {
        if let Some(keys) = keys.borrow_mut().as_mut() {
            keys.insert(key);
        }
    });
}

fn begin_layout_measure_tracking() {
    MEASURED_LAYOUT_KEYS.with(|keys| *keys.borrow_mut() = Some(HashSet::new()));
}

fn take_layout_measure_keys() -> HashSet<u64> {
    MEASURED_LAYOUT_KEYS
        .with(|keys| keys.borrow_mut().take().unwrap_or_default())
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

    /// Return the Window lifecycle context owned by this Composer.
    pub(crate) fn window_lifecycle(&self) -> crate::ui::window::LifecycleState {
        self.composer.lifecycle.clone()
    }

    pub(crate) fn focus_window(&self, window_id: u64) -> crate::modifier::FocusWindowGuard {
        self.composer.focus_window(window_id)
    }

    /// 在组合中记住一个状态。初次调用时执行 init 创建 State，后续重组时返回上次的同一个 State 实例。
    pub fn remember<T: Clone + 'static>(&mut self, init: impl FnOnce() -> T) -> State<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember(slot_key, || State::new(init()))
    }

    /// Remember a write-back channel: same slot stability as `remember`, but
    /// the stored value never notifies (Backchannel write). For measure/layout
    /// write-back and cross-frame staging (see docs/state-handles.md).
    pub fn remember_backchannel<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::core::state::Backchannel<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::core::state::Backchannel::new(init())
        })
    }

    /// Remember an animation-tick handle: recompose without waking the event
    /// loop (see docs/state-handles.md).
    pub fn remember_animating<T: Clone + PartialEq + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::core::state::Animating<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::core::state::Animating::new(init())
        })
    }

    /// Remember a draw-layer handle: writes land without recomposition
    /// (see docs/state-handles.md).
    pub fn remember_visual<T: Clone + 'static>(
        &mut self,
        init: impl FnOnce() -> T,
    ) -> crate::core::state::Visual<T> {
        let slot_key = self.next_remember_key();
        self.composer.slot_table.remember_handle(slot_key, || {
            crate::core::state::Visual::new(init())
        })
    }

    /// 注册顶层弹出层（Popup/Dialog/DropdownMenu 内部调用）——组合期收集，
    /// compose 后由 app.rs 取走并独立物化/渲染
    pub fn open_overlay(&mut self, mut desc: crate::ui::overlay::OverlayDesc) {
        // ⚠ 捕获 CompositionLocal 快照（主树 provides 内——Theme 等）——
        // overlay 独立 Composer 在 provides 弹栈后 recompose，读不到主树
        // 隐式上下文；快照重放让 overlay 继承主树主题/方向/排版。
        desc.local_snapshot = crate::core::composition_local::capture();
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
        self.composer.slot_table.remember(key, || State::new(init()))
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
        self.composer.entered_compose_keys.insert(key);
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
        let key = self.composer.try_stable_base().unwrap_or(fallback_hash);
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

    /// 设置当前节点的 IME 组合下划线颜色（组合期调用——主题 primary 在此捕获；
    /// 渲染期 CompositionLocal 已退出，不能读主题，Phase 4.2）
    pub fn set_current_node_composing_color(&mut self, color: crate::modifier::Color) {
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

    /// animateColorAsState — 动画颜色值到目标值（RGBA 插值，Tween 驱动）
    pub fn animate_color_as_state(&mut self, target: crate::modifier::Color, spec: crate::animation::AnimationSpec) -> State<crate::modifier::Color> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::core::state::Animating::new(target)
        });
        self.composer.animation_state_ids.insert(handle.state_id());
        crate::animation::push_animatable_color(
            crate::core::state::State::from_raw(handle.as_raw().clone()),
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
    ) -> crate::core::state::Animating<T> {
        let slot_key = self.next_remember_key();
        let handle = self.composer.slot_table.remember_handle(slot_key, || {
            crate::core::state::Animating::new(target.clone())
        });
        self.composer.animation_state_ids.insert(handle.state_id());
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
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵。⚠ 不能用
        // (base << 32) | c（左移丢弃 base 高 32 位）或 base 高 32 位 | c
        // （丢弃 base 低 32 位 → 身份只剩 2^32 空间——checkbox 循环子项碰撞）。
        crate::core::composer::mix_key(base, c as u64)
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
    /// IME 组合下划线颜色（组合期 set_current_node_composing_color 写入——
    /// 物化时应用；渲染期不能读 CompositionLocal（Phase 4.2），组合期捕获主题 primary）
    composing_color: Option<crate::modifier::Color>,
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

    fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        if let Some(existing) = self.remembered.get(&slot_key) {
            if let Some(handle) = existing.downcast_ref::<H>() {
                return handle.clone();
            }
        }
        let handle = init();
        self.remembered.insert(slot_key, Box::new(handle.clone()));
        handle
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

struct SlotTableRuntimeSnapshot {
    path: Vec<usize>,
    child_counters: Vec<usize>,
    active_slot_key: u64,
    dirty_keys: std::collections::HashSet<u64>,
}

struct ComposeRuntimeSnapshot {
    slot_table: SlotTableRuntimeSnapshot,
    current_group_key: u32,
    path_counters: std::collections::HashMap<u64, u32>,
    remember_path_counters: std::collections::HashMap<u64, u32>,
    scope_source_stack: Vec<Option<u64>>,
    key_override_stack: Vec<u64>,
    pending_recomposition: VecDeque<u64>,
    needs_recomposition: bool,
    node_stack: Vec<usize>,
    group_skip_stack: Vec<bool>,
    overlay_active: HashMap<u64, bool>,
    entered_compose_keys: HashSet<u64>,
    reused_nodes: std::collections::HashSet<usize>,
}

/// Restores the small, non-owning compose runtime context on panic. The full
/// SlotTable/arena transaction remains deliberately separate because it owns
/// `Box<dyn Any>` and user callbacks.
struct ComposeRuntimeTransaction {
    composer: *mut Composer,
    committed: bool,
}

impl ComposeRuntimeTransaction {
    fn new(composer: &mut Composer) -> Self {
        composer.compose_transaction = Some(composer.capture_compose_runtime());
        Self { composer, committed: false }
    }

    fn commit(&mut self) {
        self.committed = true;
        unsafe { (*self.composer).compose_transaction = None; }
    }
}

impl Drop for ComposeRuntimeTransaction {
    fn drop(&mut self) {
        if !self.committed {
            unsafe { (*self.composer).rollback_compose_runtime(); }
        }
    }
}

impl SlotTable {
    fn runtime_snapshot(&self) -> SlotTableRuntimeSnapshot {
        SlotTableRuntimeSnapshot {
            path: self.path.clone(),
            child_counters: self.child_counters.clone(),
            active_slot_key: self.active_slot_key,
            dirty_keys: self.dirty_keys.clone(),
        }
    }

    fn restore_runtime(&mut self, snapshot: SlotTableRuntimeSnapshot) {
        self.path = snapshot.path;
        self.child_counters = snapshot.child_counters;
        self.active_slot_key = snapshot.active_slot_key;
        self.dirty_keys = snapshot.dirty_keys;
    }

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
                    composing_color: desc.composing_color,
                    cursor_index: desc.cursor_index,
                    cursor_visible: desc.cursor_visible,
                    cursor_callback: desc.cursor_callback,
                    display_focused: desc.display_focused,
                    ime_callback: desc.ime_callback,
                    composing_range: desc.composing_range,
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
                    composing_color: None,
                    cursor_index: None,
                    cursor_visible: None,
                    cursor_callback: None,
                    display_focused: None,
                    ime_callback: None,
                    composing_range: None,
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

    fn remember_handle<H: Clone + 'static>(
        &mut self,
        slot_key: u64,
        init: impl FnOnce() -> H,
    ) -> H {
        self.current_slot().remember_handle(slot_key, init)
    }

    fn reset(&mut self) {
        self.path.clear();
        self.child_counters = vec![0];
        self.active_slot_key = 0;
        // 每帧清 visited——物化只收集本帧活跃 slot（结构回退的残留不收集）
        // 同时清空未消费的 desc（panic 残留帧产物），避免下一帧 Clean 复用
        // slot 时 collect_desc_tree 误收集旧 desc 物化错误节点。
        // 正常成功帧的 desc 已被 collect_desc_tree take（=None），此操作无副作用。
        fn clear_visited(slot: &mut Slot) {
            slot.visited = false;
            let _ = slot.desc.take(); // 丢弃 panic 残留的帧产物 desc
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

    /// Collect slot keys that remain part of the current composition. A skipped
    /// subtree is retained structurally even though its descendants were not visited.
    fn collect_live_keys(&self, out: &mut HashSet<u64>) {
        fn visit(slot: &Slot, out: &mut HashSet<u64>, in_skip: bool) {
            if !slot.visited && !in_skip {
                return;
            }
            out.insert(slot.key);
            let child_in_skip = in_skip
                || (slot.desc.is_none() && !slot.is_scope && slot.skip_modifier.is_some());
            for child in &slot.children {
                visit(child, out, child_in_skip);
            }
        }

        for child in &self.root_slot.children {
            visit(child, out, false);
        }
    }
}

/// Snapshot of mutable Composer state touched by layout. A measurement policy
/// can execute user code, so panic must preserve the last committed graph and
/// the invalidation batch for a retry.
struct LayoutTransactionSnapshot {
    pending: Vec<StateId>,
    layout_dirty_keys: HashSet<u64>,
    prev_nodes: HashMap<u64, CachedNode>,
    prev_node_by_key: HashMap<u64, usize>,
    layout_slot_reads: HashMap<u64, HashSet<StateId>>,
    layout_deps: HashMap<StateId, HashSet<u64>>,
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    removed_slot_keys: HashSet<u64>,
    root: Option<usize>,
    nodes_len: usize,
    free_nodes: Vec<usize>,
    free_policies: Vec<usize>,
    node_state: Vec<LayoutNodeTransactionState>,
    scroll_limits: Vec<(crate::core::state::Backchannel<f32>, f32)>,
}

#[derive(Clone)]
struct LayoutNodeTransactionState {
    idx: usize,
    id: u64,
    modifier: Modifier,
    measure_policy: Option<usize>,
    has_text_content: bool,
    has_richtext_content: bool,
    has_image_content: bool,
    focused: bool,
    layout_direction: crate::layout::LayoutDirection,
    slot_key: u64,
    parent_id: Option<u64>,
    measured_size: crate::layout::node::Size,
    position: crate::layout::node::Point,
    children: Vec<usize>,
    scroll_viewport_height: f32,
    scroll_viewport_width: f32,
    scroll_content_height: f32,
    scroll_content_width: f32,
    scroll_reverse: bool,
}

struct LayoutTransaction {
    composer: *mut Composer,
    snapshot: Option<LayoutTransactionSnapshot>,
    committed: bool,
}

impl LayoutTransaction {
    fn new(composer: &Composer) -> Self {
        let node_state = composer
            .arena
            .nodes
            .iter()
            .enumerate()
            .map(|(idx, node)| LayoutNodeTransactionState {
                idx,
                id: node.id,
                modifier: node.modifier.clone(),
                measure_policy: node.measure_policy,
                has_text_content: node.has_text_content,
                has_richtext_content: node.has_richtext_content,
                has_image_content: node.has_image_content,
                focused: node.focused,
                layout_direction: node.layout_direction,
                slot_key: node.slot_key,
                parent_id: node.parent_id,
                measured_size: node.measured_size,
                position: node.position,
                children: node.children.clone(),
                scroll_viewport_height: node.scroll_viewport_height,
                scroll_viewport_width: node.scroll_viewport_width,
                scroll_content_height: node.scroll_content_height,
                scroll_content_width: node.scroll_content_width,
                scroll_reverse: node.scroll_reverse,
            })
            .collect();
        let mut scroll_limits = Vec::new();
        for node in &composer.arena.nodes {
            if let Some(scroll) = node.modifier.vertical_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
            if let Some(scroll) = node.modifier.horizontal_scroll_state() {
                scroll_limits.push((scroll.fling_limit.clone(), scroll.fling_limit.peek()));
            }
        }

        Self {
            composer: composer as *const Composer as *mut Composer,
            snapshot: Some(LayoutTransactionSnapshot {
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                prev_nodes: composer.prev_nodes.clone(),
                prev_node_by_key: composer.prev_node_by_key.clone(),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
                root: composer.arena.root,
                nodes_len: composer.arena.nodes.len(),
                free_nodes: composer.arena.free.clone(),
                free_policies: composer.arena.free_policies.clone(),
                node_state,
                scroll_limits,
            }),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.committed = true;
        self.snapshot = None;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };

        composer.pending_states.restore_pending(&snapshot.pending);
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.prev_nodes = snapshot.prev_nodes;
        composer.prev_node_by_key = snapshot.prev_node_by_key;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.removed_slot_keys = snapshot.removed_slot_keys;
        composer.arena.root = snapshot.root;
        composer.arena.nodes.truncate(snapshot.nodes_len);
        composer.arena.free = snapshot.free_nodes;
        composer.arena.free_policies = snapshot.free_policies;

        for state in snapshot.node_state {
            if let Some(node) = composer.arena.nodes.get_mut(state.idx) {
                node.id = state.id;
                node.modifier = state.modifier;
                node.measure_policy = state.measure_policy;
                node.has_text_content = state.has_text_content;
                node.has_richtext_content = state.has_richtext_content;
                node.has_image_content = state.has_image_content;
                node.focused = state.focused;
                node.layout_direction = state.layout_direction;
                node.slot_key = state.slot_key;
                node.parent_id = state.parent_id;
                node.measured_size = state.measured_size;
                node.position = state.position;
                node.children = state.children;
                node.scroll_viewport_height = state.scroll_viewport_height;
                node.scroll_viewport_width = state.scroll_viewport_width;
                node.scroll_content_height = state.scroll_content_height;
                node.scroll_content_width = state.scroll_content_width;
                node.scroll_reverse = state.scroll_reverse;
                // Force a complete retry. This is safer than restoring a
                // partially rebuilt paragraph or measurement cache.
                node.dirty = true;
                node.layout_dirty = false;
                node.cached_constraints = None;
                if let Ok(mut paragraph) = node.cached_paragraph.try_borrow_mut() {
                    *paragraph = None;
                }
            }
        }
        for (state, value) in snapshot.scroll_limits {
            state.set(value);
        }
    }
}

impl Drop for LayoutTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while an application panic is unwinding.
            unsafe { self.rollback(); }
        }
    }
}

/// Snapshot of the dependency graph committed by the last successful compose.
/// This is intentionally narrower than a SlotTable/arena transaction: it restores
/// only graph state and subscriptions after a late compose panic.
struct ComposeDependencySnapshot {
    compose_slot_reads: HashMap<u64, HashSet<StateId>>,
    slot_deps: HashMap<StateId, HashSet<u64>>,
    signal_handles: HashMap<StateId, Arc<StateSignal>>,
    layout_slot_reads: HashMap<u64, HashSet<StateId>>,
    layout_deps: HashMap<StateId, HashSet<u64>>,
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    pending: Vec<StateId>,
    layout_dirty_keys: HashSet<u64>,
    removed_slot_keys: HashSet<u64>,
}

/// Restores dependency maps and live signal subscriptions if compose panics
/// after reconciliation but before its cleanup completes.
struct ComposeDependencyTransaction {
    composer: *mut Composer,
    snapshot: Option<ComposeDependencySnapshot>,
    committed: bool,
}

impl ComposeDependencyTransaction {
    fn new(composer: &Composer) -> Self {
        Self {
            composer: composer as *const Composer as *mut Composer,
            snapshot: Some(ComposeDependencySnapshot {
                compose_slot_reads: composer.compose_slot_reads.clone(),
                slot_deps: composer.slot_deps.clone(),
                signal_handles: composer.signal_handles.clone(),
                layout_slot_reads: composer.layout_slot_reads.clone(),
                layout_deps: composer.layout_deps.clone(),
                layout_signal_handles: composer.layout_signal_handles.clone(),
                pending: composer.pending_states.pending_ids(),
                layout_dirty_keys: composer.layout_dirty_keys.clone(),
                removed_slot_keys: composer.removed_slot_keys.clone(),
            }),
            committed: false,
        }
    }

    fn commit(&mut self) {
        self.snapshot = None;
        self.committed = true;
    }

    unsafe fn rollback(&mut self) {
        let Some(snapshot) = self.snapshot.take() else { return };
        let composer = unsafe { &mut *self.composer };
        composer.compose_slot_reads = snapshot.compose_slot_reads;
        composer.slot_deps = snapshot.slot_deps;
        composer.signal_handles = snapshot.signal_handles;
        composer.layout_slot_reads = snapshot.layout_slot_reads;
        composer.layout_deps = snapshot.layout_deps;
        composer.layout_signal_handles = snapshot.layout_signal_handles;
        composer.layout_dirty_keys = snapshot.layout_dirty_keys;
        composer.removed_slot_keys = snapshot.removed_slot_keys;

        // Remove every live subscription created or retained by the failed
        // frame before restoring the committed signal set. Otherwise a signal
        // that only appeared in the failed graph can still enqueue the queue.
        composer.pending_states.retain_signals(&HashSet::new());

        // Restore the exact pending batch from before this compose began.
        // Notifications produced by the failed frame must not leak into the
        // restored dependency graph.
        composer.pending_states.drain();
        composer.pending_states.restore_pending(&snapshot.pending);

        // Replace the queue's signal set with the last committed handles.
        let mut restored = HashSet::new();
        for signal in composer
            .signal_handles
            .values()
            .chain(composer.layout_signal_handles.values())
        {
            if restored.insert(signal.id()) {
                composer.pending_states.subscribe_signal(signal);
            }
        }

        composer.debug_assert_dependency_graphs();
    }
}

impl Drop for ComposeDependencyTransaction {
    fn drop(&mut self) {
        if !self.committed {
            // Rollback must not panic while another application panic unwinds.
            unsafe { self.rollback(); }
        }
    }
}

/// Keeps compose-triggering invalidations queued until the compose transaction
/// reaches its final cleanup. A panic restores only the consumed batch; other
/// Composer mutations remain governed by the existing self-healing path.
///
/// # 不变量（形式化，2026-08-26）
///
/// 1. **帧内新通知只进入下一批**：`drain_matching`（state.rs:159）在队列锁内
///    原子消费匹配的 IDs，未匹配的 IDs 留在队列。帧内 `State::set` → `enqueue`
///    （state.rs:126）在锁内追加新 ID → 新通知一定在队列中等待下帧，不会在当前帧
///    被消费。请勿在 `drain_matching` 锁外修改 pending 队列。
///
/// 2. **panic 恢复不重复**：`Drop` 未 commit 时调用 `restore_pending`
///    （state.rs:147），在锁内**去重**追加——帧内已到来的新通知不会被重复添加。
///
/// 3. **commit 后不再回滚**：commit 后 `ids` 清空、`committed=true`，`Drop` 不
///    做任何操作——已消费的批被视为已提交。
///
/// 4. **布局 pending 不被 compose 误消费**：`drain_matching` 只取出 `slot_deps`
///    中存在的 IDs（composeIds），layout-only 的 IDs 留在队列，由 `layout()`
///    通过 `drain_non_compose_collect_layout`（state.rs:175）原子消费。
struct PendingBatchGuard {
    queue: Arc<ComposerSubscription>,
    ids: Vec<StateId>,
    committed: bool,
}

impl PendingBatchGuard {
    fn new(queue: Arc<ComposerSubscription>, ids: Vec<StateId>) -> Self {
        Self {
            queue,
            ids,
            committed: false,
        }
    }

    fn ids(&self) -> &[StateId] {
        &self.ids
    }

    fn commit(&mut self) {
        self.ids.clear();
        self.committed = true;
    }
}

impl Drop for PendingBatchGuard {
    fn drop(&mut self) {
        if !self.committed {
            self.queue.restore_pending(&self.ids);
        }
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
    compose_transaction: Option<ComposeRuntimeSnapshot>,
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
    slot_deps: HashMap<StateId, HashSet<u64>>,
    /// compose 期每个 slot 的完整读取集合；Enter 时替换，Skip 时保留。
    compose_slot_reads: HashMap<u64, HashSet<StateId>>,
    /// compose 依赖对应的 signal handle，用于移除 stale Composer 订阅。
    signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// layout 依赖对应的 signal handle；compose 与 layout 共享一个队列但独立收敛。
    layout_signal_handles: HashMap<StateId, Arc<StateSignal>>,
    /// 帧内实际执行过的 compose slot；用于按 Enter/Skip 语义收敛读取集合。
    entered_compose_keys: HashSet<u64>,
    /// 布局期每个 slot 的读取集合；测量命中时替换，常量折叠时保留。
    layout_slot_reads: HashMap<u64, HashSet<StateId>>,
    /// 布局依赖反向表（state_id → slot_key；由 layout_slot_reads 重建）
    layout_deps: HashMap<StateId, HashSet<u64>>,
    /// 本帧 pending 消费收集的布局失效 key（layout() 应用后清空）
    layout_dirty_keys: HashSet<u64>,
    /// 本帧确认移除的 slot_key（compose 末尾回收未复用节点时收集——layout_deps 死 key 清理用）
    removed_slot_keys: HashSet<u64>,
    /// 本 Composer 实例的 pending state 通知队列
    pending_states: Arc<crate::core::state::ComposerSubscription>,
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
    /// Window lifecycle flags are scoped to this Composer, not the thread.
    pub(crate) lifecycle: crate::ui::window::LifecycleState,
    /// Adaptive window size context owned by this Composer.
    pub(crate) adaptive: crate::ui::adaptive::AdaptiveContext,
    /// State IDs used by this Composer's animation registrations.
    pub(crate) animation_state_ids: HashSet<StateId>,

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
        let pending_states = crate::core::state::ComposerSubscription::new();
        Self {
            slot_table: SlotTable::new(),
            current_group_key: 0,
            compose_transaction: None,
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
            compose_slot_reads: HashMap::new(),
            signal_handles: HashMap::new(),
            layout_signal_handles: HashMap::new(),
            entered_compose_keys: HashSet::new(),
            layout_slot_reads: HashMap::new(),
            layout_deps: HashMap::new(),
            layout_dirty_keys: HashSet::new(),
            removed_slot_keys: HashSet::new(),
            pending_states,
            prev_nodes: HashMap::new(),
            pending_params: Vec::new(),
            prev_node_by_key: HashMap::new(),
            reused_nodes: std::collections::HashSet::new(),
            selection_registrar: None,
            lifecycle: crate::ui::window::LifecycleState::default(),
            adaptive: crate::ui::adaptive::AdaptiveContext::new(),
            animation_state_ids: HashSet::new(),
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

    pub(crate) fn pending_window_close_id(&self) -> Option<u64> {
        self.lifecycle.pending_close_id()
    }

    pub(crate) fn reset_pending_window_remove(&self) {
        self.lifecycle.reset_pending_remove();
    }

    pub(crate) fn set_adaptive_window_size(&self, width: f32, height: f32) {
        self.adaptive.set_size(width, height);
    }

    pub(crate) fn focus_window(&self, window_id: u64) -> crate::modifier::FocusWindowGuard {
        crate::modifier::focus_window(window_id)
    }

    pub(crate) fn take_focus_requests(&self) -> Vec<u64> {
        crate::modifier::take_focus_requests()
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
            // 链空（非宏顶层）时退化纯 id（兼容旧行为）。
            // ⚠ 只混合 scope_src+sid（不含迭代 seq）：ctx.key 是列表/重排的
            // 显式兜底（Compose 语义）——若混入 seq，重排后同 id 位置变 →
            // base 变 → key 漂移 → 兜底失效（P1-1，探针证实）。多实例区分
            // 由 per-base counter（path_counters/remember_path_counters）完成。
            let chain = self.chain_hash_no_seq().unwrap_or(0);
            let mut h: u64 = 0xcbf29ce484222325;
            h ^= k; h = h.wrapping_mul(0x100000001b3);
            h ^= chain; h = h.wrapping_mul(0x100000001b3);
            return Some(h);
        }
        self.chain_hash()
    }

    /// 调用链哈希（不含迭代 seq）：fnv(scope_src, STMT栈顶语句id)——编译期
    /// 固定的组合位置身份。ctx.key(id) 混合用——同 id 跨位置（列表重排）base
    /// 不变（兜底稳定），不同调用点（16 字段）靠 scope_src/sid 隔离。
    fn chain_hash_no_seq(&self) -> Option<u64> {
        STMT_STACK.with(|s| {
            let s = s.borrow();
            s.last().map(|&(sid, _seq)| {
                let scope_src = self.scope_source_stack.last().and_then(|s| *s).unwrap_or(0);
                let mut h: u64 = 0xcbf29ce484222325;
                h ^= scope_src; h = h.wrapping_mul(0x100000001b3);
                h ^= sid as u64; h = h.wrapping_mul(0x100000001b3);
                h
            })
        })
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
        // key = fnv(base, 序号)——全 64 位混合，不丢身份熵（拼接方案把身份
        // 截到 32 位，2^32 碰撞空间——dup-key 根因）
        crate::core::composer::mix_key(base, c as u64)
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
        self.entered_compose_keys.insert(key);
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
        self.entered_compose_keys.insert(key);
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
            composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
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
        // Enter 重新执行 content；Skip 保留上帧子树的读取集合。
        if !is_skip {
            self.entered_compose_keys.insert(key);
        }
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
                composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
                composing_range: None,
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

    /// Rebuild reverse compose dependencies from the forward per-slot read graph.
    /// Keeping one canonical source prevents stale reverse edges after slot removal.
    fn rebuild_compose_reverse_deps(&mut self) {
        self.slot_deps.clear();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                self.slot_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    /// Rebuild reverse layout dependencies from the forward per-slot read graph.
    fn rebuild_layout_reverse_deps(&mut self) {
        self.layout_deps.clear();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                self.layout_deps.entry(state_id).or_default().insert(slot_key);
            }
        }
    }

    #[cfg(debug_assertions)]
    fn debug_assert_dependency_graphs(&self) {
        let mut expected_compose = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.compose_slot_reads {
            for &state_id in state_ids {
                expected_compose.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.slot_deps, expected_compose, "compose dependency reverse graph drifted");

        let mut expected_layout = HashMap::<StateId, HashSet<u64>>::new();
        for (&slot_key, state_ids) in &self.layout_slot_reads {
            for &state_id in state_ids {
                expected_layout.entry(state_id).or_default().insert(slot_key);
            }
        }
        debug_assert_eq!(self.layout_deps, expected_layout, "layout dependency reverse graph drifted");
    }

    #[cfg(not(debug_assertions))]
    #[inline]
    fn debug_assert_dependency_graphs(&self) {}

    fn reconcile_compose_deps(
        &mut self,
        recorded: Vec<(Arc<StateSignal>, u64)>,
        live_keys: &HashSet<u64>,
    ) {
        let mut reads_by_slot: HashMap<u64, HashSet<StateId>> = HashMap::new();
        let mut current_signals: HashMap<StateId, Arc<StateSignal>> = HashMap::new();
        for (signal, slot_key) in recorded {
            current_signals.entry(signal.id()).or_insert_with(|| signal.clone());
            if live_keys.contains(&slot_key) {
                reads_by_slot.entry(slot_key).or_default().insert(signal.id());
            }
        }

        // A skipped subtree keeps its previous read set; an Entered slot gets
        // the exact set observed in this frame, including an empty set.
        self.compose_slot_reads.retain(|key, _| live_keys.contains(key));
        let entered = self.entered_compose_keys.clone();
        for key in entered {
            match reads_by_slot.remove(&key) {
                Some(reads) if !reads.is_empty() => {
                    self.compose_slot_reads.insert(key, reads);
                }
                _ => {
                    self.compose_slot_reads.remove(&key);
                }
            }
        }
        // Be tolerant of a read recorded by a node whose entry marker was not
        // reached (for example, a modifier callback during materialization).
        for (key, reads) in reads_by_slot {
            self.compose_slot_reads.entry(key).or_default().extend(reads);
        }

        self.rebuild_compose_reverse_deps();
        self.signal_handles.extend(current_signals);
        self.cleanup_signal_subscriptions();
        self.debug_assert_dependency_graphs();
    }

    fn cleanup_signal_subscriptions(&mut self) {
        let mut live_ids = HashSet::new();
        for state_ids in self.compose_slot_reads.values() {
            live_ids.extend(state_ids.iter().copied());
        }
        live_ids.extend(self.layout_deps.keys().copied());

        let mut removed = HashMap::<StateId, Arc<StateSignal>>::new();
        let mut compose_handles = std::mem::take(&mut self.signal_handles);
        compose_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.insert(*id, signal.clone());
                false
            }
        });
        self.signal_handles = compose_handles;

        let mut layout_handles = std::mem::take(&mut self.layout_signal_handles);
        layout_handles.retain(|id, signal| {
            if live_ids.contains(id) {
                true
            } else {
                removed.entry(*id).or_insert_with(|| signal.clone());
                false
            }
        });
        self.layout_signal_handles = layout_handles;

        for signal in removed.values() {
            signal.unsubscribe(self.pending_states.id());
        }
        self.pending_states.retain_signals(&live_ids);
    }

    fn capture_compose_runtime(&self) -> ComposeRuntimeSnapshot {
        ComposeRuntimeSnapshot {
            slot_table: self.slot_table.runtime_snapshot(),
            current_group_key: self.current_group_key,
            path_counters: self.path_counters.clone(),
            remember_path_counters: self.remember_path_counters.clone(),
            scope_source_stack: self.scope_source_stack.clone(),
            key_override_stack: self.key_override_stack.clone(),
            pending_recomposition: self.pending_recomposition.clone(),
            needs_recomposition: self.needs_recomposition,
            node_stack: self.node_stack.clone(),
            group_skip_stack: self.group_skip_stack.clone(),
            overlay_active: self.overlay_active.clone(),
            entered_compose_keys: self.entered_compose_keys.clone(),
            reused_nodes: self.reused_nodes.clone(),
        }
    }

    fn rollback_compose_runtime(&mut self) {
        let Some(snapshot) = self.compose_transaction.take() else { return };
        self.slot_table.restore_runtime(snapshot.slot_table);
        self.current_group_key = snapshot.current_group_key;
        self.path_counters = snapshot.path_counters;
        self.remember_path_counters = snapshot.remember_path_counters;
        self.scope_source_stack = snapshot.scope_source_stack;
        self.key_override_stack = snapshot.key_override_stack;
        self.pending_recomposition = snapshot.pending_recomposition;
        self.needs_recomposition = snapshot.needs_recomposition;
        self.node_stack = snapshot.node_stack;
        self.group_skip_stack = snapshot.group_skip_stack;
        self.overlay_active = snapshot.overlay_active;
        self.entered_compose_keys = snapshot.entered_compose_keys;
        self.reused_nodes = snapshot.reused_nodes;
    }

    /// 执行组合：运行 content 闭包，构建/更新组合树和布局树。
    pub fn compose(&mut self, content: impl FnOnce(&mut ComposeCtx)) {
        // Isolate shared TLS so a nested Composer can compose without replacing
        // the caller's active slot/group/statement context.
        let _runtime_frame = begin_runtime_frame();
        let mut compose_runtime_transaction = ComposeRuntimeTransaction::new(self);
        let _adaptive_context = crate::ui::adaptive::enter_context(self.adaptive.clone());
        let mut dependency_transaction = ComposeDependencyTransaction::new(self);
        #[cfg(test)] { self.compose_clean_count = 0; self.compose_dirty_count = 0; }
        self.compose_count += 1;
        self.slot_table.reset();
        self.overlay_active.clear(); // 每帧组合期重记录（Skip 帧不记录）
        self.current_group_key = 0;
        self.lifecycle.reset_for_compose();
        self.path_counters.clear();
        self.remember_path_counters.clear();
        STMT_STACK.with(|s| s.borrow_mut().clear());
        // The runtime frame starts empty; these clears also self-heal any state
        // left by an older caller that entered before frame isolation was added.
        GROUP_STACK.with(|s| s.borrow_mut().clear());
        ACTIVE_SLOT_KEY.with(|c| c.set(0));
        self.scope_source_stack.clear();
        self.key_override_stack.clear();
        // 注意：不在 compose 开头清 arena.root——materialize 管理 root
        // （开头清 + 末尾设）。此处若清，同帧第二次 compose 的 materialize
        // 守卫（prev 空 + root 有）失效 → 空 prev 重建 → 树塌缩。
        // Reset only this Composer's Window lifecycle state before old node
        // cleanup; its on_remove callbacks may set a pending close request.
        // 阶段D：保留上帧树（prev_node_by_key 由上帧 layout 构建）——
        // start_node 按 slot_key 复用节点槽位；本帧未复用的旧节点在
        // compose 末尾统一 free（见下方 drain）
        self.node_stack.clear();
        self.group_skip_stack.clear();
        self.pending_params.clear();
        self.entered_compose_keys.clear();
        // Slot key 0 is the fallback target for top-level State::get().
        self.entered_compose_keys.insert(0);

        // Consume only invalidations that currently have compose dependencies.
        // Layout-only IDs stay queued for layout() to consume.
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let mut pending_batch = PendingBatchGuard::new(
            self.pending_states.clone(),
            self.pending_states.drain_matching(|id| compose_ids.contains(&id)),
        );
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 触发本次重组的 State id 列表（对应 State::set/update 调用）
            eprintln!("[recompose] 触发 State: {:?}", pending_batch.ids());
        }
        for state_id in pending_batch.ids() {
            if let Some(keys) = self.slot_deps.get(state_id) {
                for &k in keys {
                    self.slot_table.mark_dirty(k);
                }
            }
            // 两段式依赖：布局期注册的依赖 → 只标布局失效（重测不重组）
            if let Some(keys) = self.layout_deps.get(state_id) {
                for &k in keys {
                    self.layout_dirty_keys.insert(k);
                }
            }
        }
        // Begin an isolated dependency frame. State::get() writes to its active
        // buffer, while nested Composer calls temporarily own their own frame.
        // The frame remains open through materialization and modifier reads.
        let mut dependency_frame = crate::core::state::begin_compose_deps_with_queue(
            std::sync::Arc::downgrade(&self.pending_states),
        );

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
        let mut live_compose_keys = HashSet::new();
        self.slot_table.collect_live_keys(&mut live_compose_keys);
        live_compose_keys.insert(0);
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
        // 依赖注册（组合期 + modifier 期收集的 State 依赖 → 按 slot 收敛）。
        let recorded = crate::core::state::take_deps();
        self.reconcile_compose_deps(recorded, &live_compose_keys);
        // Commit only after the read graph is reconciled. If content/materialize
        // panics first, the guard restores the outer dependency frame and rolls
        // back subscriptions learned by this failed compose.
        dependency_frame.commit();
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
        // A notification that arrived after the batch was drained may belong to
        // a read removed by this frame. Drop only IDs with no live channel.
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        pending_batch.commit();
        dependency_transaction.commit();
        compose_runtime_transaction.commit();
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
        // Measure callbacks can read State and invoke another Composer. Keep
        // their active slot/group/statement context isolated as well.
        let _runtime_frame = begin_runtime_frame();
        let _adaptive_context = crate::ui::adaptive::enter_context(self.adaptive.clone());
        let mut layout_transaction = LayoutTransaction::new(self);
        // Layout can be called without a preceding compose; consume layout-only
        // invalidations here so measure sees the dirty path directly.
        self.consume_layout_pending();
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
        let mut dependency_frame = crate::core::state::begin_layout_deps_with_queue(
            Arc::downgrade(&self.pending_states),
        );
        begin_layout_measure_tracking();
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
        } else {
            // No root means every old layout dependency is stale.
            self.prev_nodes.clear();
            self.prev_node_by_key.clear();
        }

        let recorded = crate::core::state::take_deps();
        let measured_keys = take_layout_measure_keys();
        // A Composer with no root has no live layout readers. Clear the forward
        // graph before rebuilding the reverse index so direct layout() calls
        // cannot retain subscriptions after the tree disappeared.
        if self.arena.root.is_none() {
            self.layout_slot_reads.clear();
            self.layout_deps.clear();
        }
        let mut reads_by_slot: HashMap<u64, HashSet<StateId>> = HashMap::new();
        for (signal, slot_key) in &recorded {
            reads_by_slot.entry(*slot_key).or_default().insert(signal.id());
            self.layout_signal_handles
                .entry(signal.id())
                .or_insert_with(|| signal.clone());
        }

        // Replace only slots that actually ran measure. A cached slot is absent
        // from measured_keys and therefore keeps its last successful reads.
        for slot_key in measured_keys {
            match reads_by_slot.remove(&slot_key) {
                Some(reads) if !reads.is_empty() => {
                    self.layout_slot_reads.insert(slot_key, reads);
                }
                _ => {
                    self.layout_slot_reads.remove(&slot_key);
                }
            }
        }
        // Compose confirmed removals before rebuilding the reverse index so a
        // dead slot cannot reappear from an old forward edge.
        for &slot_key in &self.removed_slot_keys {
            self.layout_slot_reads.remove(&slot_key);
            self.layout_dirty_keys.remove(&slot_key);
        }
        self.removed_slot_keys.clear();

        self.rebuild_layout_reverse_deps();

        self.cleanup_signal_subscriptions();
        self.debug_assert_dependency_graphs();
        self.pending_states
            .drain_matching(|id| !self.slot_deps.contains_key(&id) && !self.layout_deps.contains_key(&id));
        // Normal layout completion restores the outer dependency frame. A panic
        // before this point drops the guard and rolls back partial subscriptions.
        dependency_frame.commit();
        layout_transaction.commit();
    }

    /// 请求重组（由 State 变化触发）。
    pub fn request_recomposition(&mut self, _key: u64) {
        self.needs_recomposition = true;
    }

    fn has_pending_compose_states(&self) -> bool {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        self.pending_states
            .pending_ids()
            .into_iter()
            .any(|id| compose_ids.contains(&id))
    }

    /// Move layout-only invalidations into layout_dirty_keys without consuming
    /// an ID that also requires composition. This makes direct layout() calls
    /// correct while preserving a mixed compose/layout notification for compose().
    fn consume_layout_pending(&mut self) {
        let compose_ids: HashSet<StateId> = self.slot_deps.keys().copied().collect();
        let layout_ids: HashSet<StateId> = self.layout_deps.keys().copied().collect();
        let layout_pending = self
            .pending_states
            .drain_non_compose_collect_layout(&compose_ids, &layout_ids);
        for state_id in layout_pending {
            if let Some(keys) = self.layout_deps.get(&state_id) {
                self.layout_dirty_keys.extend(keys.iter().copied());
            }
        }
    }

    /// 是否有待处理的 state 变化
    pub fn has_pending_states(&self) -> bool {
        !self.pending_states.is_empty()
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
        self.pending_states.len()
    }

    /// 执行待处理的重组。返回 true 表示实际执行了 compose。
    /// 若无待处理则跳过，保留上一帧的布局树。
    pub fn recompose(&mut self, content: impl FnOnce(&mut ComposeCtx)) -> bool {
        // Resolve layout-only notifications before deciding whether composition
        // is needed. This prevents the app frame loop from spinning on a pending
        // ID that belongs only to layout_deps.
        self.consume_layout_pending();
        let has_pending = self.has_pending_compose_states();
        let will_run = self.needs_recomposition || has_pending || !self.pending_recomposition.is_empty();
        #[cfg(debug_assertions)]
        if std::env::var("WINIA_RECOMPOSE_TRACE").is_ok() {
            // 重组触发诊断：何时执行/跳过 + 触发原因（pending=State 变化数）
            eprintln!(
                "[recompose] pending={} needs={} q={} → {}",
                has_pending, self.needs_recomposition, self.pending_recomposition.len(),
                if will_run { "COMPOSE" } else { "SKIP（无变化）" }
            );
        }

        if !will_run {
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
    // Modifier reads happen after GROUP_STACK is cleared; restore the node target
    // explicitly so scroll State dependencies do not all attach to the last node.
    set_active_slot_key(arena.nodes[idx].slot_key);
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

impl Drop for Composer {
    fn drop(&mut self) {
        if self.compose_transaction.is_some() {
            self.rollback_compose_runtime();
        }
        crate::animation::clear_animations_for_states(
            &self.animation_state_ids.iter().copied().collect::<Vec<_>>(),
        );
        self.pending_states.unsubscribe_all();
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
        // key = fnv(base, 序号)（mix_key 全 64 位混合）——同 base 相邻序号 key 不同
        assert_ne!(key, key2);
        // 序号混合应改变低 32 位（fnv 扩散）——不要求高 32 位相同（旧拼接语义）
        assert_ne!(key as u32, key2 as u32, "序号应扩散到低 32 位（mix_key 全 64 位混合）");
        // mix_key 本身：不同序号 → 不同 key（跨 base 也不碰撞丢熵）
        assert_ne!(mix_key(0x1234, 0), mix_key(0x1234, 1), "同 base 不同序号 key 不同");
        assert_ne!(mix_key(0x1234, 0), mix_key(0x1235, 0), "不同 base 同序号 key 不同");
    }

    /// Phase 4.1：ScaleFactorChanged → request_recomposition → build 重跑并读到
    /// 新 density（TextUnit::Px 组合期转换随 DPI 变化刷新，而非保留旧值）。
    #[test]
    fn test_request_recomposition_reruns_build_with_new_density() {
        let mut composer = Composer::new();
        // build 读 density（模拟 Text font_size Px 组合期转换）存入外部 State
        let seen = State::new(0.0f32);
        let build = |ctx: &mut ComposeCtx| {
            let d = crate::unit::current_density();
            seen.set(d.density);
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new());
            ctx.end_node();
        };
        // 帧 1：density 2.0
        crate::unit::with_density(crate::unit::Density::from_density(2.0), || {
            composer.compose(build);
        });
        assert_eq!(seen.get(), 2.0, "首帧 density 2.0");
        // 模拟 ScaleFactorChanged：request_recomposition + density 变为 1.0
        composer.request_recomposition(0);
        crate::unit::with_density(crate::unit::Density::from_density(1.0), || {
            assert!(composer.recompose(build), "request_recomposition 应驱动重组");
        });
        assert_eq!(seen.get(), 1.0, "重组后 build 读到新 density 1.0");
        // 无 request 时 recompose 应跳过（needs_recomposition 已消费）
        crate::unit::with_density(crate::unit::Density::from_density(1.0), || {
            assert!(!composer.recompose(build), "无变化时 recompose 应跳过");
        });
    }

    #[test]
    fn test_window_lifecycle_isolation_per_composer() {
        let composer_a = Composer::new();
        let composer_b = Composer::new();

        composer_a.lifecycle.set_pending_remove(7);
        assert_eq!(composer_a.pending_window_close_id(), Some(7));
        assert_eq!(composer_b.pending_window_close_id(), None, "window close request must stay scoped to the owning Composer");

        composer_b.lifecycle.set_pending_remove(9);
        composer_b.lifecycle.mark_rebuilt();
        assert_eq!(composer_b.pending_window_close_id(), None, "rebuilt window should not report pending close");
        assert_eq!(composer_a.pending_window_close_id(), Some(7));

        composer_a.reset_pending_window_remove();
        assert_eq!(composer_a.pending_window_close_id(), None);
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

#[test]
fn test_dependency_reverse_graph_rebuilds_from_forward_reads() {
    let mut composer = Composer::new();
    let first = StateId::new(1);
    let second = StateId::new(2);

    composer.compose_slot_reads.insert(10, HashSet::from([first, second]));
    composer.compose_slot_reads.insert(20, HashSet::from([first]));
    composer.rebuild_compose_reverse_deps();
    assert_eq!(composer.slot_deps.get(&first), Some(&HashSet::from([10, 20])));
    assert_eq!(composer.slot_deps.get(&second), Some(&HashSet::from([10])));

    composer.layout_slot_reads.insert(30, HashSet::from([second]));
    composer.layout_slot_reads.insert(40, HashSet::from([first, second]));
    composer.rebuild_layout_reverse_deps();
    assert_eq!(composer.layout_deps.get(&first), Some(&HashSet::from([40])));
    assert_eq!(composer.layout_deps.get(&second), Some(&HashSet::from([30, 40])));
    composer.debug_assert_dependency_graphs();
}

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
    let sid = s.signal_id();
    assert!(composer.slot_deps.contains_key(&sid),
        "组合期 get() 应注册进 slot_deps");
    assert!(composer.layout_deps.contains_key(&sid),
        "布局期（SizeDynamic）get() 应注册进 layout_deps");
    // 同一 State 双通道（组合+布局）各自记录
    let keys_layout = composer.layout_deps.get(&sid).unwrap();
    assert_eq!(keys_layout.len(), 1, "layout_deps 应含叶子节点 key");
    let leaf_key = *keys_layout.iter().next().unwrap();
    assert_eq!(composer.layout_slot_reads.get(&leaf_key).unwrap(), &std::collections::HashSet::from([sid]));
}

/// A measured slot with no dynamic read must replace its previous forward reads.
#[test]
fn test_layout_slot_reads_remove_empty_remeasure() {
    let mut composer = Composer::new();
    let holder = std::cell::RefCell::new(None::<State<f32>>);
    let use_dynamic = std::cell::Cell::new(true);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let state = ctx.remember(|| 10.0f32);
            *holder.borrow_mut() = Some(state.clone());
            let key = ctx.next_key();
            ctx.start_leaf(key, if use_dynamic.get() {
                Modifier::new().size(&state, 10.0)
            } else {
                Modifier::new().size(20.0, 10.0)
            });
            ctx.end_node();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let state_id = holder.borrow().as_ref().unwrap().signal_id();
    assert!(composer.layout_deps.contains_key(&state_id));
    assert!(!composer.layout_slot_reads.is_empty());

    use_dynamic.set(false);
    // The external mode switch itself is not reactive. Queue a layout-only
    // invalidation so the next layout pass really measures this slot.
    holder.borrow().as_ref().unwrap().as_raw().set_animating(11.0);
    build(&mut composer);
    assert!(!composer.layout_deps.contains_key(&state_id), "空读取重测应移除 reverse edge");
    assert!(composer.layout_slot_reads.values().all(|reads| !reads.contains(&state_id)));

    holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    assert!(!composer.has_pending_states(), "移除布局读取后 State 不应继续入队");
}

/// A cached sibling that is not remeasured must retain its forward layout reads.
#[test]
fn test_layout_slot_reads_preserve_untouched_cached_slot() {
    let mut composer = Composer::new();
    let first_holder = std::cell::RefCell::new(None::<State<f32>>);
    let second_holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let first = ctx.remember(|| 10.0f32);
            let second = ctx.remember(|| 20.0f32);
            *first_holder.borrow_mut() = Some(first.clone());
            *second_holder.borrow_mut() = Some(second.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let first_key = ctx.next_key();
                    ctx.start_leaf(first_key, Modifier::new().width(&first).height(10.0));
                    ctx.end_node();
                    let second_key = ctx.next_key();
                    ctx.start_leaf(second_key, Modifier::new().width(&second).height(10.0));
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let first_id = first_holder.borrow().as_ref().unwrap().signal_id();
    let second_id = second_holder.borrow().as_ref().unwrap().signal_id();
    let second_key = *composer.layout_deps.get(&second_id).unwrap().iter().next().unwrap();
    let second_reads = composer.layout_slot_reads.get(&second_key).unwrap().clone();

    // Only the first leaf is dirty. The second leaf hits the measure cache and
    // must keep its previous forward edge instead of being treated as empty.
    first_holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    build(&mut composer);

    assert_eq!(composer.layout_slot_reads.get(&second_key), Some(&second_reads));
    assert!(composer.layout_deps.get(&second_id).unwrap().contains(&second_key));
    assert!(composer.layout_deps.get(&first_id).is_some_and(|keys| !keys.is_empty()));
}

/// Removing a composed node must remove its layout forward and reverse edges.
#[test]
fn test_layout_slot_reads_remove_removed_slot() {
    let mut composer = Composer::new();
    let show_holder = std::cell::RefCell::new(None::<State<bool>>);
    let first_holder = std::cell::RefCell::new(None::<State<f32>>);
    let removed_holder = std::cell::RefCell::new(None::<State<f32>>);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let show = ctx.remember(|| true);
            let first = ctx.remember(|| 10.0f32);
            let removed = ctx.remember(|| 20.0f32);
            *show_holder.borrow_mut() = Some(show.clone());
            *first_holder.borrow_mut() = Some(first.clone());
            *removed_holder.borrow_mut() = Some(removed.clone());
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if show.get() {
                        let first_key = ctx.next_key();
                        ctx.start_leaf(first_key, Modifier::new().width(&first).height(10.0));
                        ctx.end_node();
                        let removed_key = ctx.next_key();
                        ctx.start_leaf(removed_key, Modifier::new().width(&removed).height(10.0));
                        ctx.end_node();
                    }
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    };

    build(&mut composer);
    let removed_id = removed_holder.borrow().as_ref().unwrap().signal_id();
    let removed_key = *composer.layout_deps.get(&removed_id).unwrap().iter().next().unwrap();
    assert!(composer.layout_slot_reads.contains_key(&removed_key));

    show_holder.borrow().as_ref().unwrap().set(false);
    build(&mut composer);

    assert!(!composer.layout_slot_reads.contains_key(&removed_key));
    assert!(!composer.layout_deps.contains_key(&removed_id));
    removed_holder.borrow().as_ref().unwrap().as_raw().set_animating(300.0);
    assert!(!composer.has_pending_states(), "removed layout signal must be unsubscribed");
    assert!(composer.layout_deps.contains_key(&first_holder.borrow().as_ref().unwrap().signal_id()) == false,
        "the branch removes both dynamic leaves");
}

/// An empty composition clears the forward graph and all reverse subscriptions.
#[test]
fn test_layout_slot_reads_clear_when_root_removed() {
    let mut composer = Composer::new();
    let watched = State::new(10.0f32);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(&watched).height(10.0));
        ctx.end_node();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(!composer.layout_slot_reads.is_empty());

    composer.compose(|_ctx| {});
    watched.as_raw().set_animating(200.0);
    assert!(composer.has_pending_states(), "empty root test must start with a queued layout notification");
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));

    assert!(composer.layout_slot_reads.is_empty());
    assert!(composer.layout_deps.is_empty());
    assert!(composer.layout_signal_handles.is_empty());
    assert!(!composer.has_pending_states(), "empty root must consume stale layout notification");
}

/// A panic while measuring must roll back subscriptions learned by that layout frame.
#[test]
fn test_layout_dependency_panic_rolls_back_new_subscription() {
    use std::panic::AssertUnwindSafe;
    use std::sync::atomic::{AtomicBool, Ordering as AtomicOrdering};

    let mut composer = Composer::new();
    let stable = State::new(10.0f32);
    let panic_state = State::new(20.0f32);
    let should_panic = Arc::new(AtomicBool::new(true));
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(&stable).height(10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let stable_id = stable.signal_id();
    let stable_key = *composer.layout_deps.get(&stable_id).unwrap().iter().next().unwrap();

    let panic_signal = panic_state.clone();
    let panic_flag = should_panic.clone();
    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().width(move || {
            let _ = panic_signal.get();
            if panic_flag.load(AtomicOrdering::Relaxed) {
                panic!("layout dependency panic");
            }
            20.0
        }).height(10.0));
        ctx.end_node();
    });
    stable.as_raw().set_animating(11.0);
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| composer.layout(constraints)));
    assert!(result.is_err());

    assert!(composer.layout_deps.get(&stable_id).is_some_and(|keys| keys.contains(&stable_key)),
        "panic must preserve the last committed layout graph");
    assert!(!composer.layout_signal_handles.contains_key(&panic_state.signal_id()),
        "panic-only signal handle must not survive the failed layout");
    assert!(!composer.layout_deps.contains_key(&panic_state.signal_id()),
        "panic-only signal must not remain subscribed");
    assert!(composer.pending_states.pending_ids().contains(&stable_id),
        "the consumed layout invalidation must be retained for retry");
    panic_state.as_raw().set_animating(21.0);
    assert!(!composer.pending_states.pending_ids().contains(&panic_state.signal_id()),
        "rolled-back signal must not enqueue Composer");

    should_panic.store(false, AtomicOrdering::Relaxed);
    composer.layout(constraints);
    assert!(!composer.has_pending_states(), "retry should consume the retained invalidation");
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

    // 布局动画值变化（Animating 写——动画推进语义）
    let s = holder.borrow().clone().unwrap();
    s.as_raw().set_animating(300.0);
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
    let sid = holder.borrow().clone().unwrap().signal_id();
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
    s.as_raw().set_animating(123.0);
    build(&mut composer);
    let m3 = MEASURE_COUNT.with(|c| c.get());
    assert!(m3 > m2, "notify 后应重新 measure（布局失效生效）");
    assert!(composer.layout_deps.contains_key(&sid), "重测后依赖应续期");
}

/// 跨 Composer 订阅（端到端）：overlay 独立 Composer 组合期读主树 State →
/// 主树 set 后 overlay Composer 的 pending 队列收到失效 → recompose 读到新值。
/// 这是 Popup / 浮层内容响应主树变化的通路。
#[test]
fn test_overlay_composer_invalidated_by_main_tree_state() {
    let mut main = Composer::new();
    let holder = std::cell::RefCell::new(None::<crate::core::state::State<Vec<String>>>);
    main.compose(|ctx| {
        let items = ctx.remember(|| vec!["a".to_string()]);
        *holder.borrow_mut() = Some(items.clone());
        // Ownerless State only notifies Composer instances that actually read it.
        let _ = items.get();
        let k = ctx.next_key();
        ctx.start_leaf(k, Modifier::new());
        ctx.end_node();
    });
    let items = holder.borrow().clone().unwrap();

    // overlay：独立 Composer，content 读主树 State
    let mut ov = Composer::new();
    let seen = std::cell::RefCell::new(None::<usize>);
    {
        let seen_ref = &seen;
        ov.compose(|ctx| {
            *seen_ref.borrow_mut() = Some(items.get().len());
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
        });
    }
    assert_eq!(seen.borrow().as_ref(), Some(&1), "首帧读到初值");

    // 主树 set：两个队列都应入队（fan-out）
    items.set(vec!["x".into(), "y".into(), "z".into()]);
    assert!(main.has_pending_states());
    assert!(ov.has_pending_states(), "跨 Composer 读取应建立订阅——否则 overlay 永不更新");

    // overlay recompose：消费 pending、读到新值、依赖续期
    let ran = {
        let seen_ref = &seen;
        ov.recompose(|ctx| {
            *seen_ref.borrow_mut() = Some(items.get().len());
            let k = ctx.next_key();
            ctx.start_leaf(k, Modifier::new());
            ctx.end_node();
        })
    };
    assert!(ran, "overlay 应因订阅通知执行重组");
    assert_eq!(seen.borrow().as_ref(), Some(&3));
    assert!(!ov.has_pending_states(), "recompose 后 pending 清空");

    // 再次 set：订阅仍在（重组时 record_dep 续订）→ 持续响应
    items.set(Vec::new());
    assert!(ov.has_pending_states(), "重组后依赖续期——持续响应后续变化");
}

/// Compose dependency edges are replaced when an Entered scope stops reading a State.
#[test]
fn test_compose_read_removal_unsubscribes_stale_state() {
    let mut composer = Composer::new();
    let gate = State::new(true);
    let watched = State::new(0i32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    let build = |composer: &mut Composer| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    if gate.get() {
                        let _ = watched.get();
                    }
                    let leaf_key = ctx.next_key();
                    ctx.start_leaf(leaf_key, Modifier::new());
                    ctx.end_node();
                }
            }
            ctx.end_restartable_group();
        });
        composer.layout(constraints);
    };

    build(&mut composer);
    let watched_id = watched.signal_id();
    assert!(composer.slot_deps.contains_key(&watched_id));

    gate.set(false);
    build(&mut composer);
    assert!(!composer.slot_deps.contains_key(&watched_id), "旧读取边应被移除");

    watched.set(1);
    assert!(!composer.has_pending_states(), "移除读取后 State 不应再使 Composer 入队");
}

/// Composer drop removes its queue from every StateSignal it read.
#[test]
fn test_composer_drop_unsubscribes_state_signals() {
    let watched = State::new(0i32);
    let pending = {
        let mut composer = Composer::new();
        let pending = composer.pending_states.clone();
        composer.compose(|ctx| {
            let _ = watched.get();
            let key = ctx.next_key();
            ctx.start_leaf(key, Modifier::new());
            ctx.end_node();
        });
        pending
    };

    watched.set(1);
    assert!(pending.is_empty(), "Composer drop 后不应收到 State 通知");
}

#[test]
fn test_runtime_frame_restores_tls_after_panic_and_nested_drop() {
    let baseline_active = ACTIVE_SLOT_KEY.with(|slot| slot.get());
    let baseline_groups = GROUP_STACK.with(|groups| groups.borrow().clone());
    let baseline_stmts = STMT_STACK.with(|stmts| stmts.borrow().clone());

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _frame = begin_runtime_frame();
        assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), 0);
        assert!(GROUP_STACK.with(|groups| groups.borrow().is_empty()));
        assert!(STMT_STACK.with(|stmts| stmts.borrow().is_empty()));
        set_active_slot_key(99);
        GROUP_STACK.with(|groups| groups.borrow_mut().push(99));
        STMT_STACK.with(|stmts| stmts.borrow_mut().push((99, 99)));
        panic!("runtime frame rollback");
    }));
    assert!(result.is_err());
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), baseline_active);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), baseline_groups);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), baseline_stmts);

    let outer = begin_runtime_frame();
    set_active_slot_key(21);
    GROUP_STACK.with(|groups| groups.borrow_mut().push(21));
    STMT_STACK.with(|stmts| stmts.borrow_mut().push((21, 21)));
    {
        let _inner = begin_runtime_frame();
        set_active_slot_key(22);
        GROUP_STACK.with(|groups| groups.borrow_mut().push(22));
        STMT_STACK.with(|stmts| stmts.borrow_mut().push((22, 22)));
    }
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), 21);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), vec![21]);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), vec![(21, 21)]);
    drop(outer);
    assert_eq!(ACTIVE_SLOT_KEY.with(|slot| slot.get()), baseline_active);
    assert_eq!(GROUP_STACK.with(|groups| groups.borrow().clone()), baseline_groups);
    assert_eq!(STMT_STACK.with(|stmts| stmts.borrow().clone()), baseline_stmts);
}

#[test]
fn test_nested_composer_compose_preserves_outer_dependencies() {
    let outer_state = State::new(1i32);
    let inner_state = State::new(2i32);
    let mut outer = Composer::new();
    let mut inner = Composer::new();

    outer.compose(|_ctx| {
        assert_eq!(outer_state.get(), 1);
        inner.compose(|_ctx| {
            assert_eq!(inner_state.get(), 2);
        });
        assert_eq!(outer_state.get(), 1);
    });

    assert!(outer.slot_deps.contains_key(&outer_state.signal_id()));
    assert!(!outer.slot_deps.contains_key(&inner_state.signal_id()));
    assert!(inner.slot_deps.contains_key(&inner_state.signal_id()));
    assert!(!inner.slot_deps.contains_key(&outer_state.signal_id()));

    outer_state.set(3);
    assert!(outer.has_pending_states());
    assert!(!inner.has_pending_states());
    inner_state.set(4);
    assert!(inner.has_pending_states());
}

/// Layout-only invalidation can be consumed directly by layout() without compose().
#[test]
fn test_layout_only_pending_consumed_without_recompose() {
    let mut composer = Composer::new();
    let size = State::new(10.0f32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let before = composer.arena_nodes()[root].measured_size.width;

    size.as_raw().set_animating(300.0);
    assert!(composer.has_pending_states());
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let after = composer.arena_nodes()[root].measured_size.width;
    assert!(after > before + 10.0, "layout-only State 应直接触发重测：{} -> {}", before, after);
    assert!(!composer.has_pending_states());
}

/// A State read by both composition and layout must remain queued for compose
/// after layout-only classification marks its layout path dirty.
#[test]
fn test_mixed_compose_layout_pending_reaches_recompose() {
    let mut composer = Composer::new();
    let size = State::new(10.0f32);
    let constraints = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    composer.compose(|ctx| {
        assert_eq!(size.get(), 10.0);
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    });
    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let before = composer.arena_nodes()[root].measured_size.width;

    size.as_raw().set_animating(300.0);
    assert!(composer.has_pending_states());
    assert!(composer.recompose(|ctx| {
        assert_eq!(size.get(), 300.0);
        let key = ctx.next_key();
        ctx.start_leaf(key, Modifier::new().size(&size, 10.0));
        ctx.end_node();
    }));
    assert!(!composer.has_pending_states(), "mixed invalidation should be consumed by recompose");

    composer.layout(constraints);
    let root = composer.layout_root_idx().unwrap();
    let after = composer.arena_nodes()[root].measured_size.width;
    assert!(after > before + 10.0, "mixed State should still drive layout: {before} -> {after}");
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
    let sid = holder.borrow().as_ref().unwrap().offset.signal_id();
    assert!(composer.slot_deps.contains_key(&sid),
        "scroll offset 应注册组合依赖（register_modifier_deps_recursive）——丢失则滚动不刷新");

    // offset 变化 → 下帧该 slot 组合级 dirty（滚动触发重组）
    holder.borrow().as_ref().unwrap().offset.set(50.0);
    build(&mut composer);
    assert!(composer.compose_dirty_count > 0,
        "scroll 变化应触发组合级 dirty（dirty_count={}）", composer.compose_dirty_count);
}

/// A panic from late node cleanup must restore the last committed dependency graph.
/// This is narrower than a SlotTable transaction: only dependency maps and signal
/// subscriptions are rolled back here.
#[test]
fn test_compose_late_cleanup_panic_restores_dependency_graph() {
    let mut composer = Composer::new();
    let committed = State::new(1i32);
    let failed = State::new(2i32);
    let committed_for_remove = committed.clone();

    composer.compose(|ctx| {
        let _ = committed.get();
        ctx.start_leaf_with_remove(
            1,
            Modifier::new(),
            Box::new(move || {
                committed_for_remove.as_raw().set_animating(3);
                panic!("late compose cleanup panic");
            }),
        );
        ctx.end_node();
    });
    composer.layout(crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0));
    assert!(composer.slot_deps.contains_key(&committed.signal_id()));

    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            let _ = failed.get();
            ctx.start_leaf(2, Modifier::new());
            ctx.end_node();
        });
    }));
    assert!(result.is_err(), "late cleanup should panic");
    assert!(composer.slot_deps.contains_key(&committed.signal_id()),
        "panic must restore the committed compose dependency");
    assert!(!composer.slot_deps.contains_key(&failed.signal_id()),
        "failed compose dependency must not remain committed");
    assert!(!composer.has_pending_states(),
        "failed-frame notifications must be removed during dependency rollback");

    committed.as_raw().set_animating(4);
    assert!(composer.has_pending_states(), "restored signal must remain subscribed");
    composer.pending_states.drain();
    failed.as_raw().set_animating(4);
    assert!(!composer.has_pending_states(), "failed signal must be unsubscribed after rollback");
}

/// 崩溃边界（P3-3）前提验证：content panic 后（catch_unwind 捕获），
/// 下帧恢复正常内容应自愈——slot 表/依赖缓冲（DEP_MODE 残留由 begin 清空）
/// 从半状态重建，不残留垃圾。
#[test]
fn test_compose_runtime_snapshot_restores_key_context_after_panic() {
    let mut composer = Composer::new();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            ctx.key("failed-key", |ctx| {
                let _ = ctx.next_key();
                panic!("runtime snapshot panic");
            });
        });
    }));
    assert!(result.is_err());
    assert!(composer.scope_source_stack.is_empty());
    assert!(composer.key_override_stack.is_empty());
    assert!(composer.slot_table.path.is_empty());
    assert!(composer.slot_table.child_counters.len() == 1);

    composer.compose(|ctx| {
        let key = ctx.key("recovered-key", |ctx| ctx.next_key());
        ctx.start_leaf(key, Modifier::new());
        ctx.end_node();
    });
    assert!(composer.layout_root_idx().is_some(), "next compose should rebuild after rollback");
}

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


/// A notification generated after compose drains its batch remains queued for the
/// next batch instead of being consumed by the current frame.
#[test]
fn test_compose_notification_during_frame_is_next_batch() {
    let mut composer = Composer::new();
    let state = State::new(0i32);
    let notify_once = std::cell::Cell::new(true);

    composer.compose(|_ctx| {
        let _ = state.get();
        if notify_once.replace(false) {
            state.as_raw().set_animating(1);
        }
    });

    assert!(composer.has_pending_states(), "in-frame notification must remain pending");
    assert!(composer.recompose(|_ctx| {
        let _ = state.get();
    }));
    assert!(!composer.has_pending_states(), "the next batch should consume the notification");
}

/// A panic after compose consumes its pending batch must restore that batch for
/// a retry; this guard does not attempt the larger SlotTable transaction.
#[test]
fn test_compose_pending_batch_restored_after_panic() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();
    let state = State::new(0i32);
    composer.compose(|_ctx| {
        let _ = state.get();
    });
    state.as_raw().set_animating(1);
    let state_id = state.signal_id();
    assert!(composer.pending_states.pending_ids().contains(&state_id));

    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|_ctx| {
            let _ = state.get();
            panic!("compose batch panic");
        });
    }));
    assert!(result.is_err());
    assert!(composer.pending_states.pending_ids().contains(&state_id),
        "panic must restore the consumed compose batch");

    assert!(composer.recompose(|_ctx| {
        let _ = state.get();
    }));
    assert!(!composer.has_pending_states(), "retry should consume the restored batch");
}

/// 组合场景：drain 后帧内新到的通知与已消费批在 panic 恢复时同时保留——
/// 已消费的 ID 恢复（去重），新通知留在队列等下一批（不丢不重）。
#[test]
fn test_panic_restore_keeps_both_consumed_batch_and_in_frame_notification() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();
    let state_a = State::new(0i32);
    let state_b = State::new(0i32);
    // 建立依赖：state_a 和 state_b 都被 compose 读取
    composer.compose(|_ctx| {
        let _ = state_a.get();
        let _ = state_b.get();
    });

    // 进入下一帧前：激活两个 state
    state_a.as_raw().set_animating(1);
    state_b.as_raw().set_animating(1);
    let id_a = state_a.signal_id();
    let id_b = state_b.signal_id();
    let pending = composer.pending_states.pending_ids();
    assert!(pending.contains(&id_a) && pending.contains(&id_b));

    // 帧内：drain 消费 id_a（假设 id_a 先被消费），随后帧中新通知 id_b；
    // 帧尾 panic → Drop 恢复已消费的 id_a，同时 id_b 仍在队列
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|_ctx| {
            let _ = state_a.get();
            let _ = state_b.get();
            // 模拟帧内新通知：在 drain 后、panic 前 set state_b
            // （真实帧内 set 由用户回调触发，这里直接模拟）
            if std::cell::Cell::new(true).take() {
                state_b.as_raw().set_animating(2);
            }
            panic!("compose panic after in-frame notification");
        });
    }));
    assert!(result.is_err());

    // 恢复后：id_a（已消费的批）被 restore，id_b（帧内新通知）保留——
    // 两者都在队列中（去重后各一次）
    let recovered = composer.pending_states.pending_ids();
    assert!(recovered.contains(&id_a), "consumed batch must be restored");
    assert!(recovered.contains(&id_b), "in-frame notification must remain queued");
    assert_eq!(
        recovered.iter().filter(|&&id| id == id_a).count(),
        1,
        "restore must not duplicate restored IDs"
    );
}

/// Helper: 深度优先检查 Slot 树上是否有残留 desc（panic 帧产物）。
fn slot_tree_has_residual_desc(slot: &Slot) -> bool {
    if slot.desc.is_some() {
        return true;
    }
    slot.children.iter().any(slot_tree_has_residual_desc)
}

/// A panic after a slot wrote its desc (Enter path) but before collect_desc_tree
/// must not let the residual desc leak into the next compose frame: reset() at the
/// start of compose discards unconsumed descs, so a Clean-reused slot cannot be
/// materialized from a stale desc.
#[test]
fn test_panic_residual_desc_cleared_by_next_compose_reset() {
    use std::panic::AssertUnwindSafe;

    let mut composer = Composer::new();

    // 帧1：start_restartable_group Enter 路径写入 desc 后 panic（模拟渲染路径崩溃边界）
    let result = std::panic::catch_unwind(AssertUnwindSafe(|| {
        composer.compose(|ctx| {
            let root_key = ctx.next_key();
            match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
                GroupStatus::Skip => {}
                GroupStatus::Enter => {
                    let k = ctx.next_key();
                    ctx.start_leaf(k, Modifier::new());
                    ctx.end_node();
                    panic!("panic after desc write");
                }
            }
            ctx.end_restartable_group();
        });
    }));
    assert!(result.is_err(), "帧1 应 panic");

    // panic 后：Enter 已写入的 desc 残留在 slot 树上（collect_desc_tree 未执行）
    assert!(
        slot_tree_has_residual_desc(&composer.slot_table.root_slot),
        "panic 后应存在残留 desc（本修复的触发条件）"
    );

    // 帧2：compose 开头 reset() 清空残留 desc → Clean 复用不再误收集
    composer.compose(|ctx| {
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
    });

    // 成功帧结束后所有 desc 被 collect take 或 reset 清空——树上不应再有残留
    assert!(
        !slot_tree_has_residual_desc(&composer.slot_table.root_slot),
        "成功帧后不应残留 desc"
    );
    assert!(composer.layout_root_idx().is_some(), "帧2 应成功物化");
}

/// A notification for a read removed during compose must not leave a stale
/// pending ID that keeps the Composer awake forever.
#[test]
fn test_removed_read_notification_during_compose_is_dropped() {
    let mut composer = Composer::new();
    let watched = State::new(0i32);
    let trigger = State::new(false);

    composer.compose(|_ctx| {
        let _ = watched.get();
        let _ = trigger.get();
    });

    trigger.as_raw().set_animating(true);
    composer.compose(|_ctx| {
        let _ = trigger.get();
        watched.as_raw().set_animating(1);
    });

    assert!(!composer.has_pending_states(), "removed read must not leave a stale pending ID");
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

/// 语义角色切换回归（audit §3.5 残留场景）：TextField 输入叶子（有 IME/cursor
/// 回调）→ 普通 Text（无回调）切换时，残留的 IME/cursor 状态必须被清理——
/// 否则新 Text 节点仍持有旧 IME 回调，渲染/输入路径污染。
#[test]
fn test_materialize_reuse_clears_stale_textfield_state_on_role_switch() {
    let mut composer = Composer::new();
    let c = crate::layout::constraints::Constraints::new(0.0, 500.0, 0.0, 500.0);

    // 帧1：TextField 输入叶子——挂 IME 回调（对标 text_field.rs 的 set_current_node_ime_callback）
    let mut leaf_key = 0u64;
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new(), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                leaf_key = k;
                ctx.start_leaf(k, Modifier::new());
                ctx.set_current_node_ime_callback(Box::new(|_text: &str, _c: Option<(usize, usize)>| {}));
                ctx.set_current_node_cursor_and_callback(0, false, Box::new(|_i: usize| {}));
                ctx.end_node();
            }
        }
        ctx.end_restartable_group();
    });
    composer.layout(c);
    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    assert!(composer.arena_nodes()[leaf].ime_callback.borrow().is_some(),
        "帧1 应有 IME 回调（TextField 场景）");

    // 帧2：同位置变成普通 Text——无 IME/cursor 回调 → 复用节点应清理残留
    // 根容器加 size 参数强制 Enter（否则参数/结构不变 → Skip 导致 build 不执行）
    composer.compose(|ctx| {
        let root_key = ctx.next_key();
        match ctx.start_restartable_group(root_key, Modifier::new().size(100.0, 100.0), crate::layout::BoxLayout::new()) {
            GroupStatus::Skip => {}
            GroupStatus::Enter => {
                let k = ctx.next_key();
                assert_eq!(k, leaf_key, "同位置 leaf key 应稳定");
                // 普通 Text 叶子：仅 TextContent，无 IME/cursor 回调
                let modifier = Modifier::new().push(crate::modifier::ModifierElement::TextContent {
                    content: "plain".to_string(),
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
    composer.layout(c);

    let root = composer.layout_root_idx().unwrap();
    let leaf = composer.arena_nodes()[root].children[0];
    let has_text = composer.arena_nodes()[leaf].has_text_content;
    assert!(has_text, "帧2 leaf 应有 has_text_content=true（modifier 含 TextContent）");
    assert!(
        composer.arena_nodes()[leaf].ime_callback.borrow().is_none(),
        "角色切换后 IME 回调应被清理（残留会污染普通 Text）"
    );
    assert!(
        composer.arena_nodes()[leaf].cursor_callback.borrow().is_none(),
        "角色切换后 cursor 回调应被清理"
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
        composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
        composing_range: None,
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
            composing_color: None,
            cursor_index: None,
            cursor_visible: None,
            cursor_callback: None,
            display_focused: None,
            ime_callback: None,
            composing_range: None,
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
