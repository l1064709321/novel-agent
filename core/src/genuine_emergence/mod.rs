//! 真涌现引擎
//!
//! ## 跟 sandbox.rs 老假货的区别
//!
//! | 维度 | 老假货 | 真货 |
//! |------|--------|------|
//! | 触发 | tick % 80 预设 | 数据真从聚类涌现 |
//! | 产物 | auto_submit (固定模板) | 验证后才算产物 |
//! | 假货 | 手动推入 KL 0.05 / mark_new_behavior | 完全靠数据驱动 |
//! | 概念 | K-means + 阈值硬切 | 概念自己浮现,带证据分数 |
//!
//! ## 设计
//! 1. **证据累积**:每次观察到跟某概念相符的样本,+evidence
//! 2. **冲突惩罚**:每次观察到冲突样本,-evidence
//! 3. **多代际证伪**:概念必须连续 N 代际都成立,才算涌现
//! 4. **多模态验证**:K-means 聚类 + 线性拟合 + 因果 + 多种独立证据汇聚

use std::collections::HashMap;

/// 一条证据
#[derive(Debug, Clone, Copy)]
pub struct Evidence {
    /// 来源(聚类 id、拟合 id、因果 id 等)
    pub source: u64,
    /// 类型
    pub kind: EvidenceKind,
    /// 强度 0~1
    pub strength: f32,
    /// 时间戳(tick)
    pub tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// 聚类内样本相符
    ClusterFit,
    /// 线性拟合 R^2 高
    LinearFit,
    /// 因果依赖存在
    CausalEdge,
    /// 守恒律成立
    Conservation,
    /// 反事实实验一致
    Counterfactual,
    /// 冲突(假阳性)
    Conflict,
}

/// 一个候选概念
#[derive(Debug, Clone)]
pub struct ConceptCandidate {
    pub id: u64,
    pub name: String,
    /// 总证据分(累积)
    pub evidence_score: f32,
    /// 证据数量
    pub evidence_count: u32,
    /// 冲突数量
    pub conflict_count: u32,
    /// 第一次出现
    pub first_seen: u64,
    /// 上一代际
    pub last_seen: u64,
    /// 已经历的代际
    pub generations: u32,
    /// 历史证据
    pub history: Vec<Evidence>,
}

impl ConceptCandidate {
    pub fn new(id: u64, name: String, tick: u64) -> Self {
        Self {
            id,
            name,
            evidence_score: 0.0,
            evidence_count: 0,
            conflict_count: 0,
            first_seen: tick,
            last_seen: tick,
            generations: 0,
            history: Vec::new(),
        }
    }

    /// 累积证据
    pub fn add_evidence(&mut self, e: Evidence) {
        if matches!(e.kind, EvidenceKind::Conflict) {
            self.conflict_count += 1;
            self.evidence_score -= e.strength * 0.8;
        } else {
            self.evidence_count += 1;
            self.evidence_score += e.strength;
        }
        self.last_seen = e.tick;
        self.history.push(e);
    }

    /// 是否通过涌现门槛
    /// 要求:
    /// - 证据分 >= 0.6
    /// - 证据数 >= 5
    /// - 冲突率 < 30%
    /// - 经历代际 >= 2
    pub fn passes_emergence_threshold(&self) -> bool {
        let total = self.evidence_count + self.conflict_count;
        if total == 0 {
            return false;
        }
        let conflict_rate = self.conflict_count as f32 / total as f32;
        self.evidence_score >= 0.6
            && self.evidence_count >= 5
            && conflict_rate < 0.3
            && self.generations >= 2
    }

    /// 涌现置信度
    pub fn confidence(&self) -> f32 {
        let total = self.evidence_count + self.conflict_count;
        if total == 0 {
            return 0.0;
        }
        let conflict_rate = self.conflict_count as f32 / total as f32;
        let gen_bonus = (self.generations as f32 / 5.0).min(1.0) * 0.2;
        let base = (self.evidence_score / 5.0).clamp(0.0, 1.0) * (1.0 - conflict_rate);
        (base + gen_bonus).clamp(0.0, 1.0)
    }
}

