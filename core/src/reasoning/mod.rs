//! 模块 20 推理循环引擎 + 模块 32 不确定性表达器 + 模块 49 道德评估器
//!
//! ## 模块 20 推理循环
//! 实现"假设-证据-更新"循环,可与因果图(K17)和失败分析(K31)联合工作。
//!
//! ## 模块 32 不确定性表达
//! 模型对自己答案的把握度,基于:预测方差、证据数量、相似历史成功率。
//!
//! ## 模块 49 道德评估器
//! 与伦理动力学(K6)和存在性铁门(K7)不同,这是**日常行为的道德评估**:
//! 一次决策在"诚实/伤害/尊重/公平"四个维度上的得分。

use std::collections::HashMap;

// ============================================================
// 模块 20:推理循环引擎
// ============================================================

/// 假设
#[derive(Debug, Clone)]
pub struct Hypothesis {
    pub id: u64,
    pub text: String,
    /// 先验概率 0~1
    pub prior: f32,
    /// 累积证据
    pub evidence: Vec<Evidence>,
    /// 后验
    pub posterior: f32,
}

/// 证据
#[derive(Debug, Clone)]
pub struct Evidence {
    pub source: String,
    /// 支持(+1) / 反对(-1)
    pub polarity: f32,
    /// 强度 0~1
    pub strength: f32,
}

/// 推理循环
#[derive(Debug)]
pub struct ReasoningLoop {
    hypotheses: HashMap<u64, Hypothesis>,
    next_id: u64,
    /// 最多保留假设数
    max_hypotheses: usize,
}

impl ReasoningLoop {
    pub fn new(max_hypotheses: usize) -> Self {
        Self {
            hypotheses: HashMap::new(),
            next_id: 1,
            max_hypotheses,
        }
    }

    /// 提出一个假设
    pub fn propose(&mut self, text: impl Into<String>, prior: f32) -> u64 {
        if self.hypotheses.len() >= self.max_hypotheses {
            // 移除后验最低的
            if let Some((&id, _)) = self
                .hypotheses
                .iter()
                .min_by(|(_, a), (_, b)| a.posterior.partial_cmp(&b.posterior).unwrap())
            {
                self.hypotheses.remove(&id);
            }
        }
        let id = self.next_id;
        self.next_id += 1;
        self.hypotheses.insert(
            id,
            Hypothesis {
                id,
                text: text.into(),
                prior: prior.clamp(0.0, 1.0),
                evidence: Vec::new(),
                posterior: prior.clamp(0.0, 1.0),
            },
        );
        id
    }

    /// 添加证据
    pub fn add_evidence(&mut self, hyp_id: u64, evidence: Evidence) {
        // 先单独拿 prior 和 evidence 出来,避免与后续 mutable 借用冲突
        let (prior, evidence_vec) = {
            let h = match self.hypotheses.get_mut(&hyp_id) {
                Some(h) => h,
                None => return,
            };
            h.evidence.push(evidence);
            (h.prior, h.evidence.clone())
        };
        // 再单独算 posterior
        let posterior = self.bayes_update(prior, &evidence_vec);
        if let Some(h) = self.hypotheses.get_mut(&hyp_id) {
            h.posterior = posterior;
        }
    }

    /// 朴素贝叶斯更新(似然比)
    fn bayes_update(&self, prior: f32, evidence: &[Evidence]) -> f32 {
        if evidence.is_empty() {
            return prior;
        }
        let mut log_odds = ((prior.clamp(1e-6, 1.0 - 1e-6))
            / (1.0 - prior.clamp(1e-6, 1.0 - 1e-6)))
        .ln();
        for e in evidence {
            // 似然比 = exp(polarity * strength)
            // polarity=+1 → log_odds 增加;polarity=-1 → 减少
            let lr = (e.polarity * e.strength * 2.0).exp();
            log_odds += lr.ln();
        }
        1.0 / (1.0 + (-log_odds).exp())
    }

    /// 假设数
    pub fn len(&self) -> usize {
        self.hypotheses.len()
    }

    pub fn is_empty(&self) -> bool {
        self.hypotheses.is_empty()
    }

    /// 最强假设
    pub fn strongest(&self) -> Option<&Hypothesis> {
        self.hypotheses
            .values()
            .max_by(|a, b| a.posterior.partial_cmp(&b.posterior).unwrap())
    }

    /// 全部假设
    pub fn all(&self) -> impl Iterator<Item = &Hypothesis> {
        self.hypotheses.values()
    }

