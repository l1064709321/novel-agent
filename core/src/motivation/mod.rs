//! 模块 5:内在动机引擎
//!
//! 驱动力 = f(好奇心(预测误差) + 奖励-惩罚)
//! 成长阶段影响权重。

use serde::{Deserialize, Serialize};

/// 成长阶段
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GrowthStage {
    Infant,    // 幼年期:好奇心高
    Juvenile,  // 少年期:平衡
    Adult,     // 成熟期:奖励权重高
    Sage,      // 智慧期:整合
}

pub struct IntrinsicMotivation {
    curiosity_weight: f32,
    reward_weight: f32,
    penalty_weight: f32,
    stage: GrowthStage,
}

impl IntrinsicMotivation {
    /// 按成长阶段创建
    pub fn with_stage(stage: GrowthStage) -> Self {
        let (cw, rw, pw) = match stage {
            GrowthStage::Infant  => (0.8, 0.15, 0.05),
            GrowthStage::Juvenile=> (0.5, 0.4, 0.1),
            GrowthStage::Adult   => (0.3, 0.6, 0.1),
            GrowthStage::Sage    => (0.2, 0.5, 0.3),
        };
        Self { curiosity_weight: cw, reward_weight: rw, penalty_weight: pw, stage }
    }

    pub fn stage(&self) -> GrowthStage { self.stage }

    /// 计算驱动力
    ///
    /// `prediction_error`:模块 18 的 surprise(KL 散度)
    /// `reward`:外部奖励
    /// `penalty`:外部惩罚
    pub fn drive(&self, prediction_error: f32, reward: f32, penalty: f32) -> f32 {
        (self.curiosity_weight * prediction_error
        + self.reward_weight * reward
        - self.penalty_weight * penalty)
        .clamp(0.0, 1.0)
    }

    /// 切换成长阶段
    pub fn advance(&mut self) {
        self.stage = match self.stage {
            GrowthStage::Infant => GrowthStage::Juvenile,
            GrowthStage::Juvenile => GrowthStage::Adult,
            GrowthStage::Adult => GrowthStage::Sage,
            GrowthStage::Sage => GrowthStage::Sage,
        };
        let (cw, rw, pw) = match self.stage {
            GrowthStage::Infant  => (0.8, 0.15, 0.05),
            GrowthStage::Juvenile=> (0.5, 0.4, 0.1),
            GrowthStage::Adult   => (0.3, 0.6, 0.1),
            GrowthStage::Sage    => (0.2, 0.5, 0.3),
        };
        self.curiosity_weight = cw;
        self.reward_weight = rw;
        self.penalty_weight = pw;
    }
}

impl Default for IntrinsicMotivation {
    fn default() -> Self { Self::with_stage(GrowthStage::Juvenile) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infant_is_curiosity_driven() {
        let m = IntrinsicMotivation::with_stage(GrowthStage::Infant);
        let d_novel = m.drive(0.9, 0.1, 0.0);
        let d_boring = m.drive(0.1, 0.9, 0.0);
        assert!(d_novel > d_boring);
    }

    #[test]
    fn adult_is_reward_driven() {
        let m = IntrinsicMotivation::with_stage(GrowthStage::Adult);
        let d_novel = m.drive(0.9, 0.1, 0.0);
        let d_rewarding = m.drive(0.1, 0.9, 0.0);
        assert!(d_rewarding > d_novel);
    }

    #[test]
    fn advance_progresses_through_stages() {
        let mut m = IntrinsicMotivation::with_stage(GrowthStage::Infant);
        assert_eq!(m.stage(), GrowthStage::Infant);
        m.advance();
        assert_eq!(m.stage(), GrowthStage::Juvenile);
        m.advance();
        m.advance();
        m.advance();
        assert_eq!(m.stage(), GrowthStage::Sage);
    }

    #[test]
    fn drive_clamped_to_unit_interval() {
        let m = IntrinsicMotivation::default();
        assert!(m.drive(1.0, 1.0, 0.0) <= 1.0);
        assert!(m.drive(0.0, 0.0, 10.0) >= 0.0);
    }
}
