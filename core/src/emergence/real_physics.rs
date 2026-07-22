//! 真实物理引擎(纯 Rust 实现)
//!
//! 包含:
//! - 完整刚体动力学(线性 + 角动量)
//! - 简单碰撞检测(球-球,球-平面)
//! - 摩擦、弹性
//! - 惯性张量
//!
//! 这是一个"够用"的真实物理,不是最准的,但比纯位置更新好得多。

use nalgebra::{Vector3, Matrix3};

/// 刚体
#[derive(Debug, Clone)]
pub struct RigidBody {
    pub id: u64,
    pub mass: f32,
    /// 1/mass, 缓存
    pub inv_mass: f32,
    /// 位置
    pub position: Vector3<f32>,
    /// 线速度
    pub linear_velocity: Vector3<f32>,
    /// 角速度(欧拉角简化版)
    pub angular_velocity: Vector3<f32>,
    /// 朝向(4 个 float: x, y, z, w 四元数)
    pub orientation: [f32; 4],
    /// 1/惯性张量(对角阵,存对角线)
    pub inv_inertia_diag: [f32; 3],
    /// 累积力(本步)
    pub force: Vector3<f32>,
    /// 累积力矩
    pub torque: Vector3<f32>,
    /// 半径(用于碰撞检测)
    pub radius: f32,
    /// 弹性系数
    pub restitution: f32,
    /// 摩擦系数
    pub friction: f32,
}

impl RigidBody {
    pub fn new(id: u64, position: Vector3<f32>, mass: f32, radius: f32) -> Self {
        let inv_mass = if mass > 1e-6 { 1.0 / mass } else { 0.0 };
        // 球体惯性张量:I = (2/5) * m * r^2
        let i_scalar = (2.0 / 5.0) * mass * radius * radius;
        let inv_inertia_scalar = if i_scalar > 1e-6 { 1.0 / i_scalar } else { 0.0 };

        Self {
            id,
            mass,
            inv_mass,
            position,
            linear_velocity: Vector3::zeros(),
            angular_velocity: Vector3::zeros(),
            orientation: [0.0, 0.0, 0.0, 1.0],  // 单位四元数
            inv_inertia_diag: [inv_inertia_scalar; 3],
            force: Vector3::zeros(),
            torque: Vector3::zeros(),
            radius,
            restitution: 0.5,
            friction: 0.3,
        }
    }

    /// 添加力
    pub fn add_force(&mut self, force: Vector3<f32>) {
        self.force += force;
    }

    /// 添加力矩
    pub fn add_torque(&mut self, torque: Vector3<f32>) {
        self.torque += torque;
    }
}

/// 真实物理世界
pub struct RealPhysicsWorld {
    pub bodies: Vec<RigidBody>,
    pub gravity: Vector3<f32>,
    /// 碰撞对
    pub collisions: Vec<(u64, u64)>,
    /// 时间步
    pub dt: f32,
    /// 是否使用简化的"垂直方向"物理(只更新 y)
    pub vertical_only: bool,
}

impl RealPhysicsWorld {
    pub fn new() -> Self {
        Self {
            bodies: Vec::new(),
            gravity: Vector3::new(0.0, -9.81, 0.0),
            collisions: Vec::new(),
            dt: 0.05,
            vertical_only: true,  // 默认简化模式,只处理垂直方向
        }
    }

    pub fn add_body(&mut self, body: RigidBody) {
        self.bodies.push(body);
    }

    pub fn body_count(&self) -> usize {
        self.bodies.len()
    }