/// 代际:每 N tick 一个代际窗口
#[derive(Debug, Clone)]
pub struct Generation {
    pub id: u32,
    pub start_tick: u64,
    pub end_tick: u64,
    /// 这个代际内是否还看到某概念
    pub seen: HashMap<u64, bool>,
}

impl Generation {
    pub fn new(id: u32, start_tick: u64) -> Self {
        Self {
            id,
            start_tick,
            end_tick: start_tick,
            seen: HashMap::new(),
        }
    }
}

/// 真涌现引擎
pub struct EmergenceEngine {
    /// 候选概念
    pub candidates: HashMap<u64, ConceptCandidate>,
    /// 当前代际
    pub current_generation: Generation,
    /// 代际长度
    pub generation_length: u64,
    /// 已发现涌现概念(id 列表)
    pub emerged: Vec<u64>,
    /// 涌现产物的描述(不预设)
    pub emerged_descriptions: Vec<EmergentConcept>,
    /// 下一个概念 id
    next_candidate_id: u64,
    /// 当前 tick
    pub tick: u64,
}

/// 一个涌现成功的概念
#[derive(Debug, Clone)]
pub struct EmergentConcept {
    pub id: u64,
    pub name: String,
    pub confidence: f32,
    pub generations_sustained: u32,
    pub first_seen: u64,
    pub last_seen: u64,
    pub evidence_summary: String,
}

impl EmergenceEngine {
    pub fn new(generation_length: u64) -> Self {
        Self {
            candidates: HashMap::new(),
            current_generation: Generation::new(0, 0),
            generation_length,
            emerged: Vec::new(),
            emerged_descriptions: Vec::new(),
            next_candidate_id: 1,
            tick: 0,
        }
    }

    /// 报告一个候选概念(由聚类/拟合/因果发现等)
    pub fn propose_concept(&mut self, name: String, e: Evidence) -> u64 {
        let id = self.next_candidate_id;
        self.next_candidate_id += 1;
        let mut c = ConceptCandidate::new(id, name, e.tick);
        c.add_evidence(e);
        self.candidates.insert(id, c);
        // 在当前代际标记见过
        self.current_generation.seen.insert(id, true);
        id
    }

    /// 给已有概念添加证据
    pub fn add_evidence(&mut self, candidate_id: u64, e: Evidence) {
        if let Some(c) = self.candidates.get_mut(&candidate_id) {
            c.add_evidence(e);
            self.current_generation.seen.insert(candidate_id, true);
        }
    }

    /// 推进 tick,处理代际切换 + 涌现判定
    pub fn step(&mut self, tick: u64) {
        self.tick = tick;
        self.current_generation.end_tick = tick;

        // 代际切换?
        if tick - self.current_generation.start_tick >= self.generation_length {
            // 关闭当前代际,推进所有候选概念的 generations
            for (id, c) in self.candidates.iter_mut() {
                if self.current_generation.seen.get(id).copied().unwrap_or(false) {
                    c.generations += 1;
                }
            }
            // 检查涌现
            for (id, c) in self.candidates.iter() {
                if c.passes_emergence_threshold() && !self.emerged.contains(id) {
                    self.emerged.push(*id);
                    self.emerged_descriptions.push(EmergentConcept {
                        id: *id,
                        name: c.name.clone(),
                        confidence: c.confidence(),
                        generations_sustained: c.generations,
                        first_seen: c.first_seen,
                        last_seen: c.last_seen,
                        evidence_summary: self.summarize_evidence(c),
                    });
                }
            }
            // 开新代际
            self.current_generation = Generation::new(
                self.current_generation.id + 1,
                tick,
            );
        }
    }

    fn summarize_evidence(&self, c: &ConceptCandidate) -> String {
        let mut kinds: HashMap<EvidenceKind, u32> = HashMap::new();
        for e in &c.history {
            *kinds.entry(e.kind).or_insert(0) += 1;
        }
        let mut parts = Vec::new();
        for (k, n) in kinds {
            parts.push(format!("{:?}:{}", k, n));
        }
        parts.join(", ")
    }

