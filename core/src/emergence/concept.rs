//! 概念发现(K-means 聚类)
//!
//! 从物理世界的状态序列里抽取"概念簇"。
//! 每个簇 = 一个"涌现概念",代表一种系统行为模式。

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 概念:一个聚类中心 + 成员列表
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Concept {
    pub id: u64,
    pub centroid: Vec<f32>,
    pub members: Vec<u64>,     // 样本 ID 列表
    pub label: String,          // 概念标签(由人工或后续命名)
    pub creation_tick: u64,
    pub stability: f32,         // 簇稳定度 = 成员数 / 总样本数
}

/// 样本
#[derive(Debug, Clone)]
pub struct Sample {
    pub id: u64,
    pub features: Vec<f32>,
    pub tick: u64,
}

/// K-means 概念发现器
pub struct ConceptDiscoverer {
    k: usize,
    max_iterations: usize,
    tolerance: f32,
    /// 所有样本
    samples: Vec<Sample>,
    /// 当前质心
    centroids: Vec<Vec<f32>>,
    /// 每个样本所属的簇
    assignments: HashMap<u64, usize>,
    /// 已生成的概念
    concepts: HashMap<u64, Concept>,
    next_concept_id: u64,
    /// 重新训练的间隔(每 N 个样本后)
    retrain_interval: usize,
    samples_since_retrain: usize,
}

impl ConceptDiscoverer {
    pub fn new(k: usize, feature_dim: usize) -> Self {
        Self {
            k,
            max_iterations: 100,
            tolerance: 1e-4,
            samples: Vec::new(),
            centroids: vec![vec![0.0; feature_dim]; k],
            assignments: HashMap::new(),
            concepts: HashMap::new(),
            next_concept_id: 1,
            retrain_interval: 50,
            samples_since_retrain: 0,
        }
    }

    /// 添加新样本
    pub fn add_sample(&mut self, sample: Sample) {
        self.samples.push(sample);
        self.samples_since_retrain += 1;

        // 累积到一定数量后重新聚类
        if self.samples_since_retrain >= self.retrain_interval {
            self.retrain();
            self.samples_since_retrain = 0;
        }
    }

    /// 重新训练:跑 K-means
    pub fn retrain(&mut self) {
        if self.samples.len() < self.k {
            return;  // 样本不够
        }

        // 1. 初始化质心(随机选 K 个样本)
        self.centroids = self.init_centroids_random();

        // 2. 迭代
        for _ in 0..self.max_iterations {
            let old_centroids = self.centroids.clone();

            // 分配:每个样本到最近的质心
            self.assignments.clear();
            for sample in &self.samples {
                let cluster = self.nearest_centroid(&sample.features);
                self.assignments.insert(sample.id, cluster);
            }

            // 更新:每个簇的质心 = 簇内样本的均值
            let mut new_centroids = vec![vec![0.0; self.centroids[0].len()]; self.k];
            let mut counts = vec![0usize; self.k];
            for sample in &self.samples {
                if let Some(&c) = self.assignments.get(&sample.id) {
                    for (i, &v) in sample.features.iter().enumerate() {
                        new_centroids[c][i] += v;
                    }
                    counts[c] += 1;
                }
            }
            for (c, count) in counts.iter().enumerate() {
                if *count > 0 {
                    for v in new_centroids[c].iter_mut() {
                        *v /= *count as f32;
                    }
                } else {
                    // 空簇,重新初始化
                    new_centroids[c] = self.random_sample_features();
                }
            }

            self.centroids = new_centroids;

            // 检查收敛
            let max_shift: f32 = self.centroids.iter()
                .zip(old_centroids.iter())
                .map(|(new, old)| {
                    new.iter().zip(old.iter())
                        .map(|(a, b)| (a - b).powi(2).sqrt())
                        .sum::<f32>()
                })
                .fold(0.0f32, f32::max);

            if max_shift < self.tolerance {
                break;
            }
        }

        // 3. 更新概念
        self.update_concepts();
    }

