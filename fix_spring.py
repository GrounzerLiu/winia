#!/usr/bin/env python3
import re

with open('winia/src/animation.rs', 'r', encoding='utf-8') as f:
    content = f.read()

# Replace the spring displacement function
old = '''fn compute_spring_displacement(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    dt: Duration, threshold: f32,
) -> f32 {
    let dt = dt.as_secs_f32().min(1.0 / 30.0);
    let omega0 = (stiffness / mass).sqrt();
    let damping_coeff = damping_ratio * 2.0 * omega0 * mass;
    let force = -stiffness * initial_displacement - damping_coeff * *velocity;
    *velocity += force / mass * dt;
    let displacement = initial_displacement + *velocity * dt;
    if displacement.abs() < threshold && velocity.abs() < threshold {
        *velocity = 0.0;
        return 0.0;
    }
    displacement
}'''

new = '''fn compute_spring_displacement(
    stiffness: f32, damping_ratio: f32, mass: f32,
    initial_displacement: f32, velocity: &mut f32,
    dt: Duration, threshold: f32,
) -> f32 {
    const FIXED_DT: f32 = 1.0 / 60.0;
    const MAX_STEPS: u32 = 10;
    let mut total_dt = dt.as_secs_f32().min(FIXED_DT * MAX_STEPS as f32);
    let mut displacement = initial_displacement;
    while total_dt > 0.0 {
        let step = total_dt.min(FIXED_DT);
        let omega0 = (stiffness / mass).sqrt();
        let damping_coeff = damping_ratio * 2.0 * omega0 * mass;
        let force = -stiffness * displacement - damping_coeff * *velocity;
        *velocity += force / mass * step;
        displacement += *velocity * step;
        total_dt -= step;
    }
    if displacement.abs() < threshold && velocity.abs() < threshold {
        *velocity = 0.0;
        return 0.0;
    }
    displacement
}'''

if old in content:
    content = content.replace(old, new)
    with open('winia/src/animation.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print('spring function replaced successfully')
else:
    print('old function NOT FOUND in file')
    idx = content.find('fn compute_spring_displacement')
    print(f'Found at index {idx}')
    print(content[idx:idx+500])
