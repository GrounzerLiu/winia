use std::ops::{Add, Sub, Mul};
use std::sync::{Arc, Mutex};
use crate::shared::animation::shared_animation::{AnimatableValue, Animation};
use crate::shared::AnimationSpec;

/// 可以用于弹簧动画的类型必须满足的约束
pub trait Springable: {
    /// 获取绝对值（用于判断是否接近目标）
    fn abs(&self) -> f32;

    /// 零值
    fn zero() -> Self;
}

// 为基本类型实现 Springable
impl Springable for f32 {
    fn abs(&self) -> f32 {
        f32::abs(*self)
    }

    fn zero() -> Self {
        0.0
    }
}

impl Springable for f64 {
    fn abs(&self) -> f32 {
        f64::abs(*self) as f32
    }

    fn zero() -> Self {
        0.0
    }
}
macro_rules! impl_springable {
    (i, $($t:ty),*) => {
        $(
            impl Springable for $t {
                fn abs(&self) -> f32 {
                    <$t>::abs(*self) as f32
                }

                fn zero() -> Self {
                    0
                }
            }
        )*
    };
    (u, $($t:ty),*) => {
        $(
            impl Springable for $t {
                fn abs(&self) -> f32 {
                    <$t>::abs(self) as f32
                }

                fn zero() -> Self {
                    0
                }
            }
        )*
    };
}

impl_springable!(i, i8, i16, i32, i64, isize);
impl_springable!(u, u8, u16, u32, u64, usize);


/// 弹簧动画规格
#[derive(Debug, Clone, Copy)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    pub visibility_threshold: f32,
    pub mass: f32,
}

impl Default for SpringSpec {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl SpringSpec {
    pub const fn new(damping_ratio: f32, stiffness: f32) -> Self {
        Self {
            damping_ratio,
            stiffness,
            visibility_threshold: 0.01,
            mass: 1.0,
        }
    }

    pub fn damping_ratio(mut self, damping_ratio: f32) -> Self {
        self.damping_ratio = damping_ratio;
        self
    }

    pub fn stiffness(mut self, stiffness: f32) -> Self {
        self.stiffness = stiffness;
        self
    }

    pub fn visibility_threshold(mut self, threshold: f32) -> Self {
        self.visibility_threshold = threshold;
        self
    }

    pub fn mass(mut self, mass: f32) -> Self {
        self.mass = mass;
        self
    }

    pub const DEFAULT: Self = Self::new(1.0, 1500.0);
    pub const BOUNCY: Self = Self::new(0.5, 1500.0);
    pub const STIFF: Self = Self::new(0.8, 5000.0);
    pub const SOFT: Self = Self::new(0.9, 400.0);

    fn compute_coefficients(&self) -> SpringCoefficients {
        let omega_0 = (self.stiffness / self.mass).sqrt();
        let zeta = self.damping_ratio;

        if zeta < 1.0 - 1e-6 {
            let omega_d = omega_0 * (1.0 - zeta * zeta).sqrt();
            SpringCoefficients::Underdamped { omega_0, zeta, omega_d }
        } else if zeta > 1.0 + 1e-6 {
            let r1 = -omega_0 * (zeta - (zeta * zeta - 1.0).sqrt());
            let r2 = -omega_0 * (zeta + (zeta * zeta - 1.0).sqrt());
            SpringCoefficients::Overdamped { r1, r2 }
        } else {
            SpringCoefficients::CriticallyDamped { omega_0 }
        }
    }
}

impl<T> AnimationSpec<T> for SpringSpec
where
    T: Springable + AnimatableValue + 'static
{
    fn build(self, from: T, to: T) -> Box<dyn Animation<T>> {
        let mut spring = Spring::new(self, from);
        spring.animate_to(to);
        Box::new(spring)
    }
}

#[derive(Debug, Clone, Copy)]
enum SpringCoefficients {
    Underdamped { omega_0: f32, zeta: f32, omega_d: f32 },
    CriticallyDamped { omega_0: f32 },
    Overdamped { r1: f32, r2: f32 },
}

/// 泛型弹簧动画
pub struct Spring<T: Springable + AnimatableValue> {
    spec: SpringSpec,
    coefficients: SpringCoefficients,

    position: T,
    velocity: T,
    target: T,

    accumulator: f32,
    running: bool,

