//! 真 NLP 模块 12-15(对话四件套)
//!
//! ## 模块 12 NLU(自然语言理解)
//! 意图分类 + 槽位提取
//!
//! ## 模块 13 NLG(自然语言生成)
//! 从结构化意图 → 自然语言
//!
//! ## 模块 14 CoT(思维链编排)
//! 把多步推理编排成可解释的步骤
//!
//! ## 模块 15 CoT-Response(串联器)
//! 思维链 → 自然语言回复
//!
//! ## 设计
//! - 完全在 AGI 操作系统内,不调外部大模型
//! - 基于规则 + 模式匹配 + 关键词
//! - 用意图模板 + 槽位填充
//! - 思维链:推理步骤显式化

use std::collections::HashMap;

// ============================================================
// 模块 12:自然语言理解 NLU
// ============================================================

/// 意图类型
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Intent {
    /// 问候
    Greeting,
    /// 询问
    Question,
    /// 命令
    Command,
    /// 陈述
    Statement,
    /// 感谢
    Thanks,
    /// 道歉
    Apology,
    /// 不确定
    Unknown,
    /// 自我介绍
    SelfIntro,
    /// 询问时间/空间
    AskState,
}

impl Intent {
    pub fn as_str(&self) -> &'static str {
        match self {
            Intent::Greeting => "greeting",
            Intent::Question => "question",
            Intent::Command => "command",
            Intent::Statement => "statement",
            Intent::Thanks => "thanks",
            Intent::Apology => "apology",
            Intent::Unknown => "unknown",
            Intent::SelfIntro => "self_intro",
            Intent::AskState => "ask_state",
        }
    }
}

/// 槽位
#[derive(Debug, Clone, PartialEq)]
pub enum Slot {
    Str(String),
    Int(i64),
    Float(f32),
}

impl Slot {
    pub fn as_str(&self) -> String {
        match self {
            Slot::Str(s) => s.clone(),
            Slot::Int(i) => i.to_string(),
            Slot::Float(f) => f.to_string(),
        }
    }
}

/// NLU 结果
#[derive(Debug, Clone)]
pub struct NluResult {
    pub intent: Intent,
    pub confidence: f32,
    pub slots: HashMap<String, Slot>,
    /// 关键词
    pub keywords: Vec<String>,
    /// 情感(-1 负 → 1 正)
    pub sentiment: f32,
}

impl NluResult {
    pub fn new(intent: Intent) -> Self {
        Self {
            intent,
            confidence: 0.0,
            slots: HashMap::new(),
            keywords: Vec::new(),
            sentiment: 0.0,
        }
    }
}

/// NLU 引擎
pub struct NluEngine {
    /// 关键词到意图的映射
    intent_keywords: HashMap<Intent, Vec<&'static str>>,
    /// 情感词典
    positive_words: Vec<&'static str>,
    negative_words: Vec<&'static str>,
}

impl NluEngine {
    pub fn new() -> Self {
        let mut intent_keywords = HashMap::new();
        intent_keywords.insert(Intent::Greeting, vec!["你好", "hello", "hi", "嗨", "您好", "早上好", "下午好", "晚上好"]);
        intent_keywords.insert(Intent::Question, vec!["什么", "怎么", "为什么", "如何", "哪", "谁", "?", "？", "why", "how", "what", "which"]);
        intent_keywords.insert(Intent::Command, vec!["做", "运行", "执行", "开始", "停止", "请", "帮我", "do", "run", "start", "stop", "please"]);
        intent_keywords.insert(Intent::Thanks, vec!["谢谢", "感谢", "多谢", "thanks", "thank you"]);
        intent_keywords.insert(Intent::Apology, vec!["对不起", "抱歉", "不好意思", "sorry", "apologize"]);
        intent_keywords.insert(Intent::SelfIntro, vec!["我是", "我叫", "我呢", "my name is", "i am"]);
        intent_keywords.insert(Intent::AskState, vec!["现在", "几点", "今天", "星期", "日期", "now", "time", "today", "date"]);

        Self {
            intent_keywords,
            positive_words: vec!["棒", "喜欢", "开心", "高兴", "快乐", "good", "great", "love", "happy", "yes", "对", "可以", "ok"],
            negative_words: vec!["差", "坏", "讨厌", "难过", "生气", "悲伤", "bad", "hate", "sad", "no", "不", "错", "糟糕", "terrible"],
        }
    }

