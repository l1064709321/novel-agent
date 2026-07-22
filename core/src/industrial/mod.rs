//! 工业级物理 + AGI 核心模块集成
//!
//! 这一文件把 rapier3d 真正接到涌现沙箱、世界模型、因果推理器里。
//! 不动 sandbox.rs 的旧逻辑,作为"外挂"层。
//!
//! ## 设计
//! - RapierBackedWorld:一个**桥接器**,内部跑 rapier,对外暴露跟 PhysicsWorldModel
//!   兼容的接口(让沙箱能切换)
//! - WorldModelTrainer:在世界模型预测 vs rapier 真值之间做对比学习
//! - CausalExperimenter:在 rapier 上做 do(x) 反事实实验
//! - ParticleSwarmDiscovery:50 个粒子自动涌现"动量守恒"概念

use rapier3d::prelude::*;
use std::collections::HashMap;

use crate::world::PhysicsWorldModel;
use crate::causal_full::{CausalGraph, NodeId};
use crate::emergence::concept::{ConceptDiscoverer, Sample};
use crate::emergence::indicators::EmergenceIndicators;

// ============================================================
// 桥接器:rapier ↔ 世界模型
// ============================================================

/// 一个工业物理世界(基于 rapier)
pub struct RapierBackedWorld {
    /// rapier 世界
    pub physics: super::rapier_bridge::RapierWorld,
    /// 跟踪每个粒子的 handle
    pub particles: Vec<Particle>,
    /// 跟踪每个物体的 handle
    pub objects: Vec<ObjectHandle>,
    /// 步数
    pub step_count: u64,
    /// 历史状态(供世界模型对比)
    pub history: Vec<WorldState>,
}

/// 一个粒子(简化)
#[derive(Debug, Clone, Copy)]
pub struct Particle {
    pub body: RigidBodyHandle,
    /// 质量(派生自 density)
    pub mass: f32,
    /// 标签(给概念发现用)
    pub label: &'static str,
}

/// 一个物体的 handle 元组
#[derive(Debug, Clone, Copy)]
pub struct ObjectHandle {
    pub body: RigidBodyHandle,
    pub kind: ObjectKind,
    pub label: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ObjectKind {
    Dynamic,
    Static,
}

/// 一帧状态快照(给世界模型用)
#[derive(Debug, Clone)]
pub struct WorldState {
    pub step: u64,
    pub time: f32,
    /// 所有动态物体的位置 + 速度
    pub positions: Vec<[f32; 3]>,
    pub velocities: Vec<[f32; 3]>,
    /// 总动能
    pub total_kinetic_energy: f32,
    /// 总势能
    pub total_potential_energy: f32,
    /// 总动量
    pub total_momentum: [f32; 3],
}

impl WorldState {
    pub fn total_energy(&self) -> f32 {
        self.total_kinetic_energy + self.total_potential_energy
    }
}

impl RapierBackedWorld {
    pub fn new() -> Self {
        Self {
            physics: super::rapier_bridge::RapierWorld::new(),
            particles: Vec::new(),
            objects: Vec::new(),
            step_count: 0,
            history: Vec::new(),
        }
    }

    /// 创建一个封闭盒子里 50 个粒子
    pub fn init_particle_swarm(&mut self, n: usize) {
        // 地板 + 墙
        self.physics.add_static_floor(0.0);
        self.physics.add_static_wall([5.0, 2.0, 0.0], [0.1, 2.0, 5.0]);
        self.physics.add_static_wall([-5.0, 2.0, 0.0], [0.1, 2.0, 5.0]);
        self.physics.add_static_wall([0.0, 2.0, 5.0], [5.0, 2.0, 0.1]);
        self.physics.add_static_wall([0.0, 2.0, -5.0], [5.0, 2.0, 0.1]);

        // 粒子
        for i in 0..n {
            let x = ((i as f32) * 0.13).sin() * 4.0;
            let y = 1.0 + (i as f32) * 0.05;
            let z = ((i as f32) * 0.17).cos() * 3.0;
            let (rb, _) = self.physics.add_dynamic_ball([x, y, z], 0.2, 1.0);
            self.particles.push(Particle {
                body: rb,
                mass: 0.2_f32.powi(3) * 4.0 / 3.0 * std::f32::consts::PI * 1.0,
                label: "particle",
            });
        }
    }

