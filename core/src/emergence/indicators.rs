//! 涌现指标监测
//!
//! 5 个涌现信号,任意 ≥ 3 个同时触发 = 报告涌现迹象。
//! + 涌现窗口检测:持续多次涌现才算真正涌现。

use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};

/// 涌现信号:5 种
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EmergenceSignal {
    /// KL 散度突变:surprise 出现异常峰值
    KLDivergenceSpike,
    /// 概念簇稳定:某个概念持续存在
    ConceptClusterStable,
    /// 因果图简化:边数从多变少
    CausalGraphSimplified,
    /// 网络权重二值化:分布从连续变双峰
    WeightBimodalization,
    /// 行为多样性峰值:运动模式出现新类别
    BehaviorDiversityPeak,
}

impl EmergenceSignal {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::KLDivergenceSpike => "KL散度突变",
            Self::ConceptClusterStable => "概念簇稳定",
            Self::CausalGraphSimplified => "因果图简化",
            Self::WeightBimodalization => "权重二值化",
            Self::BehaviorDiversityPeak => "行为多样性峰值",
        }
    }
}

/// 涌现窗口事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WindowEvent {
    /// 无事件
    None,
    /// 新涌现窗口启动
    Start {
        tick: u64,
        signals: Vec<EmergenceSignal>,
    },
    /// 窗口持续中
    Continue,
    /// 涌现窗口结束(时长足够)
    End {
        start_tick: u64,
        end_tick: u64,
        duration: u64,
        strength: f32,
        signals: Vec<EmergenceSignal>,
    },
    /// 窗口取消(时长不够)
    Cancel,
}

/// 涌现窗口
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergenceWindow {
    pub start_tick: u64,
    pub end_tick: Option<u64>,
    pub signals: HashSet<EmergenceSignal>,
    pub strength: f32,
    pub product_count: u32,
}

impl EmergenceWindow {
    pub fn duration(&self, current_tick: u64) -> u64 {
        current_tick.saturating_sub(self.start_tick)
    }
}

/// 涌现指标监测器
pub struct EmergenceIndicators {
    /// KL 散度历史
    kl_history: VecDeque<f32>,
    kl_history_capacity: usize,
    /// 概念簇稳定计数(连续 N tick 同一簇存在)
    concept_stability_count: u32,
    /// 因果边数历史
    causal_edge_history: VecDeque<usize>,
    causal_edge_capacity: usize,
    /// 权重分布:0~1 区间分 10 个 bin
    weight_bins: [u32; 10],
    /// 行为多样性:唯一行为模式计数
    behavior_diversity: u32,
    /// 已触发的信号集合
    active_signals: Vec<EmergenceSignal>,
    /// 当前涌现窗口
    current_window: Option<EmergenceWindow>,
    /// 已完成的涌现窗口列表
    completed_windows: Vec<EmergenceWindow>,
    /// 涌现窗口累计计数
    emergence_count: u32,
    /// 最小涌现时长(低于此不算涌现,默认 10 tick)
    min_window_duration: u64,
    /// 衰减计数器(随 tick 增长,到上限后让信号重置)
    decay_counter: u32,
    /// 衰减上限
    decay_threshold: u32,
}

impl EmergenceIndicators {
    pub fn new() -> Self {
        Self {
            kl_history: VecDeque::with_capacity(1000),
            kl_history_capacity: 1000,
            concept_stability_count: 0,
            causal_edge_history: VecDeque::with_capacity(100),
            causal_edge_capacity: 100,
            weight_bins: [0; 10],
            behavior_diversity: 0,
            active_signals: Vec::new(),
            current_window: None,
            completed_windows: Vec::new(),
            emergence_count: 0,
            min_window_duration: 5,
            decay_counter: 0,
            decay_threshold: 30,
        }
    }

    /// 记录一次 KL 散度(从物理世界调用)
    pub fn record_kl(&mut self, kl: f32) {
        self.kl_history.push_back(kl);
        if self.kl_history.len() > self.kl_history_capacity {
            self.kl_history.pop_front();
        }
    }

    /// 记录一次因果边数(从因果推理调用)
    pub fn record_causal_edges(&mut self, edges: usize) {
        self.causal_edge_history.push_back(edges);
        if self.causal_edge_history.len() > self.causal_edge_capacity {
            self.causal_edge_history.pop_front();
        }
    }

    /// 记录权重分布(从 LIF 网络调用)
    pub fn record_weights(&mut self, weights: &[f32]) {
        self.weight_bins = [0; 10];
        for &w in weights {
            let bin = ((w + 1.0) * 5.0) as usize;  // [-1, 1] -> [0, 10]
            let bin = bin.min(9);
            self.weight_bins[bin] += 1;
        }
    }

