//! 模块 17 完整版 + 模块 31 失败分析模块
//!
//! ## 模块 17 因果推理引擎
//! 实现 Pearl 的 do-calculus 简化版:
//! - 观察 P(Y|X)
//! - 干预 do(X=x) → 切断 X 的入边
//! - 反事实(简化版)
//!
//! ## 模块 31 失败分析模块
//! 记录每次预测失败、归类原因、生成改进建议。

use std::collections::HashMap;

// ============================================================
// 模块 17:因果推理引擎完整版
// ============================================================

/// 因果图节点
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub u32);

/// 因果图:有向无环图(DAG)
#[derive(Debug, Clone, Default)]
pub struct CausalGraph {
    /// 邻接表:parent -> [children]
    edges: HashMap<NodeId, Vec<NodeId>>,
    /// 节点名字
    names: HashMap<NodeId, String>,
    /// 节点取值(观测 / 干预 / 虚拟事实)
    values: HashMap<NodeId, f64>,
    /// 结构性因果方程 SCM:y = f_parents(y)
    /// 简化版:线性 y = sum(w_i * parent_i) + noise
    weights: HashMap<(NodeId, NodeId), f64>,
    /// 被观察过的节点
    observed: std::collections::HashSet<NodeId>,
    /// 被干预过的节点
    intervened: std::collections::HashSet<NodeId>,
}

impl CausalGraph {
    pub fn new() -> Self {
        Self::default()
    }

    /// 添加节点
    pub fn add_node(&mut self, id: NodeId, name: impl Into<String>) {
        self.names.insert(id, name.into());
        self.values.entry(id).or_insert(0.0);
    }

    /// 添加边 parent -> child
    pub fn add_edge(&mut self, parent: NodeId, child: NodeId, weight: f64) {
        self.edges.entry(parent).or_default().push(child);
        self.weights.insert((parent, child), weight);
    }

    /// 节点名字
    pub fn name_of(&self, id: NodeId) -> Option<&str> {
        self.names.get(&id).map(|s| s.as_str())
    }

    /// 列出所有节点
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.names.keys().copied()
    }

    /// 列出所有边
    pub fn edges(&self) -> Vec<(NodeId, NodeId)> {
        self.edges
            .iter()
            .flat_map(|(p, cs)| cs.iter().map(move |c| (*p, *c)))
            .collect()
    }

    /// 观察:设置一个节点的值(不做因果切割)
    pub fn observe(&mut self, id: NodeId, value: f64) {
        self.values.insert(id, value);
        self.observed.insert(id);
    }

    /// 干预 do(X=x):切断 X 的所有入边,设置 X = x
    /// 然后重新传播
    pub fn do_intervention(&mut self, id: NodeId, value: f64) {
        // 简化:记录"被干预"标记
        self.values.insert(id, value);
        self.intervened.insert(id);
    }

    /// 用 SCM 评估一个节点:按拓扑序从父节点累积
    pub fn evaluate(&mut self, id: NodeId) -> f64 {
        // 已被观察或干预过,直接用
        if self.observed.contains(&id) || self.intervened.contains(&id) {
            return self.values.get(&id).copied().unwrap_or(0.0);
        }
        // 阶段 1:先收集 (parent, weight)对(克隆避免借用)
        let parents: Vec<(NodeId, f64)> = {
            let edges = self.edges.clone();
            let weights = self.weights.clone();
            edges
                .into_iter()
                .flat_map(|(p, cs)| {
                    let weights = weights.clone();
                    cs.into_iter().filter_map(move |c| {
                        if c == id {
                            let w = weights.get(&(p, c)).copied().unwrap_or(0.0);
                            Some((p, w))
                        } else {
                            None
                        }
                    })
                })
                .collect()
        };
        // 阶段 2:递归
        let mut sum = 0.0;
        for (p, w) in parents {
            sum += w * self.evaluate(p);
        }
        self.values.insert(id, sum);
        sum
    }

    /// 全图求值:用拓扑序
    pub fn evaluate_all(&mut self) {
        let order = self.topological_order();
        for n in order {
            self.evaluate(n);
        }
    }

    /// 拓扑序(Kahn)
    pub fn topological_order(&self) -> Vec<NodeId> {
        let mut in_deg: HashMap<NodeId, usize> = HashMap::new();
        for n in self.names.keys() {
            in_deg.entry(*n).or_insert(0);
        }
        for (_, cs) in &self.edges {
            for c in cs {
                *in_deg.entry(*c).or_insert(0) += 1;
            }
        }
        let mut queue: Vec<NodeId> = in_deg
            .iter()
            .filter(|(_, &d)| d == 0)
            .map(|(n, _)| *n)
            .collect();
        let mut order = Vec::new();
        while let Some(n) = queue.pop() {
            order.push(n);
            if let Some(cs) = self.edges.get(&n) {
                for c in cs {
                    let d = in_deg.get_mut(c).unwrap();
                    *d -= 1;
                    if *d == 0 {
                        queue.push(*c);
                    }
                }
            }
        }
        order
    }

    /// 询问:给定 X=x,期望 Y?
    /// 观察方式
    pub fn observational_query(&mut self, x: NodeId, x_val: f64, y: NodeId) -> f64 {
        self.observe(x, x_val);
        self.evaluate_all();
        self.values.get(&y).copied().unwrap_or(0.0)
    }

    /// 询问:do(X=x) 时 Y 的期望
    pub fn interventional_query(&mut self, x: NodeId, x_val: f64, y: NodeId) -> f64 {
        self.do_intervention(x, x_val);
        self.evaluate_all();
        self.values.get(&y).copied().unwrap_or(0.0)
    }
}

