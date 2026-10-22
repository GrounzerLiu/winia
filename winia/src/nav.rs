//! 导航层（对标 Jetpack Navigation 3 设计）——状态驱动的 back stack + 声明式投影。
//!
//! 借鉴 Navigation3 的核心架构（源码：androidx/navigation3）：
//! - **你拥有 back stack**：导航状态就是普通 winia State（`State<Vec<K>>`），
//!   导航 = 改列表（push/pop/remove_last），观察者（NavDisplay）自动重组。
//!   对标 Nav3 的 `NavBackStack = SnapshotStateList<T>` 包装。
//! - **类型安全路由**：`NavKey` trait 取代字符串路由（对标 Nav3 的 `NavKey` 接口）。
//! - **NavDisplay 是状态的投影**：观察 back stack → entry_provider 生成 NavEntry →
//!   渲染当前 Scene。对标 Nav3 的 `NavDisplay = 观察 List + entryProvider`。
//! - **Scene 概念**：Scene 可渲染一个或多个 entry（多栏/自适应布局）。
//!   当前实现 SinglePane（渲染栈顶），为多栏留扩展。
//!
//! 与 Compose 的差异（winia 简化）：
//! - Compose 用 `SnapshotStateList`（细粒度 add/remove 通知）；winia 用
//!   `State<Vec<K>>` 整体替换（PartialEq 去重）——导航操作频率低，整体通知
//!   代价可接受，且与 winia 的 State 系统完全契合。
//! - 过渡动画：NavTransition 双页过渡（对标 Nav3 transitionSpec / popTransitionSpec
//!   的 ContentTransform——`NavEnter`/`NavExit` 原语成对组合，默认 fade；
//!   graphics_layer 渲染期 peek 零重组）。

use crate::runtime::state::State;
use crate::composable;
use crate::runtime::composer::ComposeCtx;
use crate::modifier::Modifier;
use crate::animation::push_animatable;
// The slide distance is shared with `AnimatedVisibility`; nav used to declare its own copy.
use crate::animation::SlideOffset;
use crate::graphics::GraphicsLayerParams;
use std::any::Any;
use std::collections::HashMap;

// ═══════════════════════════════════════════════════════════
// Entry 状态池（对标 Nav3 SaveableStateHolder——外部状态池，不依赖组合槽）
// ═══════════════════════════════════════════════════════════

/// entry 状态作用域——NavDisplay 渲染 entry 时经 CompositionLocal 提供。
///
/// 对标 Nav3 `SaveableStateHolder.SaveableStateProvider(contentKey)`：
/// - Nav3：状态存在外部 holder（HashMap<contentKey, SaveableStateRegistry>），
///   pop 时 removeState 清理，重进时恢复——不依赖组合位置
/// - winia：状态池存 `Arc<Mutex<HashMap<(u64, u32, TypeId), Box<dyn Any>>>>`
///   （NavDisplay remember，跨帧稳定）——entry 内容用 `remember_entry_state`
///   存取，跨 pop/push 保持
///
/// 组合槽机制无法自动保持 remember（槽随子树移除销毁），故用显式状态池
/// 替代（同 Nav3 的显式 SaveableStateProvider 语义）。
///
/// 作用域经 CompositionLocal 提供（而非裸 thread_local）——PopGuard 自动弹栈，
/// panic/嵌套安全（对齐 winia Theme/Density/SelectionRegistrar 的作用域模式）。
#[derive(Clone)]
pub(crate) struct EntryStateScope {
    pool: std::sync::Arc<std::sync::Mutex<HashMap<(u64, u32, std::any::TypeId), Box<dyn Any>>>>,
    key: u64,
    counter: crate::runtime::state::Backchannel<u32>,
    /// 过渡滑出层（previous）用只读作用域：命中返回现存槽，miss **不入池**。
    /// 滑出期间旧页内容每帧重跑，若照常 miss→insert 会把刚被 removeState
    /// 清理的槽重新插回（下次 pop 才再清）——破坏 Nav3 removeState 语义。
    draining: bool,
}

/// entry 状态作用域 CompositionLocal——NavDisplay 渲染 entry 时 provides，
/// `remember_entry_state` 读 current（try_current 区分作用域内外）
static ENTRY_STATE_SCOPE: std::sync::LazyLock<crate::runtime::composition_local::CompositionLocal<EntryStateScope>> =
    std::sync::LazyLock::new(|| {
        // default 不可达（try_current 不调 default）——占位
        crate::runtime::composition_local::CompositionLocal::new(|| {
            panic!("ENTRY_STATE_SCOPE 无默认值——必须经 NavDisplay 提供")
        })
    });

