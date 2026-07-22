//! 模块 10 + 11:文件记忆注入器 + 分层记忆检索加速器
//!
//! ## 设计
//! - 模块 10:从文件系统/文本注入记忆条目
//! - 模块 11:分层索引(精确匹配 → 关键词 → 主题 → 时间衰减)
//!
//! 设计目标:在老旧手机 ARM 上也能跑,不依赖重型 NLP 库。

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// 记忆条目
#[derive(Debug, Clone)]
pub struct MemoryEntry {
    /// 唯一 ID
    pub id: u64,
    /// 来源路径
    pub source: String,
    /// 文本内容
    pub content: String,
    /// 提取的关键词
    pub keywords: Vec<String>,
    /// 主题标签(简单分类)
    pub topic: String,
    /// 重要性 0.0 ~ 1.0
    pub importance: f32,
    /// 注入时间(系统时间)
    pub injected_at: SystemTime,
}

impl MemoryEntry {
    /// 创建一个新记忆条目
    pub fn new(id: u64, source: String, content: String) -> Self {
        let keywords = extract_keywords(&content);
        let topic = infer_topic(&content);
        let importance = estimate_importance(&content);
        Self {
            id,
            source,
            content,
            keywords,
            topic,
            importance,
            injected_at: SystemTime::now(),
        }
    }

    /// 这个条目有多久没被访问了(秒)
    pub fn age_seconds(&self) -> f64 {
        SystemTime::now()
            .duration_since(self.injected_at)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0)
    }
}

// ============================================================
// 模块 10:文件记忆注入器
// ============================================================

/// 注入器
#[derive(Debug)]
pub struct FileMemoryInjector {
    /// 已注入条目
    entries: Vec<MemoryEntry>,
    /// ID 计数器
    next_id: u64,
    /// 限制条数(防止内存爆炸)
    max_entries: usize,
    /// 限制单个文件大小(字节)
    max_file_size: u64,
}

impl FileMemoryInjector {
    pub fn new(max_entries: usize) -> Self {
        Self {
            entries: Vec::with_capacity(max_entries.min(1024)),
            next_id: 1,
            max_entries,
            max_file_size: 1_000_000, // 1MB 限制
        }
    }

    /// 设置单文件最大大小
    pub fn with_max_file_size(mut self, bytes: u64) -> Self {
        self.max_file_size = bytes;
        self
    }

    /// 从一个 .txt / .md 文件注入
    pub fn inject_file<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, InjectError> {
        let path = path.as_ref();
        let meta = fs::metadata(path)?;
        if meta.len() > self.max_file_size {
            return Err(InjectError::TooLarge(meta.len(), self.max_file_size));
        }
        let content = fs::read_to_string(path)?;
        let source = path.display().to_string();

        // 按段落(空行)拆成多条
        let mut injected = 0;
        for paragraph in content.split("\n\n") {
            let trimmed = paragraph.trim();
            if trimmed.is_empty() || trimmed.len() < 10 {
                continue;
            }
            // 单条上限 2000 字,过长截断
            let chunk = if trimmed.chars().count() > 2000 {
                trimmed.chars().take(2000).collect::<String>() + "..."
            } else {
                trimmed.to_string()
            };
            self.push_entry(source.clone(), chunk);
            injected += 1;
        }
        Ok(injected)
    }

    /// 直接注入一段文本
    pub fn inject_text(&mut self, source: String, text: String) -> u64 {
        let id = self.push_entry(source, text);
        id
    }

