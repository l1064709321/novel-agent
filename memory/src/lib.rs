//! 群星 A.I. OS - 双层记忆系统(模块 9)
//!
//! ## 设计
//! - **短期记忆**:环形缓冲区,容量固定(默认 20 tick),自动淘汰最旧
//! - **长期记忆**:键值存储 + 向量索引,按相似度检索
//! - **巩固机制**:CLS(互补学习系统),休门/伤门时触发
//!
//! ## 重要性评分
//! score = 基线(0.5) + 伦理冲突程度 * 0.3 + 状态变化剧烈度 * 0.2

use std::collections::{HashMap, VecDeque};
use serde::{Deserialize, Serialize};
use chrono::Utc;
use sha2::{Digest, Sha256};

/// 单条记忆
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryItem {
    pub id: String,
    pub content: serde_json::Value,
    pub importance: f32,
    pub ethical_conflict: f32,
    pub state_change_magnitude: f32,
    pub timestamp_ns: i64,
    /// 用于去重的内容哈希
    pub content_hash: String,
}

impl MemoryItem {
    pub fn new(content: serde_json::Value) -> Self {
        let canonical = serde_json::to_string(&content).unwrap_or_default();
        let mut hasher = Sha256::new();
        hasher.update(canonical.as_bytes());
        let content_hash = hex::encode(hasher.finalize());

        Self {
            id: format!("mem_{:016x}", rand::random::<u64>()),
            content,
            importance: 0.5,
            ethical_conflict: 0.0,
            state_change_magnitude: 0.0,
            timestamp_ns: Utc::now().timestamp_nanos_opt().unwrap_or(0),
            content_hash,
        }
    }

    /// 重要性评分
    pub fn compute_importance(&mut self) {
        self.importance = (0.5
            + 0.3 * self.ethical_conflict
            + 0.2 * self.state_change_magnitude)
            .clamp(0.0, 1.0);
    }
}

/// 短期记忆:环形缓冲
pub struct ShortTermMemory {
    capacity: usize,
    buffer: VecDeque<MemoryItem>,
}

impl ShortTermMemory {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            buffer: VecDeque::with_capacity(capacity),
        }
    }

    pub fn push(&mut self, item: MemoryItem) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_front();
        }
        self.buffer.push_back(item);
    }

    pub fn items(&self) -> &VecDeque<MemoryItem> {
        &self.buffer
    }

    pub fn len(&self) -> usize { self.buffer.len() }
    pub fn is_empty(&self) -> bool { self.buffer.is_empty() }
    pub fn capacity(&self) -> usize { self.capacity }
}

/// 长期记忆:键值 + 去重
pub struct LongTermMemory {
    by_hash: HashMap<String, MemoryItem>,
}

impl LongTermMemory {
    pub fn new() -> Self {
        Self { by_hash: HashMap::new() }
    }

    /// 添加(去重:相同 content_hash 不重复存)
    pub fn insert(&mut self, item: MemoryItem) -> bool {
        if self.by_hash.contains_key(&item.content_hash) {
            return false; // 重复
        }
        self.by_hash.insert(item.content_hash.clone(), item);
        true
    }

    pub fn get(&self, hash: &str) -> Option<&MemoryItem> {
        self.by_hash.get(hash)
    }

    pub fn len(&self) -> usize { self.by_hash.len() }

    /// 按重要性检索 top N
    pub fn top_by_importance(&self, n: usize) -> Vec<&MemoryItem> {
        let mut items: Vec<&MemoryItem> = self.by_hash.values().collect();
        items.sort_by(|a, b| b.importance.partial_cmp(&a.importance).unwrap());
        items.into_iter().take(n).collect()
    }

    /// 简单关键词检索
    pub fn search_keyword(&self, keyword: &str) -> Vec<&MemoryItem> {
        self.by_hash.values()
            .filter(|m| serde_json::to_string(&m.content)
                .map(|s| s.contains(keyword))
                .unwrap_or(false))
            .collect()
    }
}

impl Default for LongTermMemory {
    fn default() -> Self { Self::new() }
}

/// 双层记忆系统
pub struct DualMemory {
    pub stm: ShortTermMemory,
    pub ltm: LongTermMemory,
}

impl DualMemory {
    pub fn new(stm_capacity: usize) -> Self {
        Self {
            stm: ShortTermMemory::new(stm_capacity),
            ltm: LongTermMemory::new(),
        }
    }

    /// 写入(先入 STM,巩固时再转 LTM)
    pub fn write(&mut self, mut item: MemoryItem) {
        item.compute_importance();
        self.stm.push(item);
    }

    /// CLS 巩固:从 STM 筛选重要性高的,转移到 LTM
    /// 通常在休门/伤门时被模块 3 触发
    pub fn consolidate(&mut self) -> usize {
        let mut consolidated = 0;
        let items: Vec<MemoryItem> = self.stm.items().iter().cloned().collect();
        for item in items {
            if item.importance >= 0.7 {
                if self.ltm.insert(item) {
                    consolidated += 1;
                }
            }
        }
        consolidated
    }

    /// 检索(优先 LTM,再 STM)
    pub fn retrieve(&self, query: &str) -> Vec<&MemoryItem> {
        let mut results = self.ltm.search_keyword(query);
        if results.is_empty() {
            // fallback 到 STM
            return self.stm.items().iter()
                .filter(|m| serde_json::to_string(&m.content)
                    .map(|s| s.contains(query))
                    .unwrap_or(false))
                .collect();
        }
        results
    }

    pub fn total(&self) -> usize {
        self.stm.len() + self.ltm.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stm_evicts_oldest() {
        let mut stm = ShortTermMemory::new(3);
        for i in 0..5 {
            stm.push(MemoryItem::new(serde_json::json!({"i": i})));
        }
        assert_eq!(stm.len(), 3);
        // 应该是 i=2,3,4(最旧的 0,1 被淘汰)
        let first = &stm.items()[0].content["i"];
        assert_eq!(first.as_i64().unwrap(), 2);
    }

    #[test]
    fn ltm_dedupes_by_content_hash() {
        let mut ltm = LongTermMemory::new();
        let content = serde_json::json!({"text": "hello"});
        let item1 = MemoryItem::new(content.clone());
        let item2 = MemoryItem::new(content);
        assert!(ltm.insert(item1));
        assert!(!ltm.insert(item2), "相同内容应被去重");
        assert_eq!(ltm.len(), 1);
    }

    #[test]
    fn consolidation_only_promotes_high_importance() {
        let mut dm = DualMemory::new(10);
        let mut high = MemoryItem::new(serde_json::json!({"text": "important"}));
        high.ethical_conflict = 0.9;
        dm.write(high);
        let low = MemoryItem::new(serde_json::json!({"text": "trivial"}));
        dm.write(low);

        let n = dm.consolidate();
        assert_eq!(n, 1);
        assert_eq!(dm.ltm.len(), 1);
    }

    #[test]
    fn importance_weighted() {
        let mut item = MemoryItem::new(serde_json::json!({}));
        item.ethical_conflict = 1.0;
        item.compute_importance();
        assert!((item.importance - 0.8).abs() < 0.001, "importance={}", item.importance);
    }
}