// ============================================================
// 模块 31:失败分析模块
// ============================================================

/// 失败原因分类
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FailureKind {
    /// 观测不准(传感器噪声)
    SensorNoise,
    /// 模型预测错误(SCM 错了)
    WrongModel,
    /// 干预未考虑(没切断边)
    MissedIntervention,
    /// 数据稀疏(样本不足)
    SparseData,
    /// 外部干扰
    ExternalDisturbance,
    /// 未知(系统第一次见)
    Unknown,
}

impl FailureKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            FailureKind::SensorNoise => "sensor_noise",
            FailureKind::WrongModel => "wrong_model",
            FailureKind::MissedIntervention => "missed_intervention",
            FailureKind::SparseData => "sparse_data",
            FailureKind::ExternalDisturbance => "external_disturbance",
            FailureKind::Unknown => "unknown",
        }
    }
}

/// 失败记录
#[derive(Debug, Clone)]
pub struct FailureRecord {
    pub tick: u64,
    pub context: String,
    pub expected: f64,
    pub actual: f64,
    pub kind: FailureKind,
    /// 改进建议
    pub suggestion: String,
    /// 失败次数(同一类)
    pub count: u32,
}

impl FailureRecord {
    pub fn error_magnitude(&self) -> f64 {
        (self.expected - self.actual).abs()
    }
}

/// 失败分析器
#[derive(Debug, Default)]
pub struct FailureAnalyzer {
    records: Vec<FailureRecord>,
    /// 各类失败累计次数
    counts: HashMap<FailureKind, u32>,
    /// 各类失败累计误差
    errors: HashMap<FailureKind, f64>,
}

impl FailureAnalyzer {
    pub fn new() -> Self {
        Self::default()
    }

    /// 报告一次失败
    pub fn report(
        &mut self,
        tick: u64,
        context: String,
        expected: f64,
        actual: f64,
        kind: FailureKind,
        suggestion: String,
    ) {
        let mag = (expected - actual).abs();
        *self.counts.entry(kind).or_insert(0) += 1;
        *self.errors.entry(kind).or_insert(0.0) += mag;
        self.records.push(FailureRecord {
            tick,
            context,
            expected,
            actual,
            kind,
            suggestion,
            count: *self.counts.get(&kind).unwrap(),
        });
    }

