//! 真自我反思模块
//!
//! ## 核心思想
//! AGI 必须能观察自己(自己的状态、自己的学习、自己的决策),并从中学习。
//! 这不是元认知(meta-cognition),是真正的"系统观察自己"的循环。
//!
//! ## 三个层次
//! 1. **状态反思**:我现在在做什么?什么状态?
//! 2. **学习反思**:我刚学到了什么?这个学习让我变好了吗?
//! 3. **策略反思**:我的策略有效吗?要不要换?

use std::collections::HashMap;

/// 自我观察记录
#[derive(Debug, Clone)]
pub struct SelfObservation {
    pub tick: u64,
    pub what_i_did: String,
    pub what_i_observed: String,
    pub what_i_learned: Option<String>,
    pub confidence_change: f32,
    /// 满意吗?(-1 不满意, 0 中性, 1 满意)
    pub satisfaction: f32,
}

/// 一个学习经验
#[derive(Debug, Clone)]
pub struct LearningEpisode {
    pub id: u64,
    pub domain: String,
    /// 尝试
    pub attempt: String,
    /// 结果
    pub outcome: String,
    /// 成功?
    pub success: bool,
    /// 错误(如果有)
    pub error: Option<String>,
    /// 修正
    pub correction: Option<String>,
    /// 时长(tick)
    pub duration_ticks: u64,
}

/// 自我反思器
pub struct SelfReflector {
    pub observations: Vec<SelfObservation>,
    pub episodes: Vec<LearningEpisode>,
    pub next_episode_id: u64,
    /// 策略评分
    pub strategy_scores: HashMap<String, f32>,
    /// 自我评估统计
    pub stats: ReflectionStats,
}

/// 反思统计
#[derive(Debug, Clone, Default)]
pub struct ReflectionStats {
    /// 总尝试数
    pub total_attempts: u32,
    /// 成功数
    pub successes: u32,
    /// 自我纠正次数
    pub self_corrections: u32,
    /// 平均满意度
    pub avg_satisfaction: f32,
}

impl SelfReflector {
    pub fn new() -> Self {
        let mut strategy_scores = HashMap::new();
        // 默认值 0.7(中等信任)
        Self {
            observations: Vec::new(),
            episodes: Vec::new(),
            next_episode_id: 1,
            strategy_scores,
            stats: ReflectionStats::default(),
        }
    }

    /// 初始化一个策略分数(如果需要预设)
    pub fn init_strategy(&mut self, name: &str, score: f32) {
        self.strategy_scores.entry(name.to_string()).or_insert(score);
    }

    /// 记录一次观察
    pub fn observe(
        &mut self,
        tick: u64,
        what_i_did: String,
        what_i_observed: String,
        what_i_learned: Option<String>,
        confidence_change: f32,
        satisfaction: f32,
    ) {
        self.observations.push(SelfObservation {
            tick,
            what_i_did,
            what_i_observed,
            what_i_learned,
            confidence_change,
            satisfaction,
        });
        // 更新平均满意度
        let n = self.observations.len() as f32;
        self.stats.avg_satisfaction =
            (self.stats.avg_satisfaction * (n - 1.0) + satisfaction) / n;
    }

    /// 记录一次学习事件
    pub fn record_episode(
        &mut self,
        domain: String,
        attempt: String,
        outcome: String,
        success: bool,
        error: Option<String>,
        correction: Option<String>,
        duration_ticks: u64,
    ) -> u64 {
        let id = self.next_episode_id;
        self.next_episode_id += 1;
        let has_correction = correction.is_some();

        self.episodes.push(LearningEpisode {
            id,
            domain,
            attempt,
            outcome,
            success,
            error,
            correction,
            duration_ticks,
        });

        self.stats.total_attempts += 1;
        if success {
            self.stats.successes += 1;
        }
        if has_correction {
            self.stats.self_corrections += 1;
        }

        id
    }

    /// 评估策略
    pub fn evaluate_strategy(&mut self, strategy: &str, score: f32) {
        self.init_strategy(strategy, 0.7);
        let entry = self.strategy_scores.get_mut(strategy).unwrap();
        // 指数移动平均
        *entry = *entry * 0.7 + score * 0.3;
    }

    /// 反思:从过去 N 次观察里找模式
    pub fn reflect(&self, window: usize) -> Reflection {
        let n = self.observations.len();
        // 修复:observations 为空时不直接返回,继续算 episodes 部分
        let (avg_satisfaction, avg_confidence, learned_count, most_common) = if n == 0 {
            (0.0, 0.0, 0, None)
        } else {
            let start = n.saturating_sub(window);
            let recent = &self.observations[start..];
            let avg_sat: f32 =
                recent.iter().map(|o| o.satisfaction).sum::<f32>() / recent.len() as f32;
            let avg_conf: f32 =
                recent.iter().map(|o| o.confidence_change).sum::<f32>() / recent.len() as f32;
            let learned = recent.iter().filter(|o| o.what_i_learned.is_some()).count();

            let mut what_counts: HashMap<String, u32> = HashMap::new();
            for o in recent {
                *what_counts.entry(o.what_i_did.clone()).or_insert(0) += 1;
            }
            let most_common = what_counts
                .iter()
                .max_by_key(|(_, &c)| c)
                .map(|(k, _)| k.clone());
            (avg_sat, avg_conf, learned, most_common)
        };

        // 找成功率最低的领域(总执行,与 observations 无关)
        let mut domain_success: HashMap<String, (u32, u32)> = HashMap::new();
        for e in &self.episodes {
            let entry = domain_success.entry(e.domain.clone()).or_insert((0, 0));
            entry.1 += 1;
            if e.success {
                entry.0 += 1;
            }
        }
        let mut stats: Vec<(String, f32)> = domain_success
            .iter()
            .filter(|(_, (_, total))| *total >= 2)
            .map(|(k, (s, t))| (k.clone(), *s as f32 / *t as f32))
            .collect();
        stats.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let worst_domain = stats.first().map(|(k, _)| k.clone());

        Reflection {
            avg_satisfaction,
            avg_confidence_change: avg_confidence,
            learned_count: learned_count as u32,
            most_common_action: most_common,
            worst_domain,
            episode_count: self.episodes.len() as u32,
        }
    }

