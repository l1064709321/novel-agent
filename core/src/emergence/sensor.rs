//! 传感器到脉冲的编码器
//!
//! 把物理世界状态(连续值)编码为脉冲序列(脉冲神经网络能处理的格式)
//!
//! ## 编码方案
//! - **速率编码**(rate coding):值越大,脉冲频率越高
//! - **时间编码**(temporal coding):值越大,首次脉冲越早
//!
//! 这里用速率编码,简单稳定。

use crate::lif::{LifNeuron, LifParams, SpikingNetwork, SpikeEvent};
use crate::world::{WorldEntity, WorldState, EntityKind};

/// 编码器:把物理世界状态 → 脉冲事件
pub struct SensorEncoder {
    /// 状态 → 脉冲的最大频率
    pub max_rate_hz: f32,
    /// 值归一化范围
    pub value_min: f32,
    pub value_max: f32,
    /// 当前脉冲 ID 计数器
    spike_id: u64,
}

impl SensorEncoder {
    pub fn new() -> Self {
        Self {
            max_rate_hz: 100.0,
            value_min: 0.0,
            value_max: 10.0,
            spike_id: 1,
        }
    }

    /// 把一个实体的状态编码为脉冲事件列表
    /// 每个脉冲的目标神经元 = 0..N,值映射到频率
    pub fn encode_entity(&mut self, entity: &WorldEntity, time_ms: f32) -> Vec<SpikeEvent> {
        let mut spikes = Vec::new();

        // 编码 1: y 位置(垂直高度)
        let y_rate = self.value_to_rate(entity.position[1]);
        spikes.push(SpikeEvent {
            pre_neuron: 0,
            post_neuron: 0,
            time_ms,
            weight: y_rate,
        });

        // 编码 2: y 速度
        let vy_rate = self.value_to_rate(entity.velocity[1].abs());
        spikes.push(SpikeEvent {
            pre_neuron: 1,
            post_neuron: 1,
            time_ms,
            weight: vy_rate,
        });

        // 编码 3: 能量(动能 + 势能简化为位置 + 速度)
        let energy = entity.position[1] * 9.81 + entity.velocity.iter().map(|v| v * v).sum::<f32>() * 0.5;
        let energy_rate = self.value_to_rate(energy);
        spikes.push(SpikeEvent {
            pre_neuron: 2,
            post_neuron: 2,
            time_ms,
            weight: energy_rate,
        });

        self.spike_id += 1;
        spikes
    }

    /// 把值映射为脉冲频率
    /// 0..value_max → 0..max_rate_hz
    fn value_to_rate(&self, value: f32) -> f32 {
        let normalized = ((value - self.value_min) / (self.value_max - self.value_min))
            .clamp(0.0, 1.0);
        normalized * self.max_rate_hz
    }

    /// 把多个实体的状态批量编码
    pub fn encode_world(&mut self, world: &WorldState, time_ms: f32) -> Vec<SpikeEvent> {
        let mut all = Vec::new();
        for entity in &world.entities {
            all.extend(self.encode_entity(entity, time_ms));
        }
        all
    }
}

impl Default for SensorEncoder {
    fn default() -> Self { Self::new() }
}

/// 物理世界 ↔ LIF 网络桥接
pub struct LifWorldBridge {
    pub encoder: SensorEncoder,
    pub network: SpikingNetwork,
    /// 脉冲收集器
    pub spike_buffer: Vec<SpikeEvent>,
    /// spike 统计
    pub spikes_sent: u64,
    pub spikes_received: u64,
}

impl LifWorldBridge {
    pub fn new(n_input_neurons: u32) -> Self {
        Self {
            encoder: SensorEncoder::new(),
            network: SpikingNetwork::new(n_input_neurons),
            spike_buffer: Vec::new(),
            spikes_sent: 0,
            spikes_received: 0,
        }
    }

    /// 推送一个世界状态到 LIF 网络
    pub fn feed(&mut self, world: &WorldState, time_ms: f32) {
        let spikes = self.encoder.encode_world(world, time_ms);
        for spike in spikes {
            self.spikes_sent += 1;
            self.spike_buffer.push(spike);
        }

        // 一次性推完
        for spike in &self.spike_buffer {
            self.network.inject(*spike);
        }
    }

    /// 接收网络输出
    pub fn collect_spikes(&mut self) -> Vec<SpikeEvent> {
        let mut received = Vec::new();
        while let Some(s) = self.network.try_recv_spike() {
            self.spikes_received += 1;
            received.push(s);
        }
        received
    }

    /// 一步:feed + 短暂等 + collect
    pub fn step(&mut self, world: &WorldState, time_ms: f32, wait_ms: u64) -> Vec<SpikeEvent> {
        self.feed(world, time_ms);
        std::thread::sleep(std::time::Duration::from_millis(wait_ms));
        self.collect_spikes()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_entity() -> WorldEntity {
        WorldEntity {
            id: 1,
            kind: EntityKind::RigidBox,
            position: [0.0, 5.0, 0.0],
            velocity: [0.0, -2.0, 0.0],
            yaw: 0.0,
            angular_velocity: 0.0,
            mass: 1.0,
            restitution: 0.5,
            friction: 0.3,
        }
    }

    #[test]
    fn encode_entity_three_channels() {
        let mut enc = SensorEncoder::new();
        let e = make_entity();
        let spikes = enc.encode_entity(&e, 0.0);
        assert_eq!(spikes.len(), 3);  // 3 个通道:位置/速度/能量
    }

    #[test]
    fn value_to_rate_in_range() {
        let enc = SensorEncoder::new();
        assert_eq!(enc.value_to_rate(0.0), 0.0);
        assert!((enc.value_to_rate(5.0) - 50.0).abs() < 1e-3);
        assert!((enc.value_to_rate(10.0) - 100.0).abs() < 1e-3);
        // 超出范围应该夹紧
        assert!((enc.value_to_rate(20.0) - 100.0).abs() < 1e-3);
    }

    #[test]
    fn bridge_creates() {
        let _ = LifWorldBridge::new(10);
    }
}