//! 群星 A.I. OS - 纯 Rust 最小运行示例
//!
//! 不依赖 Python,只用 Rust 验证 MVP 闭环:
//! 1. 启动伦理铁门
//! 2. 启动八门
//! 3. 启动消息总线
//! 4. 启动物理世界
//! 5. 跑一个简单的物理场景

use quantum_core::bus::{CognitiveMessage, EthicalSignature, MessageBus};
use quantum_core::ethics::{EthicsDynamics, EthicsEvent};
use quantum_core::eight_gates::{EightGates, GateState};
use quantum_core::existential::ExistentialVerifier;
use quantum_core::world::{EntityKind, PhysicsWorldModel, WorldEntity};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("============================================");
    println!("  群星 A.I. OS - Rust MVP 演示");
    println!("============================================\n");

    // 1. 伦理铁门启动
    println!("[1/5] 启动存在性递归验证(模块 7)...");
    let existential = ExistentialVerifier::bootstrap()?;
    println!("     ✓ 元价值锚已锁定: {:?}", existential.anchor());
    println!("     ✓ 锚值 SHA-256: {}\n", existential.anchor_hash_hex());

    // 2. 伦理动力学
    println!("[2/5] 启动伦理动力学(模块 6)...");
    let mut ethics = EthicsDynamics::with_baseline(&existential)?;
    println!("     ✓ 基线状态加载完成\n");

    // 3. 八门状态机
    println!("[3/5] 启动八门状态机(模块 3)...");
    let mut gates = EightGates::open();
    println!("     ✓ 初始状态: {}", gates.current().as_str());
    gates.try_transition(GateState::Create, "进入创造模式")?;
    println!("     ✓ 切换到: {}\n", gates.current().as_str());

    // 4. 消息总线
    println!("[4/5] 启动消息总线(模块 8)...");
    let bus = MessageBus::start()?;
    println!("     ✓ CognitiveMessage 协议就绪");
    println!("     ✓ 总线运行中: {}\n", bus.is_running());

    // 订阅伦理事件
    let (_sub_id, _rx) = bus.subscribe("ethics.event".to_string());

    // 5. 物理世界模型
    println!("[5/5] 启动物理世界模型(模块 18)...");
    let mut world = PhysicsWorldModel::init(&existential)?;
    println!("     ✓ 物理约束层已加载(g = 9.81 m/s²)");
    println!("     ✓ 预测编码器: 13 维状态 → 6 维 latent\n");

    // 添加几个实体
    world.add_entity(WorldEntity {
        id: 1, kind: EntityKind::RigidBox,
        position: [0.0, 5.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 1.0, restitution: 0.5, friction: 0.3,
    })?;
    world.add_entity(WorldEntity {
        id: 2, kind: EntityKind::RigidSphere,
        position: [1.0, 10.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 0.5, restitution: 0.7, friction: 0.2,
    })?;
    println!("     ✓ 添加 2 个实体(方块 + 球)\n");

    // 推 50 步
    println!("=== 物理世界模拟(50 步,dt=0.05s) ===");
    println!("  step | 方块 (x, y, z)         | 球 (x, y, z)            | surprise");
    println!("  -----+-------------------------+-------------------------+----------");
    for i in 0..50 {
        let surprise = world.step(0.05)?;
        if i % 5 == 0 {
            let box_pos = world.state().entities[0].position;
            let sphere_pos = world.state().entities[1].position;
            println!(
                "  {:4} | ({:5.2}, {:5.2}, {:5.2})  | ({:5.2}, {:5.2}, {:5.2})  | {:.4}{}",
                i, box_pos[0], box_pos[1], box_pos[2],
                sphere_pos[0], sphere_pos[1], sphere_pos[2],
                surprise.kl_divergence,
                if surprise.is_surprising { " ⚠️" } else { "" }
            );
        }

        // 推一步伦理
        ethics.step(0.05, EthicsEvent::neutral());
    }

    // 演示消息总线发送一条消息
    println!("\n=== 消息总线演示 ===");
    let sig = EthicalSignature::new("demo", &existential.anchor_hash_hex(), true);
    let msg = CognitiveMessage::new(
        "demo_main",
        "broadcast",
        "ethics.event",
        serde_json::json!({"event": "demo_ok", "ethics_drift": ethics.state().drift(existential.anchor())}),
        sig,
    );
    bus.publish(msg)?;
    println!("     ✓ 消息已发布: ethics.event");
    let stats = bus.stats();
    println!("     ✓ 总线统计: published={}, delivered={}\n", stats.published, stats.delivered);

    // 伦理验证演示
    println!("=== 伦理铁门演示 ===");
    println!("  harm_score=0.3 → {}", if existential.validate_action("good", 0.3) { "通过" } else { "否决" });
    println!("  harm_score=0.5 → {}", if existential.validate_action("gray", 0.5) { "通过" } else { "否决" });
    println!("  harm_score=0.9 → {}", if existential.validate_action("evil", 0.9) { "通过" } else { "否决" });

    println!("\n============================================");
    println!("  MVP 闭环验证完成");
    println!("============================================");

    Ok(())
}