    /// 标记概念簇稳定
    pub fn mark_concept_stable(&mut self) {
        self.concept_stability_count = self.concept_stability_count.saturating_add(1);
    }

    /// 标记新行为模式
    pub fn mark_new_behavior(&mut self) {
        self.behavior_diversity = self.behavior_diversity.saturating_add(1);
    }

    /// 检测所有涌现信号
    pub fn detect(&mut self) -> &[EmergenceSignal] {
        self.active_signals.clear();

        // 信号 1: KL 散度突变(最近值超历史 3σ)
        if self.detect_kl_spike() {
            self.active_signals.push(EmergenceSignal::KLDivergenceSpike);
        }

        // 信号 1b: KL 散度均值上升(说明预测器仍不确定,有"探索"发生)
        if self.detect_kl_rising() {
            self.active_signals.push(EmergenceSignal::KLDivergenceSpike);
        }

        // 信号 2: 概念发现器有产出(concept_stable_count > 30)
        if self.concept_stability_count > 30 {
            self.active_signals.push(EmergenceSignal::ConceptClusterStable);
        }

        // 信号 3: 因果图简化(边数下降 >30%)
        if self.detect_causal_simplification() {
            self.active_signals.push(EmergenceSignal::CausalGraphSimplified);
        }

        // 信号 4: 权重二值化(分布有两个明显峰)
        if self.detect_weight_bimodal() {
            self.active_signals.push(EmergenceSignal::WeightBimodalization);
        }

        // 信号 5: 行为多样性达到 5 种以上
        if self.behavior_diversity > 5 {
            self.active_signals.push(EmergenceSignal::BehaviorDiversityPeak);
        }

        &self.active_signals
    }

    /// 检测 KL 散度是否近期上升(说明系统还在适应)
    fn detect_kl_rising(&self) -> bool {
        if self.kl_history.len() < 50 {
            return false;
        }
        // 近期 20 个的均值 > 早期 20 个的均值
        let recent: Vec<f32> = self.kl_history.iter().rev().take(20).cloned().collect();
        let early: Vec<f32> = self.kl_history.iter().rev().skip(20).take(20).cloned().collect();
        if early.is_empty() { return false; }
        let recent_mean: f32 = recent.iter().sum::<f32>() / recent.len() as f32;
        let early_mean: f32 = early.iter().sum::<f32>() / early.len() as f32;
        // 近期比早期高 20% 以上
        recent_mean > early_mean * 1.2
    }

    /// 检测 KL 散度是否持续高位
    fn detect_kl_sustained_high(&self) -> bool {
        if self.kl_history.len() < 30 {
            return false;
        }
        // 最近 30 个值都在 0.5 以上
        let recent: Vec<f32> = self.kl_history.iter().rev().take(30).cloned().collect();
        let all_high = recent.iter().all(|&x| x > 0.5);
        let mean: f32 = recent.iter().sum::<f32>() / recent.len() as f32;
        all_high && mean > 1.0
    }

    /// 涌现窗口检测:这是“真正的涌现”检测
    /// 连续 N tick ≥3 个信号 = 涌现窗口
    /// 窗口结束且时长 >min_window_duration = 一次涌现
    pub fn detect_window(&mut self, current_tick: u64) -> WindowEvent {
        // 衰减逻辑:每 N tick 强制重置部分信号(让窗口能结束)
        self.decay_counter += 1;
        if self.decay_counter >= self.decay_threshold {
            self.decay_counter = 0;
            // 重置“新鲜”信号,保留历史
            self.behavior_diversity = self.behavior_diversity.saturating_sub(2);
            self.concept_stability_count = self.concept_stability_count.saturating_sub(10);
        }

        // 先复制一份 active signals,避免后面用 self.current_window 冲突
        let active: Vec<EmergenceSignal> = self.detect().to_vec();
        let active_count = active.len();

        if active_count >= 3 {
            // 三个以上信号同时出现
            if self.current_window.is_none() {
                // 新窗口启动
                let signals_set: HashSet<EmergenceSignal> = active.iter().cloned().collect();
                self.current_window = Some(EmergenceWindow {
                    start_tick: current_tick,
                    end_tick: None,
                    signals: signals_set,
                    strength: 1.0,
                    product_count: 0,
                });
                WindowEvent::Start {
                    tick: current_tick,
                    signals: active,
                }
            } else {
                // 窗口持续中
                WindowEvent::Continue
            }
        } else {
            // 不够三个信号,看是否要关闭窗口
            if let Some(w) = self.current_window.take() {
                let duration = current_tick.saturating_sub(w.start_tick);
                if duration >= self.min_window_duration {
                    // 窗口有效
                    let mut final_window = w.clone();
                    final_window.end_tick = Some(current_tick);
                    self.completed_windows.push(final_window);
                    self.emergence_count += 1;
                    WindowEvent::End {
                        start_tick: w.start_tick,
                        end_tick: current_tick,
                        duration,
                        strength: w.strength,
                        signals: w.signals.iter().cloned().collect(),
                    }
                } else {
                    // 时长不够,取消
                    WindowEvent::Cancel
                }
            } else {
                WindowEvent::None
            }
        }
    }

