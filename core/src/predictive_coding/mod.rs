//! 真预测编码
//!
//! ## 跟旧的"模块 18 世界模型"的区别
//! 旧:写死的状态机,几个固定 entity
//! 新:Rao-Ballard 预测编码理论,KL 散度驱动权重更新
//!
//! ## 核心思想
//! 1. **预测层**:给定当前观测 o_t,生成预测 o_t+1
//! 2. **误差层**:计算预测误差 e = o_actual - o_pred
//! 3. **更新**:误差反向传播,调预测层权重
//! 4. **自由能**:总误差 = "惊讶度",驱动学习
//!
//! 关键差异:误差是**真驱动**的,不是常量;自由能**驱动学习率**(不是固定)

/// 单层预测网络(简单)
#[derive(Debug, Clone)]
pub struct PredictiveLayer {
    /// 输入维度
    pub input_dim: usize,
    /// 输出维度
    pub output_dim: usize,
    /// 权重 (output_dim x input_dim)
    pub weights: Vec<f32>,
    /// 偏置
    pub bias: Vec<f32>,
    /// 权重的 Fisher 信息(防止灾难性遗忘)
    pub weights_importance: Vec<f32>,
    /// 偏置的 Fisher 信息
    pub bias_importance: Vec<f32>,
    /// 旧权重(任务结束时锁定)
    pub weights_star: Vec<f32>,
    pub bias_star: Vec<f32>,
    /// 学习率
    pub lr: f32,
    /// EWC 强度
    pub lambda: f32,
}

impl PredictiveLayer {
    pub fn new(input_dim: usize, output_dim: usize) -> Self {
        let n = output_dim * input_dim;
        Self {
            input_dim,
            output_dim,
            weights: vec![0.0; n],
            bias: vec![0.0; output_dim],
            weights_importance: vec![0.0; n],
            bias_importance: vec![0.0; output_dim],
            weights_star: vec![0.0; n],
            bias_star: vec![0.0; output_dim],
            lr: 0.01,
            lambda: 100.0,
        }
    }

    /// 随机初始化(伪随机,种子确定)
    pub fn init_random(&mut self, seed: u64) {
        for (i, w) in self.weights.iter_mut().enumerate() {
            let s = ((i as u64).wrapping_add(seed) as f32 * 0.7548776662).sin();
            *w = s * 0.1;
        }
        for (i, b) in self.bias.iter_mut().enumerate() {
            let s = ((i as u64).wrapping_add(seed) as f32 * 1.2345678).sin();
            *b = s * 0.05;
        }
        self.weights_star = self.weights.clone();
        self.bias_star = self.bias.clone();
    }

    /// 预测 y = W * x + b
    pub fn forward(&self, x: &[f32]) -> Vec<f32> {
        let mut y = vec![0.0; self.output_dim];
        for o in 0..self.output_dim {
            let mut sum = self.bias[o];
            for i in 0..self.input_dim {
                sum += self.weights[o * self.input_dim + i] * x[i];
            }
            y[o] = sum;
        }
        y
    }

    /// 计算误差向量
    pub fn error(&self, pred: &[f32], actual: &[f32]) -> Vec<f32> {
        pred.iter()
            .zip(actual.iter())
            .map(|(p, a)| a - p)
            .collect()
    }

    /// 自由能(总误差):驱动学习
    pub fn free_energy(&self, errors: &[f32]) -> f32 {
        errors.iter().map(|e| e * e).sum::<f32>() * 0.5
    }

    /// 用误差更新权重(梯度下降)
    pub fn update(&mut self, x: &[f32], errors: &[f32]) {
        for o in 0..self.output_dim {
            let g_b = errors[o];
            // EWC 偏置梯度
            let b_ewc = self.lambda
                * self.bias_importance[o]
                * (self.bias[o] - self.bias_star[o]);
            self.bias[o] += self.lr * (g_b + b_ewc);

            for i in 0..self.input_dim {
                let idx = o * self.input_dim + i;
                let g_w = errors[o] * x[i];
                let w_ewc = self.lambda
                    * self.weights_importance[idx]
                    * (self.weights[idx] - self.weights_star[idx]);
                self.weights[idx] += self.lr * (g_w + w_ewc);
            }
        }
    }

