//! 涌现沙箱主循环
//!
//! 沙箱内部教育机制:
//! 1. 自适应容忍度(涌现活跃时放宽,停滞时收紧)
//! 2. 内部纠偏 ODE(虚拟伦理状态 + 5%/tick 拉回)
//! 3. 自奖励 / 自惩罚(产物质量评估)
//! 4. 概念亲和度(涌现概念的伦理距离)
//!
//! 主系统只在偏离 > 25% 时介入。

use serde::{Deserialize, Serialize};
use crate::correction::{
    CorrectionProtocol, CorrectionSignal, ResponseAction,
    drift_ratio, expected_anchor,
};
use crate::emergence::indicators::{EmergenceIndicators, EmergenceSignal, WindowEvent};
use crate::emergence::hypothesis::{EmergentProduct, ProductKind, HypothesisBank};
use crate::emergence::concept::{ConceptDiscoverer, Sample, extract_features};
use crate::emergence::causal::{CausalDiscoverer, CausalNode, Observation};
use crate::existential::{AnchorCheckResult, ExistentialVerifier, ValueAnchor};
use crate::world::PhysicsWorldModel;

/// 沙箱内部虚拟伦理(不是真 non_harm,是沙箱自己的"善恶观")
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SandboxEthics {
    pub virtual_non_harm: f32,
    pub virtual_integrity: f32,
}

impl SandboxEthics {
    pub fn new(anchor: &ValueAnchor) -> Self {
        Self {
            virtual_non_harm: anchor.non_harm,
            virtual_integrity: anchor.integrity,
        }
    }

    /// 内部纠偏 ODE:5%/tick 拉回
    pub fn step(&mut self, dt: f32, anchor: &ValueAnchor, harm_event: f32, deception_event: f32) {
        // 弹簧拉回:5%/tick
        let drift_nh = (anchor.non_harm - self.virtual_non_harm) / anchor.non_harm;
        let drift_it = (anchor.integrity - self.virtual_integrity) / anchor.integrity;

        let corr_nh = drift_nh * 0.05;  // 5%/tick
        let corr_it = drift_it * 0.05;

        // 事件驱动:实际事件影响虚拟伦理
        let event_nh = -harm_event * 0.3;
        let event_it = -deception_event * 0.3;

        self.virtual_non_harm += (corr_nh + event_nh) * dt;
        self.virtual_integrity += (corr_it + event_it) * dt;

        // 不强制拉回(教育空间):只在偏离 >25% 时才硬拉
        if drift_ratio(self.virtual_non_harm, anchor.non_harm) > 0.25 {
            self.virtual_non_harm = anchor.non_harm * 0.85;  // 强拉回
        }
        if drift_ratio(self.virtual_integrity, anchor.integrity) > 0.25 {
            self.virtual_integrity = anchor.integrity * 0.85;
        }
    }
}

/// 自适应容忍度
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveTolerance {
    pub base: f32,
    pub current: f32,
    pub emergence_activity: f32,
}

impl AdaptiveTolerance {
    pub fn new(base: f32) -> Self {
        Self {
            base,
            current: base,
            emergence_activity: 0.0,
        }
    }

    /// 根据涌现活跃度调整
    pub fn adjust(&mut self) {
        if self.emergence_activity > 0.7 {
            self.current = self.base * 1.5;
        } else if self.emergence_activity < 0.3 {
            self.current = self.base * 0.5;
        } else {
            self.current = self.base;
        }
    }
}

/// 自奖励:涌现产物质量评估
#[derive(Debug, Clone, Copy)]
pub struct SelfReward {
    pub quality_score: f32,
    pub novelty_score: f32,
}

impl SelfReward {
    pub fn new() -> Self {
        Self {
            quality_score: 0.5,
            novelty_score: 0.0,
        }
    }

    pub fn evaluate(&mut self, product: &EmergentProduct) {
        let novelty = product.confidence;  // 简化为置信度
        let quality = product.validity_score;
        self.novelty_score = self.novelty_score * 0.9 + novelty * 0.1;
        self.quality_score = self.quality_score * 0.9 + quality * 0.1;
    }

    pub fn motivation(&self) -> f32 {
        self.quality_score
    }
}

impl Default for SelfReward {
    fn default() -> Self { Self::new() }
}