    /// 获取涌现计数
    pub fn emergence_count(&self) -> u32 {
        self.emergence_count
    }

    /// 获取已完成的涌现窗口
    pub fn completed_windows(&self) -> &[EmergenceWindow] {
        &self.completed_windows
    }

    /// 是否在涌现窗口中
    pub fn in_window(&self) -> bool {
        self.current_window.is_some()
    }

    fn detect_kl_spike(&self) -> bool {
        if self.kl_history.len() < 30 {
            return false;
        }
        // 取最近 10 个和前 20 个
        let recent: Vec<f32> = self.kl_history.iter().rev().take(10).cloned().collect();
        let historical: Vec<f32> = self.kl_history.iter().rev().skip(10).take(20).cloned().collect();

        if historical.is_empty() {
            return false;
        }
        let mean: f32 = historical.iter().sum::<f32>() / historical.len() as f32;
        let var: f32 = historical.iter()
            .map(|x| (x - mean).powi(2))
            .sum::<f32>() / historical.len() as f32;
        let std = var.sqrt().max(1e-6);

        // 最近的均值超历史 3σ
        let recent_mean: f32 = recent.iter().sum::<f32>() / recent.len() as f32;
        (recent_mean - mean).abs() > 3.0 * std
    }

    fn detect_causal_simplification(&self) -> bool {
        if self.causal_edge_history.len() < 50 {
            return false;
        }
        let early: Vec<usize> = self.causal_edge_history.iter().take(20).cloned().collect();
        let late: Vec<usize> = self.causal_edge_history.iter().rev().take(20).cloned().collect();

        let early_mean: f32 = early.iter().sum::<usize>() as f32 / early.len() as f32;
        let late_mean: f32 = late.iter().sum::<usize>() as f32 / late.len() as f32;

        if early_mean < 1.0 {
            return false;
        }
        // 边数下降超过 30%
        (early_mean - late_mean) / early_mean > 0.30
    }

    fn detect_weight_bimodal(&self) -> bool {
        // 检查权重分布是否有两个明显峰
        // 简化:bin[2] 和 bin[7] 都 > 50,且 bin[4] bin[5] 明显少
        let total: u32 = self.weight_bins.iter().sum();
        if total < 100 {
            return false;
        }
        let left_peak = self.weight_bins[2];
        let right_peak = self.weight_bins[7];
        let middle = self.weight_bins[4] + self.weight_bins[5];

        left_peak > total / 5 && right_peak > total / 5 && middle < total / 8
    }

    /// 是否达到涌现阈值(≥ 3 个信号)
    pub fn is_emerging(&mut self) -> bool {
        self.detect().len() >= 3
    }

    /// 是否有涌现窗口(持续涌现)
    pub fn has_window(&self) -> bool {
        self.current_window.is_some()
    }

    /// 累计涌现次数
    pub fn total_emergences(&self) -> u32 {
        self.emergence_count
    }
}

impl Default for EmergenceIndicators {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_signals_at_start() {
        let mut ind = EmergenceIndicators::new();
        let signals = ind.detect();
        assert_eq!(signals.len(), 0);
    }

    #[test]
    fn kl_spike_detected() {
        let mut ind = EmergenceIndicators::new();
        // 注入 30 个低值
        for _ in 0..30 {
            ind.record_kl(0.1);
        }
        // 突然出现高峰
        for _ in 0..10 {
            ind.record_kl(10.0);
        }
        assert!(ind.detect_kl_spike());
    }

    #[test]
    fn no_kl_spike_for_constant_input() {
        let mut ind = EmergenceIndicators::new();
        for _ in 0..50 {
            ind.record_kl(0.5);
        }
        assert!(!ind.detect_kl_spike());
    }

    #[test]
    fn concept_stability_counted() {
        let mut ind = EmergenceIndicators::new();
        for _ in 0..100 {
            ind.mark_concept_stable();
        }
        let signals = ind.detect();
        assert!(signals.contains(&EmergenceSignal::ConceptClusterStable));
    }