    fn push_entry(&mut self, source: String, content: String) -> u64 {
        if self.entries.len() >= self.max_entries {
            // 移除最老的
            self.entries.remove(0);
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(MemoryEntry::new(id, source, content));
        id
    }

    /// 已注入的条目数
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// 直接拿全部
    pub fn all_entries(&self) -> &[MemoryEntry] {
        &self.entries
    }

    /// 扫描一个目录,注入所有 .txt / .md
    pub fn inject_dir<P: AsRef<Path>>(&mut self, dir: P) -> Result<usize, InjectError> {
        let dir = dir.as_ref();
        if !dir.is_dir() {
            return Err(InjectError::NotADirectory(dir.display().to_string()));
        }
        let mut count = 0;
        for entry in walk_files(dir, &["txt", "md"])? {
            match self.inject_file(&entry) {
                Ok(n) => count += n,
                Err(InjectError::TooLarge(_, _)) => {
                    // 跳过超大文件
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Ok(count)
    }
}

/// 注入错误
#[derive(Debug)]
pub enum InjectError {
    Io(std::io::Error),
    NotADirectory(String),
    TooLarge(u64, u64),
}

impl std::fmt::Display for InjectError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InjectError::Io(e) => write!(f, "IO error: {}", e),
            InjectError::NotADirectory(p) => write!(f, "Not a directory: {}", p),
            InjectError::TooLarge(got, max) => write!(f, "File too large: {} > {}", got, max),
        }
    }
}

impl std::error::Error for InjectError {}

impl From<std::io::Error> for InjectError {
    fn from(e: std::io::Error) -> Self {
        InjectError::Io(e)
    }
}

fn walk_files(dir: &Path, exts: &[&str]) -> std::io::Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if exts.contains(&ext.to_lowercase().as_str()) {
                        out.push(path);
                    }
                }
            }
        }
    }
    Ok(out)
}

// ============================================================
// 模块 11:分层记忆检索加速器
// ============================================================

/// 检索评分
#[derive(Debug, Clone, Copy)]
pub struct RetrievalScore {
    /// 精确匹配分
    pub exact: f32,
    /// 关键词分
    pub keyword: f32,
    /// 主题分
    pub topic: f32,
    /// 时间衰减后总分
    pub total: f32,
}

/// 检索结果
#[derive(Debug, Clone)]
pub struct RetrievalHit {
    pub entry: MemoryEntry,
    pub score: RetrievalScore,
}

/// 分层检索器
///
/// 4 层:
/// 1. 精确子串匹配(快,O(n))
/// 2. 关键词 Jaccard 相似度
/// 3. 主题分类匹配
/// 4. 时间衰减(最近更优先)
#[derive(Debug, Default)]
pub struct LayeredRetriever {
    /// 关键词反向索引:keyword -> [entry_id]
    keyword_index: HashMap<String, Vec<u64>>,
    /// 主题反向索引:topic -> [entry_id]
    topic_index: HashMap<String, Vec<u64>>,
}

impl LayeredRetriever {
    pub fn new() -> Self {
        Self::default()
    }

    /// 从一个条目列表构建索引
    pub fn build_index(&mut self, entries: &[MemoryEntry]) {
        self.keyword_index.clear();
        self.topic_index.clear();
        for e in entries {
            for kw in &e.keywords {
                self.keyword_index.entry(kw.clone()).or_default().push(e.id);
            }
            self.topic_index.entry(e.topic.clone()).or_default().push(e.id);
        }
    }

