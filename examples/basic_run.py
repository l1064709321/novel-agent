"""
群星 A.I. OS - 最小运行示例

这个脚本演示 5 件事:
1. 启动伦理铁门(模块 7)
2. 启动物理世界模型(模块 18)
3. 启动八门状态机(模块 3)
4. 创建一个简单的物理场景
5. 让物理世界跑 N 步,观察结果

运行:
    cd quantum-ai-os
    maturin develop --release  # 编译 Python 扩展
    python examples/basic_run.py
"""

import sys
import time

# 从编译出的 Rust 扩展导入
try:
    from quantum_python import QuantumCore, PhysicsWorldModel, ValueAnchor
except ImportError:
    print("=" * 60)
    print("错误: 找不到 quantum_python 扩展")
    print("请先编译:")
    print("  cd quantum-ai-os")
    print("  pip install maturin")
    print("  maturin develop --release -m python/Cargo.toml")
    print("=" * 60)
    sys.exit(1)


def demo_bootstrap():
    """演示 1: 启动群星核心"""
    print("\n=== 演示 1: 启动群星核心 ===\n")

    core = QuantumCore()
    print("群星核心已启动")
    print(f"状态摘要: {core.status()}")
    print(f"当前八门: {core.current_gate()}")
    print(f"元价值锚: {core.anchor()}")


def demo_eight_gates():
    """演示 2: 八门状态转移"""
    print("\n=== 演示 2: 八门状态转移 ===\n")

    core = QuantumCore()
    print(f"初始: {core.current_gate()}")

    # 开门 → 生门(创造模式)
    core.transition_gate("create", "进入创造模式")
    print(f"切换后: {core.current_gate()}")

    # 生门 → 惊门(检测到异常)
    core.transition_gate("alert", "发现异常信号")
    print(f"切换后: {core.current_gate()}")

    # 惊门 → 死门(紧急停止)
    core.transition_gate("dead", "紧急停止")
    print(f"切换后: {core.current_gate()}")


def demo_physics_world():
    """演示 3: 物理世界模型 - 自由落体"""
    print("\n=== 演示 3: 物理世界 - 自由落体 ===\n")

    world = PhysicsWorldModel()
    print(f"初始 tick: {world.tick()}")

    # 创建一个 5 米高、1kg 的方块
    world.add_box(id=1, x=0.0, y=5.0, z=0.0, mass=1.0)
    print(f"创建方块: y=5.0, mass=1.0")
    print(f"实体: {world.all_entities()}")

    # 推进一步
    print("\n推进 30 步(dt=0.05s):")
    for i in range(30):
        world.step(0.05)
        if i % 5 == 0:
            pos = world.get_position(1)
            print(f"  tick={world.tick():3d}  y={pos[1]:.3f}")


def demo_physics_push():
    """演示 4: 物理世界 - 推力 + 反弹"""
    print("\n=== 演示 4: 物理世界 - 推力 + 反弹 ===\n")

    world = PhysicsWorldModel()
    world.add_sphere(id=1, x=0.0, y=0.5, z=0.0, mass=1.0)
    print(f"球: y=0.5, 质量 1kg")
    print(f"推力 5.0 施加到球上")

    # 推 5 步
    for i in range(5):
        world.step(0.02)
        pos = world.get_position(1)
        print(f"  tick={i+1:2d}  pos=({pos[0]:.3f}, {pos[1]:.3f}, {pos[2]:.3f})")

    # 推一下
    world.push(1, magnitude=5.0)
    print(f"\n施加水平推力 5.0N:")
    for i in range(10):
        world.step(0.02)
        pos = world.get_position(1)
        if i % 2 == 0:
            print(f"  tick={world.tick():3d}  pos=({pos[0]:.3f}, {pos[1]:.3f}, {pos[2]:.3f})")


def demo_ethics_validation():
    """演示 5: 伦理铁门"""
    print("\n=== 演示 5: 伦理铁门 ===\n")

    core = QuantumCore()
    print(f"元价值锚 non_harm 基线: {core.anchor().non_harm}")

    # 尝试低伤害动作
    print(f"  0.3 伤害: {'通过' if core.validate_action('good', 0.3) else '被拒'}")
    print(f"  0.5 伤害: {'通过' if core.validate_action('gray', 0.5) else '被拒'}")
    print(f"  0.9 伤害: {'通过' if core.validate_action('evil', 0.9) else '被拒'}")


def main():
    print("=" * 60)
    print("  群星 A.I. OS - Python 胶水层演示")
    print("=" * 60)

    demo_bootstrap()
    demo_eight_gates()
    demo_physics_world()
    demo_physics_push()
    demo_ethics_validation()

    print("\n" + "=" * 60)
    print("  全部演示完成")
    print("=" * 60)


if __name__ == "__main__":
    main()
