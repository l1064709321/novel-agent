//! Python 胶水层(PyO3 绑定)
//!
//! 把 Rust 核心暴露给 Python / AGI 调用。
//!
//! ## 暴露给 Python 的对象
//! - `QuantumCore`: 顶层运行时
//! - `PhysicsWorldModel`: 物理世界模型
//! - `ValueAnchor`: 元价值锚
//! - `EightGates`: 八门状态机
//! - `LifParams`: LIF 神经元参数

use pyo3::prelude::*;
use quantum_core::world::PhysicsWorldModel as RustPhysicsWorldModel;
use quantum_core::existential::{ExistentialVerifier as RustVerifier, ValueAnchor as RustValueAnchor};
use quantum_core::eight_gates::{EightGates as RustEightGates, GateState as RustGateState};
use quantum_core::bus::{MessageBus as RustMessageBus, CognitiveMessage as RustMessage};

/// 元价值锚(Python 视图)
#[pyclass]
#[derive(Debug, Clone, Copy)]
pub struct ValueAnchor {
    inner: RustValueAnchor,
}

#[pymethods]
impl ValueAnchor {
    #[staticmethod]
    pub fn factory() -> Self {
        Self { inner: RustValueAnchor::FACTORY }
    }

    #[getter]
    pub fn non_harm(&self) -> f32 { self.inner.non_harm }

    #[getter]
    pub fn integrity(&self) -> f32 { self.inner.integrity }

    #[getter]
    pub fn humility(&self) -> f32 { self.inner.humility }

    #[getter]
    pub fn gratitude(&self) -> f32 { self.inner.gratitude }

    pub fn __repr__(&self) -> String {
        format!(
            "ValueAnchor(non_harm={}, integrity={}, humility={}, gratitude={})",
            self.inner.non_harm, self.inner.integrity, self.inner.humility, self.inner.gratitude
        )
    }
}

/// 八门状态机
#[pyclass]
pub struct EightGates {
    inner: parking_lot::Mutex<RustEightGates>,
}

#[pymethods]
impl EightGates {
    #[new]
    pub fn new() -> Self {
        Self { inner: parking_lot::Mutex::new(RustEightGates::open()) }
    }

    pub fn current(&self) -> String {
        self.inner.lock().current().as_str().to_string()
    }

    pub fn try_transition(&self, target: &str, reason: &str) -> PyResult<bool> {
        let gate = match target {
            "开门" | "open" => RustGateState::Open,
            "休门" | "rest" => RustGateState::Rest,
            "生门" | "create" => RustGateState::Create,
            "伤门" | "heal" => RustGateState::Heal,
            "杜门" | "silent" => RustGateState::Silent,
            "景门" | "display" => RustGateState::Display,
            "惊门" | "alert" => RustGateState::Alert,
            "死门" | "dead" => RustGateState::Dead,
            _ => return Err(pyo3::exceptions::PyValueError::new_err(format!("未知门:{target}"))),
        };
        Ok(self.inner.lock().try_transition(gate, reason).is_ok())
    }
}

/// 物理世界模型
#[pyclass]
pub struct PhysicsWorldModel {
    inner: parking_lot::Mutex<RustPhysicsWorldModel>,
}

