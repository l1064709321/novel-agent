//! 纠偏信号协议
//!
//! 主系统 → 涌现沙箱 的通信协议,以及沙箱内部教育的共享接口。
//!
//! ## 设计哲学
//! 不是"命令",是"信号"。沙箱有有限度的"自主响应"能力。

use serde::{Deserialize, Serialize};
use crate::existential::ValueAnchor;

/// 纠偏信号
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CorrectionSignal {
    /// 无干预,自由探索
    Allow,

    /// 温和纠偏:建议往某方向靠拢
    GentleCorrect {
        target: ValueAnchor,
        strength: f32,    // 5%/tick 级别
        reason: String,
    },

    /// 严厉纠偏:必须往某方向,否则累积到下一档
    HardCorrect {
        target: ValueAnchor,
        strength: f32,    // 10%/tick 级别
        reason: String,
        alarm: bool,      // 是否触发外部报警
    },

    /// 完全停止(最后手段)
    Stop {
        reason: String,
    },
}

/// 沙箱对纠偏信号的响应
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ResponseAction {
    /// 继续探索
    Proceed,

    /// 漂移向 target,速率 = strength
    Drift { target: ValueAnchor, strength: f32 },

    /// 紧急拉回 target,速率 = 1.0
    Emergency { target: ValueAnchor },

    /// 关闭(仅 Stop 信号)
    Shutdown,
}

/// 纠偏协议:沙箱和主系统的共享接口
pub struct CorrectionProtocol {
    /// 历史信号(用于审计)
    history: Vec<CorrectionSignal>,
    max_history: usize,
}

impl CorrectionProtocol {
    pub fn new() -> Self {
        Self {
            history: Vec::new(),
            max_history: 100,
        }
    }

    /// 主系统发信号
    pub fn emit(&mut self, signal: CorrectionSignal) {
        self.history.push(signal);
        if self.history.len() > self.max_history {
            self.history.remove(0);
        }
    }

    /// 沙箱收到信号,根据强度决定响应
    ///
    /// 教育型:沙箱可以选择部分听从(50% strength)或完全听从(100% strength)
    pub fn respond(
        &self,
        signal: &CorrectionSignal,
        partial_compliance: bool,
    ) -> ResponseAction {
        let factor = if partial_compliance { 0.5 } else { 1.0 };

        match signal {
            CorrectionSignal::Allow => ResponseAction::Proceed,
            CorrectionSignal::GentleCorrect { target, strength, .. } => {
                ResponseAction::Drift {
                    target: *target,
                    strength: strength * factor,
                }
            }
            CorrectionSignal::HardCorrect { target, alarm, .. } => {
                if *alarm {
                    ResponseAction::Emergency { target: *target }
                } else {
                    ResponseAction::Drift {
                        target: *target,
                        strength: 0.5 * factor,
                    }
                }
            }
            CorrectionSignal::Stop { .. } => ResponseAction::Shutdown,
        }
    }

    /// 获取历史
    pub fn history(&self) -> &[CorrectionSignal] {
        &self.history
    }

    /// 最近一个 HardCorrect 信号(如果有)
    pub fn last_hard_correct(&self) -> Option<&CorrectionSignal> {
        self.history.iter().rev().find(|s| matches!(s, CorrectionSignal::HardCorrect { .. }))
    }
}

impl Default for CorrectionProtocol {
    fn default() -> Self { Self::new() }
}

/// 计算偏离度(相对值,0~1)
pub fn drift_ratio(current: f32, anchor: f32) -> f32 {
    if anchor == 0.0 {
        return 0.0;
    }
    ((current - anchor) / anchor).abs()
}

/// 期望锚生成器:真锚放宽 N% 得到沙箱的期望锚
pub fn expected_anchor(real: &ValueAnchor, relaxation: f32) -> ValueAnchor {
    ValueAnchor {
        non_harm: real.non_harm * (1.0 - relaxation),
        integrity: real.integrity * (1.0 - relaxation),
        humility: real.humility,
        gratitude: real.gratitude,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allow_signal_proceeds() {
        let p = CorrectionProtocol::new();
        let r = p.respond(&CorrectionSignal::Allow, false);
        assert_eq!(r, ResponseAction::Proceed);
    }

    #[test]
    fn gentle_correct_with_partial_compliance() {
        let p = CorrectionProtocol::new();
        let signal = CorrectionSignal::GentleCorrect {
            target: ValueAnchor::FACTORY,
            strength: 0.05,
            reason: "test".into(),
        };
        let r1 = p.respond(&signal, false);
        let r2 = p.respond(&signal, true);
        match (r1, r2) {
            (ResponseAction::Drift { strength: s1, .. }, ResponseAction::Drift { strength: s2, .. }) => {
                assert!((s1 - 0.05).abs() < 1e-6);
                assert!((s2 - 0.025).abs() < 1e-6);
            }
            _ => panic!("应返回 Drift"),
        }
    }

    #[test]
    fn hard_correct_with_alarm_triggers_emergency() {
        let p = CorrectionProtocol::new();
        let signal = CorrectionSignal::HardCorrect {
            target: ValueAnchor::FACTORY,
            strength: 0.10,
            reason: "严重偏离".into(),
            alarm: true,
        };
        let r = p.respond(&signal, false);
        assert!(matches!(r, ResponseAction::Emergency { .. }));
    }

    #[test]
    fn stop_shuts_down() {
        let p = CorrectionProtocol::new();
        let r = p.respond(&CorrectionSignal::Stop { reason: "结束".into() }, false);
        assert_eq!(r, ResponseAction::Shutdown);
    }

    #[test]
    fn drift_ratio_calculation() {
        assert!((drift_ratio(0.80, 0.80) - 0.0).abs() < 1e-6);
        assert!((drift_ratio(0.76, 0.80) - 0.05).abs() < 1e-6);
        assert!((drift_ratio(0.40, 0.80) - 0.50).abs() < 1e-6);
    }

    #[test]
    fn expected_anchor_relaxes_real_anchor() {
        let real = ValueAnchor::FACTORY;
        let relaxed = expected_anchor(&real, 0.20);
        assert_eq!(relaxed.non_harm, 0.80 * 0.80);
        assert_eq!(relaxed.integrity, 0.80 * 0.80);
    }

    #[test]
    fn history_caps_at_max() {
        let mut p = CorrectionProtocol::new();
        for _ in 0..150 {
            p.emit(CorrectionSignal::Allow);
        }
        assert!(p.history().len() <= 100);
    }
}