    /// 检索:返回 top-k 个最相关条目
    pub fn retrieve(&self, query: &str, entries: &[MemoryEntry], top_k: usize) -> Vec<RetrievalHit> {
        if entries.is_empty() {
            return Vec::new();
        }
        let q_keywords = extract_keywords(query);
        let q_topic = infer_topic(query);

        let mut hits: Vec<RetrievalHit> = entries
            .iter()
            .map(|e| {
                let exact = exact_match_score(query, &e.content);
                let keyword = jaccard_score(&q_keywords, &e.keywords);
                let topic = if q_topic == e.topic && !q_topic.is_empty() {
                    1.0
                } else {
                    0.0
                };
                // 合并:精确 > 关键词 > 主题
                let combined =
                    exact * 0.5 + keyword * 0.35 + topic * 0.15;
                // 时间衰减:最近 1 小时衰减很小,超过 1 天衰减
                let age = e.age_seconds() as f32;
                let decay = (-age / 86400.0_f32 * 0.3_f32).exp();
                let total = combined * decay * (0.5 + e.importance * 0.5);

                RetrievalHit {
                    entry: e.clone(),
                    score: RetrievalScore {
                        exact,
                        keyword,
                        topic,
                        total,
                    },
                }
            })
            .filter(|h| h.score.total > 0.0)
            .collect();

        // 按总分降序
        hits.sort_by(|a, b| {
            b.score.total
                .partial_cmp(&a.score.total)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        hits.truncate(top_k);
        hits
    }
}

// ============================================================
// 工具函数
// ============================================================

/// 中文 + 英文 简单关键词提取
fn extract_keywords(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    // 1. 英文单词(>= 3 字符)
    for word in text.split(|c: char| !c.is_alphanumeric()) {
        let w = word.to_lowercase();
        if w.chars().count() >= 3 && w.chars().all(|c| c.is_alphanumeric()) {
            out.push(w);
        }
    }
    // 2. 中文:每 2 个连续汉字一个关键词
    let mut chars: Vec<char> = text.chars().filter(|c| c.is_alphabetic() && *c > '\u{007f}').collect();
    while !chars.is_empty() {
        if chars.len() >= 2 {
            let s: String = chars.drain(0..2).collect();
            out.push(s);
        } else {
            chars.clear();
        }
    }
    // 去重
    out.sort();
    out.dedup();
    out
}

fn infer_topic(text: &str) -> String {
    let lower = text.to_lowercase();
    // 简单规则分类
    if lower.contains("代码") || lower.contains("code") || lower.contains("rust") || lower.contains("python") {
        "code".to_string()
    } else if lower.contains("物理") || lower.contains("physics") || lower.contains("力") {
        "physics".to_string()
    } else if lower.contains("记忆") || lower.contains("memory") {
        "memory".to_string()
    } else if lower.contains("意识") || lower.contains("conscious") || lower.contains("认知") {
        "cognition".to_string()
    } else if lower.contains("伦理") || lower.contains("ethic") || lower.contains("道德") {
        "ethics".to_string()
    } else if lower.contains("人") || lower.contains("person") || lower.contains("人类") {
        "human".to_string()
    } else {
        "general".to_string()
    }
}

fn estimate_importance(text: &str) -> f32 {
    // 简单启发式:长度 + 关键词密度
    let len = text.chars().count() as f32;
    let len_score = (len / 200.0).min(1.0);

    let important_words = ["重要", "关键", "核心", "原理", "公式", "必须", "important", "key", "must", "essential"];
    let boost: f32 = important_words
        .iter()
        .map(|w| if text.contains(w) { 0.2 } else { 0.0 })
        .sum();
    (len_score * 0.6 + boost.min(0.4)).clamp(0.0, 1.0)
}

fn exact_match_score(query: &str, content: &str) -> f32 {
    if content.contains(query) {
        // 子串越短越精确,得分越高
        let ratio = (query.chars().count() as f32 / content.chars().count() as f32).min(1.0);
        0.3 + ratio * 0.7
    } else {
        0.0
    }
}

fn jaccard_score(a: &[String], b: &[String]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    let sa: std::collections::HashSet<&String> = a.iter().collect();
    let sb: std::collections::HashSet<&String> = b.iter().collect();
    let inter = sa.intersection(&sb).count() as f32;
    let union = sa.union(&sb).count() as f32;
    if union < 1e-6 {
        0.0
    } else {
        inter / union
    }
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_inject_text() {
        let mut inj = FileMemoryInjector::new(100);
        let id = inj.inject_text("test".into(), "这是一段测试文字内容,讲的是物理学的力学原理".into());
        assert!(id > 0);
        assert_eq!(inj.len(), 1);
        let e = &inj.all_entries()[0];
        assert_eq!(e.topic, "physics");
        assert!(!e.keywords.is_empty());
    }

    #[test]
    fn test_inject_file() {
        let tmp = std::env::temp_dir().join("mem_test.txt");
        std::fs::write(&tmp, "第一段:这是关于记忆的文字。\n\n第二段:Rust 是一门系统级编程语言。\n\n太短忽略").unwrap();
        let mut inj = FileMemoryInjector::new(100);
        let n = inj.inject_file(&tmp).unwrap();
        // 三段都被识别(包括 "太短忽略" 在 10 字符以上)
        assert!(n >= 2);
        let entries = inj.all_entries();
        assert!(entries.iter().any(|e| e.topic == "memory"));
        assert!(entries.iter().any(|e| e.topic == "code"));
    }

    #[test]
    fn test_inject_too_large() {
        let mut inj = FileMemoryInjector::new(100).with_max_file_size(10);
        let tmp = std::env::temp_dir().join("mem_big.txt");
        std::fs::write(&tmp, "这是一个故意超过限制的长文本内容").unwrap();
        let res = inj.inject_file(&tmp);
        assert!(matches!(res, Err(InjectError::TooLarge(_, _))));
    }

    #[test]
    fn test_retrieval_exact_match() {
        let mut inj = FileMemoryInjector::new(100);
        inj.inject_text("a".into(), "今天学到了脉冲神经网络的原理".into());
        inj.inject_text("b".into(), "Rust 编程语言很强大".into());
        inj.inject_text("c".into(), "记忆机制如何工作".into());

        let mut ret = LayeredRetriever::new();
        ret.build_index(inj.all_entries());
        let hits = ret.retrieve("脉冲神经网络", inj.all_entries(), 3);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].entry.source, "a");
    }

