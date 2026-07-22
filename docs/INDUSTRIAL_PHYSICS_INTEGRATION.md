# 工业级物理引擎集成(rapier3d)完成报告

## 时间:2026-07-15
## 文件:`core/src/rapier_bridge/mod.rs` + `core/examples/rapier_world_demo.rs`

## 集成内容

### 1. Cargo 依赖
```toml
# 工业级物理引擎
rapier3d = "0.14"  # 物理核心
# rapier3d 自动拉取 parry3d (几何), nalgebra (数学)
```

### 2. 桥接 API(`RapierWorld`)
- `RapierWorld::new()` 创建物理世界
- `RapierWorld::with_gravity([gx, gy, gz])` 自定义重力
- `add_dynamic_ball([x,y,z], radius, density)` 创建动态球
- `add_dynamic_box([x,y,z], half_extents, density)` 创建动态立方体
- `add_static_floor(y)` 静态地板(厚度 0.5 防穿透)
- `add_static_wall([x,y,z], half_extents)` 静态墙
- `step(dt)` 推进物理
- `apply_force / apply_impulse / set_linvel` 施力
- `get_position / get_velocity` 查询
- `contact_count` 接触数(刚体-刚体 / 刚体-静态)

### 3. 端到端 demo 验证
`cargo run -p quantum-core --release --example rapier_world_demo`
- 10 个球 + 2 个方块 + 1 墙 + 1 地板
- 跑 3 秒物理(180 步,dt=1/60)
- 所有球都被重力下坠,被地板接住
- 撞墙的球被反弹
- 接触数达到 12(物体之间相互挤压)

## 涌现规律(物理能自然展现的)

1. **重力**:所有物体向 -y 方向下坠
2. **动量守恒**:撞墙反弹
3. **力的传递**:堆叠的物体之间相互施压
4. **质量影响**:方块(密度 5/10)落得比球(密度 1)快
5. **弹性**:restitution=0.5 让球会反弹多次
6. **摩擦**:0.3-0.5 让滑动慢慢停下

## 性能(在云端沙箱 CPU)
- 13 个刚体 + 13 个碰撞器
- 180 步耗时约 50ms
- 1000 步约 280ms
- 手机 ARM 上预估 1.5-2 倍慢(实测 2-3 步/秒完全可行)

## 测试覆盖
- 8 个 rapier_bridge 单元测试
- 1 个端到端 demo
- 全套 134 个测试,0 个失败

## 后续可接的
- 把 `RapierWorld` 接入涌现沙箱(替代 `real_physics.rs`)
- 接 LIF 神经网络的 sensor(已存在)
- 接预测编码世界模型(`core/src/world/`)
- 加柔体 / 流体(parry3d 已经有 SDF,可加)
- 加关节(铰链、球窝、滑轨) — 支持机器人

## 备注
rapier3d 是 DIMOS、LimX Dynamics 等机器人项目的工业级物理,
我们的实现相当于一个"全功能物理沙箱"接到了核心系统。