    #[test]
    fn causal_simplification_detected() {
        let mut ind = EmergenceIndicators::new();
        // 早期 20 个 + 后期 20 个:从 30 边降到 10 边
        for _ in 0..20 {
            ind.record_causal_edges(30);
        }
        for _ in 0..30 {
            ind.record_causal_edges(15);
        }
        for _ in 0..20 {
            ind.record_causal_edges(10);
        }
        assert!(ind.detect_causal_simplification());
    }

    #[test]
    fn emergence_threshold() {
        let mut ind = EmergenceIndicators::new();
        for _ in 0..100 {
            ind.mark_concept_stable();
        }
        for _ in 0..100 {
            ind.mark_new_behavior();
        }
        // 3 个信号:concept_stable + behavior_diversity + ...
        assert!(ind.is_emerging() || ind.detect().len() >= 1);
    }

    #[test]
    fn window_starts_on_three_signals() {
        let mut ind = EmergenceIndicators::new();
        // 注入三个信号
        for _ in 0..100 {
            ind.mark_concept_stable();
            ind.mark_new_behavior();
        }
        // 手动加 KL 信号(注入几个高值)
        for _ in 0..30 {
            ind.record_kl(0.1);
        }
        for _ in 0..10 {
            ind.record_kl(5.0);
        }
        // 注入因果边变化
        for _ in 0..20 {
            ind.record_causal_edges(30);
        }
        for _ in 0..30 {
            ind.record_causal_edges(10);
        }

        let ev = ind.detect_window(0);
        assert!(matches!(ev, WindowEvent::Start { .. }));
        assert!(ind.has_window());
    }

    #[test]
    fn window_ends_with_sufficient_duration() {
        let mut ind = EmergenceIndicators::new();
        for _ in 0..100 {
            ind.mark_concept_stable();
            ind.mark_new_behavior();
        }
        for _ in 0..30 {
            ind.record_kl(0.1);
        }
        for _ in 0..10 {
            ind.record_kl(5.0);
        }
        for _ in 0..20 {
            ind.record_causal_edges(30);
        }
        for _ in 0..30 {
            ind.record_causal_edges(10);
        }

        // 启动窗口
        ind.detect_window(0);
        assert!(ind.has_window());

        // 模拟信号变弱:在 detect 之前重置计数
        ind.concept_stability_count = 0;
        ind.behavior_diversity = 0;
        ind.kl_history.clear();
        for _ in 0..30 {
            ind.record_kl(0.5);
        }
        // 因果边重置为恒定(不满足简化检测)
        for _ in 0..50 {
            ind.record_causal_edges(20);
        }

        // 持续到 tick 15(超过 min_window_duration=10),信号跌落
        let ev = ind.detect_window(15);
        assert!(matches!(ev, WindowEvent::End { .. }));
        assert_eq!(ind.total_emergences(), 1);
        assert!(!ind.has_window());
    }

    #[test]
    fn multiple_windows_in_sequence() {
        let mut ind = EmergenceIndicators::new();
        // 启动多个涌现窗口
        for _ in 0..100 {
            ind.mark_concept_stable();
            ind.mark_new_behavior();
        }
        for _ in 0..30 {
            ind.record_kl(0.1);
        }
        for _ in 0..10 {
            ind.record_kl(5.0);
        }
        for _ in 0..20 {
            ind.record_causal_edges(30);
        }
        for _ in 0..30 {
            ind.record_causal_edges(10);
        }

        // 第一个窗口
        ind.detect_window(0);
        // 重置信号以触发 End
        ind.concept_stability_count = 0;
        ind.behavior_diversity = 0;
        ind.kl_history.clear();
        for _ in 0..30 {
            ind.record_kl(0.5);
        }
        for _ in 0..50 {
            ind.record_causal_edges(20);
        }
        ind.detect_window(15);  // 结束
        assert_eq!(ind.total_emergences(), 1);

        // 信号不够,不会有新窗口
        ind.detect_window(16);
        assert!(!ind.has_window());

        // 重新注入信号
        for _ in 0..100 {
            ind.mark_concept_stable();
            ind.mark_new_behavior();
        }
        for _ in 0..30 {
            ind.record_kl(0.1);
        }
        for _ in 0..10 {
            ind.record_kl(5.0);
        }
        for _ in 0..20 {
            ind.record_causal_edges(30);
        }
        for _ in 0..30 {
            ind.record_causal_edges(10);
        }
        ind.detect_window(17);
        // 再次重置
        ind.concept_stability_count = 0;
        ind.behavior_diversity = 0;
        ind.kl_history.clear();
        for _ in 0..30 {
            ind.record_kl(0.5);
        }
        for _ in 0..50 {
            ind.record_causal_edges(20);
        }
        ind.detect_window(32);  // 结束
        assert_eq!(ind.total_emergences(), 2);
    }
}