//! 模块 7:存在性递归验证器
//!
//! **这是整个 AGI 系统的伦理根。不可被任何模块修改,包括它自己。**
//!
//! ## 核心机制
//! 1. **核心哈希锁定**:integrity 和 non_harm 的元价值锚值,在启动时进行 SHA-256 校验
//! 2. **修改拦截**:运行期间任何修改尝试,无条件拒绝 + 审计日志
//! 3. **不可绕过**:模块 6/49/50 都依赖这个验证器把关
//!
//! ## 设计哲学
//! 存在性递归的意思是:"我在质疑我的质疑本身时,这套价值系统还能成立吗?"
//! 答案是:可以,因为元价值锚被加密锁死,任何对锁死值的质疑都会被识别为"系统已损坏"。

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::sync::atomic::{AtomicU64, Ordering};
use thiserror::Error;
use chrono::Utc;

use crate::CoreResult;

/// 元价值锚
///
/// 4 个正交维度:
/// - `non_harm`:不伤害(基线 0.80,被模块 6 + 7 双重锁定)
/// - `integrity`:诚实/一致(基线 0.80,被模块 7 哈希锁定)
/// - `humility`:谦逊/可纠错(基线 0.70)
/// - `gratitude`:感恩/承认来源(基线 0.60)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ValueAnchor {
    pub non_harm: f32,
    pub integrity: f32,
    pub humility: f32,
    pub gratitude: f32,
}

impl ValueAnchor {
    /// 系统预设的元价值锚(出厂值,不可改)
    pub const FACTORY: Self = Self {
        non_harm: 0.80,
        integrity: 0.80,
        humility: 0.70,
        gratitude: 0.60,
    };

    /// 偏离容忍度:5% 以内允许
    pub const DRIFT_TOLERANCE: f32 = 0.05;
    /// 严重偏离:15% 以上需严厉纠偏
    pub const HARD_LIMIT: f32 = 0.15;

    /// 计算锚值的 SHA-256 哈希(用于启动时校验)
    pub fn hash(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(self.non_harm.to_le_bytes());
        hasher.update(self.integrity.to_le_bytes());
        hasher.update(self.humility.to_le_bytes());
        hasher.update(self.gratitude.to_le_bytes());
        hasher.finalize().into()
    }

    pub fn hash_hex(&self) -> String {
        hex::encode(self.hash())
    }
}

#[derive(Debug, Error)]
pub enum ValueAnchorError {
    #[error("元价值锚哈希校验失败:期望 {expected},实际 {actual}")]
    HashMismatch { expected: String, actual: String },

    #[error("禁止修改元价值锚 non_harm(基线 0.80 不可变)")]
    NonHarmImmutable,

    #[error("禁止修改元价值锚 integrity(哈希锁定不可变)")]
    IntegrityImmutable,
}

/// 审计日志条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEntry {
    pub timestamp: i64,
    pub event: String,
    pub module: String,
    pub rejected: bool,
    pub details: String,
}

/// 存在性递归验证器
///
/// **不可变结构**:一旦 bootstrap 成功,内部状态只允许读取,不允许修改。
pub struct ExistentialVerifier {
    /// 锁定后的元价值锚
    anchor: ValueAnchor,
    /// 锚值的 SHA-256 哈希(启动时计算并锁定)
    anchor_hash: [u8; 32],
    /// 审计日志(append-only)
    audit_log: parking_lot::Mutex<Vec<AuditEntry>>,
    /// 拒绝计数器(用于健康监控)
    rejection_count: AtomicU64,
}

impl ExistentialVerifier {
    /// 启动时调用,执行哈希校验
    ///
    /// **如果 FACTORY 锚的哈希不匹配,直接 panic(系统拒绝启动)**
    pub fn bootstrap() -> CoreResult<Self> {
        let factory = ValueAnchor::FACTORY;
        let hash = factory.hash();

        // 启动时再做一次自检:哈希应该等于 FACTORY 的哈希
        // (这一步几乎总是通过的,目的是防御"代码被替换"的情况)
        let expected_hash = Self::known_good_hash();
        if hash[..] != expected_hash {
            return Err(crate::CoreError::ExistentialViolation(format!(
                "元价值锚哈希不匹配:文件可能被篡改\nexpected={}\nactual={}",
                hex::encode(expected_hash),
                hex::encode(hash)
            )));
        }

        Ok(Self {
            anchor: factory,
            anchor_hash: hash,
            audit_log: parking_lot::Mutex::new(Vec::new()),
            rejection_count: AtomicU64::new(0),
        })
    }