    /// 固定时间步长
    fixed_dt: f32,
    /// 上次更新的时间戳（用于自动计算 dt）
    last_update_time: Option<std::time::Instant>,
}

impl<T> Spring<T>
where
    T: Springable + AnimatableValue
{
    pub fn new(spec: SpringSpec, initial_value: T) -> Self {
        let coefficients = spec.compute_coefficients();
        Self {
            spec,
            coefficients,
            position: initial_value.clone(),
            velocity: T::zero(),
            target: initial_value,
            accumulator: 0.0,
            running: false,
            fixed_dt: 1.0 / 60.0,
            last_update_time: None,
        }
    }

    /// 手动更新（需要传入时间步长）
    pub fn update_with_dt(&mut self, dt: f32) -> T {
        if !self.running {
            return self.position.clone();
        }

        const MAX_STEPS: usize = 10;

        self.accumulator += dt;
        let mut steps = 0;

        while self.accumulator >= self.fixed_dt && steps < MAX_STEPS {
            self.step(self.fixed_dt);
            self.accumulator -= self.fixed_dt;
            steps += 1;

            if !self.running {
                self.accumulator = 0.0;
                break;
            }
        }

        self.position.clone()
    }

    fn step(&mut self, dt: f32) {
        let displacement = self.position.clone() - self.target.clone();

        match self.coefficients {
            SpringCoefficients::Underdamped { omega_0, zeta, omega_d } => {
                let exp_term = (-zeta * omega_0 * dt).exp();
                let cos_term = (omega_d * dt).cos();
                let sin_term = (omega_d * dt).sin();

                let c2_numerator = self.velocity.clone() + displacement.mul_f32(zeta * omega_0);
                let c2 = c2_numerator.mul_f32(1.0 / omega_d);

                let new_displacement = (displacement.mul_f32(cos_term) + c2.mul_f32(sin_term)).mul_f32(exp_term);

                let velocity_part1 = c2.mul_f32(omega_d * cos_term) + displacement.mul_f32(-zeta * omega_0 * cos_term);
                let velocity_part2 = displacement.mul_f32(-omega_d * sin_term) + c2.mul_f32(-zeta * omega_0 * sin_term);
                let new_velocity = (velocity_part1 + velocity_part2).mul_f32(exp_term);

                self.position = self.target.clone() + new_displacement;
                self.velocity = new_velocity;
            }

            SpringCoefficients::CriticallyDamped { omega_0 } => {
                let exp_term = (-omega_0 * dt).exp();
                let c2 = self.velocity.clone() + displacement.mul_f32(omega_0);

                self.position = self.target.clone() + (displacement.clone() + c2.mul_f32(dt)).mul_f32(exp_term);
                self.velocity = (c2.clone() - (displacement + c2.mul_f32(dt)).mul_f32(omega_0)).mul_f32(exp_term);
            }

            SpringCoefficients::Overdamped { r1, r2 } => {
                let exp1 = (r1 * dt).exp();
                let exp2 = (r2 * dt).exp();

                let c2_numerator = self.velocity.clone() - displacement.mul_f32(r1);
                let c2 = c2_numerator.mul_f32(1.0 / (r2 - r1));
                let c1 = displacement - c2.clone();

                self.position = self.target.clone() + c1.mul_f32(exp1) + c2.mul_f32(exp2);
                self.velocity = c1.mul_f32(r1 * exp1) + c2.mul_f32(r2 * exp2);
            }
        }

        // 检查停止条件
        let displacement_abs = (self.position.clone() - self.target.clone()).abs();
        let velocity_abs = self.velocity.abs();

        if displacement_abs < self.spec.visibility_threshold
            && velocity_abs < self.spec.visibility_threshold
        {
            self.position = self.target.clone();
            self.velocity = T::zero();
            self.running = false;
        }
    }

    pub fn snap_to(&mut self, value: T) {
        self.position = value.clone();
        self.target = value;
        self.velocity = T::zero();
        self.accumulator = 0.0;
        self.running = false;
        self.last_update_time = None;
    }

    pub fn set_velocity(&mut self, velocity: T) {
        self.velocity = velocity;
    }

    pub fn value(&self) -> T {
        self.position.clone()
    }

    pub fn velocity(&self) -> T {
        self.velocity.clone()
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn target(&self) -> T {
        self.target.clone()
    }
}

// 为 Spring 实现 SharedAnimation trait
impl<T> Animation<T> for Spring<T>
where
    T: Springable + AnimatableValue
{
    /// 自动更新（根据实际经过的时间）
    fn update(&mut self) -> Option<T> {
        let now = std::time::Instant::now();

        let dt = if let Some(last_time) = self.last_update_time {
            now.duration_since(last_time).as_secs_f32()
        } else {
            // 第一次调用，使用固定步长
            self.fixed_dt
        };

        self.last_update_time = Some(now);

        if self.running {
            Some(self.update_with_dt(dt))
        } else {
            None
        }
    }

    fn check_finished(&mut self) -> bool {
        !self.running
    }

    fn animate_to(&mut self, target: T) {
        self.target = target;
        self.running = true;
        // 重置时间以避免大的时间跳跃
        self.last_update_time = None;
    }

    fn stop(&mut self) {
        self.running = false;
        self.velocity = T::zero();
        self.last_update_time = None;
    }
}

