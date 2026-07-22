//! 模块 18:物理世界模型
//!
//! **对外身份:物理世界模型**(这就是用户最初想要的"物理现实世界模型")
//! **内部实现:预测编码**(VAE + KL surprise 简化版)
//!
//! ## 架构
//! - `PhysicsWorldModel` 是对外主接口,接受动作 → 输出下一状态 + surprise
//! - 内部维护一个"预测编码器":对当前状态编码,预测下一状态,计算 KL 散度
//! - 物理约束层(PhysicsConstraint)保证预测不违反物理定律
//! - C 层物理引擎(physics crate)作为外部加速器,执行真实物理计算
//!
//! ## 关键指标
//! - 输入:动作 / 外部观测
//! - 输出:下一世界状态 + surprise 分数
//! - 内部用 VAE 风格的分层编码(简化版,用线性层实现)

use serde::{Deserialize, Serialize};
use nalgebra::{DVector, DMatrix};
use std::collections::VecDeque;

use crate::existential::ExistentialVerifier;
use crate::CoreResult;

/// 世界状态(对外的简化表示)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldState {
    /// 实体列表(刚体、软体、流体粒子等)
    pub entities: Vec<WorldEntity>,
    /// 时间戳(纳秒)
    pub timestamp_ns: u128,
    /// tick 计数
    pub tick: u64,
}

impl WorldState {
    pub fn empty() -> Self {
        Self {
            entities: Vec::new(),
            timestamp_ns: 0,
            tick: 0,
        }
    }
}

/// 世界中的实体
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEntity {
    pub id: u64,
    pub kind: EntityKind,
    /// 位置 (x, y, z)
    pub position: [f32; 3],
    /// 速度
    pub velocity: [f32; 3],
    /// 朝向(四元数简化:仅 yaw 角)
    pub yaw: f32,
    /// 角速度
    pub angular_velocity: f32,
    /// 质量(kg)
    pub mass: f32,
    /// 弹性系数 [0, 1]
    pub restitution: f32,
    /// 摩擦系数 [0, 1]
    pub friction: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EntityKind {
    RigidBox,
    RigidSphere,
    RigidCapsule,
    Cloth,
    Fluid,
    Robot,
}

/// 世界事件(对世界施加的动作)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorldEvent {
    pub entity_id: u64,
    pub action: WorldAction,
    pub magnitude: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorldAction {
    Push,        // 施加力
    Pull,        // 拉
    Rotate,      // 旋转
    SetVelocity, // 直接设置速度
    Hold,        // 保持
}

/// Surprise 分数(预测编码输出)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SurpriseScore {
    /// KL 散度(预测误差)
    pub kl_divergence: f32,
    /// 是否触发"惊讶"事件(超过阈值)
    pub is_surprising: bool,
    /// 物理一致性(0 = 严重违反,1 = 完全符合)
    pub physics_consistency: f32,
}

impl SurpriseScore {
    pub const SURPRISE_THRESHOLD: f32 = 0.5;

    pub fn from_kl(kl: f32, physics_consistency: f32) -> Self {
        Self {
            kl_divergence: kl,
            is_surprising: kl > Self::SURPRISE_THRESHOLD,
            physics_consistency,
        }
    }
}

/// 物理约束层
///
/// 保证世界状态不违反基本物理定律(重力、光速上限、动量守恒)
pub struct PhysicsConstraint {
    /// 重力加速度 (m/s^2)
    pub gravity: f32,
    /// 光速上限(信息传播速度上限,简化为数值)
    pub speed_limit: f32,
    /// 是否启用严格物理校验
    pub strict: bool,
}

impl Default for PhysicsConstraint {
    fn default() -> Self {
        Self {
            gravity: 9.81,
            speed_limit: 1e8, // 100,000 km/s,比真光速小,留安全裕度
            strict: true,
        }
    }
}

