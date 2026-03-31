use std::f32::consts::PI;

/// 弹簧动画规格（不可变配置）
#[derive(Debug, Clone, Copy)]
pub struct SpringSpec {
    pub damping_ratio: f32,
    pub stiffness: f32,
    pub visibility_threshold: f32,
    pub mass: f32,
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

    pub const DEFAULT: Self = Self::new(1.0, 1500.0);
    pub const BOUNCY: Self = Self::new(0.5, 1500.0);
    pub const STIFF: Self = Self::new(0.8, 5000.0);
    pub const SOFT: Self = Self::new(0.9, 400.0);

    /// 预计算系统参数（避免重复计算）
    fn compute_coefficients(&self) -> SpringCoefficients {
        let omega_0 = (self.stiffness / self.mass).sqrt(); // 自然频率
        let zeta = self.damping_ratio;

        if zeta < 1.0 - 1e-6 {
            // 欠阻尼：使用解析解
            let omega_d = omega_0 * (1.0 - zeta * zeta).sqrt();
            SpringCoefficients::Underdamped { omega_0, zeta, omega_d }
        } else if zeta > 1.0 + 1e-6 {
            // 过阻尼
            let r1 = -omega_0 * (zeta - (zeta * zeta - 1.0).sqrt());
            let r2 = -omega_0 * (zeta + (zeta * zeta - 1.0).sqrt());
            SpringCoefficients::Overdamped { r1, r2 }
        } else {
            // 临界阻尼
            SpringCoefficients::CriticallyDamped { omega_0 }
        }
    }
}

/// 预计算的系统系数
#[derive(Debug, Clone, Copy)]
enum SpringCoefficients {
    Underdamped { omega_0: f32, zeta: f32, omega_d: f32 },
    CriticallyDamped { omega_0: f32 },
    Overdamped { r1: f32, r2: f32 },
}

/// 弹簧动画状态
pub struct Spring {
    spec: SpringSpec,
    coefficients: SpringCoefficients,

    position: f32,
    velocity: f32,
    target: f32,

    /// 累积的子步长误差（用于固定时间步长）
    accumulator: f32,

    running: bool,
}

impl Spring {
    pub fn new(spec: SpringSpec, initial_value: f32) -> Self {
        let coefficients = spec.compute_coefficients();
        Self {
            spec,
            coefficients,
            position: initial_value,
            velocity: 0.0,
            target: initial_value,
            accumulator: 0.0,
            running: false,
        }
    }

    pub fn animate_to(&mut self, target: f32) {
        self.target = target;
        self.running = true;
    }

    pub fn snap_to(&mut self, value: f32) {
        self.position = value;
        self.target = value;
        self.velocity = 0.0;
        self.accumulator = 0.0;
        self.running = false;
    }

    pub fn set_velocity(&mut self, velocity: f32) {
        self.velocity = velocity;
    }

    /// 更新动画（使用固定时间步长以提高稳定性）
    pub fn update(&mut self, dt: f32) -> f32 {
        if !self.running {
            return self.position;
        }

        // 固定时间步长（60 FPS）
        const FIXED_DT: f32 = 1.0 / 60.0;
        const MAX_STEPS: usize = 10; // 防止死循环

        self.accumulator += dt;
        let mut steps = 0;

        while self.accumulator >= FIXED_DT && steps < MAX_STEPS {
            self.step(FIXED_DT);
            self.accumulator -= FIXED_DT;
            steps += 1;

            // 提前退出检查
            if !self.running {
                self.accumulator = 0.0;
                break;
            }
        }

        self.position
    }

