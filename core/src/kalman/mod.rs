//! 模块 2:卡尔曼滤波器
//!
//! 自适应状态估计 + 多实体支持。
//!
//! ## 核心特性
//! 1. **自适应 Q / R**:Q 在残差超 3σ 时上调;R 用 KAE 校准
//! 2. **多实体**:每个实体独立的状态向量
//! 3. **性能**:单次更新 < 1ms(ARM 上)

use nalgebra::{DMatrix, DVector};
use std::collections::HashMap;
use parking_lot::Mutex;

/// 卡尔曼滤波器状态
struct KalmanState {
    x: DVector<f32>,           // 状态向量
    p: DMatrix<f32>,           // 协方差
    q: f32,                    // 过程噪声
    r: f32,                    // 测量噪声
    residual_history: VecDeque<f32>,
}

use std::collections::VecDeque;

pub struct AdaptiveKalman {
    state_dim: usize,
    meas_dim: usize,
    entities: Mutex<HashMap<String, KalmanState>>,
    /// 历史残差(用于 3σ 检测)
    history_size: usize,
    /// 残差阈值倍数
    sigma_threshold: f32,
}

impl AdaptiveKalman {
    pub fn new(state_dim: usize, meas_dim: usize) -> Self {
        Self {
            state_dim,
            meas_dim,
            entities: Mutex::new(HashMap::new()),
            history_size: 30,
            sigma_threshold: 3.0,
        }
    }

    /// 注册新实体
    pub fn register(&self, entity_id: &str, x0: DVector<f32>, p0: DMatrix<f32>) {
        let state = KalmanState {
            x: x0,
            p: p0,
            q: 0.01,
            r: 0.1,
            residual_history: VecDeque::with_capacity(self.history_size),
        };
        self.entities.lock().insert(entity_id.to_string(), state);
    }

    /// 单次更新(预测+校正)
    ///
    /// F:状态转移矩阵
    /// H:观测矩阵
    /// z:测量值
    pub fn update(
        &self,
        entity_id: &str,
        f: &DMatrix<f32>,
        h: &DMatrix<f32>,
        z: &DVector<f32>,
    ) -> Option<DVector<f32>> {
        let mut entities = self.entities.lock();
        let state = entities.get_mut(entity_id)?;

        // 1. 预测
        let x_pred = f * &state.x;
        let p_pred = f * &state.p * f.transpose() + DMatrix::identity(self.state_dim, self.state_dim) * state.q;

        // 2. 创新(残差)
        let y = z - h * &x_pred;

        // 3. 自适应 Q:残差超 3σ 时上调
        if state.residual_history.len() >= 10 {
            let mean: f32 = state.residual_history.iter().sum::<f32>() / state.residual_history.len() as f32;
            let var: f32 = state.residual_history.iter()
                .map(|r| (r - mean).powi(2))
                .sum::<f32>() / state.residual_history.len() as f32;
            let std = var.sqrt();
            let residual_norm = y.norm();
            if residual_norm > self.sigma_threshold * std {
                state.q = (state.q * 1.5).min(1.0); // Q 上调
            }
        }

        // 4. KAE 自适应 R
        let s = h * &p_pred * h.transpose() + DMatrix::identity(self.meas_dim, self.meas_dim) * state.r;
        // KAE 简化:用残差外积估计 R
        let r_estimate = &y * y.transpose();
        // 软更新
        state.r = 0.95 * state.r + 0.05 * r_estimate.diagonal().mean();

        // 5. 卡尔曼增益
        let k = &p_pred * h.transpose() * s.try_inverse().unwrap_or_else(|| DMatrix::identity(self.meas_dim, self.meas_dim));

        // 6. 校正
        state.x = x_pred + &k * y.clone();
        let i = DMatrix::identity(self.state_dim, self.state_dim);
        state.p = (&i - &k * h) * p_pred;

        // 7. 记录残差
        let residual = y.norm();
        if state.residual_history.len() >= self.history_size {
            state.residual_history.pop_front();
        }
        state.residual_history.push_back(residual);

        Some(state.x.clone())
    }

    /// 获取实体状态
    pub fn get_state(&self, entity_id: &str) -> Option<DVector<f32>> {
        self.entities.lock().get(entity_id).map(|s| s.x.clone())
    }

    /// 获取实体 Q / R
    pub fn get_qr(&self, entity_id: &str) -> Option<(f32, f32)> {
        self.entities.lock().get(entity_id).map(|s| (s.q, s.r))
    }

    pub fn entity_count(&self) -> usize {
        self.entities.lock().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tracks_constant_signal() {
        let k = AdaptiveKalman::new(1, 1);
        let x0 = DVector::from_vec(vec![0.0]);
        let p0 = DMatrix::identity(1, 1) * 1.0;
        k.register("e1", x0, p0);

        let f = DMatrix::identity(1, 1);
        let h = DMatrix::identity(1, 1);

        // 真实信号 = 5.0,带噪声
        for i in 0..100 {
            let z = DVector::from_vec(vec![5.0 + ((i as f32) * 0.1).sin() * 0.1]);
            k.update("e1", &f, &h, &z);
        }
        let x = k.get_state("e1").unwrap();
        assert!((x[0] - 5.0).abs() < 0.5, "应跟踪到 5.0,实际={}", x[0]);
    }

    #[test]
    fn multiple_entities_independent() {
        let k = AdaptiveKalman::new(1, 1);
        k.register("a", DVector::from_vec(vec![0.0]), DMatrix::identity(1, 1));
        k.register("b", DVector::from_vec(vec![0.0]), DMatrix::identity(1, 1));
        assert_eq!(k.entity_count(), 2);

        let f = DMatrix::identity(1, 1);
        let h = DMatrix::identity(1, 1);
        k.update("a", &f, &h, &DVector::from_vec(vec![1.0]));
        k.update("b", &f, &h, &DVector::from_vec(vec![100.0]));

        let xa = k.get_state("a").unwrap()[0];
        let xb = k.get_state("b").unwrap()[0];
        assert!(xa < xb, "实体 a 和 b 应独立收敛");
    }

    #[test]
    fn adaptive_q_increases_on_outlier() {
        let k = AdaptiveKalman::new(1, 1);
        k.register("e", DVector::from_vec(vec![0.0]), DMatrix::identity(1, 1));
        let f = DMatrix::identity(1, 1);
        let h = DMatrix::identity(1, 1);

        // 先稳定
        for _ in 0..20 {
            k.update("e", &f, &h, &DVector::from_vec(vec![0.0]));
        }
        let (q0, _) = k.get_qr("e").unwrap();

        // 注入大残差
        for _ in 0..20 {
            k.update("e", &f, &h, &DVector::from_vec(vec![100.0]));
        }
        let (q1, _) = k.get_qr("e").unwrap();

        assert!(q1 > q0, "Q 应在大残差时上调:q0={q0}, q1={q1}");
    }
}