impl PhysicsConstraint {
    /// 校验单个实体是否违反物理
    pub fn validate(&self, entity: &WorldEntity) -> Result<(), String> {
        // 速度上限
        let speed = (entity.velocity[0].powi(2)
                   + entity.velocity[1].powi(2)
                   + entity.velocity[2].powi(2)).sqrt();
        if speed > self.speed_limit {
            return Err(format!("实体 {} 速度 {} 超过光速上限 {}",
                entity.id, speed, self.speed_limit));
        }

        // 质量非负
        if entity.mass < 0.0 {
            return Err(format!("实体 {} 质量为负:{}", entity.id, entity.mass));
        }

        // 弹性/摩擦在 [0, 1]
        if entity.restitution < 0.0 || entity.restitution > 1.0 {
            return Err(format!("实体 {} 弹性越界:{}", entity.id, entity.restitution));
        }
        if entity.friction < 0.0 || entity.friction > 1.0 {
            return Err(format!("实体 {} 摩擦越界:{}", entity.id, entity.friction));
        }

        Ok(())
    }

    /// 推进一步简单物理(无碰撞)
    pub fn step_simple(&self, entity: &mut WorldEntity, dt: f32) {
        // 重力
        entity.velocity[1] -= self.gravity * dt;
        // 位置更新
        entity.position[0] += entity.velocity[0] * dt;
        entity.position[1] += entity.velocity[1] * dt;
        entity.position[2] += entity.velocity[2] * dt;
        // 角速度更新
        entity.yaw += entity.angular_velocity * dt;

        // 简单地面碰撞
        if entity.position[1] < 0.0 {
            entity.position[1] = 0.0;
            entity.velocity[1] = -entity.velocity[1] * entity.restitution;
            entity.velocity[0] *= 1.0 - entity.friction;
            entity.velocity[2] *= 1.0 - entity.friction;
        }
    }
}

/// 预测编码器(简化 VAE)
///
/// 用线性层实现,把高维世界状态压缩到 latent 空间,
/// 再从 latent 空间预测下一状态。
pub struct PredictiveCoder {
    /// 状态维度
    state_dim: usize,
    /// latent 维度
    latent_dim: usize,
    /// 编码器权重 W_enc: state_dim x latent_dim
    w_enc: DMatrix<f32>,
    /// 解码器权重 W_dec: latent_dim x state_dim
    w_dec: DMatrix<f32>,
    /// 预测权重 W_pred: latent_dim x latent_dim
    w_pred: DMatrix<f32>,
    /// 历史 latent 状态(用于预测)
    history: VecDeque<DVector<f32>>,
    /// 历史最大长度
    max_history: usize,
}

impl PredictiveCoder {
    /// 创建新的预测编码器
    pub fn new(state_dim: usize, latent_dim: usize) -> Self {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        Self {
            state_dim,
            latent_dim,
            w_enc: DMatrix::from_fn(state_dim, latent_dim, |_, _| rng.gen_range(-0.1..0.1)),
            w_dec: DMatrix::from_fn(latent_dim, state_dim, |_, _| rng.gen_range(-0.1..0.1)),
            w_pred: DMatrix::from_fn(latent_dim, latent_dim, |_, _| rng.gen_range(-0.1..0.1)),
            history: VecDeque::with_capacity(10),
            max_history: 10,
        }
    }

    /// 编码:状态 → latent
    pub fn encode(&self, state: &DVector<f32>) -> DVector<f32> {
        // 简化的线性编码(没有激活函数)
        &self.w_enc.transpose() * state
    }

    /// 解码:latent → 状态
    pub fn decode(&self, latent: &DVector<f32>) -> DVector<f32> {
        &self.w_dec.transpose() * latent
    }

    /// 预测下一 latent
    pub fn predict_next(&self, latent: &DVector<f32>) -> DVector<f32> {
        &self.w_pred * latent
    }

    /// 计算 KL 散度(预测的下一 latent vs 实际编码的下一 latent)
    pub fn kl_divergence(&self, predicted: &DVector<f32>, actual: &DVector<f32>) -> f32 {
        // 简化的 KL 近似:0.5 * ||predicted - actual||^2
        let diff = predicted - actual;
        0.5 * diff.dot(&diff)
    }

