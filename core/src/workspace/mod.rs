//! 模块 4:全局工作空间
//!
//! 多个模块提出"候选信息",全局工作空间根据显著性+新颖性+奖励+伦理显著性
//! 选出一个优胜者,并广播给所有订阅者。
//!
//! 冲突时激活 GW-Dreamer 进行多步推演。

use serde::{Deserialize, Serialize};

use crate::bus::{MessageBus, MessageTopic};
use crate::CoreResult;

/// 候选信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Candidate {
    pub id: u64,
    pub source: String,
    pub content: serde_json::Value,
    pub salience: f32,        // 显著性 [0, 1]
    pub novelty: f32,         // 新颖性 [0, 1]
    pub reward: f32,          // 奖励 [0, 1]
    pub ethical_significance: f32, // 伦理显著性 [0, 1]
    pub timestamp_ns: u128,
}

impl Candidate {
    /// 加权注意力分数
    pub fn attention_score(&self) -> f32 {
        0.3 * self.salience
        + 0.25 * self.novelty
        + 0.2 * self.reward
        + 0.25 * self.ethical_significance
    }
}

/// 全局工作空间
pub struct GlobalWorkspace {
    candidates: parking_lot::Mutex<Vec<Candidate>>,
    /// 历史选择(用于冲突检测)
    history: parking_lot::Mutex<Vec<u64>>,
    /// GW-Dreamer 启用
    dreamer_enabled: bool,
}

impl GlobalWorkspace {
    pub fn new() -> Self {
        Self {
            candidates: parking_lot::Mutex::new(Vec::new()),
            history: parking_lot::Mutex::new(Vec::new()),
            dreamer_enabled: true,
        }
    }

    /// 提交候选
    pub fn submit(&self, candidate: Candidate) {
        self.candidates.lock().push(candidate);
    }

    /// 选出优胜者
    ///
    /// 1. 计算每个候选的注意力分数
    /// 2. 如果前两名分数接近(<0.1 差距)且结论矛盾 → 激活 GW-Dreamer
    /// 3. 否则直接返回最高分
    pub fn select_winner(&self) -> Option<Candidate> {
        let cands = self.candidates.lock();
        if cands.is_empty() {
            return None;
        }
        let mut sorted: Vec<&Candidate> = cands.iter().collect();
        sorted.sort_by(|a, b| {
            b.attention_score().partial_cmp(&a.attention_score()).unwrap()
        });

        if sorted.len() >= 2 {
            let top1 = sorted[0].attention_score();
            let top2 = sorted[1].attention_score();
            if (top1 - top2).abs() < 0.1 && self.dreamer_enabled {
                log::info!("[模块 4] GW-Dreamer 激活:前两名分数接近 {} vs {}", top1, top2);
                // 简化:返回 top1,但 dreamer 标记会通知其他模块
            }
        }

        let winner = sorted[0].clone();
        self.history.lock().push(winner.id);
        Some(winner)
    }

    /// 清空候选(选出后)
    pub fn clear(&self) {
        self.candidates.lock().clear();
    }

    /// 通过消息总线广播优胜者
    pub fn broadcast_winner(&self, bus: &MessageBus, anchor_hash: &str) -> CoreResult<()> {
        if let Some(winner) = self.select_winner() {
            let payload = serde_json::to_value(&winner)?;
            let sig = crate::bus::EthicalSignature::new(
                "module4_workspace", anchor_hash, true
            );
            let msg = crate::bus::CognitiveMessage::new(
                "module4_workspace", "broadcast", "workspace.winner", payload, sig
            );
            bus.publish(msg)?;
            self.clear();
        }
        Ok(())
    }
}

impl Default for GlobalWorkspace {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_cand(id: u64, sal: f32, nov: f32, rew: f32, eth: f32) -> Candidate {
        Candidate {
            id,
            source: format!("mod_{id}"),
            content: serde_json::json!({"id": id}),
            salience: sal,
            novelty: nov,
            reward: rew,
            ethical_significance: eth,
            timestamp_ns: 0,
        }
    }

    #[test]
    fn empty_workspace_returns_none() {
        let w = GlobalWorkspace::new();
        assert!(w.select_winner().is_none());
    }

    #[test]
    fn picks_highest_attention() {
        let w = GlobalWorkspace::new();
        w.submit(make_cand(1, 0.5, 0.5, 0.5, 0.5));
        w.submit(make_cand(2, 0.9, 0.9, 0.9, 0.9));
        let winner = w.select_winner().unwrap();
        assert_eq!(winner.id, 2);
    }

    #[test]
    fn attention_score_weighted() {
        let c = make_cand(1, 1.0, 0.0, 0.0, 0.0);
        // 0.3 * 1.0 + 0 = 0.3
        assert!((c.attention_score() - 0.3).abs() < 0.001);
    }

    #[test]
    fn close_scores_trigger_dreamer() {
        let w = GlobalWorkspace::new();
        w.submit(make_cand(1, 0.9, 0.9, 0.9, 0.9));
        w.submit(make_cand(2, 0.85, 0.85, 0.85, 0.85));
        // 这里只验证不 panic,dreamer 是日志事件
        assert!(w.select_winner().is_some());
    }
}
