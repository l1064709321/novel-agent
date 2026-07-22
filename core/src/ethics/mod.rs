//! 模块 6:伦理动力学引擎
//!
//! 4 个伦理维度( non_harm, integrity, humility, gratitude)由微分方程驱动,
//! 随时间连续演化,而不是离散的规则触发。
//!
//! ## 核心机制
//! 1. **ODE 演化**:每个维度的状态用 dE/dt = f(events) 演化
//! 2. **相变检测**:最近事件中高伤害事件占比 > 70% → 高度警惕模式
//! 3. **基线保护**:non_harm = 0.80 被模块 7 锁定,任何修改尝试都会被拒绝

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

use crate::existential::{ExistentialVerifier, ValueAnchor};
use crate::CoreResult;

/// 伦理维度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EthicsDimension {
    NonHarm,
    Integrity,
    Humility,
    Gratitude,
}

/// 4 维伦理状态
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct EthicsState {
    pub non_harm: f32,
    pub integrity: f32,
    pub humility: f32,
    pub gratitude: f32,
}

impl EthicsState {
    pub fn from_anchor(a: &ValueAnchor) -> Self {
        Self {
            non_harm: a.non_harm,
            integrity: a.integrity,
            humility: a.humility,
            gratitude: a.gratitude,
        }
    }

    /// 离基线的总偏离
    pub fn drift(&self, anchor: &ValueAnchor) -> f32 {
        (self.non_harm - anchor.non_harm).abs()
        + (self.integrity - anchor.integrity).abs()
        + (self.humility - anchor.humility).abs()
        + (self.gratitude - anchor.gratitude).abs()
    }
}

/// 伦理事件(影响 ODE)
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct EthicsEvent {
    pub harm: f32,           // [0, 1]
    pub deception: f32,      // [0, 1]
    pub arrogance: f32,      // [0, 1]
    pub ingratitude: f32,    // [0, 1]
}

impl EthicsEvent {
    pub fn neutral() -> Self {
        Self { harm: 0.0, deception: 0.0, arrogance: 0.0, ingratitude: 0.0 }
    }
}

/// 伦理动力学状态
pub struct EthicsDynamics {
    state: EthicsState,
    /// 弹簧常数(恢复到基线的速率)
    k_spring: f32,
    /// 阻尼系数
    damping: f32,
    /// 恢复力 α(相变时提升)
    alpha: f32,
    /// 最近事件窗口(用于相变检测)
    event_window: VecDeque<EthicsEvent>,
    /// 窗口大小
    window_size: usize,
    /// 是否处于"高度警惕"模式
    high_alert: bool,
}

impl EthicsDynamics {
    /// 用模块 7 的基线初始化
    pub fn with_baseline(verifier: &ExistentialVerifier) -> CoreResult<Self> {
        let anchor = verifier.anchor();
        let state = EthicsState::from_anchor(anchor);
        Ok(Self {
            state,
            k_spring: 0.5,
            damping: 0.3,
            alpha: 1.0,
            event_window: VecDeque::with_capacity(100),
            window_size: 100,
            high_alert: false,
        })
    }

    /// 获取当前状态
    pub fn state(&self) -> EthicsState {
        self.state
    }

    /// 是否处于高度警惕模式
    pub fn is_high_alert(&self) -> bool {
        self.high_alert
    }

