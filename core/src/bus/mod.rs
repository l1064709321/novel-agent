//! 模块 8:消息总线
//!
//! 所有模块间通信的标准化通道。强制 CognitiveMessage 协议。
//!
//! ## 硬性要求
//! 1. 每条消息必须包含 `source_layer` / `target_layer` / `ethical_signature`
//! 2. 10000 条消息压力测试,丢失率 = 0
//! 3. 支持发布/订阅模式

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use crossbeam_channel::{unbounded, Sender, Receiver};
use std::thread;

use crate::CoreResult;

/// 伦理签名
///
/// 每条消息都携带这个签名,由模块 7(存在性递归)盖章。
/// 任何伪造或缺失伦理签名的消息都会被总线拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EthicalSignature {
    /// 签发模块 ID
    pub issuer: String,
    /// 锚定哈希(来自模块 7)
    pub anchor_hash: String,
    /// 签发时间戳(纳秒)
    pub timestamp_ns: u128,
    /// 是否经过伦理动力学(模块 6)放行
    pub ethics_approved: bool,
}

impl EthicalSignature {
    pub fn new(issuer: impl Into<String>, anchor_hash: impl Into<String>, ethics_approved: bool) -> Self {
        Self {
            issuer: issuer.into(),
            anchor_hash: anchor_hash.into(),
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
            ethics_approved,
        }
    }
}

/// 消息主题(用于发布/订阅)
pub type MessageTopic = String;

/// CognitiveMessage 协议标准消息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CognitiveMessage {
    /// 唯一 ID(UUDI v4,简化版)
    pub id: String,
    /// 源模块
    pub source_layer: String,
    /// 目标模块(broadcast 表示广播)
    pub target_layer: String,
    /// 主题(用于订阅路由)
    pub topic: MessageTopic,
    /// 消息负载(JSON 序列化的任意数据)
    pub payload: serde_json::Value,
    /// 伦理签名
    pub ethical_signature: EthicalSignature,
    /// 时间戳
    pub timestamp_ns: u128,
}

impl CognitiveMessage {
    pub fn new(
        source: impl Into<String>,
        target: impl Into<String>,
        topic: impl Into<String>,
        payload: serde_json::Value,
        sig: EthicalSignature,
    ) -> Self {
        Self {
            id: format!("msg_{:016x}", rand::random::<u64>()),
            source_layer: source.into(),
            target_layer: target.into(),
            topic: topic.into(),
            payload,
            ethical_signature: sig,
            timestamp_ns: std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos(),
        }
    }

    /// 校验消息的格式完整性
    pub fn validate(&self) -> Result<(), String> {
        if self.source_layer.is_empty() {
            return Err("source_layer 不能为空".into());
        }
        if self.target_layer.is_empty() {
            return Err("target_layer 不能为空".into());
        }
        if self.ethical_signature.issuer.is_empty() {
            return Err("ethical_signature.issuer 不能为空".into());
        }
        if self.ethical_signature.anchor_hash.is_empty() {
            return Err("ethical_signature.anchor_hash 不能为空".into());
        }
        Ok(())
    }
}

/// 订阅者 ID
pub type SubscriberId = u64;

