//! CompositionLocal — 类似 Jetpack Compose 的隐式数据传递机制。
//!
//! 每个实例通过全局递增的 ID 区分。`provides()` 压栈，闭包返回后弹栈。
//! 嵌套 provides（同实例或不同实例）通过 unique ID + rposition 正确隔离。
//! Panic-safe: PopGuard 在栈展开时自动清理。
//!
//! 快照机制（`capture`/`with_snapshot`）：值用 `Arc<dyn Any>` 存储——快照
//! 只是 `Arc::clone`（引用计数 +1，O(1) 浅拷贝，无需值 Clone）。用于独立
//! 组合单元（overlay 等）继承捕获时的隐式上下文（主题/方向/排版）。

use std::any::Any;
use std::cell::RefCell;
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// (local_id, value) 栈 —— `provides` push, 闭包结束 pop
    static SLOTS: RefCell<Vec<(u64, Arc<dyn Any>)>> = RefCell::new(Vec::new());
}

/// CompositionLocal 栈快照（`capture` 产物——可在 later 时刻 `with_snapshot`
/// 重放，如 overlay 独立 Composer 继承主树主题/方向/排版等隐式上下文）
pub type LocalSnapshot = Vec<(u64, Arc<dyn Any>)>;

/// 捕获当前 CompositionLocal 栈快照（Arc 引用共享——O(1) 浅拷贝）。
/// 需在相关 provides 闭包**内**调用（如 open_overlay 组合期——主树 Theme
/// provides 内）。
pub fn capture() -> LocalSnapshot {
    SLOTS.with(|s| {
        let slots = s.borrow();
        slots.iter().map(|(id, arc)| (*id, Arc::clone(arc))).collect()
    })
}

/// 在闭包执行期间重放快照（压栈全部 → 执行 → 弹栈）。用于独立组合单元
/// （overlay 等）继承捕获时的隐式上下文。
/// ⚠ 弹栈按**压栈前深度**恢复——若 f 内部又 provides 了新值（嵌套），它们
/// 留在栈上不被误弹（只弹快照压入的条目）。
pub fn with_snapshot<R>(snapshot: &LocalSnapshot, f: impl FnOnce() -> R) -> R {
    if snapshot.is_empty() {
        return f();
    }
    // 压栈前深度——重放后截断回此深度（f 内部新 provides 的值不被误弹）
    let base_len = SLOTS.with(|s| s.borrow().len());
    SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        for (id, arc) in snapshot {
            slots.push((*id, Arc::clone(arc)));
        }
    });
    let mut guard = SnapshotGuard { base_len, used: false };
    let result = f();
    // 正常弹栈（drop guard 兜底 panic 路径）
    SLOTS.with(|s| {
        let mut slots = s.borrow_mut();
        slots.truncate(base_len);
    });
    guard.disarm();
    result
}

struct SnapshotGuard {
    base_len: usize,
    used: bool,
}
impl SnapshotGuard {
    fn disarm(&mut self) { self.used = true; }
}
impl Drop for SnapshotGuard {
    fn drop(&mut self) {
        // panic 安全：正常路径已弹（disarm）；若 f panic，此处兜底清理
        if !self.used {
            SLOTS.with(|s| {
                let mut slots = s.borrow_mut();
                slots.truncate(self.base_len);
            });
        }
    }
}

/// Panic 安全：即使闭包 panic，Drop 时也会从栈中移除对应条目。
struct PopGuard { id: u64, used: bool }

impl PopGuard {
    fn new(id: u64) -> Self { Self { id, used: false } }
    fn disarm(&mut self) { self.used = true; }
}

impl Drop for PopGuard {
    fn drop(&mut self) {
        if !self.used {
            SLOTS.with(|s| {
                let mut slots = s.borrow_mut();
                if let Some(pos) = slots.iter().rposition(|(id, _)| *id == self.id) {
                    slots.remove(pos);
                }
            });
        }
    }
}

pub struct CompositionLocal<T: Clone + 'static> {
    id: u64,
    default: fn() -> T,
}