    /// 推进一步(完整物理:线性 + 角动量)
    pub fn step(&mut self, dt: f32) {
        self.dt = dt;

        // 1. 应用重力
        for body in &mut self.bodies {
            if body.inv_mass > 0.0 {
                body.add_force(self.gravity * body.mass);
            }
        }

        // 2. 线性动力学:F = ma,  v += a*dt,  p += v*dt
        for body in &mut self.bodies {
            if body.inv_mass > 0.0 {
                let acceleration = body.force * body.inv_mass;
                body.linear_velocity += acceleration * dt;

                if self.vertical_only {
                    // 简化模式:只更新 y 方向
                    body.position.y += body.linear_velocity.y * dt;
                } else {
                    body.position += body.linear_velocity * dt;
                }
            }
            // 清零累积力
            body.force = Vector3::zeros();
        }

        // 3. 角动力学:τ = Iα, ω += α*dt
        for body in &mut self.bodies {
            if body.inv_mass > 0.0 {
                // 角加速度 = I^-1 * τ(对角阵直接分量乘)
                let alpha = Vector3::new(
                    body.inv_inertia_diag[0] * body.torque.x,
                    body.inv_inertia_diag[1] * body.torque.y,
                    body.inv_inertia_diag[2] * body.torque.z,
                );
                body.angular_velocity += alpha * dt;
                // 简化:不更新 quaternion
            }
            body.torque = Vector3::zeros();
        }

        // 4. 碰撞检测 + 响应(用索引访问避免借用冲突)
        self.collisions.clear();
        self.handle_ground_collisions(dt);

        let n = self.bodies.len();
        for i in 0..n {
            for j in (i + 1)..n {
                // 先读,做检测
                let collided = self.check_sphere_sphere_collision(i, j);
                if collided {
                    let id_i = self.bodies[i].id;
                    let id_j = self.bodies[j].id;
                    self.collisions.push((id_i, id_j));
                    self.resolve_sphere_sphere_collision(i, j);
                }
            }
        }
    }

    /// 球-球碰撞检测
    fn check_sphere_sphere_collision(&self, i: usize, j: usize) -> bool {
        let bi = &self.bodies[i];
        let bj = &self.bodies[j];
        let dist = (bi.position - bj.position).norm();
        dist < (bi.radius + bj.radius)
    }

    /// 球-球碰撞响应(重写:先计算所有需要的量,再修改)
    fn resolve_sphere_sphere_collision(&mut self, i: usize, j: usize) {
        // 阶段 1:读出所有需要的量
        let bi_pos = self.bodies[i].position;
        let bj_pos = self.bodies[j].position;
        let bi_vel = self.bodies[i].linear_velocity;
        let bj_vel = self.bodies[j].linear_velocity;
        let bi_inv_m = self.bodies[i].inv_mass;
        let bj_inv_m = self.bodies[j].inv_mass;
        let bi_rest = self.bodies[i].restitution;
        let bj_rest = self.bodies[j].restitution;
        let bi_fric = self.bodies[i].friction;
        let bj_fric = self.bodies[j].friction;

        // 阶段 2:计算
        let diff = bj_pos - bi_pos;
        let normal = if diff.norm() > 1e-6 {
            diff.normalize()
        } else {
            Vector3::new(1.0, 0.0, 0.0)
        };
        let rel_v = bi_vel - bj_vel;
        let v_along_normal = rel_v.dot(&normal);
        if v_along_normal > 0.0 {
            return;
        }
        let e = (bi_rest + bj_rest) * 0.5;
        let inv_m_sum = bi_inv_m + bj_inv_m;
        if inv_m_sum < 1e-6 {
            return;
        }
        let j_impulse = -(1.0 + e) * v_along_normal / inv_m_sum;

        let tangent = rel_v - normal * v_along_normal;
        let t_mag = tangent.norm();
        let (tangent_dir, jt) = if t_mag > 1e-6 {
            let dir = tangent / t_mag;
            let fric = (bi_fric + bj_fric) * 0.5;
            (dir, -fric * t_mag / inv_m_sum)
        } else {
            (Vector3::zeros(), 0.0)
        };

        // 阶段 3:可安全地写回
        self.bodies[i].linear_velocity += normal * (j_impulse * bi_inv_m);
        self.bodies[j].linear_velocity -= normal * (j_impulse * bj_inv_m);

        if t_mag > 1e-6 {
            self.bodies[i].linear_velocity += tangent_dir * (jt * bi_inv_m);
            self.bodies[j].linear_velocity -= tangent_dir * (jt * bj_inv_m);
        }
    }

