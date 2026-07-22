//! 模块 24:持续学习引擎(EWC - Elastic Weight Consolidation)
//!
//! 核心思想:Kirkpatrick et al. 2017
//! - 重要权重被保护,新任务学完不会让旧任务表现崩
//! - 用 Fisher 信息矩阵的对角线估计每个权重的重要性
//! - 损失函数:L = L_new + (lambda/2) * sum_i F_i * (theta_i - theta*_i)^2

use std::collections::HashMap;

/// 单个权重
#[derive(Debug, Clone, Copy)]
pub struct Weight {
    pub value: f32,
    /// Fisher 信息(重要性)
    pub importance: f32,
    /// 上一任务结束时的值(用来算漂移)
    pub star: f32,
}

impl Weight {
    pub fn new(value: f32) -> Self {
        Self {
            value,
            importance: 0.0,
            star: value,
        }
    }
}

/// 一个"任务" = 一组观察样本
#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    /// 样本 (input, target)
    pub samples: Vec<(Vec<f32>, Vec<f32>)>,
}

/// 简单线性网络(够用就行)
#[derive(Debug)]
pub struct EwcNetwork {
    /// 输入维度
    pub input_dim: usize,
    /// 输出维度
    pub output_dim: usize,
    /// 权重矩阵 (output_dim x input_dim)
    pub weights: Vec<Weight>,
    /// 偏置
    pub bias: Vec<Weight>,
    /// 学习率
    pub lr: f32,
    /// EWC 强度
    pub lambda: f32,
    /// 任务计数
    task_count: usize,
    /// 任务历史
    tasks_done: Vec<String>,
}

impl EwcNetwork {
    pub fn new(input_dim: usize, output_dim: usize) -> Self {
        let n = output_dim * input_dim;
        Self {
            input_dim,
            output_dim,
            weights: (0..n).map(|_| Weight::new(0.0)).collect(),
            bias: (0..output_dim).map(|_| Weight::new(0.0)).collect(),
            lr: 0.01,
            lambda: 500.0,
            task_count: 0,
            tasks_done: Vec::new(),
        }
    }

    /// 设置 EWC 强度
    pub fn with_lambda(mut self, lambda: f32) -> Self {
        self.lambda = lambda;
        self
    }

    /// 初始化权重(均匀小随机)
    pub fn init_random(&mut self, seed: u64) {
        // 简化:用 sin 制造伪随机
        for (i, w) in self.weights.iter_mut().enumerate() {
            let s = ((i as u64).wrapping_add(seed) as f32 * 0.7548776662).sin();
            w.value = s * 0.1;
            w.star = w.value;
        }
        for (i, b) in self.bias.iter_mut().enumerate() {
            let s = ((i as u64).wrapping_add(seed) as f32 * 1.2345678).sin();
            b.value = s * 0.05;
            b.star = b.value;
        }
    }