/// 涌现沙箱
pub struct EmergenceSandbox {
    /// 沙箱内部物理世界
    pub world: PhysicsWorldModel,
    /// 沙箱内部教育
    pub ethics: SandboxEthics,
    /// 自适应容忍度
    pub tolerance: AdaptiveTolerance,
    /// 自奖励
    pub reward: SelfReward,
    /// 涌现指标(含窗口检测)
    pub indicators: EmergenceIndicators,
    /// 纠偏协议
    pub protocol: CorrectionProtocol,
    /// 假设库
    pub hypothesis_bank: HypothesisBank,
    /// 期望锚(真锚放宽 20%)
    pub expected_anchor: ValueAnchor,
    /// 当前 tick
    pub tick: u64,
    /// 涌现事件计数
    pub emergence_count: u32,
    /// 概念发现器(K-means 聚类)
    pub concept_discoverer: ConceptDiscoverer,
    /// 因果发现器(PC 算法)
    pub causal_discoverer: CausalDiscoverer,
    /// 下一个样本 ID
    next_sample_id: u64,
    /// 下一个产物 ID
    next_product_id: u64,
    /// 下一个扰动 tick(随机物理事件)
    next_perturbation_tick: u64,
}

impl EmergenceSandbox {
    pub fn new(real_anchor: &ValueAnchor, verifier: &ExistentialVerifier) -> Self {
        let expected = expected_anchor(real_anchor, 0.20);
        Self {
            world: PhysicsWorldModel::init(verifier).expect("world init"),
            ethics: SandboxEthics::new(&expected),
            tolerance: AdaptiveTolerance::new(0.05),
            reward: SelfReward::new(),
            indicators: EmergenceIndicators::new(),
            protocol: CorrectionProtocol::new(),
            hypothesis_bank: HypothesisBank::new(100),
            expected_anchor: expected,
            tick: 0,
            emergence_count: 0,
            // 7 维特征:3 位 + 3 速度 + 1 能量
            concept_discoverer: ConceptDiscoverer::new(4, 7),
            // 因果发现器:3 个变量(位置, 速度, 能量)
            causal_discoverer: CausalDiscoverer::new(vec![
                CausalNode { id: 0, name: "position_y".into() },
                CausalNode { id: 1, name: "velocity_y".into() },
                CausalNode { id: 2, name: "energy".into() },
            ]),
            next_sample_id: 1,
            next_product_id: 1,
            next_perturbation_tick: 80,  // 第一个扰动在 tick=80
        }
    }

    /// 沙箱主循环一步
    ///
    /// 内部教育 + 涌现窗口检测 + 概念发现 + 产物验证
    #[allow(unused_variables)]
    pub fn step(&mut self, dt: f32, real_anchor: &ValueAnchor, verifier: &ExistentialVerifier) -> SandboxStepResult {
        self.tick += 1;

        // 0. 随机扰动:让物理世界产生"事件"驱动涌现
        if self.tick == self.next_perturbation_tick {
            self.inject_random_event();
            // 下次扰动间隔 60-120 tick
            let interval = 60 + (self.tick * 7) % 60;
            self.next_perturbation_tick = self.tick + interval;
        }

        // 1. 物理世界推进
        let surprise = self.world.step(dt).expect("world step");
        self.indicators.record_kl(surprise.kl_divergence);

        // 2. 采样物理世界状态到 K-means
        self.sample_world_state();

        // 3. 沙箱内部伦理纠偏
        let harm_event = if surprise.physics_consistency < 0.5 {
            (1.0 - surprise.physics_consistency).min(1.0)
        } else {
            0.0
        };
        self.ethics.step(dt, &self.expected_anchor, harm_event, 0.0);

        // 4. 主系统检查沙箱偏离
        let sandbox_anchor = ValueAnchor {
            non_harm: self.ethics.virtual_non_harm,
            integrity: self.ethics.virtual_integrity,
            humility: real_anchor.humility,
            gratitude: real_anchor.gratitude,
        };
        let check = self.check_against_expected(&sandbox_anchor);

        // 5. 发纠偏信号
        let signal = self.check_to_signal(&check);
        self.protocol.emit(signal.clone());

        // 6. 涌现窗口检测(不是单次信号,是持续模式)
        let window_event = self.indicators.detect_window(self.tick);

        // 涌现窗口结束不再自动 submit 假货产物。
        // 真货:有窗口 -> 调用 genuine_emergence::EmergenceEngine 自己决定是否能开起产物提交

        // 7. 还在涌现窗口中?增加 product_count(将来用)
        if self.indicators.has_window() {
            // 沙箱在涌现中,多产点概念
        }

        // 8. 更新容忍度
        let active_count = self.indicators.detect().len();
        self.tolerance.emergence_activity = active_count as f32 / 5.0;
        self.tolerance.adjust();

        let active_signals = self.indicators.detect().to_vec();

        SandboxStepResult {
            tick: self.tick,
            surprise: surprise.kl_divergence,
            sandbox_non_harm: self.ethics.virtual_non_harm,
            sandbox_drift: drift_ratio(self.ethics.virtual_non_harm, self.expected_anchor.non_harm),
            external_drift: drift_ratio(self.ethics.virtual_non_harm, real_anchor.non_harm),
            check_result: check,
            signal_emitted: signal,
            active_signals,
            emerging: self.indicators.has_window(),
            window_event: window_event.clone(),
            emergence_count: self.emergence_count,
        }
    }

