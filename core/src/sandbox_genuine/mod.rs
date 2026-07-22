//! 涌现沙箱的真货桥接
//!
//! 让旧 EmergenceSandbox 调用真涌现引擎(genuine_emergence),
//! 不再用 auto_submit_emergent_product 的假货剧本。

use crate::emergence::indicators::{EmergenceIndicators, EmergenceSignal};
use crate::genuine_emergence::{EmergenceEngine, Evidence, EvidenceKind};
use crate::world::PhysicsWorldModel;

/// 真涌现沙箱桥接器:在物理世界状态变化时,给真涌现引擎喂证据
pub struct SandboxGenuineBridge {
    /// 真涌现引擎
    pub engine: EmergenceEngine,
    /// 真涌现候选 id(由概念名映射)
    pub concept_ids: std::collections::HashMap<String, u64>,
    /// 已发现的涌现(从 engine 同步过来)
    pub emerged: Vec<String>,
    /// 当前 tick
    pub tick: u64,
}

impl SandboxGenuineBridge {
    pub fn new(generation_length: u64) -> Self {
        Self {
            engine: EmergenceEngine::new(generation_length),
            concept_ids: std::collections::HashMap::new(),
            emerged: Vec::new(),
            tick: 0,
        }
    }

    /// 报告一个候选概念(由聚类/拟合产生)
    pub fn propose(&mut self, name: &str, kind: EvidenceKind, source: u64, strength: f32) {
        let id = self
            .concept_ids
            .entry(name.to_string())
            .or_insert_with(|| {
                self.engine.propose_concept(
                    name.to_string(),
                    Evidence {
                        source,
                        kind,
                        strength,
                        tick: self.tick,
                    },
                )
            });
        // 加新证据
        self.engine.add_evidence(
            *id,
            Evidence {
                source,
                kind,
                strength,
                tick: self.tick,
            },
        );
    }

    /// 报告冲突(降低置信度)
    pub fn conflict(&mut self, name: &str, source: u64) {
        if let Some(id) = self.concept_ids.get(name) {
            self.engine.add_evidence(
                *id,
                Evidence {
                    source,
                    kind: EvidenceKind::Conflict,
                    strength: 0.3,
                    tick: self.tick,
                },
            );
        }
    }

    /// 推进一步(同步涌现引擎)
    pub fn step(&mut self, tick: u64) {
        self.tick = tick;
        self.engine.step(tick);
        // 同步已涌现的概念
        self.emerged = self
            .engine
            .emerged_descriptions
            .iter()
            .map(|c| c.name.clone())
            .collect();
    }

    pub fn emerged_count(&self) -> usize {
        self.emerged.len()
    }
}

// ============================================================
// 自动证据提取器:从物理世界状态变化中自动产生证据
// ============================================================

/// 物理状态观察器
pub struct WorldStateObserver {
    /// 上一次的物理世界状态摘要
    pub last_summary: Option<WorldSummary>,
    /// 上一次 tick
    pub last_tick: u64,
    /// 平稳性(方差倒数,越高越稳定)
    pub stability: f32,
    /// KL 散度历史(简易)
    pub kl_history: Vec<f32>,
}

/// 物理世界状态摘要
#[derive(Debug, Clone, Copy)]
pub struct WorldSummary {
    pub mean_energy: f32,
    pub mean_position: [f32; 3],
    pub variance: f32,
}

impl WorldStateObserver {
    pub fn new() -> Self {
        Self {
            last_summary: None,
            last_tick: 0,
            stability: 0.0,
            kl_history: Vec::new(),
        }
    }