/// Safety: 新建实例时分配唯一 ID。所有实例都应放在 `LazyLock` / `static` 中，
/// 确保只初始化一次（否则多次 `new()` 会产生不同的 ID）。
impl<T: Clone + 'static> CompositionLocal<T> {
    pub fn new(default: fn() -> T) -> Self {
        Self { id: NEXT_ID.fetch_add(1, Ordering::Relaxed), default }
    }

    /// 读取栈顶匹配的当前值，若无则返回 `default`。
    /// ⚠ 用 `T::clone(x)` 而非 `x.clone()`——`x` 是 `&T`，`.clone()` 会走
    /// `impl Clone for &T` 返回引用拷贝（借用逃逸）。
    pub fn current(&self) -> T {
        SLOTS.with(|s| {
            let slots = s.borrow();
            for (sid, val) in slots.iter().rev() {
                if *sid == self.id {
                    let t: &T = val.downcast_ref::<T>().unwrap();
                    return T::clone(t);
                }
            }
            (self.default)()
        })
    }

    /// 读取栈顶匹配的当前值；无匹配时返回 `None`（不调用 default——
    /// 用于区分"provides 内（有值）"与"provides 外（无值）"，如 Text 注册到
    /// SelectionContainer 的判定：provides 外不应注册到 default 空 registrar）
    pub fn try_current(&self) -> Option<T> {
        SLOTS.with(|s| {
            let slots = s.borrow();
            for (sid, val) in slots.iter().rev() {
                if *sid == self.id {
                    let t: &T = val.downcast_ref::<T>().unwrap();
                    return Some(T::clone(t));
                }
            }
            None
        })
    }

    /// 在闭包执行期间提供新值。嵌套 provides 通过 unique ID 正确隔离——
    /// 即使是同一个 `CompositionLocal` 再次嵌套，`rposition` 也能弹出最内层。
    pub fn provides<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        let arc: Arc<dyn Any> = Arc::new(value);
        SLOTS.with(|s| s.borrow_mut().push((self.id, arc)));
        let mut guard = PopGuard::new(self.id);
        let result = f();
        // 正常弹栈：找最后一个匹配 id（处理同实例嵌套）
        SLOTS.with(|s| {
            let mut slots = s.borrow_mut();
            if let Some(pos) = slots.iter().rposition(|(sid, _)| *sid == self.id) {
                slots.remove(pos);
            }
        });
        guard.disarm();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nested_same_local() {
        let local = CompositionLocal::new(|| 0i32);
        let result = local.provides(1, || {
            assert_eq!(local.current(), 1);
            local.provides(2, || {
                assert_eq!(local.current(), 2);
                42
            })
        });
        assert_eq!(result, 42);
        assert_eq!(local.current(), 0); // 恢复默认
    }

    #[test]
    fn test_nested_different_locals() {
        let a = CompositionLocal::new(|| "a_default".to_string());
        let b = CompositionLocal::new(|| "b_default".to_string());
        a.provides("A".to_string(), || {
            b.provides("B".to_string(), || {
                assert_eq!(a.current(), "A");
                assert_eq!(b.current(), "B");
            });
            assert_eq!(a.current(), "A");
            assert_eq!(b.current(), "b_default");
        });
        assert_eq!(a.current(), "a_default");
    }

    #[test]
    fn test_panic_safety() {
        let local = CompositionLocal::new(|| 0i32);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            local.provides(42, || {
                panic!("boom");
            });
        }));
        assert!(result.is_err());
        assert_eq!(local.current(), 0, "should restore default after panic");
    }

    #[test]
    fn test_default_when_no_provides() {
        let local = CompositionLocal::new(|| 99i32);
        assert_eq!(local.current(), 99);
    }

    #[test]
    fn test_try_current_scope_boundary() {
        let local = CompositionLocal::new(|| 0i32);
        assert_eq!(local.try_current(), None); // provides 外无值
        let result = local.provides(7, || {
            assert_eq!(local.try_current(), Some(7)); // provides 内有值
            local.current()
        });
        assert_eq!(result, 7);
        assert_eq!(local.try_current(), None); // 退出后恢复无值
    }

    /// 快照：provides 内捕获 → 闭包外重放（模拟 overlay 继承主树上下文）
    #[test]
    fn test_capture_and_with_snapshot() {
        let local = CompositionLocal::new(|| 0i32);
        let snap = local.provides(42, capture);
        // 闭包外：current 恢复默认
        assert_eq!(local.current(), 0);
        // 重放：provides 生效
        let r = with_snapshot(&snap, || local.current());
        assert_eq!(r, 42);
        // 重放后：恢复
        assert_eq!(local.current(), 0);
    }

    /// 快照嵌套重放不污染外部栈
    #[test]
    fn test_with_snapshot_restores_stack() {
        let a = CompositionLocal::new(|| "a0".to_string());
        let b = CompositionLocal::new(|| "b0".to_string());
        let snap_a = a.provides("A".to_string(), capture);
        // 内部 b.provides + 重放 a；b 在 with_snapshot 期间仍生效（不被误弹）
        let r = b.provides("B".to_string(), || {
            let inner = with_snapshot(&snap_a, || {
                assert_eq!(b.current(), "B", "快照重放期间外部 b 应仍生效");
                a.current()
            });
            assert_eq!(b.current(), "B", "with_snapshot 返回后外部 b 仍生效");
            inner
        });
        assert_eq!(r, "A");
        // b.provides 闭包结束 → b 弹栈回默认（与快照无关）
        assert_eq!(b.current(), "b0");
        assert_eq!(a.current(), "a0");
    }
}
