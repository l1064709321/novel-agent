//! 涌现模块入口
//!
//! 包含:
//! - 涌现沙箱(主类 + 内部教育)
//! - 涌现指标(5 个信号检测)
//! - 假设库(产物接收 + 三关验证)

pub mod sandbox;
pub mod indicators;
pub mod hypothesis;
pub mod concept;
pub mod causal;
pub mod sensor;
pub mod real_physics;

pub use sandbox::{
    EmergenceSandbox, SandboxEthics, AdaptiveTolerance, SelfReward,
    SandboxStepResult, ValidationOutcome,
};
pub use indicators::{EmergenceIndicators, EmergenceSignal, WindowEvent, EmergenceWindow};
pub use hypothesis::{HypothesisBank, EmergentProduct, ProductKind, ValidationResult};
pub use concept::{ConceptDiscoverer, Concept, Sample, extract_features};
pub use causal::{CausalDiscoverer, CausalGraph, CausalNode, CausalEdge, Observation, do_intervention_effect};
pub use sensor::{SensorEncoder, LifWorldBridge};
pub use real_physics::{RealPhysicsWorld, RigidBody};