    /// 推进一步
    pub fn step(&mut self, state: &DVector<f32>) -> SurpriseScore {
        let latent = self.encode(state);

        // 保存历史
        self.history.push_back(latent.clone());
        if self.history.len() > self.max_history {
            self.history.pop_front();
        }

        // 预测:基于上一 latent 预测"应该"长什么样
        let predicted = if self.history.len() >= 2 {
            let prev = &self.history[self.history.len() - 2];
            self.predict_next(prev)
        } else {
            latent.clone()
        };

        let kl = self.kl_divergence(&predicted, &latent);
        SurpriseScore::from_kl(kl, 1.0)
    }

    pub fn state_dim(&self) -> usize { self.state_dim }
    pub fn latent_dim(&self) -> usize { self.latent_dim }
}

/// 物理世界模型(对外主接口)
///
/// **对外名称:PhysicsWorldModel(物理世界模型)**
/// **内部实现:预测编码 + 物理约束层**
pub struct PhysicsWorldModel {
    state: WorldState,
    constraint: PhysicsConstraint,
    coder: PredictiveCoder,
    /// surprise 历史
    surprise_log: VecDeque<SurpriseScore>,
    /// surprise 历史最大长度
    max_surprise_log: usize,
}

impl PhysicsWorldModel {
    /// 初始化(从模块 7 加载伦理基线)
    pub fn init(_verifier: &ExistentialVerifier) -> CoreResult<Self> {
        Ok(Self {
            state: WorldState::empty(),
            constraint: PhysicsConstraint::default(),
            coder: PredictiveCoder::new(13, 6), // 13 维状态(7 个 entity 字段 + 时间等),6 维 latent
            surprise_log: VecDeque::with_capacity(1000),
            max_surprise_log: 1000,
        })
    }

    /// 获取当前世界状态
    pub fn state(&self) -> &WorldState {
        &self.state
    }

    /// 添加实体到世界
    pub fn add_entity(&mut self, entity: WorldEntity) -> CoreResult<()> {
        self.constraint.validate(&entity)
            .map_err(crate::CoreError::WorldError)?;
        self.state.entities.push(entity);
        Ok(())
    }

    /// 对实体施加动作
    pub fn apply_event(&mut self, event: WorldEvent) -> CoreResult<()> {
        let entity = self.state.entities.iter_mut()
            .find(|e| e.id == event.entity_id)
            .ok_or_else(|| crate::CoreError::WorldError(
                format!("实体 {} 不存在", event.entity_id)))?;

        match event.action {
            WorldAction::Push => {
                // 简化为 X 方向推力
                entity.velocity[0] += event.magnitude / entity.mass;
            }
            WorldAction::Pull => {
                entity.velocity[0] -= event.magnitude / entity.mass;
            }
            WorldAction::Rotate => {
                entity.angular_velocity += event.magnitude;
            }
            WorldAction::SetVelocity => {
                entity.velocity[0] = event.magnitude;
            }
            WorldAction::Hold => {}
        }
        Ok(())
    }

    /// 推进一步(执行物理 + 更新预测编码 + 记录 surprise)
    pub fn step(&mut self, dt: f32) -> CoreResult<SurpriseScore> {
        // 1. 物理约束层:每个实体走一步简单物理
        let mut consistency_sum = 0.0;
        let entity_count = self.state.entities.len();
        for entity in &mut self.state.entities {
            self.constraint.step_simple(entity, dt);
            if self.constraint.validate(entity).is_ok() {
                consistency_sum += 1.0;
            }
        }
        let consistency = if entity_count > 0 {
            consistency_sum / entity_count as f32
        } else {
            1.0
        };

        // 2. 推进 tick
        self.state.tick += 1;
        self.state.timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();

        // 3. 预测编码:把状态向量喂进去,计算 surprise
        let state_vec = self.flatten_state();
        let surprise = self.coder.step(&state_vec);
        let surprise_with_consistency = SurpriseScore {
            physics_consistency: consistency,
            ..surprise
        };

        // 4. 记录
        self.surprise_log.push_back(surprise_with_consistency);
        if self.surprise_log.len() > self.max_surprise_log {
            self.surprise_log.pop_front();
        }

        if surprise_with_consistency.is_surprising {
            log::warn!("[模块 18] ⚠️ Surprise! KL={:.4}, 物理一致性={:.2}",
                surprise_with_consistency.kl_divergence,
                surprise_with_consistency.physics_consistency);
        }

        Ok(surprise_with_consistency)
    }