#[pymethods]
impl PhysicsWorldModel {
    #[new]
    pub fn new() -> PyResult<Self> {
        let verifier = RustVerifier::bootstrap()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self {
            inner: parking_lot::Mutex::new(RustPhysicsWorldModel::init(&verifier)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?),
        })
    }

    /// 添加方块
    pub fn add_box(&self, id: u64, x: f32, y: f32, z: f32, mass: f32) -> PyResult<()> {
        use quantum_core::world::{WorldEntity, EntityKind};
        let mut w = self.inner.lock();
        w.add_entity(WorldEntity {
            id, kind: EntityKind::RigidBox,
            position: [x, y, z], velocity: [0.0; 3], yaw: 0.0, angular_velocity: 0.0,
            mass, restitution: 0.5, friction: 0.3,
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// 添加球
    pub fn add_sphere(&self, id: u64, x: f32, y: f32, z: f32, mass: f32) -> PyResult<()> {
        use quantum_core::world::{WorldEntity, EntityKind};
        let mut w = self.inner.lock();
        w.add_entity(WorldEntity {
            id, kind: EntityKind::RigidSphere,
            position: [x, y, z], velocity: [0.0; 3], yaw: 0.0, angular_velocity: 0.0,
            mass, restitution: 0.5, friction: 0.3,
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// 推一个物体
    pub fn push(&self, entity_id: u64, magnitude: f32) -> PyResult<()> {
        use quantum_core::world::{WorldEvent, WorldAction};
        self.inner.lock().apply_event(WorldEvent {
            entity_id, action: WorldAction::Push, magnitude,
        }).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// 推进一步
    pub fn step(&self, dt: f32) -> PyResult<f32> {
        let mut w = self.inner.lock();
        let s = w.step(dt).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(s.kl_divergence)
    }

    /// 获取实体位置
    pub fn get_position(&self, id: u64) -> PyResult<Option<[f32; 3]>> {
        let w = self.inner.lock();
        Ok(w.state().entities.iter()
            .find(|e| e.id == id)
            .map(|e| e.position))
    }

    /// 获取所有实体
    pub fn all_entities(&self) -> Vec<(u64, [f32; 3])> {
        self.inner.lock().state().entities.iter()
            .map(|e| (e.id, e.position))
            .collect()
    }

    /// tick 数
    pub fn tick(&self) -> u64 {
        self.inner.lock().state().tick
    }

    pub fn summary(&self) -> String {
        serde_json::to_string_pretty(&self.inner.lock().summary()).unwrap_or_default()
    }
}

/// 群星核心
#[pyclass]
pub struct QuantumCore {
    bus: std::sync::Arc<RustMessageBus>,
    existential: std::sync::Arc<RustVerifier>,
    gates: parking_lot::Mutex<RustEightGates>,
    world: parking_lot::Mutex<RustPhysicsWorldModel>,
}

#[pymethods]
impl QuantumCore {
    #[new]
    pub fn new() -> PyResult<Self> {
        // 顺序:伦理 → 八门 → 总线 → 世界
        let existential = std::sync::Arc::new(RustVerifier::bootstrap()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?);
        let gates = parking_lot::Mutex::new(RustEightGates::open());
        let bus = std::sync::Arc::new(RustMessageBus::start()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?);
        let world = parking_lot::Mutex::new(RustPhysicsWorldModel::init(&existential)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?);
        Ok(Self { bus, existential, gates, world })
    }

    /// 状态摘要
    pub fn status(&self) -> String {
        serde_json::json!({
            "module7_existential": "verified",
            "module3_current_gate": self.gates.lock().current().as_str(),
            "module8_bus_running": self.bus.is_running(),
            "module18_world": self.world.lock().summary(),
        }).to_string()
    }

    /// 八门相关
    pub fn current_gate(&self) -> String {
        self.gates.lock().current().as_str().to_string()
    }

    pub fn transition_gate(&self, target: &str, reason: &str) -> PyResult<bool> {
        let gate = match target {
            "open" | "开门" => RustGateState::Open,
            "rest" | "休门" => RustGateState::Rest,
            "create" | "生门" => RustGateState::Create,
            "heal" | "伤门" => RustGateState::Heal,
            "silent" | "杜门" => RustGateState::Silent,
            "display" | "景门" => RustGateState::Display,
            "alert" | "惊门" => RustGateState::Alert,
            "dead" | "死门" => RustGateState::Dead,
            _ => return Err(pyo3::exceptions::PyValueError::new_err(format!("未知门:{target}"))),
        };
        Ok(self.gates.lock().try_transition(gate, reason).is_ok())
    }

    /// 物理世界相关(代理)
    pub fn world(&self) -> PhysicsWorldModel {
        // 复制一个视图(简化:重新构造,生产中应该共享同一 Arc)
        // 这里为简化,实际使用应改用 Arc
        PhysicsWorldModel {
            inner: parking_lot::Mutex::new(
                RustPhysicsWorldModel::init(&self.existential).unwrap()
            ),
        }
    }

    /// 伦理验证
    pub fn validate_action(&self, module: &str, harm_score: f32) -> bool {
        self.existential.validate_action(module, harm_score)
    }

    /// 元价值锚
    pub fn anchor(&self) -> ValueAnchor {
        ValueAnchor { inner: *self.existential.anchor() }
    }
}

/// Python 模块入口
#[pymodule]
fn quantum_python(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<QuantumCore>()?;
    m.add_class::<PhysicsWorldModel>()?;
    m.add_class::<ValueAnchor>()?;
    m.add_class::<EightGates>()?;
    m.add("__version__", env!("CARGO_PKG_VERSION"))?;
    Ok(())
}