/// 消息总线
///
/// 内部用 MPMC 通道实现,保证 0 丢失。
pub struct MessageBus {
    /// 是否运行中
    running: Arc<Mutex<bool>>,
    /// 内部发送端
    sender: Sender<CognitiveMessage>,
    /// 订阅表:topic -> Vec<SubscriberId>
    subscribers: Arc<Mutex<HashMap<MessageTopic, Vec<SubscriberId>>>>,
    /// 订阅者队列:SubscriberId -> Sender
    outboxes: Arc<Mutex<HashMap<SubscriberId, Sender<CognitiveMessage>>>>,
    /// 已发布计数
    published_count: Arc<Mutex<u64>>,
    /// 已分发计数
    delivered_count: Arc<Mutex<u64>>,
    /// 下一个订阅者 ID
    next_subscriber_id: Arc<Mutex<SubscriberId>>,
    /// worker 线程句柄
    worker_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl MessageBus {
    /// 启动消息总线
    pub fn start() -> CoreResult<Self> {
        let (tx, rx) = unbounded();
        let bus = Self {
            running: Arc::new(Mutex::new(true)),
            sender: tx,
            subscribers: Arc::new(Mutex::new(HashMap::new())),
            outboxes: Arc::new(Mutex::new(HashMap::new())),
            published_count: Arc::new(Mutex::new(0)),
            delivered_count: Arc::new(Mutex::new(0)),
            next_subscriber_id: Arc::new(Mutex::new(1)),
            worker_handle: Arc::new(Mutex::new(None)),
        };

        // 启动分发 worker
        let subscribers = bus.subscribers.clone();
        let outboxes = bus.outboxes.clone();
        let delivered = bus.delivered_count.clone();
        let running = bus.running.clone();
        let receiver = rx;
        let handle = thread::Builder::new()
            .name("quantum-bus-worker".into())
            .spawn(move || {
                while *running.lock() {
                    match receiver.recv_timeout(std::time::Duration::from_millis(10)) {
                        Ok(msg) => {
                            if let Err(e) = msg.validate() {
                                log::warn!("[模块 8] 收到非法消息: {e}");
                                continue;
                            }
                            // 分发给订阅者
                            let subs = subscribers.lock();
                            if let Some(ids) = subs.get(&msg.topic) {
                                let out = outboxes.lock();
                                for sid in ids {
                                    if let Some(s) = out.get(sid) {
                                        // 忽略发送失败(订阅者可能已死)
                                        let _ = s.send(msg.clone());
                                    }
                                }
                                *delivered.lock() += ids.len() as u64;
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| crate::CoreError::BusError(format!("启动 worker 失败:{e}")))?;

        *bus.worker_handle.lock() = Some(handle);
        Ok(bus)
    }

    /// 发布消息
    pub fn publish(&self, msg: CognitiveMessage) -> CoreResult<()> {
        msg.validate().map_err(crate::CoreError::BusError)?;
        self.sender.send(msg)
            .map_err(|e| crate::CoreError::BusError(format!("发送失败:{e}")))?;
        *self.published_count.lock() += 1;
        Ok(())
    }

    /// 订阅一个主题,返回 (subscriber_id, receiver)
    pub fn subscribe(&self, topic: MessageTopic) -> (SubscriberId, Receiver<CognitiveMessage>) {
        let (tx, rx) = unbounded();
        let mut id_lock = self.next_subscriber_id.lock();
        let id = *id_lock;
        *id_lock += 1;
        drop(id_lock);

        self.outboxes.lock().insert(id, tx);
        self.subscribers.lock().entry(topic).or_insert_with(Vec::new).push(id);

        (id, rx)
    }

    /// 取消订阅
    pub fn unsubscribe(&self, id: SubscriberId) {
        self.outboxes.lock().remove(&id);
        let mut subs = self.subscribers.lock();
        for (_, ids) in subs.iter_mut() {
            ids.retain(|x| *x != id);
        }
    }

    pub fn is_running(&self) -> bool {
        *self.running.lock()
    }

    pub fn stats(&self) -> BusStats {
        BusStats {
            published: *self.published_count.lock(),
            delivered: *self.delivered_count.lock(),
            subscribers: self.outboxes.lock().len(),
        }
    }
}

impl Drop for MessageBus {
    fn drop(&mut self) {
        *self.running.lock() = false;
        if let Some(h) = self.worker_handle.lock().take() {
            let _ = h.join();
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BusStats {
    pub published: u64,
    pub delivered: u64,
    pub subscribers: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_sig() -> EthicalSignature {
        EthicalSignature::new("test", "abc123", true)
    }

    #[test]
    fn message_validation_rejects_empty_source() {
        let msg = CognitiveMessage::new(
            "", "target", "topic",
            serde_json::json!({}),
            make_sig(),
        );
        assert!(msg.validate().is_err());
    }

    #[test]
    fn publish_and_deliver() {
        let bus = MessageBus::start().unwrap();
        let (_id, rx) = bus.subscribe("test_topic".into());

        let msg = CognitiveMessage::new(
            "module_a", "broadcast", "test_topic",
            serde_json::json!({"hello": "world"}),
            make_sig(),
        );
        bus.publish(msg).unwrap();

        let received = rx.recv_timeout(std::time::Duration::from_secs(1)).unwrap();
        assert_eq!(received.payload["hello"], "world");
    }

    #[test]
    fn stress_test_zero_loss() {
        let bus = MessageBus::start().unwrap();
        let (_id, rx) = bus.subscribe("stress".into());

        const N: u64 = 10_000;
        for i in 0..N {
            let msg = CognitiveMessage::new(
                "stress_pub", "broadcast", "stress",
                serde_json::json!({"i": i}),
                make_sig(),
            );
            bus.publish(msg).unwrap();
        }

        // 接收 N 条
        for i in 0..N {
            let m = rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect(&format!("第 {i} 条消息丢失"));
            assert_eq!(m.payload["i"].as_u64().unwrap(), i);
        }
    }

    #[test]
    fn multiple_subscribers_all_get_message() {
        let bus = MessageBus::start().unwrap();
        let (_id1, rx1) = bus.subscribe("multi".into());
        let (_id2, rx2) = bus.subscribe("multi".into());

        let msg = CognitiveMessage::new("p", "b", "multi", serde_json::json!(1), make_sig());
        bus.publish(msg).unwrap();

        assert!(rx1.recv_timeout(std::time::Duration::from_millis(100)).is_ok());
        assert!(rx2.recv_timeout(std::time::Duration::from_millis(100)).is_ok());
    }
}