    /// 单步更新（使用解析解或改进的数值方法）
    fn step(&mut self, dt: f32) {
        let displacement = self.position - self.target;

        match self.coefficients {
            SpringCoefficients::Underdamped { omega_0, zeta, omega_d } => {
                // 欠阻尼的解析解
                let exp_term = (-zeta * omega_0 * dt).exp();
                let cos_term = (omega_d * dt).cos();
                let sin_term = (omega_d * dt).sin();

                let c1 = displacement;
                let c2 = (self.velocity + zeta * omega_0 * displacement) / omega_d;

                let new_displacement = exp_term * (c1 * cos_term + c2 * sin_term);
                let new_velocity = exp_term * (
                    (c2 * omega_d - zeta * omega_0 * c1) * cos_term
                        - (c1 * omega_d + zeta * omega_0 * c2) * sin_term
                );

                self.position = self.target + new_displacement;
                self.velocity = new_velocity;
            }

            SpringCoefficients::CriticallyDamped { omega_0 } => {
                // 临界阻尼的解析解
                let exp_term = (-omega_0 * dt).exp();
                let c1 = displacement;
                let c2 = self.velocity + omega_0 * displacement;

                self.position = self.target + exp_term * (c1 + c2 * dt);
                self.velocity = exp_term * (c2 - omega_0 * (c1 + c2 * dt));
            }

            SpringCoefficients::Overdamped { r1, r2 } => {
                // 过阻尼的解析解
                let exp1 = (r1 * dt).exp();
                let exp2 = (r2 * dt).exp();

                let c2 = (self.velocity - r1 * displacement) / (r2 - r1);
                let c1 = displacement - c2;

                self.position = self.target + c1 * exp1 + c2 * exp2;
                self.velocity = c1 * r1 * exp1 + c2 * r2 * exp2;
            }
        }

        // 检查停止条件
        if displacement.abs() < self.spec.visibility_threshold
            && self.velocity.abs() < self.spec.visibility_threshold
        {
            self.position = self.target;
            self.velocity = 0.0;
            self.running = false;
        }
    }

    pub fn value(&self) -> f32 {
        self.position
    }

    pub fn velocity(&self) -> f32 {
        self.velocity
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn target(&self) -> f32 {
        self.target
    }
}

/// 向量弹簧动画（支持多维）
#[derive(Clone)]
pub struct SpringVec<const N: usize> {
    spec: SpringSpec,
    coefficients: SpringCoefficients,

    position: [f32; N],
    velocity: [f32; N],
    target: [f32; N],

    accumulator: f32,
    running: bool,
}

impl<const N: usize> SpringVec<N> {
    pub fn new(spec: SpringSpec, initial_value: [f32; N]) -> Self {
        let coefficients = spec.compute_coefficients();
        Self {
            spec,
            coefficients,
            position: initial_value,
            velocity: [0.0; N],
            target: initial_value,
            accumulator: 0.0,
            running: false,
        }
    }

    pub fn animate_to(&mut self, target: [f32; N]) {
        self.target = target;
        self.running = true;
    }

    pub fn update(&mut self, dt: f32) -> [f32; N] {
        if !self.running {
            return self.position;
        }

        const FIXED_DT: f32 = 1.0 / 60.0;
        const MAX_STEPS: usize = 10;

        self.accumulator += dt;
        let mut steps = 0;

        while self.accumulator >= FIXED_DT && steps < MAX_STEPS {
            self.step(FIXED_DT);
            self.accumulator -= FIXED_DT;
            steps += 1;

            if !self.running {
                self.accumulator = 0.0;
                break;
            }
        }

        self.position
    }

