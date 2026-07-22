//! CompositionLocal — 类似 Jetpack Compose 的隐式数据传递机制。
//!
//! 每个实例通过全局递增的 ID 区分。`provides()` 压栈，闭包返回后弹栈。
//! 嵌套 provides（同实例或不同实例）通过 unique ID + rposition 正确隔离。
//! Panic-safe: PopGuard 在栈展开时自动清理。

use std::any::Any;
use std::cell::RefCell;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

thread_local! {
    /// (local_id, value) 栈 —— `provides` push, 闭包结束 pop
    static SLOTS: RefCell<Vec<(u64, Box<dyn Any>)>> = RefCell::new(Vec::new());
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
    pub fn current(&self) -> T {
        SLOTS.with(|s| {
            let slots = s.borrow();
            for (sid, val) in slots.iter().rev() {
                if *sid == self.id {
                    return val.downcast_ref::<T>().unwrap().clone();
                }
            }
            (self.default)()
        })
    }

    /// 在闭包执行期间提供新值。嵌套 provides 通过 unique ID 正确隔离——
    /// 即使是同一个 `CompositionLocal` 再次嵌套，`rposition` 也能弹出最内层。
    pub fn provides<R>(&self, value: T, f: impl FnOnce() -> R) -> R {
        SLOTS.with(|s| s.borrow_mut().push((self.id, Box::new(value))));
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
}