/// 在 entry 内容中记住状态——跨导航（pop/push）保持。
///
/// 对标 Nav3 `rememberSaveable`（经 SaveableStateHolder 保存）：
/// - 同一 **contentKey** 的 entry 每次进入返回同一 State（值保持）——默认
///   contentKey = key 的确定性 hash；`NavEntry::with_content_key` 可让不同路由
///   共享同一份状态（Nav3 语义：同内容 = 同状态）
/// - 状态池由 NavDisplay 持有（跨帧稳定，Arc<Mutex> 内部可变），不依赖组合槽
/// - 同一 entry 内多次调用按调用顺序分配独立槽
///
/// 用法（entry 内容内）：
/// ```rust
/// let count = nav::remember_entry_state(|| 0i32);
/// ```
///
/// ⚠ 约束（槽 key = (contentKey, 调用序号, 类型)）：
/// - 调用位置/顺序须逐帧稳定——勿放进条件分支或循环内长度可变的路径
///   （条件翻转会导致序号整体移位、槽重映射，语义同 Compose remember 的位置制）；
/// - 同一 back stack 内 **contentKey 应唯一**——重复 contentKey 的实例共享同一
///   池槽（可利用：`Detail(1)`/`Detail(2)` 用同一 contentKey 即共享状态）；
///   同 key 不同 contentKey 则相互独立。
///
/// 注意：只有本函数（外部状态池）保证跨 pop/push 保持；entry 内容里的普通
/// `ctx.remember` 依赖组合槽位置，双层过渡的槽位按调用序分配，pop/push 边界
/// 不保证存活（见 [`RememberStateDecorator`] 文档）。
pub fn remember_entry_state<T: Clone + PartialEq + 'static>(
    init: impl FnOnce() -> T,
) -> State<T> {
    let Some(scope) = ENTRY_STATE_SCOPE.try_current() else {
        panic!("remember_entry_state 必须在 NavDisplay 的 entry 内容内调用（CompositionLocal 作用域外）");
    };
    // 槽 key = (entry key, 调用序号, 类型)——类型参与 key：不同 T 不撞槽，
    // 命中即类型正确（无需 downcast 失败回退）
    let seq = scope.counter.peek();
    scope.counter.set(seq + 1);
    let slot_key = (scope.key, seq, std::any::TypeId::of::<T>());
    let mut pool = scope.pool.lock().unwrap();
    if let Some(b) = pool.get(&slot_key) {
        // TypeId 参与 key——命中必是 State<T>（fail-fast 防御）
        return b.downcast_ref::<State<T>>()
            .expect("状态池槽类型不匹配（TypeId 相同但 downcast 失败——内部错误）")
            .clone();
    }
    if scope.draining {
        // 滑出层只读作用域：miss 不入池（防重污染）——返回一次性 State，
        // 值不跨帧保持（该页正在淡出，视觉上仅是退出动画的临时内容）
        return State::new(init());
    }
    let s = State::new(init());
    pool.insert(slot_key, Box::new(s.clone()));
    s
}

/// 清理 entry 状态（pop 时由装饰器调用——对标 Nav3 removeState）
pub(crate) fn clear_entry_state(key: u64, pool: &std::sync::Arc<std::sync::Mutex<HashMap<(u64, u32, std::any::TypeId), Box<dyn Any>>>>) {
    let mut p = pool.lock().unwrap();
    p.retain(|(k, _, _), _| *k != key);
}

/// 路由 key 的稳定 hash（状态池槽 key 用）。
/// ⚠ 不能用 DefaultHasher——其种子随机，同一值每次 hash 不同 → 槽 key 漂移
/// → 状态池永不命中。用 FNV-1a（确定性，与 winia mix_key 同源）。
fn key_hash<K: NavKey>(key: &K) -> u64 {
    fnv_hash(key)
}

/// 确定性 FNV-1a（框架内部标识用——scene key 等场景无关 hash）
fn fnv_hash(value: &impl std::hash::Hash) -> u64 {
    use std::hash::Hasher;
    struct FnvHasher(u64);
    impl std::hash::Hasher for FnvHasher {
        fn finish(&self) -> u64 { self.0 }
        fn write(&mut self, bytes: &[u8]) {
            for &b in bytes {
                self.0 ^= b as u64;
                self.0 = self.0.wrapping_mul(0x100000001b3);
            }
        }
    }
    let mut h = FnvHasher(0xcbf29ce484222325);
    value.hash(&mut h);
    h.finish()
}

// ═══════════════════════════════════════════════════════════
// NavKey — 路由标识（对标 Nav3 的 NavKey 接口）
// ═══════════════════════════════════════════════════════════

/// 导航路由标识——类型安全取代字符串路由。
///
/// 用户定义自己的路由类型（enum 或 data struct），实现本 trait：
/// ```rust
/// #[derive(Clone, PartialEq, Eq, Debug, Hash)]
/// enum Route {
///     Home,
///     Detail(u64),
/// }
/// // Route 自动满足 NavKey（trait 无方法，仅约束）
/// ```
///
/// 对标 Nav3：`NavKey` 是标记接口（`@Serializable` 用于持久化）。winia 无
/// 序列化要求，trait 仅作类型约束（`Clone + PartialEq + Eq + Hash + Debug +
/// Send + Sync + 'static`——Hash 用于状态保持装饰器的固定组合 key；Send+Sync
/// 用于 dismiss 等跨线程回调）。
mod back_stack;
mod decorator;
mod display;
mod key;

pub use back_stack::*;
pub use decorator::*;
pub use display::*;
pub use key::*;

#[cfg(test)]
mod tests;
