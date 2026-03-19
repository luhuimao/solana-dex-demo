# Solana DEX Demo — AMM 智能合约

一个基于**常数乘积公式（x × y = k）**的 AMM DEX 链上程序示例，使用原生 Solana Program SDK（无 Anchor）实现。

---

## 项目结构

```
solana_dex_demo/
├── src/
│   ├── lib.rs          # 入口 + DexInstruction 枚举
│   ├── state.rs        # Pool 账户数据结构
│   ├── error.rs        # 自定义错误码
│   ├── math.rs         # AMM 数学：swap / LP 计算（含单元测试）
│   └── processor.rs    # 四条指令的执行逻辑（CPI to SPL Token）
└── Cargo.toml
```

---

## 支持的指令

| 指令 | 说明 |
|------|------|
| `InitializePool` | 创建并初始化流动性池，设置手续费比例 |
| `AddLiquidity` | 存入 Token A + Token B，铸造 LP Token |
| `RemoveLiquidity` | 燃烧 LP Token，按比例取回 Token A + Token B |
| `Swap` | 使用常数乘积公式兑换 Token（支持 A→B 和 B→A） |
| `GetPoolInfo` | **只读**查询：链上打印池的全部状态（余额、价格、k 值等），结果从交易 logs 中读取 |

---

## 核心设计

### 常数乘积公式（AMM Math）

```
x × y = k

swap 输出：
  amount_out = (reserve_out × amount_in × fee_factor)
             / (reserve_in × fee_denominator + amount_in × fee_factor)

fee_factor = fee_denominator - fee_numerator
```

- 手续费示例：`fee_numerator=3, fee_denominator=1000` → 0.3% 手续费
- 所有计算使用 `u128` 中间值防止溢出

### 首次注入流动性

```
LP = √(token_a_amount × token_b_amount)  （几何平均数）
```

### 后续注入

```
LP = min(token_a / reserve_a, token_b / reserve_b) × lp_supply
```

过多投入任意一侧不会获得额外 LP（鼓励按比例注入）。

### Pool Authority PDA

pool 的 vault 代币账户和 LP Mint 的权限均由一个 PDA 控制：

```
seeds = ["pool_authority", pool_account_pubkey]
```

只有该程序可以通过 `invoke_signed` 以此 PDA 的名义转账/铸币，保证资金安全。

---

## 账户布局

### InitializePool

```
0. [signer, writable]  pool_account     — 预分配 202 字节
1. []                  token_a_mint
2. []                  token_b_mint
3. [writable]          token_a_vault    — 池 A Token 金库（authority = PDA）
4. [writable]          token_b_vault    — 池 B Token 金库（authority = PDA）
5. [writable]          lp_mint          — LP Token Mint（authority = PDA）
6. []                  pool_authority   — PDA：seeds=["pool_authority", pool]
7. []                  token_program
```

### GetPoolInfo

```
0. []  pool_account   — 只读，无需 signer
```

链上程序通过 `msg!` 输出以下信息，可从交易 logs 中解析：

| 字段 | 说明 |
|------|------|
| `Reserve A / B` | 池内 Token A / B 当前余额 |
| `LP supply` | LP Token 总发行量 |
| `Price A→B` | 即时现货价格（×10⁶ 精度，纯整数运算）|
| `k (A×B)` | 恒积常数 k |
| `A/B per LP×1e6` | 每枚 LP Token 可赎回的底层 token 数量 |
| `Fee` | 手续费（分子/分母）|

客户端也可通过 `get_account_data()` + `Pool::try_from_slice()` **本地解析**，无需发送交易。

---

## 运行步骤

### 1. 编译

```bash
cargo build-sbf
```

### 2. 运行单元测试（AMM 数学）

```bash
cargo test -- --nocapture
```

预期输出：
```
running 9 tests
test math::tests::test_constant_product_invariant ... ok
test math::tests::test_lp_first_deposit_equal ... ok
...
test result: ok. 9 passed
```

### 3. 运行客户端示例

> 运行前请确保本地验证器已启动，且程序已部署（见步骤 4）。

```bash
cargo run --example client
```

客户端会依次执行：`InitializePool` → `AddLiquidity` → `GetPoolInfo` → `Swap` → `GetPoolInfo` → `RemoveLiquidity` → `GetPoolInfo`，并在每次操作后打印池的实时状态。

### 4. 本地部署

```bash
# 启动本地验证节点
solana-test-validator

# 编译 SBF 程序
cargo build-sbf

# 部署（首次 / 每次修改程序后都需重新部署）
solana program deploy ./target/deploy/solana_dex_demo.so --url localhost
```

> ⚠️ 每次修改链上程序后务必重新 `build-sbf` + `program deploy`，否则客户端发送新指令将因反序列化失败而报错。

---

## 扩展方向

- [ ] 添加 `CollectFees` 指令（协议手续费归集）
- [ ] 集成 Pyth 预言机实现 TWAP 价格保护
- [ ] 迁移到 Anchor 框架简化账户验证
- [ ] 添加集中流动性（Concentrated Liquidity，类 Uniswap V3）
- [ ] 支持 Token-2022 扩展（TransferFee、InterestBearing）