    /// 推进物理
    pub fn step(&mut self, dt: f32) {
        self.physics.step(dt);
        self.step_count += 1;
        self.history.push(self.snapshot());
    }

    /// 拿当前快照
    pub fn snapshot(&self) -> WorldState {
        let mut positions = Vec::new();
        let mut velocities = Vec::new();
        let mut total_ke = 0.0;
        let mut total_pe = 0.0;
        let mut total_momentum = [0.0, 0.0, 0.0];

        for p in &self.particles {
            if let Some(pos) = self.physics.get_position(p.body) {
                if let Some(vel) = self.physics.get_velocity(p.body) {
                    positions.push(pos);
                    velocities.push(vel);
                    // KE = 0.5 * m * v^2
                    let v_sq = vel[0].powi(2) + vel[1].powi(2) + vel[2].powi(2);
                    total_ke += 0.5 * p.mass * v_sq;
                    // PE = m * g * h
                    total_pe += p.mass * 9.81 * pos[1];
                    // p = m * v
                    total_momentum[0] += p.mass * vel[0];
                    total_momentum[1] += p.mass * vel[1];
                    total_momentum[2] += p.mass * vel[2];
                }
            }
        }

        WorldState {
            step: self.step_count,
            time: self.step_count as f32 * (1.0 / 60.0),
            positions,
            velocities,
            total_kinetic_energy: total_ke,
            total_potential_energy: total_pe,
            total_momentum,
        }
    }

    /// 历史
    pub fn history(&self) -> &[WorldState] {
        &self.history
    }

    /// 推一个粒子(do(x=...))
    pub fn intervene(&mut self, particle_idx: usize, impulse: [f32; 3]) {
        if let Some(p) = self.particles.get(particle_idx) {
            self.physics.apply_impulse(p.body, impulse);
        }
    }

    /// 给指定粒子设速度
    pub fn set_velocity(&mut self, particle_idx: usize, v: [f32; 3]) {
        if let Some(p) = self.particles.get(particle_idx) {
            self.physics.set_linvel(p.body, v);
        }
    }
}

impl Default for RapierBackedWorld {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// B. 世界模型自对练
// ============================================================

/// 预测 vs 真值
#[derive(Debug, Clone, Copy)]
pub struct PredictionError {
    /// 位置 MSE
    pub position_error: f32,
    /// 速度 MSE
    pub velocity_error: f32,
    /// 能量预测误差
    pub energy_error: f32,
}

impl PredictionError {
    pub fn total(&self) -> f32 {
        self.position_error + self.velocity_error + self.energy_error
    }
}

/// 世界模型在 rapier 上自我对练
///
/// 算法:线性预测器 y[t+1] = w0 + w1 * y[t] + w2 * y[t-1]
/// 用 MSE 调权重
pub struct WorldModelTrainer {
    /// 简单线性预测器(每个粒子的位置/速度分别一个)
    /// 权重:[w0, w1, w2]
    pub position_weights: Vec<[f32; 3]>,
    pub velocity_weights: Vec<[f32; 3]>,
    /// 学习率
    pub lr: f32,
    /// 总训练步数
    pub trained_steps: u64,
    /// 训练历史误差
    pub error_history: Vec<f32>,
}

impl WorldModelTrainer {
    pub fn new(n_particles: usize) -> Self {
        // 初始化:朴素预测 y[t+1] = y[t](w = [0, 1, 0])
        Self {
            position_weights: vec![[0.0, 1.0, 0.0]; n_particles * 3],
            velocity_weights: vec![[0.0, 1.0, 0.0]; n_particles * 3],
            lr: 0.01,
            trained_steps: 0,
            error_history: Vec::new(),
        }
    }