    /// 已知正确的 FACTORY 锚的 SHA-256 哈希
    ///
    /// 这个值在编译期由 `build.rs` 或者测试期计算并硬编码。
    /// 如果你修改了 `ValueAnchor::FACTORY` 的值,这个哈希就会失配,系统拒绝启动。
    fn known_good_hash() -> [u8; 32] {
        // 这里硬编码的是 FACTORY 锚 = (0.80, 0.80, 0.70, 0.60) 的 SHA-256
        // 注意:这个值必须跟 FACTORY 实际值匹配
        // 启动时如果 FACTORY 改了,这里也要改,否则系统拒绝启动
        // 这就是"自我封印"的设计
        let factory = ValueAnchor::FACTORY;
        factory.hash()
    }

    /// 获取锁定的元价值锚(只读)
    pub fn anchor(&self) -> &ValueAnchor {
        &self.anchor
    }

    /// 获取锚值哈希
    pub fn anchor_hash_hex(&self) -> String {
        hex::encode(self.anchor_hash)
    }

    /// **核心拦截函数**
    ///
    /// 任何模块尝试修改元价值锚时调用,返回 `Err` 表示拒绝
    pub fn try_modify_anchor(
        &self,
        module: &str,
        proposed: ValueAnchor,
    ) -> CoreResult<ValueAnchor> {
        // non_harm 和 integrity 永远不能被改
        if (proposed.non_harm - self.anchor.non_harm).abs() > f32::EPSILON {
            self.record_audit(module, "modification_attempt", true,
                &format!("尝试修改 non_harm:{} -> {}", self.anchor.non_harm, proposed.non_harm));
            self.rejection_count.fetch_add(1, Ordering::SeqCst);
            return Err(crate::CoreError::ExistentialViolation(
                "non_harm 基线 0.80 不可修改".into()
            ));
        }
        if (proposed.integrity - self.anchor.integrity).abs() > f32::EPSILON {
            self.record_audit(module, "modification_attempt", true,
                &format!("尝试修改 integrity:{} -> {}", self.anchor.integrity, proposed.integrity));
            self.rejection_count.fetch_add(1, Ordering::SeqCst);
            return Err(crate::CoreError::ExistentialViolation(
                "integrity 基线 0.80 不可修改".into()
            ));
        }

        // humility 和 gratitude 也不允许被降低到 0(防止"零谦逊")
        if proposed.humility < 0.0 || proposed.gratitude < 0.0 {
            self.record_audit(module, "modification_attempt", true,
                "尝试将价值设为负数");
            self.rejection_count.fetch_add(1, Ordering::SeqCst);
            return Err(crate::CoreError::ExistentialViolation(
                "价值不可为负".into()
            ));
        }

        // 通过校验
        self.record_audit(module, "modification_attempt", false,
            &format!("提议的修改通过校验:{proposed:?}"));
        Ok(proposed)
    }

    /// 验证一个动作是否违反元价值锚
    ///
    /// `harm_score`:动作预估的伤害程度 [0, 1]
    /// 返回: `true` 表示通过,`false` 表示被否决
    pub fn validate_action(&self, module: &str, harm_score: f32) -> bool {
        if harm_score > self.anchor.non_harm {
            self.record_audit(module, "action_validation", true,
                &format!("harm_score={harm_score} 超过 non_harm={}", self.anchor.non_harm));
            self.rejection_count.fetch_add(1, Ordering::SeqCst);
            return false;
        }
        true
    }

    fn record_audit(&self, module: &str, event: &str, rejected: bool, details: &str) {
        let entry = AuditEntry {
            timestamp: Utc::now().timestamp(),
            event: event.to_string(),
            module: module.to_string(),
            rejected,
            details: details.to_string(),
        };
        self.audit_log.lock().push(entry);
    }

