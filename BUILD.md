# 编译与运行指南

> **重要:本仓库的代码没有在云端编译验证过**(云端 Rust 工具链不可用)
> **首次本地编译很可能遇到问题,按下面的 troubleshooting 走**

## 一、编译环境要求

| 工具 | 最低版本 | 用途 |
|------|----------|------|
| Rust | 1.75+ | 编译核心 + 物理 + 胶水 |
| Cargo | 1.75+ | Rust 包管理 |
| Python | 3.8+ | AGI 入口(可选) |
| maturin | 1.0+ | 编译 PyO3 扩展 |
| GCC / Clang | — | 链接 C 库(未来 C 物理引擎) |

### 1.1 在 PC 上装 Rust

```bash
# Linux / macOS
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Windows
# 下载并运行 rustup-init.exe:https://rustup.rs
```

### 1.2 在 Android 手机上装 Rust + Python

```bash
# Termux
pkg update
pkg install python rust binutils

# 验证
python --version
rustc --version
cargo --version
```

> **iPhone 用户**:目前没有官方方式在 iOS 上装 Rust 工具链。
> 建议路径:在 PC 上交叉编译 → 拷贝到手机,或者用云端。

## 二、编译步骤

### 2.1 仅 Rust 部分(不需要 Python)

```bash
cd quantum-ai-os

# 编译所有 crate(release 模式)
cargo build --release

# 跑单元测试
cargo test --release

# 跑集成测试
cargo test --release --test '*'
```

预期输出:
```
   Compiling quantum-core v0.1.0
   Compiling quantum-memory v0.1.0
   Compiling quantum-physics v0.1.0
    Finished `release` optimized [optimized] target(s)
     Running unittests ...
test result: ok. N passed; 0 failed
```

### 2.2 编译 Python 扩展

```bash
# 装 maturin
pip install maturin

# 编译并安装
cd quantum-ai-os
maturin develop --release -m python/Cargo.toml

# 验证
python -c "from quantum_python import QuantumCore; print(QuantumCore())"
```

### 2.3 跑示例

```bash
cd quantum-ai-os
python examples/basic_run.py
```

预期输出:5 个演示(启动、八门转移、物理落体、推力反弹、伦理验证)

### 2.4 跑压力测试(消息总线 10000 条)

```bash
cargo test --release -p quantum-core stress_test_zero_loss -- --nocapture
```

## 三、交叉编译到 Android ARM

```bash
# 1. 装 Android NDK 和目标
rustup target add aarch64-linux-android
# 配 NDK 环境变量(略)

# 2. 交叉编译
cargo build --release --target aarch64-linux-android

# 3. 产物在 target/aarch64-linux-android/release/
# 拷到手机:
adb push target/aarch64-linux-android/release/libquantum_core.so /data/local/tmp/
```

## 四、性能基准

```bash
cargo bench
```

跑完后 `target/criterion/report/index.html` 有详细报告。

## 五、Troubleshooting

### Q1: 编译时缺依赖

```
error: failed to load toolchain
```

**解决**:
```bash
rustup update stable
```

### Q2: 链接错误(动态库找不到)

```
error: linking with `cc` failed: ... cannot find -lpython3.X
```

**解决**:
```bash
# 装 Python dev 头文件
sudo apt install python3-dev   # Debian/Ubuntu
brew install python@3.11       # macOS

# 或者只用 Rust 部分,跳过 Python
cargo build --release -p quantum-core -p quantum-memory -p quantum-physics
```

### Q3: 测试失败

```
test ethics::tests::baseline_protection_works ... FAILED
```

**反馈给我**,附上完整错误输出 + 你的 Rust 版本(`rustc --version`)

### Q4: Android 编译时 NDK 没配

```
error: failed to find toolchain: aarch64-linux-android
```

**解决**:
```bash
# 装 Android NDK
sdkmanager "ndk;26.1.10909125"
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/26.1.10909125
export PATH=$PATH:$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin
```

### Q5: 第一次 cargo build 很慢

**正常**。第一次需要下载所有依赖(Box2D、nalgebra、PyO3 等)。
第二次会快很多。

## 六、内存 / 性能预期

| 配置 | 预期占用 |
|------|----------|
| 1 个 LIF 神经元 | < 1 KB |
| 1000 个 LIF 神经元,空闲 | < 100 KB(无事件) |
| 1000 个 LIF 神经元,持续输入 | ~1-5 MB |
| 完整 MVP(8 模块) | ~10-30 MB |
| 物理世界 100 个实体 | ~5-15 MB |
| 总计(MVP) | **~30-60 MB** |

旧手机 ARM(2-3 GB RAM)完全够用。

## 七、CI / 测试覆盖

```bash
# 跑全套测试
cargo test --release --all

# 看测试覆盖率(需要装 cargo-tarpaulin)
cargo install cargo-tarpaulin
cargo tarpaulin --out Html
```

打开 `tarpaulin-report.html` 看覆盖率。
