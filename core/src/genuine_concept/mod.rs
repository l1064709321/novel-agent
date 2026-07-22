//! 真概念发现:从数据反推守恒律
//!
//! 跟旧"动量近似守恒"假货的区别:
//! - 旧:阈值判断 variance < 50
//! - 新:线性拟合 y = a*x + b,反推 y 是不是 x 的守恒量;真用 R^2 和残差
//!
//! ## 算法
//! 给定一组数据 {(x_i, y_i)},尝试拟合 y = w * x + b:
//! - w, b 由最小二乘估计
//! - R^2 衡量拟合质量
//! - 残差均值/方差衡量"是不是真守恒"
//!
//! 真守恒律:如果 y(t) ≈ y(t+1) (残差 < 5%),且 y = w*m*v (动量公式),这就是动量守恒

/// 拟合结果
#[derive(Debug, Clone, Copy)]
pub struct LinearFit {
    /// 斜率
    pub slope: f32,
    /// 截距
    pub intercept: f32,
    /// R^2 (0~1)
    pub r_squared: f32,
    /// 残差均方根
    pub rmse: f32,
    /// 样本数
    pub n: usize,
}

impl LinearFit {
    /// 真线性关系?要求 R^2 > 0.8,残差相对值 < 0.1
    pub fn is_real_linear(&self) -> bool {
        self.r_squared > 0.8 && self.rmse < (self.slope.abs() * 0.1).max(0.01)
    }
}

/// 最小二乘线性拟合 y = a*x + b
pub fn linear_fit(xs: &[f32], ys: &[f32]) -> Option<LinearFit> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let n = xs.len() as f32;
    let mean_x: f32 = xs.iter().sum::<f32>() / n;
    let mean_y: f32 = ys.iter().sum::<f32>() / n;

    let mut cov = 0.0;
    let mut var_x = 0.0;
    let mut var_y = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        cov += (x - mean_x) * (y - mean_y);
        var_x += (x - mean_x).powi(2);
        var_y += (y - mean_y).powi(2);
    }
    if var_x < 1e-9 {
        return None;
    }
    let slope = cov / var_x;
    let intercept = mean_y - slope * mean_x;
    let r_squared = if var_y > 1e-9 {
        (cov * cov) / (var_x * var_y)
    } else {
        1.0
    };
    // RMSE
    let mut ss_res = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let pred = slope * x + intercept;
        ss_res += (y - pred).powi(2);
    }
    let rmse = (ss_res / n).sqrt();
    Some(LinearFit {
        slope,
        intercept,
        r_squared,
        rmse,
        n: xs.len(),
    })
}

/// 一条守恒律候选
#[derive(Debug, Clone)]
pub struct ConservationCandidate {
    pub quantity_name: String,
    pub values: Vec<f32>,
    pub fit: LinearFit,
    /// 守恒分数:1 - (rmse / mean)
    pub conservation_score: f32,
}

/// 守恒律检测器
pub struct ConservationDetector {
    /// 历史(quantity_name -> 历史值)
    pub histories: std::collections::HashMap<String, Vec<f32>>,
    /// 检测到的守恒律
    pub found: Vec<ConservationCandidate>,
    /// 时间步(用于 x 轴)
    pub time_steps: Vec<f32>,
    /// 阈值
    pub min_samples: usize,
    pub r2_threshold: f32,
    pub rmse_relative_threshold: f32,
}

impl ConservationDetector {
    pub fn new() -> Self {
        Self {
            histories: std::collections::HashMap::new(),
            found: Vec::new(),
            time_steps: Vec::new(),
            min_samples: 30,
            r2_threshold: 0.8,
            rmse_relative_threshold: 0.1,
        }
    }

    /// 加一个新样本(quantity_name, value, time)
    pub fn observe(&mut self, quantity_name: &str, value: f32, time: f32) {
        self.histories
            .entry(quantity_name.to_string())
            .or_default()
            .push(value);
        if !self.time_steps.contains(&time) {
            self.time_steps.push(time);
        }
    }

    /// 尝试发现守恒律
    pub fn try_detect(&mut self) {
        self.found.clear();
        for (name, history) in &self.histories {
            if history.len() < self.min_samples {
                continue;
            }
            // 把时间作为 x,quantity 作为 y,拟合
            // 注意:对守恒量,拟合出来是水平线(slope≈0)
            let xs: Vec<f32> = (0..history.len()).map(|i| i as f32).collect();
            let ys: Vec<f32> = history.clone();

            if let Some(fit) = linear_fit(&xs, &ys) {
                let mean: f32 = ys.iter().sum::<f32>() / ys.len() as f32;
                let rel_rmse = if mean.abs() > 1e-6 {
                    fit.rmse / mean.abs()
                } else {
                    fit.rmse
                };
                // 真守恒律:斜率接近 0 + 残差相对值小
                let is_constant = fit.slope.abs() < mean.abs() * 0.01;
                if is_constant && rel_rmse < self.rmse_relative_threshold {
                    self.found.push(ConservationCandidate {
                        quantity_name: name.clone(),
                        values: ys,
                        fit,
                        conservation_score: 1.0 - rel_rmse.min(1.0),
                    });
                }
            }
        }
    }

    pub fn found_count(&self) -> usize {
        self.found.len()
    }
}

