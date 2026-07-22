//! 因果发现:PC 算法(简化版)
//!
//! 经典 PC 算法:
//! 1. Start:完全图(所有变量两两相连)
//! 2. 条件独立检验:对每对变量 X-Y,找条件集 S,使得 X⊥Y|S
//! 3. 删除边:如果条件独立,删 X-Y
//! 4. 定向:用 v-structure 找方向
//!
//! 简化:不做条件集枚举,用偏相关做局部检验
//!
//! **注意:这是简化版,不是真 PC 算法**,但能给出"哪些变量相关"的粗略估计。

use serde::{Deserialize, Serialize};

/// 因果图节点
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalNode {
    pub id: usize,
    pub name: String,
}

/// 因果图边
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct CausalEdge {
    pub from: usize,
    pub to: usize,
    /// 边的强度 0~1
    pub strength: f32,
}

/// 因果图(DAG,有向无环图)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CausalGraph {
    pub nodes: Vec<CausalNode>,
    pub edges: Vec<CausalEdge>,
}

/// 观测样本:多个变量在多个时间点的值
#[derive(Debug, Clone)]
pub struct Observation {
    pub timestamp: u64,
    pub values: Vec<f32>,
}

/// 因果发现器
pub struct CausalDiscoverer {
    /// 节点(变量)定义
    nodes: Vec<CausalNode>,
    /// 观测历史
    observations: Vec<Observation>,
    /// 当前图
    graph: CausalGraph,
    /// 历史因果边数(用于"因果图简化"信号)
    edge_count_history: Vec<usize>,
    max_history: usize,
}

impl CausalDiscoverer {
    pub fn new(nodes: Vec<CausalNode>) -> Self {
        let graph = CausalGraph {
            nodes: nodes.clone(),
            edges: Vec::new(),
        };
        Self {
            nodes,
            observations: Vec::new(),
            graph,
            edge_count_history: Vec::new(),
            max_history: 100,
        }
    }

    /// 添加一次观测
    pub fn add_observation(&mut self, obs: Observation) {
        if obs.values.len() != self.nodes.len() {
            return;  // 维度不匹配
        }
        self.observations.push(obs);
    }

    /// 重训练:跑 PC 算法(简化版)
    pub fn retrain(&mut self) {
        if self.observations.len() < 10 {
            return;  // 样本不够
        }

        // 1. 计算两两相关矩阵
        let n = self.nodes.len();
        let corr_matrix = self.compute_correlation_matrix();

        // 2. 简化版:阈值过滤
        //    如果 |corr(X,Y)| < 0.3,认为条件独立,删边
        //    否则保留(无向)边
        const THRESHOLD: f32 = 0.3;

        let mut new_edges = Vec::new();
        for i in 0..n {
            for j in (i + 1)..n {
                let corr = corr_matrix[(i, j)];
                if corr.abs() > THRESHOLD {
                    new_edges.push(CausalEdge {
                        from: i,
                        to: j,
                        strength: corr.abs(),
                    });
                }
            }
        }

        // 3. 简化定向:用时序因果(谁先变)
        //    如果 X(t-1) → Y(t) 显著,定向为 X → Y
        //    否则保持原样
        for edge in &mut new_edges {
            let inferred_from = self.infer_direction(edge.from, edge.to);
            if inferred_from != edge.from {
                // 翻转方向
                let tmp = edge.from;
                edge.from = edge.to;
                edge.to = tmp;
            }
        }

        // 4. 避免环(DAG):粗略检测 + 删除
        new_edges = self.remove_cycles(new_edges);

        self.graph.edges = new_edges;
        self.edge_count_history.push(self.graph.edges.len());
        if self.edge_count_history.len() > self.max_history {
            self.edge_count_history.remove(0);
        }
    }

