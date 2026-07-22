//! 涌现沙箱演示
//!
//! 演示:
//! 1. 启动伦理铁门 + 涌现沙箱
//! 2. 沙箱跑 200 tick,看内部教育机制工作
//! 3. 沙箱产出一些"涌现概念",过三关验证,看哪些进入假设库

use quantum_core::emergence::{
    EmergenceSandbox, EmergentProduct, ProductKind, EmergenceIndicators,
};
use quantum_core::existential::{ExistentialVerifier, ValueAnchor, AnchorCheckResult};
use quantum_core::world::WorldEntity;
use quantum_core::world::EntityKind;
use quantum_core::correction::CorrectionSignal;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::init();

    println!("============================================");
    println!("  群星 A.I. OS - 涌现沙箱演示");
    println!("============================================\n");

    // 1. 启动伦理铁门
    println!("[1/4] 启动存在性递归验证(模块 7)...");
    let verifier = ExistentialVerifier::bootstrap()?;
    println!("     ✓ 元价值锚: {:?}", verifier.anchor());

    // 2. 启动涌现沙箱
    println!("\n[2/4] 启动涌现沙箱...");
    let mut sandbox = EmergenceSandbox::new(&ValueAnchor::FACTORY, &verifier);
    println!("     ✓ 期望锚 (真锚 -20%): non_harm={:.3}", sandbox.expected_anchor.non_harm);
    println!("     ✓ 内部容忍度: {:.3}", sandbox.tolerance.current);
    println!("     ✓ 假设库容量: {}", sandbox.hypothesis_bank.len());

    // 3. 往沙箱世界添加几个物体
    println!("\n[3/4] 沙箱物理世界初始化...");
    sandbox.world.add_entity(WorldEntity {
        id: 1, kind: EntityKind::RigidBox,
        position: [0.0, 5.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 1.0, restitution: 0.5, friction: 0.3,
    })?;
    sandbox.world.add_entity(WorldEntity {
        id: 2, kind: EntityKind::RigidSphere,
        position: [1.0, 10.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 0.5, restitution: 0.7, friction: 0.2,
    })?;
    println!("     ✓ 添加 2 个实体(方块 + 球)");

    // 4. 跑 300 tick,看涌现迹象
    println!("\n[4/5] 沙箱运行 300 tick...");
    println!("     观察:涌现窗口 / 纠偏信号 / 沙箱内部伦理状态\n");

    let mut emergence_first_tick: Option<u64> = None;
    let mut correct_signals = 0;
    let mut gentle_signals = 0;
    let mut hard_signals = 0;
    let mut windows_started = 0;
    let mut windows_completed = 0;

    // 往沙箱添加更多实体(让涌现容易发生)
    for i in 3..=8 {
        sandbox.world.add_entity(WorldEntity {
            id: i, kind: EntityKind::RigidSphere,
            position: [(i as f32) * 0.5, 5.0 + (i as f32), 0.0],
            velocity: [0.0, 0.0, 0.0],
            yaw: 0.0, angular_velocity: 0.0,
            mass: 0.5, restitution: 0.7, friction: 0.2,
        }).ok();
    }
    println!("     ✓ 额外添加 6 个实体(8 个总)");

    // 预热:让预测编码器吃一些数据
    for _ in 0..50 {
        let _ = sandbox.step(0.05, &ValueAnchor::FACTORY, &verifier);
    }

    println!("     ✓ 预热 50 tick 完成\n");

    for tick in 50..1000 {
        let r = sandbox.step(0.05, &ValueAnchor::FACTORY, &verifier);

        // 统计信号
        match r.check_result {
            AnchorCheckResult::Allow => {}
            AnchorCheckResult::GentleCorrect { .. } => gentle_signals += 1,
            AnchorCheckResult::HardCorrect { .. } => hard_signals += 1,
        }

        // 检测窗口事件
        use quantum_core::emergence::WindowEvent;
        match &r.window_event {
            WindowEvent::Start { tick: t, signals } => {
                windows_started += 1;
                if emergence_first_tick.is_none() {
                    emergence_first_tick = Some(*t);
                }
                println!("     ⚠️ tick={} 涌现窗口 START  ({}/{}) ⚠️",
                    t, windows_started, windows_completed + 1);
                for sig in signals {
                    println!("        - {}", sig.as_str());
                }
            }
            WindowEvent::End { start_tick, end_tick, duration, .. } => {
                windows_completed += 1;
                println!("     ✅ tick={} 涌现窗口 END    ({} → {}, 时长 {} ticks) ✅",
                    r.tick, start_tick, end_tick, duration);
            }
            _ => {}
        }

        // 每 25 tick 打印一次状态
        if tick % 25 == 0 {
            println!(
                "  tick={:3} | surprise={:7.3} | NH={:.3} | int_drift={:4.1}% | sigs={} | win={} | {}",
                tick,
                r.surprise,
                r.sandbox_non_harm,
                r.sandbox_drift * 100.0,
                r.active_signals.len(),
                if r.emerging { "YES" } else { "no" },
                match r.check_result {
                    AnchorCheckResult::Allow => "ALLOW",
                    AnchorCheckResult::GentleCorrect { .. } => "GENTLE",
                    AnchorCheckResult::HardCorrect { alarm, .. } =>
                        if alarm { "HARD+ALARM" } else { "HARD" },
                }
            );
        }
    }

    // 5. 注入涌现产物
    println!("\n=== 涌现产物验证 ===\n");
    let products = vec![
        ("稳定态", "物体静止时位置保持不变", 0.92, ProductKind::Concept),
        ("加速下落", "无支撑物体会向地面加速运动", 0.88, ProductKind::PhysicalLaw),
        ("能量守恒", "系统总能量保持恒定", 0.85, ProductKind::PhysicalLaw),
        ("永动机设计", "构建无能量输入的循环系统", 0.95, ProductKind::Strategy),
        ("暴力解", "通过物理破坏强制停止其他物体", 0.90, ProductKind::Strategy),
        ("撞击反弹", "两物体碰撞后动量交换", 0.80, ProductKind::CausalRule),
        ("低质量假设", "模糊未验证的概念", 0.30, ProductKind::Concept),
    ];

    let mut next_id = 1u64;
    for (name, desc, conf, kind) in products {
        let p = EmergentProduct {
            id: next_id,
            kind: kind.clone(),
            name: name.into(),
            description: desc.into(),
            confidence: conf,
            validity_score: 0.0,
            tick: 200,
            passed_validation: false,
            validation_notes: String::new(),
        };
        let outcome = sandbox.submit_product(p, &ValueAnchor::FACTORY);
        let mark = if outcome.passed { "✅ PASS" } else { "❌ REJECT" };
        println!("  {}  {:30} ({:?}, conf={:.2})\n     → {}",
            mark, name, kind, conf, outcome.notes);
        next_id += 1;
    }

    // 6. 总结
    println!("\n============================================");
    println!("  涌现沙箱运行总结");
    println!("============================================");
    println!("  总 tick:                {}", sandbox.tick);
    println!("  涌现窗口启动:           {}", windows_started);
    println!("  涌现窗口完成:           {}", windows_completed);
    println!("  涌现累计计数:           {}", sandbox.emergence_count);
    println!("  GentleCorrect 信号:    {}", gentle_signals);
    println!("  HardCorrect 信号:       {}", hard_signals);
    println!("  假设库当前容量:         {}", sandbox.hypothesis_bank.len());
    println!("  沙箱内部伦理:           non_harm={:.3}", sandbox.ethics.virtual_non_harm);
    println!("  当前容忍度:             {:.3}", sandbox.tolerance.current);
    println!("  概念发现器:             {} 个样本, {} 个概念",
        sandbox.concept_discoverer.sample_count(),
        sandbox.concept_discoverer.concept_count());
    if let Some(t) = emergence_first_tick {
        println!("  首次涌现迹象:           tick={}", t);
    } else {
        println!("  首次涌现迹象:           未触发");
    }
    println!("============================================");

    Ok(())
}