    /// 建议:基于反思给一个改进建议
    pub fn suggest(&self) -> String {
        let r = self.reflect(20);
        if r.avg_satisfaction < -0.3 {
            return "我最近表现不好,需要重新评估策略.".to_string();
        }
        if r.avg_confidence_change < -0.1 {
            return "我的置信度在下降,可能遇到了新问题,需要更多数据.".to_string();
        }
        if let Some(domain) = &r.worst_domain {
            return format!("我在「{}」领域表现最差,应该重点改进.", domain);
        }
        if r.learned_count < 3 {
            return "我学到的东西太少,需要更多探索.".to_string();
        }
        "我表现正常,继续.".to_string()
    }
}

impl Default for SelfReflector {
    fn default() -> Self {
        Self::new()
    }
}

/// 反思结果
#[derive(Debug, Clone, Default)]
pub struct Reflection {
    pub avg_satisfaction: f32,
    pub avg_confidence_change: f32,
    pub learned_count: u32,
    pub most_common_action: Option<String>,
    pub worst_domain: Option<String>,
    pub episode_count: u32,
}

impl Reflection {
    pub fn describe(&self) -> String {
        format!(
            "近况: 平均满意度 {:.2}, 置信度变化 {:+.2}, 学到 {} 次, 最常做:{:?}, 最差领域:{:?}",
            self.avg_satisfaction,
            self.avg_confidence_change,
            self.learned_count,
            self.most_common_action,
            self.worst_domain
        )
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_observe_records() {
        let mut r = SelfReflector::new();
        r.observe(1, "做X".into(), "看到Y".into(), Some("学到Z".into()), 0.1, 0.5);
        assert_eq!(r.observations.len(), 1);
        assert!(r.stats.avg_satisfaction > 0.0);
    }

    #[test]
    fn test_record_episode() {
        let mut r = SelfReflector::new();
        let id = r.record_episode(
            "动量守恒".into(),
            "猜测 p=mv".into(),
            "R²=0.99".into(),
            true,
            None,
            None,
            10,
        );
        assert_eq!(id, 1);
        assert_eq!(r.stats.successes, 1);
        assert_eq!(r.stats.total_attempts, 1);
    }

    #[test]
    fn test_strategy_scoring() {
        let mut r = SelfReflector::new();
        r.evaluate_strategy("最小二乘", 0.9);
        r.evaluate_strategy("最小二乘", 0.8);
        let s = r.strategy_scores.get("最小二乘").unwrap();
        // EMA:0.7*0.7 + 0.9*0.3 = 0.76,然后 0.76*0.7 + 0.8*0.3 = 0.772
        assert!(*s > 0.7 && *s < 0.9, "s = {}", s);
    }

    #[test]
    fn test_reflect_finds_worst_domain() {
        let mut r = SelfReflector::new();
        for i in 0..5 {
            r.record_episode("动量".into(), format!("a{}", i), "ok".into(), true, None, None, 1);
        }
        for i in 0..5 {
            r.record_episode(
                "能量".into(),
                format!("b{}", i),
                "fail".into(),
                false,
                Some("wrong formula".into()),
                Some("fix".into()),
                2,
            );
        }
        let refl = r.reflect(100);
        assert_eq!(refl.worst_domain, Some("能量".into()));
        assert_eq!(r.stats.successes, 5);
    }

    #[test]
    fn test_suggest_low_satisfaction() {
        let mut r = SelfReflector::new();
        for i in 0..10 {
            r.observe(i, "做X".into(), "看到坏".into(), None, -0.2, -0.5);
        }
        let s = r.suggest();
        assert!(s.contains("不好") || s.contains("差") || s.contains("下降"));
    }

    #[test]
    fn test_suggest_normal() {
        let mut r = SelfReflector::new();
        for i in 0..10 {
            r.observe(
                i,
                "做X".into(),
                "看到好".into(),
                Some("学到了".into()),
                0.1,
                0.3,
            );
        }
        let s = r.suggest();
        // 正常情况,可能给一个继续的建议
        assert!(!s.is_empty());
    }

    #[test]
    fn test_reflection_stats() {
        let mut r = SelfReflector::new();
        assert_eq!(r.stats.total_attempts, 0);
        r.record_episode("a".into(), "a1".into(), "ok".into(), true, None, None, 1);
        r.record_episode(
            "a".into(),
            "a2".into(),
            "fail".into(),
            false,
            Some("err".into()),
            Some("fix".into()),
            1,
        );
        assert_eq!(r.stats.total_attempts, 2);
        assert_eq!(r.stats.successes, 1);
        assert_eq!(r.stats.self_corrections, 1);
    }
}