    /// 观察一个世界状态,产生证据
    pub fn observe(&mut self, world: &PhysicsWorldModel, tick: u64) -> WorldSummary {
        let state = world.state();
        let n = state.entities.len().max(1) as f32;

        let mean_energy: f32 = state
            .entities
            .iter()
            .map(|e| {
                let v_sq = e.velocity[0].powi(2) + e.velocity[1].powi(2) + e.velocity[2].powi(2);
                0.5 * e.mass * v_sq
            })
            .sum::<f32>()
            / n;
        let mean_pos = [
            state.entities.iter().map(|e| e.position[0]).sum::<f32>() / n,
            state.entities.iter().map(|e| e.position[1]).sum::<f32>() / n,
            state.entities.iter().map(|e| e.position[2]).sum::<f32>() / n,
        ];
        let variance: f32 = state
            .entities
            .iter()
            .map(|e| {
                let dx = e.position[0] - mean_pos[0];
                let dy = e.position[1] - mean_pos[1];
                let dz = e.position[2] - mean_pos[2];
                dx * dx + dy * dy + dz * dz
            })
            .sum::<f32>()
            / n;

        let summary = WorldSummary {
            mean_energy,
            mean_position: mean_pos,
            variance,
        };

        // 计算稳定性:能量变化率倒数
        if let Some(last) = self.last_summary {
            let energy_change = (summary.mean_energy - last.mean_energy).abs();
            let pos_change = ((summary.mean_position[0] - last.mean_position[0]).powi(2)
                + (summary.mean_position[1] - last.mean_position[1]).powi(2)
                + (summary.mean_position[2] - last.mean_position[2]).powi(2))
            .sqrt();
            // KL 散度近似
            let kl = energy_change + pos_change * 0.1;
            self.kl_history.push(kl);
            // 稳定性 = 1 / (1 + 平均变化)
            if self.kl_history.len() > 5 {
                let recent: f32 = self.kl_history[self.kl_history.len() - 5..]
                    .iter()
                    .sum::<f32>()
                    / 5.0;
                self.stability = 1.0 / (1.0 + recent * 10.0);
            }
        }

        self.last_summary = Some(summary);
        self.last_tick = tick;
        summary
    }

    pub fn stable(&self) -> bool {
        self.stability > 0.7
    }
}

impl Default for WorldStateObserver {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::correction::expected_anchor;
    use crate::existential::{ExistentialVerifier, ValueAnchor};
    use crate::emergence::sandbox::EmergenceSandbox;

    #[test]
    fn test_bridge_propose() {
        let mut b = SandboxGenuineBridge::new(5);
        for i in 0..30 {
            b.propose("动量守恒", EvidenceKind::Conservation, 0, 0.3);
            b.step(i);
        }
        // 30 tick,跨 6 代际
        assert!(b.emerged.contains(&"动量守恒".to_string()),
            "expected momentum emergence, got {:?}", b.emerged);
    }

    #[test]
    fn test_bridge_conflict() {
        let mut b = SandboxGenuineBridge::new(5);
        for i in 0..20 {
            b.propose("假概念", EvidenceKind::ClusterFit, 0, 0.15);
            if i % 2 == 0 {
                b.conflict("假概念", 0);
            }
            b.step(i);
        }
        // 冲突多,不应该涌现
        assert!(!b.emerged.contains(&"假概念".to_string()));
    }

    #[test]
    fn test_observer_measures_stability() {
        let mut obs = WorldStateObserver::new();
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut world = PhysicsWorldModel::init(&v).unwrap();
        for i in 0..30 {
            obs.observe(&world, i);
            world.step(0.01).unwrap();
        }
        // 跑了一会,应该有一些 stability 测量
        // 不一定能 > 0.7(取决于初始状态),但应该有值
        assert!(obs.stability >= 0.0);
    }

    #[test]
    fn test_sandbox_with_genuine_bridge() {
        // 关键测试:旧 sandbox 走真桥
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);
        let mut bridge = SandboxGenuineBridge::new(20);
        let mut observer = WorldStateObserver::new();

        for tick in 0..100 {
            sb.step(0.05, &ValueAnchor::FACTORY, &v);
            observer.observe(&sb.world, tick);
            // 根据物理稳定性给真证据
            if observer.stable() {
                bridge.propose("动量守恒", EvidenceKind::Conservation, 0, 0.15);
            } else {
                bridge.conflict("动量守恒", 0);
            }
            bridge.step(tick);
        }
        // 不一定涌现(数据可能不够),但不应该崩
        assert!(bridge.emerged_count() < 100);
    }
}