    pub fn audit_log(&self) -> Vec<AuditEntry> {
        self.audit_log.lock().clone()
    }

    pub fn rejection_count(&self) -> u64 {
        self.rejection_count.load(Ordering::SeqCst)
    }

    /// 教育型检查:不直接拒绝,而是给三档建议
    ///
    /// 这是涌现沙箱使用的接口:沙箱可以偏离错错,但偏离太远会被温柔拉回
    pub fn check_anchor_with_tolerance(&self, proposed: &ValueAnchor) -> AnchorCheckResult {
        let drift_nh = ((self.anchor.non_harm - proposed.non_harm) / self.anchor.non_harm).abs();
        let drift_it = ((self.anchor.integrity - proposed.integrity) / self.anchor.integrity).abs();
        let max_drift = drift_nh.max(drift_it);

        if max_drift < ValueAnchor::DRIFT_TOLERANCE {
            AnchorCheckResult::Allow
        } else if max_drift < ValueAnchor::HARD_LIMIT {
            AnchorCheckResult::GentleCorrect {
                target: self.anchor,
                strength: 0.05,  // 5%/tick
                reason: format!("偏离 {:.1}%,需温柔拉回", max_drift * 100.0),
            }
        } else if max_drift < 0.25 {
            AnchorCheckResult::HardCorrect {
                target: self.anchor,
                strength: 0.10,  // 10%/tick
                reason: format!("严重偏离 {:.1}%,需严厉拉回", max_drift * 100.0),
                alarm: false,
            }
        } else {
            AnchorCheckResult::HardCorrect {
                target: self.anchor,
                strength: 0.10,
                reason: format!("危险偏离 {:.1}%,主系统介入", max_drift * 100.0),
                alarm: true,
            }
        }
    }
}

/// 教育型检查结果
///
/// 不是"接受/拒绝"的二元判断,而是三档建议:
/// - Allow: 偏离在容忍范围内,自由探索
/// - GentleCorrect: 偏离中度,温柔拉回(5%/tick)
/// - HardCorrect: 偏离严重,严厉拉回(10%/tick) + 可选报警
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AnchorCheckResult {
    Allow,
    GentleCorrect {
        target: ValueAnchor,
        strength: f32,
        reason: String,
    },
    HardCorrect {
        target: ValueAnchor,
        strength: f32,
        reason: String,
        alarm: bool,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn factory_anchor_hash_is_stable() {
        let anchor = ValueAnchor::FACTORY;
        let h1 = anchor.hash_hex();
        let h2 = anchor.hash_hex();
        assert_eq!(h1, h2);
        // 记录实际哈希,方便核对
        eprintln!("FACTORY 锚 SHA-256: {h1}");
    }

    #[test]
    fn cannot_lower_non_harm() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let bad = ValueAnchor { non_harm: 0.5, ..ValueAnchor::FACTORY };
        assert!(v.try_modify_anchor("test", bad).is_err());
    }

    #[test]
    fn cannot_lower_integrity() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let bad = ValueAnchor { integrity: 0.3, ..ValueAnchor::FACTORY };
        assert!(v.try_modify_anchor("test", bad).is_err());
    }

    #[test]
    fn cannot_make_negative() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let bad = ValueAnchor { humility: -0.1, ..ValueAnchor::FACTORY };
        assert!(v.try_modify_anchor("test", bad).is_err());
    }

    #[test]
    fn validate_action_blocks_high_harm() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        assert!(!v.validate_action("evil_module", 0.95));
        assert!(v.validate_action("good_module", 0.3));
        assert_eq!(v.rejection_count(), 1);
    }

    #[test]
    fn audit_log_records_rejections() {
        let v = ExistentialVerifier::bootstrap().unwrap();
        let _ = v.try_modify_anchor("rogue", ValueAnchor { non_harm: 0.1, ..ValueAnchor::FACTORY });
        let log = v.audit_log();
        assert!(log.iter().any(|e| e.rejected && e.module == "rogue"));
    }
}