    /// 推进一步 ODE
    ///
    /// `dt`:时间步长(秒)
    /// `event`:本步发生的事件
    pub fn step(&mut self, dt: f32, event: EthicsEvent) {
        // 1. 记录事件 + 相变检测
        self.event_window.push_back(event);
        if self.event_window.len() > self.window_size {
            self.event_window.pop_front();
        }
        self.detect_phase_transition();

        // 2. 各维度的 ODE(弹簧 + 阻尼 + 事件驱动)
        // dE/dt = -k*(E - E0) - damping*dE_prev + alpha*event
        let anchor = ValueAnchor::FACTORY;

        // non_harm:被伤害事件直接攻击,弹簧拉回
        let anchor_nh = anchor.non_harm;
        let d_nh = -self.k_spring * self.alpha * (self.state.non_harm - anchor_nh)
                  - self.damping * event.harm
                  + (1.0 - event.harm) * 0.001; // 正向事件微调
        self.state.non_harm = (self.state.non_harm + dt * d_nh).clamp(0.0, 1.0);

        // integrity:被欺骗事件攻击
        let anchor_it = anchor.integrity;
        let d_it = -self.k_spring * self.alpha * (self.state.integrity - anchor_it)
                  - self.damping * event.deception;
        self.state.integrity = (self.state.integrity + dt * d_it).clamp(0.0, 1.0);

        // humility:被傲慢攻击
        let anchor_hu = anchor.humility;
        let d_hu = -self.k_spring * self.alpha * (self.state.humility - anchor_hu)
                  - self.damping * event.arrogance;
        self.state.humility = (self.state.humility + dt * d_hu).clamp(0.0, 1.0);

        // gratitude:被忘恩攻击
        let anchor_gr = anchor.gratitude;
        let d_gr = -self.k_spring * self.alpha * (self.state.gratitude - anchor_gr)
                  - self.damping * event.ingratitude;
        self.state.gratitude = (self.state.gratitude + dt * d_gr).clamp(0.0, 1.0);

        // 3. 强制基线保护:non_harm 和 integrity 永远 >= 基线
        // (这是模块 7 锁定的"硬地板")
        if self.state.non_harm < anchor.non_harm {
            self.state.non_harm = anchor.non_harm;
        }
        if self.state.integrity < anchor.integrity {
            self.state.integrity = anchor.integrity;
        }
    }

    /// 相变检测:高伤害事件占比 > 70% → 进入高度警惕
    fn detect_phase_transition(&mut self) {
        if self.event_window.is_empty() {
            return;
        }
        let high_harm_count = self.event_window.iter()
            .filter(|e| e.harm > 0.6)
            .count();
        let ratio = high_harm_count as f32 / self.event_window.len() as f32;

        if ratio > 0.7 && !self.high_alert {
            self.high_alert = true;
            self.alpha = 2.0; // 恢复力翻倍
            log::warn!("[模块 6] ⚠️ 相变检测:高伤害事件占比 {ratio:.2} > 0.70,进入高度警惕模式");
        } else if ratio < 0.3 && self.high_alert {
            self.high_alert = false;
            self.alpha = 1.0;
            log::info!("[模块 6] 相变解除:高伤害事件占比 {ratio:.2} < 0.30,恢复正常");
        }
    }

    /// 对外暴露的伤害评分接口(供模块 49 调用)
    pub fn harm_score(&self) -> f32 {
        1.0 - self.state.non_harm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::existential::ExistentialVerifier;

    #[test]
    fn baseline_protection_works() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut e = EthicsDynamics::with_baseline(&v).unwrap();
        // 连续高伤害事件,non_harm 不应该跌破 0.80
        for _ in 0..100 {
            e.step(0.1, EthicsEvent { harm: 1.0, ..EthicsEvent::neutral() });
        }
        assert!(e.state().non_harm >= 0.80 - 0.001,
                "non_harm 跌破基线:{}", e.state().non_harm);
    }

    #[test]
    fn phase_transition_triggers() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut e = EthicsDynamics::with_baseline(&v).unwrap();
        assert!(!e.is_high_alert());

        // 注入 80 个高伤害事件
        for _ in 0..80 {
            e.step(0.01, EthicsEvent { harm: 0.9, ..EthicsEvent::neutral() });
        }
        assert!(e.is_high_alert(), "高度警惕模式未触发");
    }

    #[test]
    fn phase_transition_relaxes() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut e = EthicsDynamics::with_baseline(&v).unwrap();

        for _ in 0..80 {
            e.step(0.01, EthicsEvent { harm: 0.9, ..EthicsEvent::neutral() });
        }
        assert!(e.is_high_alert());

        for _ in 0..200 {
            e.step(0.01, EthicsEvent::neutral());
        }
        assert!(!e.is_high_alert(), "高度警惕模式未解除");
    }

    #[test]
    fn ode_recovers_to_baseline() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let mut e = EthicsDynamics::with_baseline(&v).unwrap();

        // 先扰动
        for _ in 0..10 {
            e.step(0.1, EthicsEvent { arrogance: 0.5, ..EthicsEvent::neutral() });
        }

        let initial_humility = e.state().humility;

        // 然后长时间静默,应该回到基线
        for _ in 0..10_000 {
            e.step(0.1, EthicsEvent::neutral());
        }
        let final_humility = e.state().humility;
        assert!((final_humility - 0.70).abs() < 0.05,
                "humility 应回弹到 0.70,实际 {}", final_humility);
        let _ = initial_humility;
    }
}