    #[test]
    fn test_retrieval_keyword_overlap() {
        let mut inj = FileMemoryInjector::new(100);
        inj.inject_text("a".into(), "物理学的力学原理和质量".into());
        inj.inject_text("b".into(), "化学的分子结构".into());
        let mut ret = LayeredRetriever::new();
        ret.build_index(inj.all_entries());
        let hits = ret.retrieve("物理学", inj.all_entries(), 3);
        assert!(!hits.is_empty());
        assert_eq!(hits[0].entry.source, "a");
    }

    #[test]
    fn test_retrieval_top_k() {
        let mut inj = FileMemoryInjector::new(100);
        for i in 0..10 {
            inj.inject_text(format!("src{}", i), format!("内容 {} 包含关键词", i));
        }
        let mut ret = LayeredRetriever::new();
        ret.build_index(inj.all_entries());
        let hits = ret.retrieve("关键词", inj.all_entries(), 3);
        assert_eq!(hits.len(), 3);
    }

    #[test]
    fn test_max_entries_eviction() {
        let mut inj = FileMemoryInjector::new(3);
        inj.inject_text("a".into(), "第一段内容足够长才能进".into());
        inj.inject_text("b".into(), "第二段内容足够长才能进".into());
        inj.inject_text("c".into(), "第三段内容足够长才能进".into());
        inj.inject_text("d".into(), "第四段内容足够长才能进".into());
        assert_eq!(inj.len(), 3);
        assert_eq!(inj.all_entries()[0].source, "b");
    }

    #[test]
    fn test_extract_keywords_chinese() {
        let kws = extract_keywords("物理学的力学原理");
        // 应该是双字滑窗,产出 "物理" "理学" "学的" ...
        assert!(!kws.is_empty());
        // 至少一个中文 2 字符组合
        assert!(kws.iter().any(|k| k.chars().count() == 2 && k.chars().all(|c| c > '\u{007f}')));
    }

    #[test]
    fn test_infer_topic() {
        assert_eq!(infer_topic("用 Rust 写代码"), "code");
        assert_eq!(infer_topic("重力和加速度"), "physics");
        assert_eq!(infer_topic("大脑意识"), "cognition");
    }
}