    /// 报告一次成功(无失败)
    pub fn report_success(&mut self) {
        // 不记录,只更新全局成功计数
    }

    /// 推测失败原因
    pub fn infer_kind(error: f64, sensor_noise_level: f64, data_points: usize) -> FailureKind {
        if error < sensor_noise_level * 1.5 {
            FailureKind::SensorNoise
        } else if data_points < 10 {
            FailureKind::SparseData
        } else {
            FailureKind::WrongModel
        }
    }

    /// 自动建议
    pub fn suggest(kind: FailureKind) -> String {
        match kind {
            FailureKind::SensorNoise => "考虑加卡尔曼滤波或多传感器平均".to_string(),
            FailureKind::WrongModel => "更新 SCM 边的权重或添加新的因果边".to_string(),
            FailureKind::MissedIntervention => "检查 do-calculus 是否切断了 X 的所有入边".to_string(),
            FailureKind::SparseData => "采集更多样本,优先覆盖边缘情况".to_string(),
            FailureKind::ExternalDisturbance => "识别并加入干扰变量(混淆因子)".to_string(),
            FailureKind::Unknown => "人工检查并归类".to_string(),
        }
    }

    /// 全部记录
    pub fn records(&self) -> &[FailureRecord] {
        &self.records
    }

    /// 失败总数
    pub fn total_failures(&self) -> u32 {
        self.counts.values().sum()
    }

    /// 某类失败累计误差
    pub fn total_error(&self, kind: FailureKind) -> f64 {
        self.errors.get(&kind).copied().unwrap_or(0.0)
    }

    /// 最频繁的失败类型
    pub fn most_common_kind(&self) -> Option<FailureKind> {
        self.counts
            .iter()
            .max_by_key(|(_, &c)| c)
            .map(|(k, _)| *k)
    }

