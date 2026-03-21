# Anchor DEX (Solana DEX Demo)

这是一个基于 Solana 和 [Anchor 框架](https://www.anchor-lang.com/)构建的去中心化交易所（DEX）示例项目。该合约实现了一个功能完备的自动化做市商（AMM）池，采用经典的恒定乘积公式（Constant Product Market Maker, $x \cdot y = k$）。

本项目是学习 Solana DeFi 智能合约开发和 Anchor 框架的最佳实践，同时兼容最新的 Token Extensions（Token2022）标准。

## 核心特性

- **常量乘积 AMM**：使用经典的 $x \cdot y = k$ 曲线进行资产自动定价与交易。
- **Token2022 兼容**：底层的合约代码选用了 Anchor 生态下的 `token_interface`，使得其既支持传统的 SPL Token 标准，也完全支持新一代的 Token-2022 规范。
- **自定义手续费**：在初始化流动性池时，创建者可以自定义配置交易手续费率（只需设定自定义的手续费分子和分母）。
- **完整的交易生命周期**：涵盖核心的 DEX 功能：创建资金池、注入流动性、撤销流动性和代币互换（Swap）。
- **完善的防滑点机制**：提供了 `min_lp_amount` 和 `min_amount_out` 等参数拦截恶意套利交易并防止交易滑点亏损。

## 智能合约架构

该程序的主要逻辑都在 `programs/anchor_dex/src` 目录下：

- **`lib.rs`**: 包含程序的上下文（Context）定义与全部指令入口。包含了以下主要的交互指令：
  - `initialize_pool`: 初始化资金池。
  - `add_liquidity`: 注入流动性资金生息。
  - `remove_liquidity`: 销毁 LP Token 撤离资金。
  - `swap`: 根据设定的滑点在此池子内进行代币互换交易。
  - `get_pool_info`: 只读指令，日志模式诊断并打印池中资产情况和价格。
- **`state.rs`**: 定义了 `Pool` 结构以及相关的内存占用空间（Space）布局。
- **`math.rs`**: 包含 AMM 产品设计的核心数学计算库，执行：兑换数量核算、LP 发行数量预估，以及整数平方根的快速计算。
- **`error.rs`**: 收集整合了本项目中抛出的业务和逻辑异常（例如：`SlippageExceeded` 滑点溢出、`InsufficientLiquidity` 流动性不足等）。

## 测试与开发指南

### 🔧 前置要求

在运行和编译环境前，请确保您已经安装了如下依赖：

- [Rust](https://www.rust-lang.org/tools/install) 和 Cargo
- [Solana CLI](https://docs.solana.com/cli/install-solana-cli-tools) (`v1.14` 或更高)
- [Anchor CLI](https://www.anchor-lang.com/docs/installation)
- Node.js & Yarn (用于执行 TypeScript/Mocha 测试)

### 🚀 安装与测试

1. 获取代码后，在当前 `anchor_dex` 目录中安装 NPM 依赖：
   ```bash
   yarn install
   ```

2. 运行本地的 Anchor 集成测试套件：
   ```bash
   anchor test
   ```
   > 💡 该命令会自动编译 Rust 智能合约，并在本地启动一个临时的 Solana Test Validator 对整个生命周期（池子创建、添加/移除流动性和交易等过程）执行完整的 TS 模拟测试。相关的测试用例位于 `tests/anchor_dex.ts` 中。

## 安全与免责声明

该合约为用于教学与演练 Solana 架构设计的**示例演示版 (Demo)代码**。尽管使用了推荐的安全检查，但您不应当未经过第三方严苛的渗透安全审计审查就将该代码在 Mainnet-Beta 高价值金库中运用。