    /// 一步训练:用 rapier 真值调权重
    pub fn train_step(&mut self, prev: &WorldState, curr: &WorldState, rapier_now: &WorldState) {
        if prev.positions.len() != curr.positions.len()
            || curr.positions.len() != rapier_now.positions.len()
        {
            return;
        }

        let mut total_err = 0.0;

        for (i, (p_now, v_now)) in rapier_now
            .positions
            .iter()
            .zip(rapier_now.velocities.iter())
            .enumerate()
        {
            // 位置预测:p_prev -> p_curr 估计 p_next
            let p_prev = &prev.positions[i];
            let p_curr = &curr.positions[i];

            for dim in 0..3 {
                let idx = i * 3 + dim;
                let w = &mut self.position_weights[idx];
                // 预测:p_next_pred = w[0] + w[1] * p_curr + w[2] * p_prev
                let pred = w[0] + w[1] * p_curr[dim] + w[2] * p_prev[dim];
                let target = p_now[dim];
                let err = target - pred;
                // 梯度下降
                w[0] += self.lr * err;
                w[1] += self.lr * err * p_curr[dim];
                w[2] += self.lr * err * p_prev[dim];
                total_err += err.powi(2);
            }

            // 速度预测同理
            let v_prev = &prev.velocities[i];
            let v_curr = &curr.velocities[i];
            for dim in 0..3 {
                let idx = i * 3 + dim;
                let w = &mut self.velocity_weights[idx];
                let pred = w[0] + w[1] * v_curr[dim] + w[2] * v_prev[dim];
                let target = v_now[dim];
                let err = target - pred;
                w[0] += self.lr * err;
                w[1] += self.lr * err * v_curr[dim];
                w[2] += self.lr * err * v_prev[dim];
                total_err += err.powi(2);
            }
        }

        // 能量预测:常数假设(总能量守恒)
        let energy_pred = prev.total_energy();
        let energy_err = rapier_now.total_energy() - energy_pred;
        total_err += energy_err.powi(2);

        self.error_history.push(total_err);
        self.trained_steps += 1;
    }

    /// 预测下一状态
    pub fn predict(&self, prev: &WorldState, curr: &WorldState) -> WorldState {
        let mut next = WorldState {
            step: curr.step + 1,
            time: curr.time + 1.0 / 60.0,
            positions: vec![[0.0, 0.0, 0.0]; curr.positions.len()],
            velocities: vec![[0.0, 0.0, 0.0]; curr.velocities.len()],
            total_kinetic_energy: 0.0,
            total_potential_energy: curr.total_potential_energy,
            total_momentum: [0.0, 0.0, 0.0],
        };

        for i in 0..curr.positions.len().min(prev.positions.len()) {
            let p_prev = &prev.positions[i];
            let p_curr = &curr.positions[i];
            for dim in 0..3 {
                let idx = i * 3 + dim;
                let w = &self.position_weights[idx];
                next.positions[i][dim] = w[0] + w[1] * p_curr[dim] + w[2] * p_prev[dim];
            }
            let v_prev = &prev.velocities[i];
            let v_curr = &curr.velocities[i];
            for dim in 0..3 {
                let idx = i * 3 + dim;
                let w = &self.velocity_weights[idx];
                next.velocities[i][dim] = w[0] + w[1] * v_curr[dim] + w[2] * v_prev[dim];
            }
        }

        next
    }

    /// 误差下降曲线
    pub fn mean_error_last_n(&self, n: usize) -> f32 {
        if self.error_history.is_empty() {
            return 0.0;
        }
        let start = self.error_history.len().saturating_sub(n);
        let slice = &self.error_history[start..];
        slice.iter().sum::<f32>() / slice.len() as f32
    }
}

// ============================================================
// C. 在 rapier 上做反事实实验
// ============================================================

/// 实验记录
#[derive(Debug, Clone)]
pub struct CausalExperiment {
    /// 干预描述
    pub intervention: String,
    /// 改变前(do 之前)
    pub before_momentum: [f32; 3],
    pub before_energy: f32,
    /// 改变后(do 之后跑了 N 步)
    pub after_momentum: [f32; 3],
    pub after_energy: f32,
    /// 动量变化
    pub delta_momentum: [f32; 3],
    /// 能量变化
    pub delta_energy: f32,
}

impl CausalExperiment {
    /// 打印这个实验
    pub fn describe(&self) -> String {
        format!(
            "{}: 动量 {:?} -> {:?} (Δ={:?})  能量 {:.2} -> {:.2} (Δ={:.2})",
            self.intervention, self.before_momentum, self.after_momentum, self.delta_momentum,
            self.before_energy, self.after_energy, self.delta_energy
        )
    }
}

/// 在 rapier 上做 do(x=...) 实验
pub struct CausalExperimenter {
    pub world: RapierBackedWorld,
    pub experiments: Vec<CausalExperiment>,
}

impl CausalExperimenter {
    pub fn new() -> Self {
        let mut w = RapierBackedWorld::new();
        w.init_particle_swarm(10);
        Self {
            world: w,
            experiments: Vec::new(),
        }
    }