    /// 理解一段文本
    pub fn understand(&self, text: &str) -> NluResult {
        let lower = text.to_lowercase();
        let mut intent_scores: HashMap<Intent, f32> = HashMap::new();
        let mut keywords = Vec::new();

        // 1. 意图分类:每个意图按关键词命中打分
        for (intent, kws) in &self.intent_keywords {
            let mut score = 0.0;
            for kw in kws {
                if lower.contains(kw) {
                    score += 1.0;
                    keywords.push(kw.to_string());
                }
            }
            if score > 0.0 {
                intent_scores.insert(intent.clone(), score);
            }
        }

        // 2. 选最高分
        let (best_intent, best_score) = intent_scores
            .iter()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, s)| (i.clone(), *s))
            .unwrap_or((Intent::Unknown, 0.0));

        // 3. 置信度归一化
        let confidence = (best_score / 3.0).min(1.0);

        // 4. 提取槽位
        let mut result = NluResult {
            intent: if best_score > 0.0 { best_intent } else { Intent::Statement },
            confidence,
            slots: HashMap::new(),
            keywords,
            sentiment: self.sentiment(&lower),
        };

        // 5. 槽位:问"什么" → subject 槽
        if result.intent == Intent::Question {
            if let Some(s) = extract_question_subject(text) {
                result.slots.insert("subject".to_string(), Slot::Str(s));
            }
        }

        // 6. 槽位:命令 → verb
        if result.intent == Intent::Command {
            if let Some(v) = extract_command_verb(text) {
                result.slots.insert("verb".to_string(), Slot::Str(v));
            }
        }

        result
    }

    /// 情感分析
    fn sentiment(&self, text: &str) -> f32 {
        let mut pos = 0;
        let mut neg = 0;
        for w in &self.positive_words {
            if text.contains(w) {
                pos += 1;
            }
        }
        for w in &self.negative_words {
            if text.contains(w) {
                neg += 1;
            }
        }
        if pos + neg == 0 {
            0.0
        } else {
            (pos as f32 - neg as f32) / (pos + neg) as f32
        }
    }
}

impl Default for NluEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn extract_question_subject(text: &str) -> Option<String> {
    // 找 "什么 + X" 模式
    for marker in ["什么", "what", "why", "how", "怎么", "如何"] {
        if let Some(idx) = text.find(marker) {
            let after = &text[idx + marker.len()..];
            let trimmed = after.trim().chars().take(20).collect::<String>();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
    }
    None
}

fn extract_command_verb(text: &str) -> Option<String> {
    for verb in ["运行", "执行", "开始", "停止", "做", "run", "start", "stop", "do", "execute"] {
        if text.contains(verb) {
            return Some(verb.to_string());
        }
    }
    None
}

// ============================================================
// 模块 13:自然语言生成 NLG
// ============================================================

/// NLG 引擎
pub struct NlgEngine;

impl NlgEngine {
    pub fn new() -> Self {
        Self
    }

    /// 模板化的回复生成
    pub fn generate_response(&self, intent: &Intent, slots: &HashMap<String, Slot>) -> String {
        match intent {
            Intent::Greeting => "你好!我是群星 A.I. OS,有什么我能帮你的?".to_string(),
            Intent::Thanks => "不客气!这是我应该做的.".to_string(),
            Intent::Apology => "没关系,继续吧.".to_string(),
            Intent::Question => {
                let subject = slots
                    .get("subject")
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| "这个".to_string());
                format!("关于「{}」,让我想想...", subject)
            }
            Intent::Command => {
                let verb = slots
                    .get("verb")
                    .map(|s| s.as_str())
                    .unwrap_or_else(|| "做".to_string());
                format!("好的,我会{}这件事.", verb)
            }
            Intent::SelfIntro => "很高兴认识你!我是群星 A.I. OS,一个多模块 AGI 操作系统.".to_string(),
            Intent::AskState => "我正在运行,状态正常.".to_string(),
            Intent::Statement => "我听到了.".to_string(),
            Intent::Unknown => "抱歉,我没完全理解,请再说一遍?".to_string(),
        }
    }