    /// 计算相关矩阵
    fn compute_correlation_matrix(&self) -> nalgebra::DMatrix<f32> {
        let n = self.nodes.len();
        let mut m = nalgebra::DMatrix::zeros(n, n);
        if self.observations.is_empty() {
            return m;
        }
        let count = self.observations.len() as f32;

        for i in 0..n {
            for j in 0..n {
                if i == j {
                    m[(i, j)] = 1.0;
                    continue;
                }
                let xs: Vec<f32> = self.observations.iter().map(|o| o.values[i]).collect();
                let ys: Vec<f32> = self.observations.iter().map(|o| o.values[j]).collect();
                let corr = pearson_correlation(&xs, &ys);
                m[(i, j)] = corr;
            }
        }
        let _ = count;  // unused warning 抑制
        m
    }

    /// 推断方向:基于滞后相关
    /// 如果 X(t) 显著预测 Y(t+1),方向 X → Y
    fn infer_direction(&self, a: usize, b: usize) -> usize {
        if self.observations.len() < 5 {
            return a;  // 样本不够,保持原方向
        }
        // 计算 lag=1 的相关:X(t) vs Y(t+1)
        let lag = 1;
        let xs: Vec<f32> = self.observations.iter()
            .take(self.observations.len() - lag)
            .map(|o| o.values[a])
            .collect();
        let ys: Vec<f32> = self.observations.iter()
            .skip(lag)
            .map(|o| o.values[b])
            .collect();
        let corr_ab = pearson_correlation(&xs, &ys);

        // 反向
        let xs2: Vec<f32> = self.observations.iter()
            .take(self.observations.len() - lag)
            .map(|o| o.values[b])
            .collect();
        let ys2: Vec<f32> = self.observations.iter()
            .skip(lag)
            .map(|o| o.values[a])
            .collect();
        let corr_ba = pearson_correlation(&xs2, &ys2);

        // 滞后相关更强的方向 = 因果方向
        if corr_ab.abs() > corr_ba.abs() {
            a  // a -> b
        } else {
            b  // b -> a
        }
    }

    /// 移除环(粗略:删最后加入的边,直到无环)
    fn remove_cycles(&self, edges: Vec<CausalEdge>) -> Vec<CausalEdge> {
        let mut result = Vec::new();
        for edge in edges {
            result.push(edge);
            if self.has_cycle(&result) {
                result.pop();
            }
        }
        result
    }

    /// 简单环检测(DFS)
    fn has_cycle(&self, edges: &[CausalEdge]) -> bool {
        // 邻接表
        let mut adj: std::collections::HashMap<usize, Vec<usize>> =
            std::collections::HashMap::new();
        for e in edges {
            adj.entry(e.from).or_insert_with(Vec::new).push(e.to);
        }

        // 0=未访问,1=访问中,2=访问完
        let mut state: std::collections::HashMap<usize, u8> = std::collections::HashMap::new();
        for n in &self.nodes {
            state.insert(n.id, 0u8);
        }

        for n in &self.nodes {
            let s = *state.get(&n.id).unwrap_or(&0u8);
            if s == 0 {
                if Self::dfs_cycle(n.id, &adj, &mut state) {
                    return true;
                }
            }
        }
        false
    }

    fn dfs_cycle(
        node: usize,
        adj: &std::collections::HashMap<usize, Vec<usize>>,
        state: &mut std::collections::HashMap<usize, u8>,
    ) -> bool {
        state.insert(node, 1u8);
        if let Some(neighbors) = adj.get(&node) {
            let neighbors_copy: Vec<usize> = neighbors.clone();
            for next in neighbors_copy {
                let s = *state.get(&next).unwrap_or(&0u8);
                if s == 1 {
                    return true;  // 环
                }
                if s == 0 && Self::dfs_cycle(next, adj, state) {
                    return true;
                }
            }
        }
        state.insert(node, 2u8);
        false
    }

    pub fn graph(&self) -> &CausalGraph { &self.graph }
    pub fn edge_count(&self) -> usize { self.graph.edges.len() }

    pub fn edge_count_history(&self) -> &[usize] {
        &self.edge_count_history
    }
}