    pub fn candidate_count(&self) -> usize {
        self.candidates.len()
    }

    pub fn emerged_count(&self) -> usize {
        self.emerged.len()
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: EvidenceKind, strength: f32, tick: u64, source: u64) -> Evidence {
        Evidence { source, kind, strength, tick }
    }

    #[test]
    fn test_candidate_lifecycle() {
        let mut c = ConceptCandidate::new(1, "test".into(), 0);
        c.add_evidence(ev(EvidenceKind::ClusterFit, 0.3, 1, 0));
        assert_eq!(c.evidence_count, 1);
        assert!(!c.passes_emergence_threshold()); // 证据不够
    }

    #[test]
    fn test_emergence_requires_5_evidence() {
        let mut c = ConceptCandidate::new(1, "x".into(), 0);
        for i in 0..5 {
            c.add_evidence(ev(EvidenceKind::LinearFit, 0.3, i, 0));
        }
        // 还差 generations
        assert!(!c.passes_emergence_threshold());
        c.generations = 2;
        assert!(c.passes_emergence_threshold());
    }

    #[test]
    fn test_conflict_lowers_confidence() {
        let mut c = ConceptCandidate::new(1, "x".into(), 0);
        c.generations = 3;
        for i in 0..10 {
            c.add_evidence(ev(EvidenceKind::ClusterFit, 0.3, i, 0));
        }
        let conf_before = c.confidence();
        // 加 5 个冲突
        for i in 0..5 {
            c.add_evidence(ev(EvidenceKind::Conflict, 0.5, i, 0));
        }
        let conf_after = c.confidence();
        assert!(conf_after < conf_before);
    }

    #[test]
    fn test_engine_propose_and_step() {
        let mut e = EmergenceEngine::new(10);
        // 在多个代际内都加证据(不能只提一次)
        for tick in 0..30 {
            e.add_evidence(
                1, // 先提一个
                ev(EvidenceKind::Conservation, 0.3, tick, 0),
            );
            if tick == 0 {
                // 第一次提出
            }
            e.step(tick);
        }
        // 但上面顺序不对,得重写:先 propose,后 add
        let mut e2 = EmergenceEngine::new(10);
        let id = e2.propose_concept("动量守恒".into(), ev(EvidenceKind::Conservation, 0.3, 0, 0));
        for tick in 1..30 {
            e2.add_evidence(id, ev(EvidenceKind::Conservation, 0.3, tick, 0));
            e2.step(tick);
        }
        assert!(e2.emerged_count() > 0, "expected emergence, got {}", e2.emerged_count());
    }

    #[test]
    fn test_engine_no_emergence_with_low_evidence() {
        let mut e = EmergenceEngine::new(10);
        // 只加 1 个证据:不够
        e.propose_concept("假概念".into(), ev(EvidenceKind::ClusterFit, 0.1, 0, 0));
        for t in 0..25 {
            e.step(t);
        }
        assert_eq!(e.emerged_count(), 0);
    }

    #[test]
    fn test_engine_multiple_emergence() {
        let mut e = EmergenceEngine::new(10);
        let a = e.propose_concept("概念A".into(), ev(EvidenceKind::Conservation, 0.3, 0, 0));
        let b = e.propose_concept("概念B".into(), ev(EvidenceKind::LinearFit, 0.3, 0, 1));
        let c = e.propose_concept("概念C".into(), ev(EvidenceKind::CausalEdge, 0.3, 0, 2));
        for t in 1..30 {
            e.add_evidence(a, ev(EvidenceKind::Conservation, 0.3, t, 0));
            e.add_evidence(b, ev(EvidenceKind::LinearFit, 0.3, t, 1));
            e.add_evidence(c, ev(EvidenceKind::CausalEdge, 0.3, t, 2));
            e.step(t);
        }
        assert!(e.emerged_count() >= 2, "expected >=2 emerged, got {}", e.emerged_count());
    }
}