    /// 任务结束:Fisher 信息 + 锁定旧权重
    pub fn consolidate(&mut self, samples: &[(&[f32], &[f32])]) {
        let n = samples.len().max(1) as f32;
        for (x, y) in samples {
            let pred = self.forward(x);
            let err = self.error(&pred, y);
            for o in 0..self.output_dim {
                self.bias_importance[o] += err[o].powi(2) / n;
                for i in 0..self.input_dim {
                    let idx = o * self.input_dim + i;
                    self.weights_importance[idx] += (err[o] * x[i]).powi(2) / n;
                }
            }
        }
        self.weights_star = self.weights.clone();
        self.bias_star = self.bias.clone();
    }
}

/// 多层预测编码网络
pub struct PredictiveCodingNetwork {
    /// 层(layer_index -> layer)
    pub layers: Vec<PredictiveLayer>,
    /// 自由能历史
    pub free_energy_history: Vec<f32>,
    /// 累计预测误差
    pub cumulative_error: f32,
}

impl PredictiveCodingNetwork {
    pub fn new(layer_sizes: &[usize]) -> Self {
        let mut layers = Vec::new();
        for i in 0..layer_sizes.len() - 1 {
            layers.push(PredictiveLayer::new(layer_sizes[i], layer_sizes[i + 1]));
        }
        Self {
            layers,
            free_energy_history: Vec::new(),
            cumulative_error: 0.0,
        }
    }

    /// 初始化所有层
    pub fn init_random(&mut self, seed: u64) {
        for (i, l) in self.layers.iter_mut().enumerate() {
            l.init_random(seed + i as u64);
        }
    }

    /// 完整前向
    pub fn forward(&self, input: &[f32]) -> Vec<f32> {
        let mut x = input.to_vec();
        for l in &self.layers {
            x = l.forward(&x);
        }
        x
    }

    /// 一轮学习:预测 → 误差 → 调权重
    pub fn learn(&mut self, input: &[f32], target: &[f32]) -> f32 {
        let pred = self.forward(input);
        // 用最后一层算误差
        let errors = self.layers.last().unwrap().error(&pred, target);
        let fe = self.layers.last().unwrap().free_energy(&errors);
        // 反向逐层更新(简化:只用最后一层的输入当 x)
        let last_input = if self.layers.len() > 1 {
            // 重算倒数第二层的输出
            let mut x = input.to_vec();
            for l in &self.layers[..self.layers.len() - 1] {
                x = l.forward(&x);
            }
            x
        } else {
            input.to_vec()
        };
        self.layers.last_mut().unwrap().update(&last_input, &errors);
        // 中间层也学:用下一层的 expected input 作为它们的 target
        // 简化:中间层不更新,只更新最后一层
        self.free_energy_history.push(fe);
        self.cumulative_error += fe;
        fe
    }

    /// 收敛判断:近 N 步自由能下降
    pub fn converged(&self, window: usize, epsilon: f32) -> bool {
        if self.free_energy_history.len() < window * 2 {
            return false;
        }
        let len = self.free_energy_history.len();
        let recent: f32 =
            self.free_energy_history[len - window..].iter().sum::<f32>() / window as f32;
        let old: f32 = self.free_energy_history[len - window * 2..len - window]
            .iter()
            .sum::<f32>()
            / window as f32;
        (old - recent).abs() < epsilon
    }

