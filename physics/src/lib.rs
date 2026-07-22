//! 群星 A.I. OS - 物理引擎接口(对接 C 层真实物理)
//!
//! **当前状态:占位实现**
//! 实际生产时,这里会链接 Box2D / Bullet / 自写 C 物理引擎
//!
//! ## 当前实现
//! 用 Rust 实现的简化物理(刚体直线运动 + 地面碰撞),作为占位
//! 等用户选定具体物理引擎(Box2D / Bullet / 自写),我们再链接 C 库
//!
//! ## 接口
//! - `PhysicsEngine::new()`: 创建引擎
//! - `engine.add_body()`: 添加刚体
//! - `engine.step()`: 推进一步
//! - `engine.get_state()`: 获取状态

use serde::{Deserialize, Serialize};
use nalgebra::{Vector3, Matrix3};

/// 刚体描述
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RigidBody {
    pub id: u64,
    pub position: Vector3<f32>,
    pub velocity: Vector3<f32>,
    pub mass: f32,
    pub restitution: f32,
    pub friction: f32,
    pub shape: Shape,
    /// 朝向(3x3 旋转矩阵,简化)
    pub orientation: Matrix3<f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Shape {
    Box { half_extents: [f32; 3] },
    Sphere { radius: f32 },
    Capsule { radius: f32, half_height: f32 },
}

impl Default for Shape {
    fn default() -> Self { Self::Sphere { radius: 0.5 } }
}

impl RigidBody {
    pub fn new(id: u64, position: Vector3<f32>, mass: f32) -> Self {
        Self {
            id, position, velocity: Vector3::zeros(), mass,
            restitution: 0.5, friction: 0.3,
            shape: Shape::default(),
            orientation: Matrix3::identity(),
        }
    }
}

/// 物理引擎(简化版,占位)
pub struct PhysicsEngine {
    bodies: Vec<RigidBody>,
    gravity: Vector3<f32>,
}

impl PhysicsEngine {
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            gravity: Vector3::new(0.0, -9.81, 0.0),
        }
    }

    pub fn add_body(&mut self, body: RigidBody) {
        self.bodies.push(body);
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    pub fn get_body(&self, id: u64) -> Option<&RigidBody> {
        self.bodies.iter().find(|b| b.id == id)
    }

    /// 推进一步(简化物理)
    pub fn step(&mut self, dt: f32) {
        for body in &mut self.bodies {
            // 重力
            body.velocity += self.gravity * dt;
            // 位置
            body.position += body.velocity * dt;
            // 地面
            if body.position.y < 0.0 {
                body.position.y = 0.0;
                body.velocity.y = -body.velocity.y * body.restitution;
                // 摩擦
                let friction_factor = 1.0 - body.friction * dt * 10.0;
                body.velocity.x *= friction_factor.max(0.0);
                body.velocity.z *= friction_factor.max(0.0);
            }
        }
    }

    /// 获取所有刚体状态
    pub fn states(&self) -> Vec<RigidBody> {
        self.bodies.clone()
    }
}

impl Default for PhysicsEngine {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_pulls_down() {
        let mut e = PhysicsEngine::new();
        e.add_body(RigidBody::new(1, nalgebra::Vector3::new(0.0, 10.0, 0.0), 1.0));
        for _ in 0..50 {
            e.step(0.01);
        }
        let body = e.get_body(1).unwrap();
        assert!(body.position.y < 10.0, "应下落,实际 y={}", body.position.y);
    }

    #[test]
    fn ground_collision_works() {
        let mut e = PhysicsEngine::new();
        e.add_body(RigidBody::new(1, nalgebra::Vector3::new(0.0, 0.0, 0.0), 1.0));
        for _ in 0..1000 {
            e.step(0.01);
        }
        let body = e.get_body(1).unwrap();
        assert!(body.position.y >= -0.001, "不应穿透地面,实际 y={}", body.position.y);
    }

    #[test]
    fn multiple_bodies_independent() {
        let mut e = PhysicsEngine::new();
        e.add_body(RigidBody::new(1, nalgebra::Vector3::new(0.0, 5.0, 0.0), 1.0));
        e.add_body(RigidBody::new(2, nalgebra::Vector3::new(1.0, 10.0, 0.0), 1.0));
        assert_eq!(e.body_count(), 2);
        for _ in 0..30 {
            e.step(0.01);
        }
        let b1 = e.get_body(1).unwrap();
        let b2 = e.get_body(2).unwrap();
        assert!(b1.position.y < 5.0);
        assert!(b2.position.y < 10.0);
    }
}