/// 皮尔逊相关系数
fn pearson_correlation(xs: &[f32], ys: &[f32]) -> f32 {
    let n = xs.len().min(ys.len());
    if n < 2 {
        return 0.0;
    }
    let mean_x: f32 = xs.iter().take(n).sum::<f32>() / n as f32;
    let mean_y: f32 = ys.iter().take(n).sum::<f32>() / n as f32;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for i in 0..n {
        let dx = xs[i] - mean_x;
        let dy = ys[i] - mean_y;
        cov += dx * dy;
        var_x += dx * dx;
        var_y += dy * dy;
    }
    let denom = (var_x * var_y).sqrt();
    if denom < 1e-6 {
        0.0
    } else {
        cov / denom
    }
}

/// do-calculus 简化版:给定 do(X=x) 后,预测 Y 的期望
///
/// 这不是真 do-calculus(那是 Pearl 的圣杯),只是一个"如果 X 是因,Y 是果,改变 X 后 Y 会变多少"的估计
pub fn do_intervention_effect(
    obs_x: &[f32],
    obs_y: &[f32],
    intervention_x: f32,
) -> f32 {
    // 用线性回归:Y = a*X + b
    let n = obs_x.len().min(obs_y.len());
    if n < 2 {
        return 0.0;
    }
    let mean_x: f32 = obs_x.iter().take(n).sum::<f32>() / n as f32;
    let mean_y: f32 = obs_y.iter().take(n).sum::<f32>() / n as f32;
    let mut cov = 0.0;
    let mut var_x = 0.0;
    for i in 0..n {
        let dx = obs_x[i] - mean_x;
        cov += dx * (obs_y[i] - mean_y);
        var_x += dx * dx;
    }
    if var_x < 1e-6 {
        return mean_y;
    }
    let a = cov / var_x;
    let b = mean_y - a * mean_x;
    a * intervention_x + b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pearson_perfect_positive() {
        let xs = vec![1.0, 2.0, 3.0, 4.0];
        let ys = vec![2.0, 4.0, 6.0, 8.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r - 1.0).abs() < 1e-6);
    }

    #[test]
    fn pearson_perfect_negative() {
        let xs = vec![1.0, 2.0, 3.0, 4.0];
        let ys = vec![4.0, 3.0, 2.0, 1.0];
        let r = pearson_correlation(&xs, &ys);
        assert!((r - (-1.0)).abs() < 1e-6);
    }

    #[test]
    fn pearson_uncorrelated() {
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![1.0, 1.0, 1.0, 1.0, 1.0];
        let r = pearson_correlation(&xs, &ys);
        assert!(r.abs() < 1e-6);
    }

    #[test]
    fn discoverer_finds_dag() {
        let nodes = vec![
            CausalNode { id: 0, name: "X".into() },
            CausalNode { id: 1, name: "Y".into() },
            CausalNode { id: 2, name: "Z".into() },
        ];
        let mut d = CausalDiscoverer::new(nodes);

        // X -> Y -> Z 的数据
        for t in 0..50 {
            let x = (t as f32 * 0.1).sin();
            let y = x + (t as f32 * 0.05).sin() * 0.1;  // Y 跟随 X
            let z = y + (t as f32 * 0.07).sin() * 0.1;  // Z 跟随 Y
            d.add_observation(Observation {
                timestamp: t,
                values: vec![x, y, z],
            });
        }
        d.retrain();
        // 应该发现至少一条边
        assert!(d.edge_count() > 0);
    }

    #[test]
    fn do_intervention_works() {
        // Y = 2X + 1
        let xs: Vec<f32> = (0..100).map(|i| i as f32).collect();
        let ys: Vec<f32> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let y_at_10 = do_intervention_effect(&xs, &ys, 10.0);
        assert!((y_at_10 - 21.0).abs() < 0.5);
    }

    #[test]
    fn no_cycle_in_result() {
        let nodes = vec![
            CausalNode { id: 0, name: "A".into() },
            CausalNode { id: 1, name: "B".into() },
        ];
        let mut d = CausalDiscoverer::new(nodes);
        for t in 0..30 {
            d.add_observation(Observation {
                timestamp: t,
                values: vec![t as f32, t as f32 * 0.5],
            });
        }
        d.retrain();
        // 不应该有环
        assert!(!d.has_cycle(&d.graph.edges));
    }
}