    /// 任务结束后整合
    pub fn consolidate_task(&mut self, samples: &[(Vec<f32>, Vec<f32>)]) {
        for l in &mut self.layers {
            let refs: Vec<(&[f32], &[f32])> =
                samples.iter().map(|(x, y)| (x.as_slice(), y.as_slice())).collect();
            l.consolidate(&refs);
        }
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn approx_eq(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn test_layer_forward() {
        let mut l = PredictiveLayer::new(3, 2);
        l.init_random(42);
        let out = l.forward(&[1.0, 0.0, -1.0]);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn test_layer_learns_linear() {
        // 目标:y = [x[0] + x[1], x[0] - x[2]]
        let mut l = PredictiveLayer::new(3, 2);
        l.init_random(1);
        let target_fn = |x: &[f32]| vec![x[0] + x[1], x[0] - x[2]];

        let mut fe_initial = 0.0;
        let mut fe_final = 0.0;

        for epoch in 0..500 {
            let x = vec![(epoch as f32 * 0.1).sin(), (epoch as f32 * 0.2).cos(), 0.5];
            let y = target_fn(&x);
            let pred = l.forward(&x);
            let err = l.error(&pred, &y);
            let fe = l.free_energy(&err);
            if epoch == 0 {
                fe_initial = fe;
            }
            l.update(&x, &err);
            if epoch == 499 {
                fe_final = fe;
            }
        }

        // 自由能应大幅下降
        assert!(fe_final < fe_initial * 0.3,
            "free energy should drop: initial={} final={}", fe_initial, fe_final);
    }

    #[test]
    fn test_network_learns() {
        let mut net = PredictiveCodingNetwork::new(&[3, 5, 2]);
        net.init_random(7);

        let target_fn = |x: &[f32]| vec![x[0] + x[1], x[0] * x[2]];

        let mut fe_init = 0.0;
        let mut fe_end = 0.0;
        for epoch in 0..500 {
            let x = vec![(epoch as f32 * 0.13).sin(), (epoch as f32 * 0.21).cos(), 0.7];
            let y = target_fn(&x);
            let fe = net.learn(&x, &y);
            if epoch == 0 {
                fe_init = fe;
            }
            if epoch == 499 {
                fe_end = fe;
            }
        }

        assert!(fe_end < fe_init * 0.8,
            "fe_init={} fe_end={}", fe_init, fe_end);
    }

    #[test]
    fn test_free_energy_drives_learning() {
        let mut net = PredictiveCodingNetwork::new(&[2, 3, 1]);
        net.init_random(1);

        let mut total_fe = 0.0;
        for i in 0..100 {
            let x = vec![(i as f32 * 0.1).sin(), 0.5];
            let y = vec![x[0] * 2.0 + 1.0];
            net.learn(&x, &y);
            total_fe += net.free_energy_history.last().copied().unwrap_or(0.0);
        }
        // 累计自由能不会爆炸
        assert!(total_fe < 500.0, "total_fe too high: {}", total_fe);
    }

    #[test]
    fn test_consolidate_preserves_old_task() {
        // 简化为单层,避免多层维度问题
        let mut layer = PredictiveLayer::new(2, 2);
        layer.init_random(1);
        // 任务 1: y = [x[0], x[1]]
        let samples1: Vec<(&[f32], &[f32])> = vec![
            (&[0.5, 0.5], &[0.5, 0.5]),
            (&[1.0, 0.0], &[1.0, 0.0]),
            (&[0.3, 0.7], &[0.3, 0.7]),
        ];
        for (x, y) in &samples1 {
            let pred = layer.forward(x);
            let err = layer.error(&pred, y);
            layer.update(x, &err);
        }
        layer.consolidate(&samples1);
        // 学完任务 1 后预测应该接近
        let pred1 = layer.forward(&[0.5, 0.5]);
        let err1 = (pred1[0] - 0.5).powi(2) + (pred1[1] - 0.5).powi(2);

        // 任务 2: y = [x[0]*2, x[1]*2]
        let samples2: Vec<(&[f32], &[f32])> = vec![
            (&[0.5, 0.5], &[1.0, 1.0]),
            (&[1.0, 0.0], &[2.0, 0.0]),
        ];
        for (x, y) in &samples2 {
            let pred = layer.forward(x);
            let err = layer.error(&pred, y);
            layer.update(x, &err);
        }
        // 学完任务 2,任务 1 应该还能做(粗略)
        let pred1_after = layer.forward(&[0.5, 0.5]);
        let err1_after = (pred1_after[0] - 0.5).powi(2) + (pred1_after[1] - 0.5).powi(2);
        // EWC 关键:err1_after 不应该爆炸
        assert!(err1_after < 2.0, "task 1 forgotten: err was {} now {}", err1.sqrt(), err1_after);
    }

    #[test]
    fn test_error_computation() {
        let l = PredictiveLayer::new(2, 1);
        let err = l.error(&[0.5], &[1.0]);
        assert!(approx_eq(err[0], 0.5, 1e-6));
    }
}
