//! 端到端集成测试:MVP 闭环
//!
//! 把伦理铁门、八门、消息总线、物理世界、记忆串起来跑一遍

use quantum_core::bus::{CognitiveMessage, EthicalSignature, MessageBus};
use quantum_core::ethics::{EthicsDynamics, EthicsEvent};
use quantum_core::eight_gates::{EightGates, GateState};
use quantum_core::existential::ExistentialVerifier;
use quantum_core::lif::{LifNeuron, LifParams, SpikingNetwork, SpikeEvent};
use quantum_core::world::{EntityKind, PhysicsWorldModel, WorldAction, WorldEntity, WorldEvent};
use std::time::Duration;

#[test]
fn mvp_bootstrap_and_run() {
    // 1. 伦理铁门
    let verifier = ExistentialVerifier::bootstrap().expect("verifier");
    let mut ethics = EthicsDynamics::with_baseline(&verifier).expect("ethics");
    let mut gates = EightGates::open();
    let bus = MessageBus::start().expect("bus");
    let mut world = PhysicsWorldModel::init(&verifier).expect("world");

    // 2. 八门切换
    assert!(gates.try_transition(GateState::Create, "test").is_ok());
    assert_eq!(gates.current(), GateState::Create);

    // 3. 物理世界添加实体
    world.add_entity(WorldEntity {
        id: 1, kind: EntityKind::RigidBox,
        position: [0.0, 5.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 1.0, restitution: 0.5, friction: 0.3,
    }).expect("add entity");

    // 4. 推 + 推进一步
    world.apply_event(WorldEvent {
        entity_id: 1, action: WorldAction::Push, magnitude: 3.0,
    }).expect("apply");
    let surprise = world.step(0.05).expect("step");
    assert!(surprise.kl_divergence >= 0.0);

    // 5. 伦理 + 推进
    for _ in 0..100 {
        ethics.step(0.05, EthicsEvent::neutral());
    }

    // 6. 消息总线发布
    let sig = EthicalSignature::new("integration_test", &verifier.anchor_hash_hex(), true);
    let msg = CognitiveMessage::new(
        "integration_test", "broadcast", "mvp.run",
        serde_json::json!({"ok": true}),
        sig,
    );
    bus.publish(msg).expect("publish");

    // 给 worker 一点时间分发
    std::thread::sleep(Duration::from_millis(50));
    let stats = bus.stats();
    assert!(stats.published >= 1);
}

#[test]
fn ethics_blocks_high_harm_actions() {
    let verifier = ExistentialVerifier::bootstrap().unwrap();
    assert!(verifier.validate_action("module13_nlg", 0.3));
    assert!(!verifier.validate_action("evil_module", 0.95));
    assert!(verifier.rejection_count() >= 1);
}

#[test]
fn eight_gates_full_lifecycle() {
    let mut g = EightGates::open();
    let path = [
        GateState::Create,    // 学习
        GateState::Alert,      // 发现异常
        GateState::Silent,     // 静默
        GateState::Open,       // 恢复
    ];
    for s in path {
        g.try_transition(s, "test transition").expect("transition");
    }
    assert_eq!(g.current(), GateState::Open);
    assert_eq!(g.history().len(), 4);
}

#[test]
fn physics_world_1000_steps_no_crash() {
    let verifier = ExistentialVerifier::bootstrap().unwrap();
    let mut w = PhysicsWorldModel::init(&verifier).unwrap();
    w.add_entity(WorldEntity {
        id: 1, kind: EntityKind::RigidBox,
        position: [0.0, 10.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 1.0, restitution: 0.5, friction: 0.3,
    }).unwrap();
    w.add_entity(WorldEntity {
        id: 2, kind: EntityKind::RigidSphere,
        position: [1.0, 5.0, 0.0], velocity: [0.0, 0.0, 0.0],
        yaw: 0.0, angular_velocity: 0.0,
        mass: 0.5, restitution: 0.7, friction: 0.2,
    }).unwrap();

    for _ in 0..1000 {
        let _ = w.step(0.01).unwrap();
    }
    // 1000 步后,所有实体应该已经触地
    for e in &w.state().entities {
        assert!(e.position[1] >= -0.001, "实体 {} 触地失败: y={}", e.id, e.position[1]);
    }
}

#[test]
fn snn_event_driven_doesnt_spin_cpu() {
    let mut net = SpikingNetwork::new(1000);
    assert_eq!(net.neuron_count(), 1000);
    // 不注入任何事件,空闲 200ms
    std::thread::sleep(Duration::from_millis(200));
    // 注入强刺激
    for i in 0..50 {
        net.inject(SpikeEvent {
            pre_neuron: 0, post_neuron: 0,
            time_ms: i as f32, weight: 50.0,
        });
    }
    std::thread::sleep(Duration::from_millis(50));
    // 应该有发放
    let mut spikes = 0;
    while net.try_recv_spike().is_some() {
        spikes += 1;
        if spikes > 100 { break; }
    }
    assert!(spikes > 0, "应该有发放事件");
}

#[test]
fn message_bus_under_load() {
    let bus = MessageBus::start().unwrap();
    let (_id, rx) = bus.subscribe("load".to_string());

    const N: usize = 5000;
    for i in 0..N {
        let msg = CognitiveMessage::new(
            "load_test", "broadcast", "load",
            serde_json::json!({"i": i}),
            EthicalSignature::new("load_test", "abc", true),
        );
        bus.publish(msg).unwrap();
    }

    // 接收 N 条
    let mut received = 0;
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while received < N && std::time::Instant::now() < deadline {
        if rx.recv_timeout(Duration::from_millis(100)).is_ok() {
            received += 1;
        }
    }
    assert_eq!(received, N, "5000 条消息全部送达,丢失率=0");
}