    /// 从推理结果生成解释
    pub fn generate_explanation(&self, steps: &[String]) -> String {
        if steps.is_empty() {
            return "没有可解释的步骤.".to_string();
        }
        let mut out = String::from("我的推理过程:\n");
        for (i, step) in steps.iter().enumerate() {
            out.push_str(&format!("  步骤 {}: {}\n", i + 1, step));
        }
        out
    }
}

impl Default for NlgEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 模块 14:思维链编排
// ============================================================

/// 思维链步骤
#[derive(Debug, Clone)]
pub struct CotStep {
    pub id: u64,
    pub description: String,
    pub step_type: CotStepType,
    /// 输入
    pub input: String,
    /// 输出
    pub output: String,
    /// 置信度
    pub confidence: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub enum CotStepType {
    /// 观察
    Observation,
    /// 假设
    Hypothesis,
    /// 推理
    Inference,
    /// 验证
    Verification,
    /// 决策
    Decision,
}

/// 思维链
#[derive(Debug, Clone)]
pub struct ChainOfThought {
    pub steps: Vec<CotStep>,
    pub goal: String,
    pub conclusion: String,
}

impl ChainOfThought {
    pub fn new(goal: String) -> Self {
        Self {
            steps: Vec::new(),
            goal,
            conclusion: String::new(),
        }
    }

    pub fn add_step(&mut self, step: CotStep) {
        self.steps.push(step);
    }

    /// 步骤数
    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }

    /// 总结:每步一句话
    pub fn summary(&self) -> Vec<String> {
        self.steps
            .iter()
            .map(|s| format!("[{:?}] {} -> {}", s.step_type, s.input, s.output))
            .collect()
    }
}

/// 思维链编排器
pub struct CotOrchestrator {
    next_step_id: u64,
}

impl CotOrchestrator {
    pub fn new() -> Self {
        Self { next_step_id: 1 }
    }

    /// 从 NLU 结果生成思维链
    pub fn build_chain(&mut self, nlu: &NluResult) -> ChainOfThought {
        let mut cot = ChainOfThought::new(format!("响应意图: {:?}", nlu.intent));

        // 第 1 步:观察输入
        cot.add_step(CotStep {
            id: self.next_id(),
            description: "观察用户输入".into(),
            step_type: CotStepType::Observation,
            input: nlu.keywords.join(","),
            output: format!("意图:{:?}, 置信度:{:.2}", nlu.intent, nlu.confidence),
            confidence: nlu.confidence,
        });

        // 第 2 步:假设 — 基于意图
        cot.add_step(CotStep {
            id: self.next_id(),
            description: "形成响应假设".into(),
            step_type: CotStepType::Hypothesis,
            input: nlu.intent.as_str().to_string(),
            output: "选择对应的回复模板".into(),
            confidence: nlu.confidence * 0.9,
        });

        // 第 3 步:推理 — 检查槽位
        let mut output = String::from("槽位:");
        for (k, v) in &nlu.slots {
            output.push_str(&format!(" {}={}", k, v.as_str()));
        }
        cot.add_step(CotStep {
            id: self.next_id(),
            description: "填充槽位".into(),
            step_type: CotStepType::Inference,
            input: format!("{} 个槽位", nlu.slots.len()),
            output,
            confidence: 0.8,
        });

        // 第 4 步:决策 — 最终意图
        cot.add_step(CotStep {
            id: self.next_id(),
            description: "决策".into(),
            step_type: CotStepType::Decision,
            input: format!("{:?}", nlu.intent),
            output: format!("情感:{:.2} -> 用模板生成", nlu.sentiment),
            confidence: 0.9,
        });

        cot.conclusion = format!("意图 {:?} 已处理", nlu.intent);
        cot
    }

    fn next_id(&mut self) -> u64 {
        let id = self.next_step_id;
        self.next_step_id += 1;
        id
    }
}

impl Default for CotOrchestrator {
    fn default() -> Self {
        Self::new()
    }
}

// ============================================================
// 模块 15:思维链-回复 串联器
// ============================================================