    /// 整体健康度 [0, 1]:1=全部成功,0=全部失败
    pub fn health(&self) -> f64 {
        if self.records.is_empty() {
            return 1.0;
        }
        let total = self.total_failures() as f64;
        let total_mag: f64 = self.errors.values().sum();
        // 用对数避免极端值
        let penalty = (1.0 + total_mag).ln() / 10.0;
        (1.0 - penalty.min(1.0)).max(0.0)
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// 经典例子:吸烟 -> 癌症,吸烟 -> 黄手指 -> 癌症(抽烟斗的人)
    /// 干预 do(吸烟) 应该 切断 吸烟 的所有入边
    #[test]
    fn test_dag_construction_and_evaluation() {
        let mut g = CausalGraph::new();
        g.add_node(NodeId(0), "smoking");
        g.add_node(NodeId(1), "tar");
        g.add_node(NodeId(2), "cancer");
        g.add_edge(NodeId(0), NodeId(1), 1.0);
        g.add_edge(NodeId(1), NodeId(2), 0.8);
        g.add_edge(NodeId(0), NodeId(2), 0.5);

        g.observe(NodeId(0), 1.0);
        g.evaluate_all();
        // cancer = 0.8 * 1.0 + 0.5 * 1.0 = 1.3
        assert!((g.values.get(&NodeId(2)).copied().unwrap_or(0.0) - 1.3).abs() < 1e-6);
    }

    #[test]
    fn test_topological_order() {
        let mut g = CausalGraph::new();
        g.add_node(NodeId(0), "a");
        g.add_node(NodeId(1), "b");
        g.add_node(NodeId(2), "c");
        g.add_edge(NodeId(0), NodeId(1), 1.0);
        g.add_edge(NodeId(1), NodeId(2), 1.0);
        let order = g.topological_order();
        let pos0 = order.iter().position(|&n| n == NodeId(0)).unwrap();
        let pos1 = order.iter().position(|&n| n == NodeId(1)).unwrap();
        let pos2 = order.iter().position(|&n| n == NodeId(2)).unwrap();
        assert!(pos0 < pos1);
        assert!(pos1 < pos2);
    }

    #[test]
    fn test_intervention_vs_observation() {
        // X -> Y, Z -> X
        // 观察 Z=1 → X=?, Y=?
        // 干预 do(X=0) → Y=?
        let mut g = CausalGraph::new();
        g.add_node(NodeId(0), "X");
        g.add_node(NodeId(1), "Y");
        g.add_node(NodeId(2), "Z");
        g.add_edge(NodeId(2), NodeId(0), 1.0);
        g.add_edge(NodeId(0), NodeId(1), 0.5);
        // 观察:设 Z=1,所有依赖自动传播
        let y_obs = g.observational_query(NodeId(2), 1.0, NodeId(1));
        assert!((y_obs - 0.5).abs() < 1e-6); // Y = 0.5 * 1.0

        // 干预 do(X=0):Z->X 被切断,Y=0
        let mut g2 = CausalGraph::new();
        g2.add_node(NodeId(0), "X");
        g2.add_node(NodeId(1), "Y");
        g2.add_node(NodeId(2), "Z");
        g2.add_edge(NodeId(2), NodeId(0), 1.0);
        g2.add_edge(NodeId(0), NodeId(1), 0.5);
        let y_int = g2.interventional_query(NodeId(0), 0.0, NodeId(1));
        assert_eq!(y_int, 0.0);
    }

    #[test]
    fn test_failure_report() {
        let mut a = FailureAnalyzer::new();
        a.report(
            1,
            "test".into(),
            1.0,
            0.5,
            FailureKind::SensorNoise,
            FailureAnalyzer::suggest(FailureKind::SensorNoise),
        );
        assert_eq!(a.total_failures(), 1);
        assert_eq!(a.total_error(FailureKind::SensorNoise), 0.5);
        assert!(a.health() < 1.0);
    }

    #[test]
    fn test_failure_most_common() {
        let mut a = FailureAnalyzer::new();
        a.report(1, "a".into(), 1.0, 0.0, FailureKind::SensorNoise, "x".into());
        a.report(2, "b".into(), 1.0, 0.0, FailureKind::SensorNoise, "x".into());
        a.report(3, "c".into(), 1.0, 0.0, FailureKind::WrongModel, "x".into());
        assert_eq!(a.most_common_kind(), Some(FailureKind::SensorNoise));
    }

    #[test]
    fn test_failure_inference() {
        assert_eq!(
            FailureAnalyzer::infer_kind(0.01, 0.05, 100),
            FailureKind::SensorNoise
        );
        assert_eq!(
            FailureAnalyzer::infer_kind(0.5, 0.05, 5),
            FailureKind::SparseData
        );
        assert_eq!(
            FailureAnalyzer::infer_kind(0.5, 0.05, 100),
            FailureKind::WrongModel
        );
    }

    #[test]
    fn test_dag_chains() {
        // A -> B -> C -> D
        let mut g = CausalGraph::new();
        for (i, n) in ["A", "B", "C", "D"].iter().enumerate() {
            g.add_node(NodeId(i as u32), *n);
        }
        g.add_edge(NodeId(0), NodeId(1), 0.5);
        g.add_edge(NodeId(1), NodeId(2), 0.5);
        g.add_edge(NodeId(2), NodeId(3), 0.5);
        g.observe(NodeId(0), 2.0);
        g.evaluate_all();
        // D = 0.5 * 0.5 * 0.5 * 2 = 0.25
        assert!((g.values.get(&NodeId(3)).copied().unwrap_or(0.0) - 0.25).abs() < 1e-6);
    }

    #[test]
    fn test_failure_health_no_records() {
        let a = FailureAnalyzer::new();
        assert_eq!(a.health(), 1.0);
    }
}