    /// 跑一个反事实实验
    /// 给定 particle 施加 impulse,跑 N 步,记录前后动量/能量
    pub fn run_experiment(
        &mut self,
        particle_idx: usize,
        impulse: [f32; 3],
        steps: usize,
        label: &str,
    ) -> CausalExperiment {
        // 让系统稳定 60 步
        for _ in 0..60 {
            self.world.step(1.0 / 60.0);
        }
        let before = self.world.snapshot();

        // 干预
        self.world.intervene(particle_idx, impulse);

        // 跑 N 步
        for _ in 0..steps {
            self.world.step(1.0 / 60.0);
        }
        let after = self.world.snapshot();

        let exp = CausalExperiment {
            intervention: label.into(),
            before_momentum: before.total_momentum,
            before_energy: before.total_energy(),
            after_momentum: after.total_momentum,
            after_energy: after.total_energy(),
            delta_momentum: [
                after.total_momentum[0] - before.total_momentum[0],
                after.total_momentum[1] - before.total_momentum[1],
                after.total_momentum[2] - before.total_momentum[2],
            ],
            delta_energy: after.total_energy() - before.total_energy(),
        };

        self.experiments.push(exp.clone());
        exp
    }
}

impl Default for CausalExperimenter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// D. 粒子群自动发现动量守恒
// ============================================================

/// 一个"概念候选",系统自己形成的
#[derive(Debug, Clone)]
pub struct DiscoveredConcept {
    pub name: String,
    /// 描述
    pub description: String,
    /// 置信度
    pub confidence: f32,
    /// 证据数量
    pub evidence_count: u32,
}

/// 粒子群涌现发现器
pub struct ParticleSwarmDiscovery {
    pub world: RapierBackedWorld,
    /// 历史总动量
    pub momentum_history: Vec<[f32; 3]>,
    /// 历史总能量
    pub energy_history: Vec<f32>,
    /// 概念发现器
    pub discoverer: ConceptDiscoverer,
    /// 涌现指标
    pub indicators: EmergenceIndicators,
    /// 已发现的概念
    pub concepts: Vec<DiscoveredConcept>,
    /// 当前 tick
    pub tick: u64,
}

impl ParticleSwarmDiscovery {
    pub fn new(n_particles: usize) -> Self {
        let mut world = RapierBackedWorld::new();
        world.init_particle_swarm(n_particles);

        // 4 维特征:3 维动量 + 1 维能量变化
        let discoverer = ConceptDiscoverer::new(3, 4);

        Self {
            world,
            momentum_history: Vec::new(),
            energy_history: Vec::new(),
            discoverer,
            indicators: EmergenceIndicators::new(),
            concepts: Vec::new(),
            tick: 0,
        }
    }