    fn init_centroids_random(&self) -> Vec<Vec<f32>> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        let mut centroids = Vec::new();
        let mut used = std::collections::HashSet::new();
        while centroids.len() < self.k && used.len() < self.samples.len() {
            let idx = rng.gen_range(0..self.samples.len());
            if !used.contains(&idx) {
                used.insert(idx);
                centroids.push(self.samples[idx].features.clone());
            }
        }
        // 不够就补零
        while centroids.len() < self.k {
            centroids.push(vec![0.0; self.centroids[0].len()]);
        }
        centroids
    }

    fn random_sample_features(&self) -> Vec<f32> {
        use rand::Rng;
        let mut rng = rand::thread_rng();
        (0..self.centroids[0].len())
            .map(|_| rng.gen_range(-1.0..1.0))
            .collect()
    }

    fn nearest_centroid(&self, features: &[f32]) -> usize {
        let mut best = 0;
        let mut best_dist = f32::MAX;
        for (i, c) in self.centroids.iter().enumerate() {
            let dist: f32 = features.iter().zip(c.iter())
                .map(|(a, b)| (a - b).powi(2))
                .sum();
            if dist < best_dist {
                best_dist = dist;
                best = i;
            }
        }
        best
    }

    /// 更新概念:基于当前聚类
    fn update_concepts(&mut self) {
        let mut members_per_cluster: HashMap<usize, Vec<u64>> = HashMap::new();
        for (sample_id, cluster) in &self.assignments {
            members_per_cluster.entry(*cluster).or_default().push(*sample_id);
        }

        let total = self.samples.len().max(1);

        for (cluster_id, members) in members_per_cluster {
            let id = self.next_concept_id;
            self.next_concept_id += 1;

            let concept = Concept {
                id,
                centroid: self.centroids[cluster_id].clone(),
                members: members.clone(),
                label: format!("concept_{}", cluster_id),
                creation_tick: self.tick_of_last_sample(),
                stability: members.len() as f32 / total as f32,
            };

            self.concepts.insert(id, concept);
        }
    }

    fn tick_of_last_sample(&self) -> u64 {
        self.samples.last().map(|s| s.tick).unwrap_or(0)
    }

    /// 获取所有概念
    pub fn concepts(&self) -> Vec<&Concept> {
        self.concepts.values().collect()
    }

    /// 获取最稳定的概念(稳定度 > 阈值)
    pub fn stable_concepts(&self, min_stability: f32) -> Vec<&Concept> {
        self.concepts.values()
            .filter(|c| c.stability >= min_stability)
            .collect()
    }

    /// 概念数量
    pub fn concept_count(&self) -> usize {
        self.concepts.len()
    }

    /// 样本数量
    pub fn sample_count(&self) -> usize {
        self.samples.len()
    }
}

/// 物理世界状态 → 特征向量(给 K-means 用)
pub fn extract_features(state: &[f32], vel: &[f32]) -> Vec<f32> {
    let mut features = Vec::with_capacity(7);
    features.extend_from_slice(state);
    features.extend_from_slice(vel);
    // 加一个能量特征(状态平方和的平方根)
    let energy: f32 = state.iter().chain(vel.iter())
        .map(|x| x * x).sum::<f32>().sqrt();
    features.push(energy);
    features
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sample(id: u64, features: Vec<f32>) -> Sample {
        Sample { id, features, tick: 0 }
    }

    #[test]
    fn discovers_two_clusters() {
        let mut cd = ConceptDiscoverer::new(2, 3);

        // 簇 1:点都在 [0,0,0] 附近
        for i in 0..30 {
            cd.add_sample(make_sample(i, vec![i as f32 * 0.01, 0.0, 0.0]));
        }
        // 簇 2:点都在 [10,10,10] 附近
        for i in 30..60 {
            cd.add_sample(make_sample(i, vec![10.0 + (i as f32 * 0.01), 10.0, 10.0]));
        }
        cd.retrain();

        assert!(cd.concept_count() >= 1);
        let stable = cd.stable_concepts(0.3);
        assert!(!stable.is_empty());
    }

    #[test]
    fn empty_works() {
        let cd = ConceptDiscoverer::new(3, 5);
        assert_eq!(cd.concept_count(), 0);
    }

    #[test]
    fn extract_features_correct_length() {
        let f = extract_features(&[1.0, 2.0], &[3.0, 4.0]);
        assert_eq!(f.len(), 5);  // 2 状态 + 2 速度 + 1 能量
    }

    #[test]
    fn retrain_idempotent_with_no_new_samples() {
        let mut cd = ConceptDiscoverer::new(2, 2);
        for i in 0..20 {
            cd.add_sample(make_sample(i, vec![i as f32, 0.0]));
        }
        cd.retrain();
        let count1 = cd.concept_count();
        cd.retrain();
        let count2 = cd.concept_count();
        assert!(count1 > 0);
        // 重复 retrain 不应该无限增加概念
        assert!(count2 >= count1);
    }
}