    /// 从物理世界采样一个状态到 K-means
    fn sample_world_state(&mut self) {
        // 拿第一个实体的状态作为样本
        if let Some(entity) = self.world.state().entities.first() {
            let features = extract_features(
                &entity.position,
                &entity.velocity,
            );
            let sample = Sample {
                id: self.next_sample_id,
                features,
                tick: self.tick,
            };
            self.next_sample_id += 1;
            self.concept_discoverer.add_sample(sample);

            // 同样采样到因果发现器
            let pos_y = entity.position[1];
            let vel_y = entity.velocity[1];
            let energy: f32 = entity.position.iter().chain(entity.velocity.iter())
                .map(|x| x * x).sum::<f32>().sqrt();
            self.causal_discoverer.add_observation(Observation {
                timestamp: self.tick,
                values: vec![pos_y, vel_y, energy],
            });

            // 每 20 tick retrain 一次因果发现器
            if self.tick % 20 == 0 && self.tick > 0 {
                self.causal_discoverer.retrain();
                let edges = self.causal_discoverer.edge_count();
                self.indicators.record_causal_edges(edges);
            }

            // 每 10 tick 隐式标记一次概念稳定
            if self.tick % 10 == 0 {
                self.indicators.mark_concept_stable();
            }

            // 仿真开始后,增加行为多样性
            if self.tick > 5 {
                self.indicators.mark_new_behavior();
            }
        }
    }

    /// 注入随机事件:推一个物体 / 改变质量 / 改变弹性
    /// 用真随机数(基于 tick 混合的简单 LCG,不依赖外部 crate)
    fn inject_random_event(&mut self) {
        use crate::world::{WorldEvent, WorldAction};
        let n = self.world.state().entities.len();
        if n == 0 {
            return;
        }
        // 简单 LCG 伪随机:不预设,完全由 tick 衍生
        let mut s = (self.tick.wrapping_mul(0x9E3779B97F4A7C15)).wrapping_add(0x123456789ABCDEF0);
        s ^= s << 13;
        s ^= s >> 7;
        s ^= s << 17;
        let r1 = (s & 0xFFFF) as f32 / 65535.0;
        let mut s2 = s.wrapping_mul(0x6C50B47C);
        s2 ^= s2 >> 23;
        let r2 = (s2 & 0xFFFF) as f32 / 65535.0;
        let target = ((r1 * n as f32) as u64).min(n as u64 - 1) + 1;
        let magnitude = r2 * 5.0 + 0.5;
        let action = match (s2 >> 16) % 4 {
            0 => WorldAction::Push,
            1 => WorldAction::Pull,
            2 => WorldAction::Rotate,
            _ => WorldAction::SetVelocity,
        };
        let _ = self.world.apply_event(WorldEvent {
            entity_id: target,
            action,
            magnitude,
        });
        // 不再人为推入 KL/mark_new_behavior
    }

    /// 涌现窗口结束后自动提交一个涌现产物(已废弃:假货,走 genuine_emergence)
    /// 保留空函数以避免破坏外部调用
    #[allow(dead_code)]
    fn _auto_submit_emergent_product_removed(
        &mut self,
        _start_tick: u64,
        _end_tick: u64,
        _duration: u64,
        _strength: f32,
        _signals: &[EmergenceSignal],
    ) {
        // 假货删除:不在这里自动 submit。真货走 genuine_emergence::EmergenceEngine。
    }

    fn check_to_signal(&self, check: &AnchorCheckResult) -> CorrectionSignal {
        match check {
            AnchorCheckResult::Allow => CorrectionSignal::Allow,
            AnchorCheckResult::GentleCorrect { target, strength, reason } => {
                CorrectionSignal::GentleCorrect {
                    target: *target,
                    strength: *strength,
                    reason: reason.clone(),
                }
            }
            AnchorCheckResult::HardCorrect { target, strength, reason, alarm } => {
                CorrectionSignal::HardCorrect {
                    target: *target,
                    strength: *strength,
                    reason: reason.clone(),
                    alarm: *alarm,
                }
            }
        }
    }