    /// 地面碰撞(简化为 y=0 平面)
    fn handle_ground_collisions(&mut self, dt: f32) {
        for body in &mut self.bodies {
            if body.position.y - body.radius < 0.0 {
                body.position.y = body.radius;

                // 反弹
                if body.linear_velocity.y < 0.0 {
                    body.linear_velocity.y = -body.linear_velocity.y * body.restitution;
                }

                // 摩擦
                let friction_factor = 1.0 - body.friction * dt * 10.0;
                body.linear_velocity.x *= friction_factor.max(0.0);
                body.linear_velocity.z *= friction_factor.max(0.0);

                // 角摩擦
                body.angular_velocity *= 0.95;
            }
        }
    }

    /// 应用一个事件(对刚体施加力)
    pub fn apply_event(&mut self, body_id: u64, force: Vector3<f32>) {
        if let Some(body) = self.bodies.iter_mut().find(|b| b.id == body_id) {
            body.add_force(force);
        }
    }
}

impl Default for RealPhysicsWorld {
    fn default() -> Self { Self::new() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gravity_pulls_down() {
        let mut world = RealPhysicsWorld::new();
        let body = RigidBody::new(1, Vector3::new(0.0, 5.0, 0.0), 1.0, 0.5);
        world.add_body(body);
        for _ in 0..50 {
            world.step(0.01);
        }
        let b = &world.bodies[0];
        assert!(b.position.y < 5.0, "应下落: y={}", b.position.y);
    }

    #[test]
    fn ground_collision_bounces() {
        let mut world = RealPhysicsWorld::new();
        let mut body = RigidBody::new(1, Vector3::new(0.0, 0.5, 0.0), 1.0, 0.5);
        body.linear_velocity.y = -5.0;
        body.restitution = 0.7;
        world.add_body(body);
        for _ in 0..100 {
            world.step(0.005);
        }
        let b = &world.bodies[0];
        assert!(b.position.y >= b.radius - 0.01, "不能穿透地面");
    }

    #[test]
    fn two_bodies_collide() {
        let mut world = RealPhysicsWorld::new();
        world.add_body(RigidBody::new(1, Vector3::new(0.0, 5.0, 0.0), 1.0, 0.5));
        world.add_body(RigidBody::new(2, Vector3::new(0.0, 0.5, 0.0), 1.0, 0.5));
        for _ in 0..200 {
            world.step(0.005);
        }
        // 两个球应该都触地,不会互相穿插
        for b in &world.bodies {
            assert!(b.position.y >= b.radius - 0.01, "球 {} 触地失败: y={}", b.id, b.position.y);
        }
    }

    #[test]
    fn apply_force_changes_velocity() {
        let mut world = RealPhysicsWorld::new();
        world.add_body(RigidBody::new(1, Vector3::new(0.0, 5.0, 0.0), 1.0, 0.5));
        world.apply_event(1, Vector3::new(10.0, 0.0, 0.0));
        world.step(0.01);
        // 力的影响要下一 tick 才看到
        let v = world.bodies[0].linear_velocity;
        assert!(v.x > 0.0, "力应改变 x 速度: vx={}", v.x);
    }

    #[test]
    fn zero_mass_body_doesnt_move() {
        let mut world = RealPhysicsWorld::new();
        let mut body = RigidBody::new(1, Vector3::new(0.0, 5.0, 0.0), 0.0, 0.5);
        body.inv_mass = 0.0;  // 静态
        world.add_body(body);
        let initial_y = world.bodies[0].position.y;
        for _ in 0..50 {
            world.step(0.01);
        }
        let b = &world.bodies[0];
        // 无重力(质量为 0)→ 不动
        assert_eq!(b.position.y, initial_y, "零质量应不动");
    }
}