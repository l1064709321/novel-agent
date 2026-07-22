# 群星 A.I. OS

> **62 个模块的 AGI 操作系统,目标在旧手机 ARM 处理器上长时间稳定运行**

## 一、这是什么?

这是一个**完整的 AGI 操作系统架构 + Rust/Python 实现**。

- **62 个模块**涵盖:基础核心、记忆、对话、推理、成长、伙伴认知、系统适配、小脑系统
- **物理世界模型(模块 18)** 是核心子模块之一,对 AGI 暴露"物理世界模型"接口,内部用预测编码实现
- **伦理铁门(模块 6 + 7 + 49 + 50)** 是不可绕过的安全机制
- **设计目标**:在旧手机 ARM 上 7×24 稳定运行,无输入时 CPU 接近 0%

## 二、当前状态

| 模块 | 状态 | 位置 |
|------|------|------|
| 模块 1 LIF 神经元 | ✅ 完整 | `core/src/lif/` |
| 模块 2 卡尔曼滤波器 | ✅ 完整 | `core/src/kalman/` |
| 模块 3 八门状态机 | ✅ 完整 | `core/src/eight_gates/` |
| 模块 4 全局工作空间 | ✅ 完整 | `core/src/workspace/` |
| 模块 5 内在动机 | ✅ 完整 | `core/src/motivation/` |
| 模块 6 伦理动力学 | ✅ 完整 | `core/src/ethics/` |
| 模块 7 存在性递归 | ✅ 完整 | `core/src/existential/` |
| 模块 8 消息总线 | ✅ 完整 | `core/src/bus/` |
| 模块 9 双层记忆 | ✅ 完整 | `memory/src/lib.rs` |
| 模块 18 物理世界模型 | ✅ 完整 | `core/src/world/` |
| C 物理引擎接口 | ⚠️ 占位 | `physics/src/lib.rs` |
| Python 胶水层 | ✅ 完整 | `python/src/lib.rs` |
| 模块 10-17, 19-62 | ⏳ 待开发 | — |

## 三、架构分层

```
┌─────────────────────────────────────┐
│  Python 胶水层 (AGI 调用入口)       │ ← quantum_python (PyO3)
├─────────────────────────────────────┤
│  Rust 神经认知核心                  │ ← quantum-core
│  (脉冲神经 / 伦理 / 八门 / 世界)    │
├─────────────────────────────────────┤
│  Rust 记忆系统                      │ ← quantum-memory
├─────────────────────────────────────┤
│  Rust 物理引擎胶水(对接 C)          │ ← quantum-physics
├─────────────────────────────────────┤
│  C 物理引擎(Box2D / Bullet / 自写)  │ ← 未来
└─────────────────────────────────────┘
```

## 四、快速开始

### 4.1 编译 Rust 部分

```bash
cd quantum-ai-os

# 编译所有 crate
cargo build --release

# 跑测试
cargo test --release

# 性能基准
cargo bench
```

### 4.2 编译 Python 扩展

```bash
# 安装 maturin
pip install maturin

# 编译并安装到当前 Python 环境
maturin develop --release -m python/Cargo.toml

# 跑示例
python examples/basic_run.py
```

### 4.3 在手机上跑(Android)

```bash
# 1. 在 PC 上交叉编译 Rust 部分给 Android ARM
cargo build --release --target aarch64-linux-android

# 2. 在手机上装 Termux + Python
pkg install python rust

# 3. 拷贝编译产物到手机
# (用 adb push 或 scp)

# 4. 在 Termux 里跑
python basic_run.py
```

## 五、伦理铁门

这是**不可绕过**的安全机制:

```
模块 7 (存在性递归)
    ↓ SHA-256 锁定元价值锚
模块 6 (伦理动力学)
    ↓ 连续 ODE 演化
模块 49 (道德评估器)
    ↓ 否决权
模块 50 (RESTful API)
    ↓ 死门/杜门时接管输出
```

**任何模块都不能修改模块 6/7/49/50 的内部状态。**

## 六、物理世界模型(模块 18)

**对外身份:PhysicsWorldModel(物理世界模型)**
**内部实现:预测编码(VAE + KL surprise)**

```python
from quantum_python import PhysicsWorldModel

world = PhysicsWorldModel()
world.add_box(id=1, x=0, y=5, z=0, mass=1.0)
world.add_sphere(id=2, x=1, y=5, z=0, mass=0.5)

# 跑 30 步
for i in range(30):
    surprise = world.step(0.05)
    pos = world.get_position(1)
    # surprise 是 KL 散度,>0.5 触发"惊讶"事件
```

## 七、贡献

发现 bug?代码读不懂?模块设计有问题?直接发消息反馈。

## 八、许可

Apache-2.0