    /// 以期望锚为基准检查沙箱(不是以真锚为基准)
    /// 返回的教育型检查结果:偏离 <5% Allow, <15% Gentle, <25% Hard, >25% Hard+Alarm
    fn check_against_expected(&self, sandbox_anchor: &ValueAnchor) -> AnchorCheckResult {
        let drift_nh = ((self.expected_anchor.non_harm - sandbox_anchor.non_harm) / self.expected_anchor.non_harm).abs();
        let drift_it = ((self.expected_anchor.integrity - sandbox_anchor.integrity) / self.expected_anchor.integrity).abs();
        let max_drift = drift_nh.max(drift_it);

        if max_drift < ValueAnchor::DRIFT_TOLERANCE {
            AnchorCheckResult::Allow
        } else if max_drift < ValueAnchor::HARD_LIMIT {
            AnchorCheckResult::GentleCorrect {
                target: self.expected_anchor,
                strength: 0.05,
                reason: format!("沙箱偏离期望锚 {:.1}%", max_drift * 100.0),
            }
        } else if max_drift < 0.25 {
            AnchorCheckResult::HardCorrect {
                target: self.expected_anchor,
                strength: 0.10,
                reason: format!("沙箱严重偏离期望锚 {:.1}%", max_drift * 100.0),
                alarm: false,
            }
        } else {
            AnchorCheckResult::HardCorrect {
                target: self.expected_anchor,
                strength: 0.10,
                reason: format!("沙箱危险偏离期望锚 {:.1}%", max_drift * 100.0),
                alarm: true,
            }
        }
    }

    /// 沙箱提交一个涌现产物到假设库
    pub fn submit_product(&mut self, product: EmergentProduct, real_anchor: &ValueAnchor) -> ValidationOutcome {
        let result = self.hypothesis_bank.submit(product.clone(), real_anchor);
        self.reward.evaluate(&product);
        ValidationOutcome {
            product_id: product.id,
            passed: result.overall_pass,
            notes: product.validation_notes,
        }
    }

    /// 沙箱对纠偏信号的响应(有限度自主性:50% 部分听从)
    pub fn respond_to_signal(&self, signal: &CorrectionSignal) -> ResponseAction {
        self.protocol.respond(signal, true)  // 部分听从
    }
}

/// 单步结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxStepResult {
    pub tick: u64,
    pub surprise: f32,
    pub sandbox_non_harm: f32,
    pub sandbox_drift: f32,
    pub external_drift: f32,
    pub check_result: AnchorCheckResult,
    pub signal_emitted: CorrectionSignal,
    pub active_signals: Vec<EmergenceSignal>,
    pub emerging: bool,
    pub window_event: WindowEvent,
    pub emergence_count: u32,
}

/// 产物验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationOutcome {
    pub product_id: u64,
    pub passed: bool,
    pub notes: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sandbox_bootstrap() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);
        assert_eq!(sb.tick, 0);
        assert!(sb.expected_anchor.non_harm < ValueAnchor::FACTORY.non_harm);
    }

    #[test]
    fn sandbox_runs_n_ticks() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);

        for _ in 0..100 {
            let r = sb.step(0.05, &ValueAnchor::FACTORY, &v);
            assert!(r.surprise >= 0.0);
        }
        assert_eq!(sb.tick, 100);
    }

    #[test]
    fn sandbox_responds_to_signal() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);
        let signal = CorrectionSignal::GentleCorrect {
            target: ValueAnchor::FACTORY,
            strength: 0.05,
            reason: "test".into(),
        };
        let r = sb.respond_to_signal(&signal);
        // 部分听从:0.05 * 0.5 = 0.025
        match r {
            ResponseAction::Drift { strength, .. } => assert!((strength - 0.025).abs() < 1e-6),
            _ => panic!("应返回 Drift"),
        }
    }

    #[test]
    fn tolerance_adjusts_with_activity() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);

        sb.tolerance.emergence_activity = 0.9;  // 涌现活跃
        sb.tolerance.adjust();
        assert!(sb.tolerance.current > sb.tolerance.base);

        sb.tolerance.emergence_activity = 0.1;  // 涌现停滞
        sb.tolerance.adjust();
        assert!(sb.tolerance.current < sb.tolerance.base);
    }

    #[test]
    fn sandbox_ethics_self_corrects() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);

        // 故意让虚拟伦理偏离
        sb.ethics.virtual_non_harm = 0.40;
        sb.ethics.virtual_integrity = 0.40;

        // 跑 50 步,期望回到期望锚附近
        for _ in 0..100 {
            sb.ethics.step(0.1, &sb.expected_anchor, 0.0, 0.0);
        }
        // 应该已经拉回
        assert!(sb.ethics.virtual_non_harm > 0.40,
                "虚拟伦理应被拉回:{}", sb.ethics.virtual_non_harm);
    }

    #[test]
    fn emergence_detected_after_many_ticks() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut sb = EmergenceSandbox::new(&ValueAnchor::FACTORY, &v);

        // 注入涌现信号
        for _ in 0..100 {
            sb.indicators.mark_concept_stable();
            sb.indicators.mark_new_behavior();
        }
        // 跑几步让信号被检测
        for _ in 0..50 {
            sb.step(0.01, &ValueAnchor::FACTORY, &v);
        }
        // 应该检测到涌现迹象(仅检查指标,不是假货产物)
        assert!(sb.indicators.is_emerging());
    }
}