//! 真物理世界模型端到端 demo
//!
//! 展示 4 个真东西:
//! A. 真物理后端(rapier3d)
//! B. 流体(粒子系统 + 粘性)
//! C. 关节(单摆 + 双摆 + 机械臂)
//! D. 接触力可视化

use quantum_core::true_world_model::{Preset, TruePhysicsWorld};

fn main() {
    println!("=== 真物理世界模型 demo ===\n");

    // 1. 单摆
    println!("--- 1. 单摆(应该周期性摆动)---");
    let mut w = Preset::build(Preset::Pendulum);
    let initial_pos = w.joints.values().next().and_then(|j| w.backend.get_position(j.body2));
    println!("t=0.0s 摆锤位置: {:?}", initial_pos);
    for t in 1..=3 {
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        let pos = w.joints.values().next().and_then(|j| w.backend.get_position(j.body2));
        println!("t={:.1}s 摆锤位置: {:?}", t as f32, pos);
    }
    println!("接触数: {}", w.contact_count());
    println!();

    // 2. 双摆(混沌)
    println!("--- 2. 双摆(混沌,运动范围应该很大)---");
    let mut w = Preset::build(Preset::DoublePendulum);
    if let Some(j) = w.joints.values().next() {
        w.backend.apply_impulse(j.body2, [2.0, 0.0, 0.0]);
    }
    let mut xs = Vec::new();
    for _ in 0..300 {
        w.step(1.0 / 60.0);
        if let Some(j) = w.joints.values().nth(1) {
            if let Some(p) = w.backend.get_position(j.body2) {
                xs.push(p[0]);
            }
        }
    }
    let max = xs.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
    let min = xs.iter().cloned().fold(f32::INFINITY, f32::min);
    println!("下摆 x 范围: [{:.3}, {:.3}], 跨度 {:.3}", min, max, max - min);
    println!();

    // 3. 3 自由度机械臂
    println!("--- 3. 3 自由度机械臂(应该不散架)---");
    let mut w = Preset::build(Preset::RoboticArm3Dof);
    let stats_before = w.stats();
    for _ in 0..60 {
        w.step(1.0 / 60.0);
    }
    let stats_after = w.stats();
    println!("之前: 关节={} 接触={} 能量={:.2}",
        stats_before.joint_count,
        stats_before.contact_count,
        stats_before.total_kinetic_energy + stats_before.total_potential_energy);
    println!("之后: 关节={} 接触={} 能量={:.2}",
        stats_after.joint_count,
        stats_after.contact_count,
        stats_after.total_kinetic_energy + stats_after.total_potential_energy);
    // 检查所有 body 没飞出
    for j in w.joints.values() {
        for h in [j.body1, j.body2] {
            if let Some(p) = w.backend.get_position(h) {
                let ok = p[1] >= -1.0 && p[1] <= 15.0;
                println!("  关节 body @ ({:.2}, {:.2}, {:.2}) - {}", p[0], p[1], p[2],
                    if ok { "OK" } else { "飞了!" });
            }
        }
    }
    println!();

    // 4. 水流入杯子
    println!("--- 4. 水流入杯子(粒子系统 + 接触)---");
    let mut w = Preset::build(Preset::WaterIntoCup);
    for t in 0..5 {
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        let stats = w.stats();
        let fluid_stats = w.fluids.first().map(|c| c.statistics()).unwrap();
        println!("t={:.1}s 粒子={} 接触={} 水均位={:.2} 总KE={:.3}",
            t as f32 * 0.5 + 0.5,
            stats.fluid_particle_count,
            stats.contact_count,
            fluid_stats.mean_position[1],
            fluid_stats.total_kinetic_energy);
    }
    println!();

    // 5. 综合统计
    println!("--- 5. 综合统计 ---");
    let stats = w.stats();
    println!("刚体数: {}", stats.body_count);
    println!("关节数: {}", stats.joint_count);
    println!("流体粒子数: {}", stats.fluid_particle_count);
    println!("接触数: {}", stats.contact_count);
    println!("总动能: {:.3} J", stats.total_kinetic_energy);
    println!("总势能: {:.3} J", stats.total_potential_energy);
    println!("总接触冲量: {:.3} N·s", stats.total_contact_impulse);
    println!();

    println!("=== 完成 ===");
    println!();
    println!("真物理世界模型 4 大能力:");
    println!("  A. 真物理后端(rapier3d) ✓");
    println!("  B. 流体粒子(SPH 简化 + 粘性) ✓");
    println!("  C. 关节(单摆/双摆/机械臂) ✓");
    println!("  D. 接触力(法向+切向+冲量) ✓");
}