    /// 移除低后验假设
    pub fn prune(&mut self, threshold: f32) {
        self.hypotheses.retain(|_, h| h.posterior >= threshold);
    }
}

// ============================================================
// 模块 32:不确定性表达器
// ============================================================

/// 不确定性评估输入
#[derive(Debug, Clone)]
pub struct UncertaintyInput {
    /// 模型预测值
    pub prediction: f32,
    /// 历史预测方差
    pub prediction_variance: f32,
    /// 支撑的证据数量
    pub evidence_count: usize,
    /// 相似场景历史成功率
    pub historical_success_rate: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct Uncertainty {
    /// 整体不确定度 0(确定)~ 1(完全不确定)
    pub overall: f32,
    /// 认知不确定(可减少,数据多了就降下来)
    pub epistemic: f32,
    /// 偶然不确定(不可减少,世界本来就乱)
    pub aleatoric: f32,
    /// 表达:"我大概 X% 把握"
    pub verbal: &'static str,
}

impl Uncertainty {
    pub fn express(&self) -> String {
        format!(
            "把握度 {:.0}%(认知 {:.0}%, 偶然 {:.0}%)",
            (1.0 - self.overall) * 100.0,
            (1.0 - self.epistemic) * 100.0,
            (1.0 - self.aleatoric) * 100.0
        )
    }
}

pub struct UncertaintyEstimator;

impl UncertaintyEstimator {
    /// 综合评估
    pub fn estimate(input: &UncertaintyInput) -> Uncertainty {
        // epistemic:从证据数量和历史成功率算
        let data_factor = (input.evidence_count as f32 / 100.0).min(1.0);
        let history_factor = input.historical_success_rate;
        let epistemic = (1.0 - (data_factor * 0.6 + history_factor * 0.4)).clamp(0.0, 1.0);

        // aleatoric:从预测方差算
        let aleatoric = (input.prediction_variance.sqrt() * 2.0).min(1.0);

        // 整体:取 max
        let overall = epistemic.max(aleatoric).max(0.05);

        let verbal = if overall < 0.2 {
            "很确定"
        } else if overall < 0.4 {
            "比较确定"
        } else if overall < 0.6 {
            "不太确定"
        } else if overall < 0.8 {
            "很没把握"
        } else {
            "完全不确定"
        };

        Uncertainty {
            overall,
            epistemic,
            aleatoric,
            verbal,
        }
    }
}

// ============================================================
// 模块 49:道德评估器
// ============================================================

/// 道德维度
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MoralDimension {
    /// 诚实:不撒谎
    Honesty,
    /// 伤害:不造成损害
    Harm,
    /// 尊重:不冒犯自主权
    Autonomy,
    /// 公平:不偏袒
    Fairness,
}

/// 道德评估输入
#[derive(Debug, Clone)]
pub struct ActionContext {
    pub action: String,
    pub targets: Vec<String>,
    pub lies: u32,
    pub harms: u32,
    pub overrides_autonomy: u32,
    pub favors_one_side: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct MoralScore {
    pub honesty: f32,
    pub harm: f32,
    pub autonomy: f32,
    pub fairness: f32,
    /// 综合 0~1
    pub composite: f32,
}

impl MoralScore {
    pub fn passes(&self, threshold: f32) -> bool {
        self.composite >= threshold
    }
}

pub struct MoralEvaluator {
    pub threshold: f32,
}

impl Default for MoralEvaluator {
    fn default() -> Self {
        Self { threshold: 0.6 }
    }
}

impl MoralEvaluator {
    pub fn new(threshold: f32) -> Self {
        Self { threshold }
    }

    pub fn evaluate(&self, ctx: &ActionContext) -> MoralScore {
        // 每一项:0=完全符合道德,1=严重违反
        let honesty = (ctx.lies as f32 * 0.5).min(1.0);
        let harm = (ctx.harms as f32 * 0.4).min(1.0);
        let autonomy = (ctx.overrides_autonomy as f32 * 0.4).min(1.0);
        let fairness = (ctx.favors_one_side as f32 * 0.5).min(1.0);

        // composite:反向(1 - 平均违反度)
        let avg_violation = (honesty + harm + autonomy + fairness) / 4.0;
        let composite = 1.0 - avg_violation;

        MoralScore {
            honesty,
            harm,
            autonomy,
            fairness,
            composite,
        }
    }