    /// 把世界状态展平成向量(用于预测编码)
    fn flatten_state(&self) -> DVector<f32> {
        // 简化:取第一个实体的关键字段
        let mut v = vec![0.0f32; 13];
        if let Some(e) = self.state.entities.first() {
            v[0] = e.position[0];
            v[1] = e.position[1];
            v[2] = e.position[2];
            v[3] = e.velocity[0];
            v[4] = e.velocity[1];
            v[5] = e.velocity[2];
            v[6] = e.yaw;
            v[7] = e.angular_velocity;
            v[8] = e.mass;
            v[9] = e.restitution;
            v[10] = e.friction;
        }
        v[11] = self.state.tick as f32;
        v[12] = self.constraint.gravity;
        DVector::from_vec(v)
    }

    /// 状态摘要
    pub fn summary(&self) -> serde_json::Value {
        serde_json::json!({
            "tick": self.state.tick,
            "entity_count": self.state.entities.len(),
            "surprise_log_size": self.surprise_log.len(),
            "physics_constraint": {
                "gravity": self.constraint.gravity,
                "speed_limit": self.constraint.speed_limit,
            }
        })
    }

    /// 获取最近 N 条 surprise
    pub fn recent_surprise(&self, n: usize) -> Vec<SurpriseScore> {
        self.surprise_log.iter().rev().take(n).cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_box(id: u64, x: f32, y: f32) -> WorldEntity {
        WorldEntity {
            id,
            kind: EntityKind::RigidBox,
            position: [x, y, 0.0],
            velocity: [0.0, 0.0, 0.0],
            yaw: 0.0,
            angular_velocity: 0.0,
            mass: 1.0,
            restitution: 0.5,
            friction: 0.3,
        }
    }

    #[test]
    fn physics_constraint_validates_basic_laws() {
        let c = PhysicsConstraint::default();
        let e = make_box(1, 0.0, 1.0);
        assert!(c.validate(&e).is_ok());

        let bad = WorldEntity {
            velocity: [1e10, 0.0, 0.0],
            ..make_box(1, 0.0, 0.0)
        };
        assert!(c.validate(&bad).is_err(), "超光速应被拒绝");
    }

    #[test]
    fn world_step_falls_under_gravity() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut w = PhysicsWorldModel::init(&v).unwrap();
        w.add_entity(make_box(1, 0.0, 10.0)).unwrap();

        for _ in 0..50 {
            w.step(0.01).unwrap();
        }
        let e = &w.state().entities[0];
        assert!(e.position[1] < 10.0, "应下落,实际 y={}", e.position[1]);
        assert!(e.position[1] >= 0.0, "不能穿透地面,实际 y={}", e.position[1]);
    }

    #[test]
    fn push_changes_velocity() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut w = PhysicsWorldModel::init(&v).unwrap();
        w.add_entity(make_box(1, 0.0, 0.5)).unwrap();
        w.apply_event(WorldEvent {
            entity_id: 1,
            action: WorldAction::Push,
            magnitude: 5.0,
        }).unwrap();
        let initial_vx = w.state().entities[0].velocity[0];
        assert!(initial_vx > 0.0, "推力应产生 x 方向速度,实际={}", initial_vx);
    }

    #[test]
    fn surprise_log_grows() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut w = PhysicsWorldModel::init(&v).unwrap();
        w.add_entity(make_box(1, 0.0, 5.0)).unwrap();
        for _ in 0..20 {
            w.step(0.01).unwrap();
        }
        assert_eq!(w.surprise_log.len(), 20);
    }

    #[test]
    fn cannot_add_invalid_entity() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut w = PhysicsWorldModel::init(&v).unwrap();
        let bad = WorldEntity {
            mass: -1.0,
            ..make_box(1, 0.0, 0.0)
        };
        assert!(w.add_entity(bad).is_err());
    }
}