    fn step(&mut self, dt: f32) {
        let mut all_settled = true;

        for i in 0..N {
            let displacement = self.position[i] - self.target[i];

            match self.coefficients {
                SpringCoefficients::Underdamped { omega_0, zeta, omega_d } => {
                    let exp_term = (-zeta * omega_0 * dt).exp();
                    let cos_term = (omega_d * dt).cos();
                    let sin_term = (omega_d * dt).sin();

                    let c1 = displacement;
                    let c2 = (self.velocity[i] + zeta * omega_0 * displacement) / omega_d;

                    let new_displacement = exp_term * (c1 * cos_term + c2 * sin_term);
                    let new_velocity = exp_term * (
                        (c2 * omega_d - zeta * omega_0 * c1) * cos_term
                            - (c1 * omega_d + zeta * omega_0 * c2) * sin_term
                    );

                    self.position[i] = self.target[i] + new_displacement;
                    self.velocity[i] = new_velocity;
                }

                SpringCoefficients::CriticallyDamped { omega_0 } => {
                    let exp_term = (-omega_0 * dt).exp();
                    let c1 = displacement;
                    let c2 = self.velocity[i] + omega_0 * displacement;

                    self.position[i] = self.target[i] + exp_term * (c1 + c2 * dt);
                    self.velocity[i] = exp_term * (c2 - omega_0 * (c1 + c2 * dt));
                }

                SpringCoefficients::Overdamped { r1, r2 } => {
                    let exp1 = (r1 * dt).exp();
                    let exp2 = (r2 * dt).exp();

                    let c2 = (self.velocity[i] - r1 * displacement) / (r2 - r1);
                    let c1 = displacement - c2;

                    self.position[i] = self.target[i] + c1 * exp1 + c2 * exp2;
                    self.velocity[i] = c1 * r1 * exp1 + c2 * r2 * exp2;
                }
            }

            if displacement.abs() >= self.spec.visibility_threshold
                || self.velocity[i].abs() >= self.spec.visibility_threshold
            {
                all_settled = false;
            }
        }

        if all_settled {
            self.position = self.target;
            self.velocity = [0.0; N];
            self.running = false;
        }
    }

    pub fn value(&self) -> [f32; N] {
        self.position
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analytical_solution() {
        let mut spring = Spring::new(SpringSpec::DEFAULT, 0.0);
        spring.animate_to(100.0);

        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            spring.update(dt);
        }

        assert!((spring.value() - 100.0).abs() < 0.1);
    }

    #[test]
    fn test_vector_spring() {
        let mut spring = SpringVec::<3>::new(SpringSpec::DEFAULT, [0.0, 0.0, 0.0]);
        spring.animate_to([100.0, 50.0, 75.0]);

        let dt = 1.0 / 60.0;
        for _ in 0..120 {
            spring.update(dt);
        }

        let final_pos = spring.value();
        assert!((final_pos[0] - 100.0).abs() < 0.1);
        assert!((final_pos[1] - 50.0).abs() < 0.1);
        assert!((final_pos[2] - 75.0).abs() < 0.1);
    }

    #[test]
    fn test_stability_high_stiffness() {
        // 高刚度测试
        let spec = SpringSpec::new(1.0, 100000.0);
        let mut spring = Spring::new(spec, 0.0);
        spring.animate_to(100.0);

        let dt = 1.0 / 60.0;
        for _ in 0..200 {
            let pos = spring.update(dt);
            // 不应该发散
            assert!(pos.abs() < 1000.0, "弹簧发散了！");
        }
    }
}

fn main() {
    println!("=== 解析解弹簧动画 ===");
    let mut spring = Spring::new(SpringSpec::DEFAULT, 0.0);
    spring.animate_to(100.0);

    let dt = 1.0 / 60.0;
    for frame in 0..60 {
        let pos = spring.update(dt);
        if frame % 10 == 0 {
            println!("Frame {}: {:.2}", frame, pos);
        }
    }

    println!("\n=== 3D向量弹簧 ===");
    let mut vec_spring = SpringVec::<3>::new(SpringSpec::BOUNCY, [0.0, 0.0, 0.0]);
    vec_spring.animate_to([100.0, 50.0, 75.0]);

    for frame in 0..60 {
        let pos = vec_spring.update(dt);
        if frame % 10 == 0 {
            println!("Frame {}: [{:.2}, {:.2}, {:.2}]", frame, pos[0], pos[1], pos[2]);
        }
    }
}