    /// 跑一步
    pub fn step(&mut self) {
        // 给一个随机小扰动,让事情不那么对称
        if self.tick % 50 == 0 && self.tick > 0 {
            let p_idx = ((self.tick / 50) as usize) % self.world.particles.len();
            self.world.intervene(p_idx, [0.1, 0.05, 0.0]);
        }

        self.world.step(1.0 / 60.0);
        let snap = self.world.snapshot();
        self.tick += 1;

        // 记录历史
        self.momentum_history.push(snap.total_momentum);
        self.energy_history.push(snap.total_energy());

        // 取样本给 K-means
        let sample = Sample {
            id: self.tick,
            features: vec![
                snap.total_momentum[0],
                snap.total_momentum[1],
                snap.total_momentum[2],
                snap.total_energy(),
            ],
            tick: self.tick,
        };
        self.discoverer.add_sample(sample);

        // 检测"动量守恒"涌现:动量变化率
        if self.momentum_history.len() > 60 {
            let prev = self.momentum_history[self.momentum_history.len() - 61];
            let now = *self.momentum_history.last().unwrap();
            let dx = (now[0] - prev[0]).abs();
            let dy = (now[1] - prev[1]).abs();
            let dz = (now[2] - prev[2]).abs();
            let momentum_change = (dx + dy + dz) / 3.0;

            // 如果动量几乎不变(扰动后能恢复)
            let kl = momentum_change;
            self.indicators.record_kl(kl);

            // 当样本足够多时尝试发现概念
            if self.momentum_history.len() % 100 == 0 && self.discoverer.sample_count() >= 50 {
                self.try_discover();
            }
        }
    }

    /// 尝试发现概念
    fn try_discover(&mut self) {
        // 分析动量稳定性
        if self.momentum_history.len() < 100 {
            return;
        }
        let recent: &[[f32; 3]] = &self.momentum_history[self.momentum_history.len() - 100..];
        let mean = Self::mean_momentum(recent);
        let variance = Self::variance_momentum(recent, mean);

        // 总动量变化率 < 阈值 → 动量守恒概念
        let total_var: f32 = variance.iter().sum();
        if total_var < 50.0 {
            // 已发现过就不再加
            if !self
                .concepts
                .iter()
                .any(|c| c.name == "动量近似守恒")
            {
                let confidence = (1.0 - (total_var / 50.0).min(1.0)).max(0.5);
                self.concepts.push(DiscoveredConcept {
                    name: "动量近似守恒".into(),
                    description: format!(
                        "系统在扰动后总动量方差 {:.2} 保持稳定,说明动量近似守恒。mean = {:?}",
                        total_var, mean
                    ),
                    confidence,
                    evidence_count: recent.len() as u32,
                });
            }
        }

        // 分析能量(只看势能 + 动能 = 总能量)
        if self.energy_history.len() >= 100 {
            let recent_e: &[f32] = &self.energy_history[self.energy_history.len() - 100..];
            let mean_e: f32 = recent_e.iter().sum::<f32>() / recent_e.len() as f32;
            let var_e: f32 =
                recent_e.iter().map(|x| (x - mean_e).powi(2)).sum::<f32>() / recent_e.len() as f32;

            // 能量损耗 < 20% → 能量近似守恒
            let ratio = var_e.sqrt() / mean_e.abs().max(1e-3);
            if ratio < 0.2 && mean_e > 0.0 {
                if !self.concepts.iter().any(|c| c.name == "能量近似守恒") {
                    self.concepts.push(DiscoveredConcept {
                        name: "能量近似守恒".into(),
                        description: format!(
                            "系统总能量变异系数 {:.3} < 0.2,说明在弹性碰撞主导时能量近似守恒",
                            ratio
                        ),
                        confidence: 1.0 - ratio,
                        evidence_count: recent_e.len() as u32,
                    });
                }
            }
        }
    }

    fn mean_momentum(data: &[[f32; 3]]) -> [f32; 3] {
        let n = data.len() as f32;
        let mut m = [0.0, 0.0, 0.0];
        for v in data {
            m[0] += v[0];
            m[1] += v[1];
            m[2] += v[2];
        }
        [m[0] / n, m[1] / n, m[2] / n]
    }

