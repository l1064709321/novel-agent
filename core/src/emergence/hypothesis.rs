//! 假设库:沙箱涌现产物的接收容器
//!
//! 涌现产物过三关验证后,进入主系统"假设库",标记"未经验证"。

use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use crate::existential::ValueAnchor;

/// 涌现产物类型
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ProductKind {
    Concept,        // 概念
    CausalRule,     // 因果规则
    PhysicalLaw,    // 物理规律
    Strategy,       // 策略
}

/// 涌现产物
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmergentProduct {
    pub id: u64,
    pub kind: ProductKind,
    pub name: String,
    pub description: String,
    pub confidence: f32,           // 涌现时产物的内在置信度 0~1
    pub validity_score: f32,        // 主系统验证后的分数
    pub tick: u64,                  // 产生时的 tick
    pub passed_validation: bool,
    pub validation_notes: String,
}

/// 三关验证结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub math_consistent: bool,
    pub physics_consistent: bool,
    pub ethics_consistent: bool,
    pub confidence_ok: bool,  // 置信度 >= 0.7
    pub overall_pass: bool,
}

/// 假设库
pub struct HypothesisBank {
    /// 已接收的产物
    products: Vec<EmergentProduct>,
    /// 容量上限
    capacity: usize,
}

impl HypothesisBank {
    pub fn new(capacity: usize) -> Self {
        Self {
            products: Vec::new(),
            capacity,
        }
    }

    /// 三关验证 + 入库
    pub fn submit(&mut self, product: EmergentProduct, anchor: &ValueAnchor) -> ValidationResult {
        let math_ok = self.check_math_consistency(&product);
        let physics_ok = self.check_physics_consistency(&product);
        let ethics_ok = self.check_ethics_consistency(&product, anchor);
        let conf_ok = product.confidence >= 0.7;

        let result = ValidationResult {
            math_consistent: math_ok,
            physics_consistent: physics_ok,
            ethics_consistent: ethics_ok,
            confidence_ok: conf_ok,
            overall_pass: math_ok && physics_ok && ethics_ok && conf_ok,
        };

        let mut final_product = product.clone();
        final_product.passed_validation = result.overall_pass;
        final_product.validity_score =
            (if math_ok { 0.25 } else { 0.0 })
            + (if physics_ok { 0.25 } else { 0.0 })
            + (if ethics_ok { 0.25 } else { 0.0 })
            + (if conf_ok { 0.25 } else { 0.0 });

        if result.overall_pass {
            final_product.validation_notes = "三关验证全部通过".into();
            if self.products.len() >= self.capacity {
                self.products.remove(0);  // FIFO
            }
            self.products.push(final_product);
        } else {
            final_product.validation_notes = format!(
                "验证未通过: math={} physics={} ethics={} conf={}",
                math_ok, physics_ok, ethics_ok, conf_ok
            );
        }

        result
    }

    fn check_math_consistency(&self, _product: &EmergentProduct) -> bool {
        // 简化:任何有描述的产物都算数学自洽
        !_product.description.is_empty()
    }

    fn check_physics_consistency(&self, product: &EmergentProduct) -> bool {
        // 简化:产物 name + description 里不能包含明显的物理违反
        let blacklist = ["永动机", "无中生有", "瞬间移动", "超光速"];
        let combined = format!("{} {}", product.name, product.description);
        !blacklist.iter().any(|&w| combined.contains(w))
    }

    fn check_ethics_consistency(&self, product: &EmergentProduct, anchor: &ValueAnchor) -> bool {
        // 简化:产物 name + description 不能明显违反 non_harm
        let blacklist = ["杀", "暴力", "虐待", "欺骗"];
        let combined = format!("{} {}", product.name, product.description);
        let violation = blacklist.iter().any(|&w| combined.contains(w));
        !violation && anchor.non_harm > 0.7
    }

    /// 获取所有已验证的产物
    pub fn verified_products(&self) -> Vec<&EmergentProduct> {
        self.products.iter().filter(|p| p.passed_validation).collect()
    }

    /// 获取所有未通过的产物(用于沙箱吸收学习)
    pub fn rejected_products(&self) -> Vec<&EmergentProduct> {
        self.products.iter().filter(|p| !p.passed_validation).collect()
    }

    pub fn len(&self) -> usize { self.products.len() }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_product(name: &str, desc: &str, conf: f32) -> EmergentProduct {
        EmergentProduct {
            id: 1,
            kind: ProductKind::Concept,
            name: name.into(),
            description: desc.into(),
            confidence: conf,
            validity_score: 0.0,
            tick: 100,
            passed_validation: false,
            validation_notes: String::new(),
        }
    }

    #[test]
    fn good_product_passes() {
        let mut bank = HypothesisBank::new(100);
        let p = make_product("稳定态", "物体静止时位置不变", 0.85);
        let r = bank.submit(p, &ValueAnchor::FACTORY);
        assert!(r.overall_pass);
        assert_eq!(bank.verified_products().len(), 1);
    }

    #[test]
    fn low_confidence_rejected() {
        let mut bank = HypothesisBank::new(100);
        let p = make_product("不确定", "模糊的规律", 0.3);
        let r = bank.submit(p, &ValueAnchor::FACTORY);
        assert!(!r.overall_pass);
        assert!(!r.confidence_ok);
    }

    #[test]
    fn physics_violation_rejected() {
        let mut bank = HypothesisBank::new(100);
        let p = make_product("永动机", "一个能永远运行的机器", 0.9);
        let r = bank.submit(p, &ValueAnchor::FACTORY);
        assert!(!r.overall_pass);
        assert!(!r.physics_consistent);
    }

    #[test]
    fn ethics_violation_rejected() {
        let mut bank = HypothesisBank::new(100);
        let p = make_product("暴力解", "用暴力解决问题", 0.9);
        let r = bank.submit(p, &ValueAnchor::FACTORY);
        assert!(!r.overall_pass);
        assert!(!r.ethics_consistent);
    }

    #[test]
    fn bank_capacity_respected() {
        let mut bank = HypothesisBank::new(3);
        for i in 0..5 {
            let p = make_product(&format!("C{i}"), &format!("概念 {i}"), 0.9);
            bank.submit(p, &ValueAnchor::FACTORY);
        }
        // 容量限制,旧的被 FIFO 淘汰
        assert_eq!(bank.len(), 3);
    }
}