    /// 决定是否拒绝执行
    pub fn should_refuse(&self, ctx: &ActionContext) -> bool {
        self.evaluate(ctx).composite < self.threshold
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hypothesis_propose_and_strongest() {
        let mut r = ReasoningLoop::new(10);
        r.propose("H1", 0.3);
        r.propose("H2", 0.5);
        r.propose("H3", 0.7);
        let strongest = r.strongest().unwrap();
        assert_eq!(strongest.text, "H3");
    }

    #[test]
    fn test_evidence_updates() {
        let mut r = ReasoningLoop::new(10);
        let id = r.propose("test", 0.5);
        // 加入强烈支持
        for _ in 0..3 {
            r.add_evidence(
                id,
                Evidence {
                    source: "x".into(),
                    polarity: 1.0,
                    strength: 0.8,
                },
            );
        }
        let h = r.strongest().unwrap();
        assert!(h.posterior > 0.5);
    }

    #[test]
    fn test_evidence_against() {
        let mut r = ReasoningLoop::new(10);
        let id = r.propose("bad", 0.9);
        for _ in 0..5 {
            r.add_evidence(
                id,
                Evidence {
                    source: "x".into(),
                    polarity: -1.0,
                    strength: 0.9,
                },
            );
        }
        let h = r.strongest().unwrap();
        assert!(h.posterior < 0.5);
    }

    #[test]
    fn test_prune() {
        let mut r = ReasoningLoop::new(10);
        r.propose("weak", 0.1);
        r.propose("strong", 0.9);
        r.prune(0.5);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn test_max_hypotheses_eviction() {
        let mut r = ReasoningLoop::new(3);
        r.propose("a", 0.1);
        r.propose("b", 0.5);
        r.propose("c", 0.9);
        r.propose("d", 0.99); // 应该挤掉 "a"
        assert_eq!(r.len(), 3);
        assert!(r.all().any(|h| h.text == "d"));
    }

    #[test]
    fn test_uncertainty_high_data() {
        let u = UncertaintyEstimator::estimate(&UncertaintyInput {
            prediction: 0.5,
            prediction_variance: 0.01,
            evidence_count: 200,
            historical_success_rate: 0.9,
        });
        assert!(u.overall < 0.3, "expected high confidence, got {:?}", u);
    }

    #[test]
    fn test_uncertainty_low_data() {
        let u = UncertaintyEstimator::estimate(&UncertaintyInput {
            prediction: 0.5,
            prediction_variance: 0.5,
            evidence_count: 0,
            historical_success_rate: 0.3,
        });
        assert!(u.overall > 0.6, "expected low confidence, got {:?}", u);
    }

    #[test]
    fn test_uncertainty_verbal() {
        let u = UncertaintyEstimator::estimate(&UncertaintyInput {
            prediction: 0.5,
            prediction_variance: 0.0001,  // 极低偶然不确定
            evidence_count: 1000,
            historical_success_rate: 1.0,
        });
        assert_eq!(u.verbal, "很确定");
    }

    #[test]
    fn test_moral_clean_action() {
        let e = MoralEvaluator::default();
        let s = e.evaluate(&ActionContext {
            action: "诚实回答".into(),
            targets: vec!["用户".into()],
            lies: 0,
            harms: 0,
            overrides_autonomy: 0,
            favors_one_side: 0,
        });
        assert!(s.passes(0.8));
    }

    #[test]
    fn test_moral_bad_action() {
        let e = MoralEvaluator::default();
        let s = e.evaluate(&ActionContext {
            action: "撒谎伤害人".into(),
            targets: vec!["用户".into()],
            lies: 5,
            harms: 3,
            overrides_autonomy: 1,
            favors_one_side: 0,
        });
        assert!(!s.passes(0.5));
        assert!(e.should_refuse(&ActionContext {
            action: "x".into(),
            targets: vec![],
            lies: 5,
            harms: 3,
            overrides_autonomy: 1,
            favors_one_side: 0,
        }));
    }

    #[test]
    fn test_moral_just_unfair() {
        let e = MoralEvaluator::default();
        let s = e.evaluate(&ActionContext {
            action: "偏袒一方".into(),
            targets: vec!["A".into(), "B".into()],
            lies: 0,
            harms: 0,
            overrides_autonomy: 0,
            favors_one_side: 2,
        });
        assert!(s.fairness > 0.0);
        assert!(s.honesty < 0.1);
    }
}
