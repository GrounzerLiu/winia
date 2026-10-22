use super::*;

// ═══════════════════════════════════════════════════════════
// DerivedValue<T> — 泛型派生值（State 变换的延迟表达式，对标 Compose derivedStateOf）
// ═══════════════════════════════════════════════════════════

/// 泛型派生值：`&State<f32>` 算术运算或其他 State 变换的延迟表达式。
///
/// 读取时执行闭包（内部 `State::get()` 在组合/测量上下文注册依赖），
/// 动画值变化 → 依赖节点 dirty → 重组重测 → 表达式重算。
/// f32 特化支持算术运算符（`&alpha * 200.0 + 50.0`）；任意类型用 `DerivedValue::new`。
#[derive(Clone)]
pub struct DerivedValue<T>(pub(crate) Arc<dyn Fn() -> T + Send + Sync>);

impl<T> DerivedValue<T> {
    /// 从闭包构建派生值（复杂表达式/自定义逻辑用，对标 Compose `derivedStateOf { }`）。
    /// 闭包内 `State::get()` 在组合/测量上下文注册依赖 → 动画值变化触发节点重组重测。
    pub fn new(f: impl Fn() -> T + Send + Sync + 'static) -> Self {
        DerivedValue(Arc::new(f))
    }

    /// 求值当前表达式（组合/测量上下文内调用 → 内部 State::get() 注册依赖）
    pub fn get(&self) -> T {
        (self.0)()
    }
}

/// f32 派生值别名（算术运算符的返回类型）
pub type DerivedFloat = DerivedValue<f32>;

macro_rules! impl_derived_arith {
    ($trait:ident, $method:ident, $op:tt) => {
        impl std::ops::$trait<f32> for DerivedFloat {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let f = self.0.clone();
                DerivedValue(Arc::new(move || f() $op rhs))
            }
        }
        impl std::ops::$trait<f32> for &DerivedFloat {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let f = self.0.clone();
                DerivedValue(Arc::new(move || f() $op rhs))
            }
        }
        impl std::ops::$trait<f32> for &State<f32> {
            type Output = DerivedFloat;
            fn $method(self, rhs: f32) -> DerivedFloat {
                let s = self.clone();
                DerivedValue(Arc::new(move || s.get() $op rhs))
            }
        }
    };
}

impl_derived_arith!(Add, add, +);
impl_derived_arith!(Sub, sub, -);
impl_derived_arith!(Mul, mul, *);
impl_derived_arith!(Div, div, /);

/// 常数在左：`2.0 * alpha`
impl std::ops::Mul<&State<f32>> for f32 {
    type Output = DerivedFloat;
    fn mul(self, rhs: &State<f32>) -> DerivedFloat {
        let s = rhs.clone();
        DerivedValue(Arc::new(move || self * s.get()))
    }
}