    /// 前向:input -> output
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        let mut out = vec![0.0; self.output_dim];
        for o in 0..self.output_dim {
            let mut sum = self.bias[o].value;
            for i in 0..self.input_dim {
                sum += self.weights[o * self.input_dim + i].value * input[i];
            }
            out[o] = sum;
        }
        out
    }

    /// 计算 MSE 损失
    pub fn loss(&self, samples: &[(Vec<f32>, Vec<f32>)]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let mut total = 0.0;
        for (x, y) in samples {
            let o = self.forward(x);
            for (a, b) in o.iter().zip(y) {
                total += (a - b).powi(2);
            }
        }
        total / samples.len() as f32
    }

    /// 计算 EWC 惩罚:对所有权重算重要性 * (theta - star)^2
    pub fn ewc_penalty(&self) -> f32 {
        let mut pen = 0.0;
        for w in &self.weights {
            pen += w.importance * (w.value - w.star).powi(2);
        }
        for b in &self.bias {
            pen += b.importance * (b.value - b.star).powi(2);
        }
        pen
    }

    /// 总损失
    pub fn total_loss(&self, samples: &[(Vec<f32>, Vec<f32>)]) -> f32 {
        self.loss(samples) + (self.lambda / 2.0) * self.ewc_penalty()
    }

    /// 训练一个任务(标准 SGD + EWC 惩罚)
    /// 步数 = epochs * samples.len()
    pub fn train(&mut self, task: &Task, epochs: usize) {
        for _ in 0..epochs {
            for (x, y) in &task.samples {
                self.sgd_step(x, y);
            }
        }
    }

    /// 一步 SGD
    fn sgd_step(&mut self, x: &[f32], y: &[f32]) {
        // 1. 前向
        let out = self.forward(x);
        // 2. 误差
        let err: Vec<f32> = out.iter().zip(y).map(|(a, b)| a - b).collect();
        // 3. 梯度
        for o in 0..self.output_dim {
            let g_bias = err[o];
            // EWC 偏置梯度
            let b_ewc = self.lambda
                * self.bias[o].importance
                * (self.bias[o].value - self.bias[o].star);
            self.bias[o].value -= self.lr * (g_bias + b_ewc);

            for i in 0..self.input_dim {
                let g_w = err[o] * x[i];
                let w_idx = o * self.input_dim + i;
                let w_ewc = self.lambda
                    * self.weights[w_idx].importance
                    * (self.weights[w_idx].value - self.weights[w_idx].star);
                self.weights[w_idx].value -= self.lr * (g_w + w_ewc);
            }
        }
    }

    /// 任务训练完,标记为"老任务":更新 star,计算 Fisher 信息
    pub fn consolidate(&mut self, task: &Task) {
        // 1. 计算 Fisher 信息 ≈ (grad^2) 在任务样本上的平均
        let mut w_fisher = vec![0.0; self.weights.len()];
        let mut b_fisher = vec![0.0; self.bias.len()];

        let n = task.samples.len().max(1) as f32;
        for (x, y) in &task.samples {
            let out = self.forward(x);
            let err: Vec<f32> = out.iter().zip(y).map(|(a, b)| a - b).collect();
            for o in 0..self.output_dim {
                b_fisher[o] += err[o].powi(2);
                for i in 0..self.input_dim {
                    let w_idx = o * self.input_dim + i;
                    w_fisher[w_idx] += (err[o] * x[i]).powi(2);
                }
            }
        }
        for w_f in w_fisher.iter_mut() {
            *w_f /= n;
        }
        for b_f in b_fisher.iter_mut() {
            *b_f /= n;
        }

        // 2. 累积到现有重要性
        for (w, &f) in self.weights.iter_mut().zip(w_fisher.iter()) {
            w.importance += f;
            w.star = w.value;
        }
        for (b, &f) in self.bias.iter_mut().zip(b_fisher.iter()) {
            b.importance += f;
            b.star = b.value;
        }

        self.task_count += 1;
        self.tasks_done.push(task.name.clone());
    }

    /// 已完成的任务数
    pub fn tasks_completed(&self) -> usize {
        self.task_count
    }

    /// 任务名字
    pub fn task_names(&self) -> &[String] {
        &self.tasks_done
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn make_task(name: &str, pattern: fn(&[f32]) -> Vec<f32>, n: usize) -> Task {
        let mut samples = Vec::new();
        for i in 0..n {
            let x: Vec<f32> = (0..3).map(|j| ((i + j) as f32 * 0.1).sin()).collect();
            let y = pattern(&x);
            samples.push((x, y));
        }
        Task {
            name: name.into(),
            samples,
        }
    }

    #[test]
    fn test_forward_shape() {
        let net = EwcNetwork::new(3, 2);
        let out = net.forward(&[1.0, 0.0, -1.0]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_train_task1_decreases_loss() {
        let mut net = EwcNetwork::new(3, 2);
        net.init_random(42);
        let task1 = make_task("t1", |x| vec![x[0] + x[1], x[1] - x[2]], 20);
        let loss_before = net.loss(&task1.samples);
        net.train(&task1, 50);
        let loss_after = net.loss(&task1.samples);
        assert!(loss_after < loss_before);
    }

    #[test]
    fn test_consolidate_records_task() {
        let mut net = EwcNetwork::new(3, 2);
        net.init_random(1);
        let task = make_task("t1", |x| vec![x[0], x[1]], 10);
        net.train(&task, 20);
        net.consolidate(&task);
        assert_eq!(net.tasks_completed(), 1);
    }

    #[test]
    fn test_ewc_penalty_zero_for_untrained() {
        let net = EwcNetwork::new(2, 2);
        assert_eq!(net.ewc_penalty(), 0.0);
    }

    #[test]
    fn test_ewc_penalty_grows_with_importance() {
        let mut net = EwcNetwork::new(2, 1);
        net.init_random(1);
        let task = make_task("t1", |x| vec![x[0] + x[1]], 5);
        net.consolidate(&task);
        let pen1 = net.ewc_penalty();
        // 改一个权重
        net.weights[0].value += 1.0;
        let pen2 = net.ewc_penalty();
        assert!(pen2 > pen1);
    }

    /// 关键测试:旧任务不被大幅遗忘
    #[test]
    fn test_old_task_not_forgotten() {
        let mut net = EwcNetwork::new(3, 2).with_lambda(50000.0);
        net.init_random(7);
        let task1 = make_task("t1", |x| vec![x[0] + x[1], x[1] - x[2]], 20);
        let task2 = make_task("t2", |x| vec![x[0] * 2.0, x[2] * 2.0], 20);

        // 学会 t1
        net.train(&task1, 100);
        net.consolidate(&task1);
        let loss1_after_t1 = net.loss(&task1.samples);

        // 适度学 t2
        net.train(&task2, 50);
        let loss1_after_t2 = net.loss(&task1.samples);

        // EWC 关键断言:学了 t2 后,t1 的损失不应大幅恶化
        let ratio = loss1_after_t2 / loss1_after_t1.max(1e-6);
        assert!(
            ratio < 50.0,
            "t1 forgotten too much: ratio = {} (loss {} -> {})",
            ratio,
            loss1_after_t1,
            loss1_after_t2
        );
    }

    #[test]
    fn test_without_ewc_does_forget() {
        // 验证:没 EWC 时,旧任务确实会大崩
        let mut net = EwcNetwork::new(3, 2).with_lambda(0.0);
        net.init_random(7);
        let task1 = make_task("t1", |x| vec![x[0] + x[1], x[1] - x[2]], 20);
        let task2 = make_task("t2", |x| vec![x[0] * 2.0, x[2] * 2.0], 20);

        net.train(&task1, 100);
        net.consolidate(&task1);
        let loss1_after_t1 = net.loss(&task1.samples);

        net.train(&task2, 500);
        let loss1_after_t2 = net.loss(&task1.samples);

        let ratio = loss1_after_t2 / loss1_after_t1.max(1e-6);
        // 没有 EWC,ratio 应该比有 EWC 大
        assert!(ratio > 1.5, "without EWC, expected to forget, got ratio = {}", ratio);
    }
}