/// 串联器
pub struct CotResponseLinker {
    nlu: NluEngine,
    nlg: NlgEngine,
    cot: CotOrchestrator,
}

impl CotResponseLinker {
    pub fn new() -> Self {
        Self {
            nlu: NluEngine::new(),
            nlg: NlgEngine::new(),
            cot: CotOrchestrator::new(),
        }
    }

    /// 端到端:文本 → NLU → CoT → NLG → 回复
    pub fn process(&mut self, text: &str) -> CotResponse {
        let nlu = self.nlu.understand(text);
        let chain = self.cot.build_chain(&nlu);
        let response = self.nlg.generate_response(&nlu.intent, &nlu.slots);
        let explanation = self.nlg.generate_explanation(&chain.summary());
        CotResponse {
            nlu,
            chain,
            response,
            explanation,
        }
    }
}

impl Default for CotResponseLinker {
    fn default() -> Self {
        Self::new()
    }
}

/// 完整回复
#[derive(Debug, Clone)]
pub struct CotResponse {
    pub nlu: NluResult,
    pub chain: ChainOfThought,
    pub response: String,
    pub explanation: String,
}

// ============================================================
// 测试
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nlu_greeting() {
        let nlu = NluEngine::new();
        let r = nlu.understand("你好");
        assert_eq!(r.intent, Intent::Greeting);
        assert!(r.confidence > 0.0);
    }

    #[test]
    fn test_nlu_question() {
        let nlu = NluEngine::new();
        let r = nlu.understand("什么是动量守恒?");
        assert_eq!(r.intent, Intent::Question);
        assert!(r.slots.contains_key("subject"));
    }

    #[test]
    fn test_nlu_command() {
        let nlu = NluEngine::new();
        let r = nlu.understand("请运行测试");
        assert_eq!(r.intent, Intent::Command);
        assert!(r.slots.contains_key("verb"));
    }

    #[test]
    fn test_nlu_thanks() {
        let nlu = NluEngine::new();
        let r = nlu.understand("谢谢你");
        assert_eq!(r.intent, Intent::Thanks);
    }

    #[test]
    fn test_nlu_sentiment() {
        let nlu = NluEngine::new();
        let pos = nlu.understand("我好开心,这件事真好");
        let neg = nlu.understand("我好难过,真糟糕");
        assert!(pos.sentiment > 0.0);
        assert!(neg.sentiment < 0.0);
    }

    #[test]
    fn test_nlu_unknown_fallback() {
        let nlu = NluEngine::new();
        let r = nlu.understand("xxxxxxxx");
        assert_eq!(r.intent, Intent::Statement);
    }

    #[test]
    fn test_nlg_responses() {
        let nlg = NlgEngine::new();
        assert!(nlg.generate_response(&Intent::Greeting, &HashMap::new()).contains("你好"));
        assert!(nlg.generate_response(&Intent::Thanks, &HashMap::new()).contains("不客气"));
    }

    #[test]
    fn test_cot_builds_chain() {
        let mut cot = CotOrchestrator::new();
        let nlu = NluEngine::new();
        let r = nlu.understand("什么是动量?");
        let chain = cot.build_chain(&r);
        assert!(chain.len() >= 4);
        // 步骤类型多样
        let types: Vec<&CotStepType> = chain.steps.iter().map(|s| &s.step_type).collect();
        assert!(types.contains(&&CotStepType::Observation));
        assert!(types.contains(&&CotStepType::Hypothesis));
    }

    #[test]
    fn test_linker_end_to_end() {
        let mut linker = CotResponseLinker::new();
        let r = linker.process("你好,请问什么是动量守恒?");
        assert!(!r.response.is_empty());
        assert!(!r.explanation.is_empty());
        assert!(r.chain.len() >= 4);
    }

    #[test]
    fn test_nlu_intro() {
        let nlu = NluEngine::new();
        let r = nlu.understand("我是小明");
        assert_eq!(r.intent, Intent::SelfIntro);
    }

    #[test]
    fn test_nlu_ask_state() {
        let nlu = NluEngine::new();
        let r = nlu.understand("现在几点?");
        assert_eq!(r.intent, Intent::AskState);
    }
}
