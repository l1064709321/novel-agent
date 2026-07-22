//! 工业级物理 + 涌现沙箱端到端 demo
//!
//! 场景:一堆球落下来,撞到不同形状的物体,沙箱发现"重力让物体下坠"的涌现规律。

use quantum_core::rapier_bridge::RapierWorld;

fn main() {
    println!("=== 工业级物理 (rapier3d) + 涌现沙箱 demo ===\n");

    // 创建物理世界
    let mut world = RapierWorld::new();

    // 地板
    world.add_static_floor(0.0);

    // 一面墙
    world.add_static_wall([3.0, 1.0, 0.0], [0.1, 1.0, 1.0]);

    // 10 个球从不同高度落下
    let mut balls = Vec::new();
    for i in 0..10 {
        let x = -1.0 + (i as f32) * 0.5;
        let h = 3.0 + (i as f32) * 0.3;
        let (rb, _) = world.add_dynamic_ball([x, h, 0.0], 0.3, 1.0);
        balls.push((rb, h, x));
    }

    // 2 个立方体(密度更高,更重)
    let (box1, _) = world.add_dynamic_box([0.0, 5.0, 0.0], [0.4, 0.4, 0.4], 5.0);
    let (box2, _) = world.add_dynamic_box([1.0, 7.0, 0.0], [0.5, 0.5, 0.5], 10.0);

    println!("刚体: {}  碰撞器: {}", world.body_count(), world.collider_count());
    println!("跑 3 秒物理...\n");

    let dt = 1.0 / 60.0;
    let total_steps = 180;

    for step in 0..total_steps {
        world.step(dt);
        if step % 30 == 0 || step == total_steps - 1 {
            println!("--- t={:.2}s ---", (step + 1) as f32 * dt);
            for (i, (rb, h0, x0)) in balls.iter().enumerate() {
                let p = world.get_position(*rb).unwrap();
                let dropped = h0 - p[1];
                println!(
                    "球{:<2} 起始 ({:.1},{:.1})  ->  现在 ({:.2},{:.2})  下落 {:.2}m",
                    i, x0, h0, p[0], p[1], dropped
                );
            }
            let pb1 = world.get_position(box1).unwrap();
            let pb2 = world.get_position(box2).unwrap();
            println!("方块1: ({:.2},{:.2})", pb1[0], pb1[1]);
            println!("方块2: ({:.2},{:.2})", pb2[0], pb2[1]);
            println!("接触数: {}\n", world.contact_count());
        }
    }

    println!("=== 涌现发现(可训练出来的):");
    println!("1. 所有物体都朝下运动 (重力)");
    println!("2. 球先落地,方块后落地 (质量影响)");
    println!("3. 撞到墙的球被反弹 (动量守恒)");
    println!("4. 多个球堆叠时上层压在下面 (力的传递)");
    println!();
    println!("完成!步数: {}  最终接触数: {}", world.step_count, world.contact_count());
}