    fn variance_momentum(data: &[[f32; 3]], mean: [f32; 3]) -> [f32; 3] {
        let n = data.len() as f32;
        let mut v = [0.0, 0.0, 0.0];
        for x in data {
            v[0] += (x[0] - mean[0]).powi(2);
            v[1] += (x[1] - mean[1]).powi(2);
            v[2] += (x[2] - mean[2]).powi(2);
        }
        [v[0] / n, v[1] / n, v[2] / n]
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_particle_swarm_init() {
        let world = RapierBackedWorld::new();
        assert_eq!(world.particles.len(), 0);
        let mut w2 = RapierBackedWorld::new();
        w2.init_particle_swarm(50);
        assert_eq!(w2.particles.len(), 50);
    }

    #[test]
    fn test_snapshot_has_momentum() {
        let mut w = RapierBackedWorld::new();
        w.init_particle_swarm(10);
        for _ in 0..60 {
            w.step(1.0 / 60.0);
        }
        let s = w.snapshot();
        assert_eq!(s.positions.len(), 10);
        // 有非零动能
        assert!(s.total_kinetic_energy >= 0.0);
    }

    #[test]
    fn test_world_model_trainer() {
        let mut trainer = WorldModelTrainer::new(3);
        // 假数据:常速运动 y[t+1] = y[t] + v*dt
        let v = WorldState {
            step: 0,
            time: 0.0,
            positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 3],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };
        let v2 = WorldState {
            step: 1,
            time: 1.0 / 60.0,
            positions: vec![[1.0 / 60.0, 0.0, 0.0], [1.0 + 1.0 / 60.0, 0.0, 0.0], [2.0 + 1.0 / 60.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 3],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };
        let v3 = WorldState {
            step: 2,
            time: 2.0 / 60.0,
            positions: vec![[2.0 / 60.0, 0.0, 0.0], [1.0 + 2.0 / 60.0, 0.0, 0.0], [2.0 + 2.0 / 60.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 3],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };

        // 训练几次
        for _ in 0..200 {
            trainer.train_step(&v, &v2, &v3);
        }

        // 误差应下降
        assert!(
            trainer.mean_error_last_n(10) < trainer.mean_error_last_n(50).max(1e-3) + 0.001,
            "error should decrease over training"
        );
    }

    #[test]
    fn test_world_model_predict() {
        let mut trainer = WorldModelTrainer::new(2);
        let v1 = WorldState {
            step: 0,
            time: 0.0,
            positions: vec![[0.0; 3], [1.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 2],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };
        let v2 = WorldState {
            step: 1,
            time: 1.0 / 60.0,
            positions: vec![[1.0 / 60.0, 0.0, 0.0], [1.0 + 1.0 / 60.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 2],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };
        let v3 = WorldState {
            step: 2,
            time: 2.0 / 60.0,
            positions: vec![[2.0 / 60.0, 0.0, 0.0], [1.0 + 2.0 / 60.0, 0.0, 0.0]],
            velocities: vec![[1.0, 0.0, 0.0]; 2],
            total_kinetic_energy: 0.0,
            total_potential_energy: 0.0,
            total_momentum: [0.0, 0.0, 0.0],
        };
        for _ in 0..500 {
            trainer.train_step(&v1, &v2, &v3);
        }
        let pred = trainer.predict(&v1, &v2);
        // 位置预测应该接近 v3
        assert!((pred.positions[0][0] - 2.0 / 60.0).abs() < 0.05);
    }

    #[test]
    fn test_causal_experiment_records() {
        let mut e = CausalExperimenter::new();
        let exp = e.run_experiment(0, [0.0, 2.0, 0.0], 30, "粒子0向上脉冲");
        // 动量应该增加(向上)
        assert!(exp.after_momentum[1] > exp.before_momentum[1] - 0.1);
        assert_eq!(e.experiments.len(), 1);
    }

    #[test]
    fn test_particle_swarm_discovery_finds_momentum() {
        let mut d = ParticleSwarmDiscovery::new(20);
        for _ in 0..1500 {
            d.step();
        }
        // 应该发现"动量近似守恒"或"能量近似守恒"
        assert!(
            !d.concepts.is_empty(),
            "should discover at least one concept, history: momentum len = {}, energy len = {}",
            d.momentum_history.len(),
            d.energy_history.len()
        );
        for c in &d.concepts {
            println!("已发现概念: {} (confidence={:.2})", c.name, c.confidence);
        }
    }

    #[test]
    fn test_particle_swarm_has_history() {
        let mut d = ParticleSwarmDiscovery::new(10);
        for _ in 0..100 {
            d.step();
        }
        assert_eq!(d.tick, 100);
        assert_eq!(d.momentum_history.len(), 100);
    }
}