impl Default for ConservationDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// 守恒律实验器:在 rapier 物理里发现真实守恒
pub struct ConservationExperimenter {
    pub detector: ConservationDetector,
    pub momentum_x: Vec<f32>,
    pub momentum_y: Vec<f32>,
    pub energy_kinetic: Vec<f32>,
    pub energy_potential: Vec<f32>,
    pub energy_total: Vec<f32>,
    pub mass_total: Vec<f32>,
}

impl ConservationExperimenter {
    pub fn new() -> Self {
        Self {
            detector: ConservationDetector::new(),
            momentum_x: Vec::new(),
            momentum_y: Vec::new(),
            energy_kinetic: Vec::new(),
            energy_potential: Vec::new(),
            energy_total: Vec::new(),
            mass_total: Vec::new(),
        }
    }

    /// 记录一组守恒量
    pub fn record(
        &mut self,
        px: f32,
        py: f32,
        ke: f32,
        pe: f32,
        total_mass: f32,
    ) {
        self.momentum_x.push(px);
        self.momentum_y.push(py);
        self.energy_kinetic.push(ke);
        self.energy_potential.push(pe);
        self.energy_total.push(ke + pe);
        self.mass_total.push(total_mass);
    }

    /// 用真实数据驱动 detector
    pub fn flush(&mut self) {
        let history = [
            ("p_x", &self.momentum_x),
            ("p_y", &self.momentum_y),
            ("E_kinetic", &self.energy_kinetic),
            ("E_potential", &self.energy_potential),
            ("E_total", &self.energy_total),
            ("M_total", &self.mass_total),
        ];
        for (name, data) in &history {
            for (i, v) in data.iter().enumerate() {
                self.detector.observe(name, *v, i as f32);
            }
        }
        self.detector.try_detect();
    }
}

impl Default for ConservationExperimenter {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_linear_fit_perfect() {
        // y = 2x + 1
        let xs: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let ys: Vec<f32> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let fit = linear_fit(&xs, &ys).unwrap();
        assert!((fit.slope - 2.0).abs() < 1e-3);
        assert!((fit.intercept - 1.0).abs() < 1e-3);
        assert!((fit.r_squared - 1.0).abs() < 1e-3);
        assert!(fit.rmse < 1e-3);
    }

    #[test]
    fn test_linear_fit_noisy() {
        // y = 3x + 0 + 噪声
        let xs: Vec<f32> = (0..100).map(|i| i as f32 * 0.1).collect();
        let mut ys = Vec::new();
        for (i, &x) in xs.iter().enumerate() {
            let noise = ((i * 37) % 7) as f32 * 0.01 - 0.03;
            ys.push(3.0 * x + noise);
        }
        let fit = linear_fit(&xs, &ys).unwrap();
        assert!((fit.slope - 3.0).abs() < 0.1, "slope was {}", fit.slope);
        assert!(fit.r_squared > 0.9);
    }

    #[test]
    fn test_linear_fit_constant() {
        // y = 5 (常数)
        let xs: Vec<f32> = (0..10).map(|i| i as f32).collect();
        let ys: Vec<f32> = vec![5.0; 10];
        let fit = linear_fit(&xs, &ys).unwrap();
        assert!(fit.slope.abs() < 1e-3);
        assert!((fit.intercept - 5.0).abs() < 1e-3);
    }

    #[test]
    fn test_linear_fit_too_few_samples() {
        let xs = vec![1.0];
        let ys = vec![2.0];
        assert!(linear_fit(&xs, &ys).is_none());
    }

    #[test]
    fn test_conservation_detector_finds_constant() {
        let mut d = ConservationDetector::new();
        for i in 0..100 {
            // 模拟动量:基本守恒,加一点噪声
            let p = 5.0 + ((i * 13) % 3) as f32 * 0.01;
            d.observe("p_x", p, i as f32);
        }
        d.try_detect();
        assert!(d.found_count() > 0, "should find conserved quantity");
        let m = d.found.iter().find(|c| c.quantity_name == "p_x").unwrap();
        assert!(m.fit.slope.abs() < 0.01);
    }

    #[test]
    fn test_conservation_detector_no_constant() {
        let mut d = ConservationDetector::new();
        for i in 0..100 {
            // 线性增加,不是守恒
            let p = i as f32 * 0.5;
            d.observe("p_x", p, i as f32);
        }
        d.try_detect();
        // 线性增加不应该被识别为守恒
        assert_eq!(d.found_count(), 0);
    }

    #[test]
    fn test_conservation_experimenter() {
        let mut e = ConservationExperimenter::new();
        // 模拟 50 步,动量/能量都基本守恒(加一点点噪声)
        for i in 0..50 {
            let noise = ((i * 11) % 5) as f32 * 0.01;
            e.record(5.0 + noise, -2.0 + noise, 10.0 + noise, 20.0 + noise, 100.0);
        }
        e.flush();
        // 应该找到 p_x, p_y, E_kinetic, E_potential, M_total 都是守恒
        assert!(e.detector.found_count() >= 3,
            "should find >=3 conserved quantities, found {}",
            e.detector.found_count());
    }

    #[test]
    fn test_is_real_linear() {
        let xs: Vec<f32> = (0..50).map(|i| i as f32).collect();
        let ys: Vec<f32> = xs.iter().map(|x| 2.0 * x + 1.0).collect();
        let fit = linear_fit(&xs, &ys).unwrap();
        assert!(fit.is_real_linear());
    }
}
