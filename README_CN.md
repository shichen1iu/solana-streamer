# Solana Streamer
[中文](https://github.com/0xfnzero/solana-streamer/blob/main/README_CN.md) | [English](https://github.com/0xfnzero/solana-streamer/blob/main/README.md) | [Telegram](https://t.me/fnzero_group)

一个轻量级的 Rust 库，用于从 Solana DEX 交易程序中实时流式传输事件。该库为 PumpFun、PumpSwap、Bonk 和 Raydium CPMM 协议提供高效的事件解析和订阅功能。

## 项目特性

1. **实时事件流**: 订阅多个 Solana DEX 协议的实时交易事件
2. **Yellowstone gRPC 支持**: 使用 Yellowstone gRPC 进行高性能事件订阅
3. **ShredStream 支持**: 使用 ShredStream 协议进行替代事件流传输
4. **多协议支持**: 
   - **PumpFun**: 迷因币交易平台事件
   - **PumpSwap**: PumpFun 的交换协议事件
   - **Bonk**: 代币发布平台事件 (letsbonk.fun)
   - **Raydium CPMM**: Raydium 集中池做市商事件
   - **Raydium CLMM**: Raydium 集中流动性做市商事件
5. **统一事件接口**: 在所有支持的协议中保持一致的事件处理
6. **事件解析系统**: 自动解析和分类协议特定事件
7. **高性能**: 针对低延迟事件处理进行优化
8. **批处理优化**: 批量处理事件以减少回调开销
9. **性能监控**: 内置性能指标监控，包括事件处理速度、内存使用等
10. **内存优化**: 对象池和缓存机制减少内存分配

## 安装

### 直接克隆

将项目克隆到您的项目目录：

```bash
cd your_project_root_directory
git clone https://github.com/0xfnzero/solana-streamer
```

在您的 `Cargo.toml` 中添加依赖：

```toml
# 添加到您的 Cargo.toml
solana-streamer-sdk = { path = "./solana-streamer", version = "0.1.8" }
```

### 使用 crates.io

```toml
# 添加到您的 Cargo.toml
solana-streamer-sdk = "0.1.8"
```

## 使用示例

```rust
use solana_streamer_sdk::{
    match_event,
    streaming::{
        event_parser::{
            protocols::{
                bonk::{parser::BONK_PROGRAM_ID, BonkPoolCreateEvent, BonkTradeEvent},
                pumpfun::{parser::PUMPFUN_PROGRAM_ID, PumpFunCreateTokenEvent, PumpFunTradeEvent},
                pumpswap::{
                    parser::PUMPSWAP_PROGRAM_ID, PumpSwapBuyEvent, PumpSwapCreatePoolEvent,
                    PumpSwapDepositEvent, PumpSwapSellEvent, PumpSwapWithdrawEvent,
                },
                raydium_clmm::{
                    parser::RAYDIUM_CLMM_PROGRAM_ID, RaydiumClmmSwapEvent, RaydiumClmmSwapV2Event,
                },
                raydium_cpmm::{parser::RAYDIUM_CPMM_PROGRAM_ID, RaydiumCpmmSwapEvent},
            },
            Protocol, UnifiedEvent,
        },
        ShredStreamGrpc, YellowstoneGrpc,
    },
};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    test_grpc().await?;
    test_shreds().await?;
    Ok(())
}

async fn test_grpc() -> Result<(), Box<dyn std::error::Error>> {
    println!("正在订阅 Yellowstone gRPC 事件...");

    // 创建 gRPC 客户端并启用性能监控
    let grpc = YellowstoneGrpc::new_with_config(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
        None,
        true, // 启用性能监控
    )?;

    let callback = create_event_callback();

    // 将会从交易中尝试解析对应的协议事件
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    // 过滤账号
    let account_include = vec![
        PUMPFUN_PROGRAM_ID.to_string(),      // 监听 pumpfun 程序ID
        PUMPSWAP_PROGRAM_ID.to_string(),     // 监听 pumpswap 程序ID
        BONK_PROGRAM_ID.to_string(),         // 监听 bonk 程序ID
        RAYDIUM_CPMM_PROGRAM_ID.to_string(), // 监听 raydium_cpmm 程序ID
        RAYDIUM_CLMM_PROGRAM_ID.to_string(), // 监听 raydium_clmm 程序ID
        "xxxxxxxx".to_string(),              // 监听 xxxxx 账号
    ];
    let account_exclude = vec![];
    let account_required = vec![];

    println!("开始监听事件，按 Ctrl+C 停止...");
    grpc.subscribe_events_v2(
        protocols,
        None,
        account_include,
        account_exclude,
        account_required,
        None,
        callback,
    )
    .await?;

    Ok(())
}

async fn test_shreds() -> Result<(), Box<dyn std::error::Error>> {
    println!("正在订阅 ShredStream 事件...");

    // 创建 ShredStream 客户端并启用性能监控
    let shred_stream = ShredStreamGrpc::new_with_config(
        "http://127.0.0.1:10800".to_string(),
        true, // 启用性能监控
    ).await?;
    let callback = create_event_callback();
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    println!("开始监听事件，按 Ctrl+C 停止...");
    shred_stream
        .shredstream_subscribe(protocols, None, callback)
        .await?;

    Ok(())
}

fn create_event_callback() -> impl Fn(Box<dyn UnifiedEvent>) {
    |event: Box<dyn UnifiedEvent>| {
        match_event!(event, {
            BonkPoolCreateEvent => |e: BonkPoolCreateEvent| {
                // 使用grpc的时候，可以从每个事件中获取到block_time
                println!("block_time: {:?}, block_time_ms: {:?}", e.metadata.block_time, e.metadata.block_time_ms);
                println!("BonkPoolCreateEvent: {:?}", e.base_mint_param.symbol);
            },
            BonkTradeEvent => |e: BonkTradeEvent| {
                println!("BonkTradeEvent: {:?}", e);
            },
            PumpFunTradeEvent => |e: PumpFunTradeEvent| {
                println!("PumpFunTradeEvent: {:?}", e);
            },
            PumpFunCreateTokenEvent => |e: PumpFunCreateTokenEvent| {
                println!("PumpFunCreateTokenEvent: {:?}", e);
            },
            PumpSwapBuyEvent => |e: PumpSwapBuyEvent| {
                println!("Buy event: {:?}", e);
            },
            PumpSwapSellEvent => |e: PumpSwapSellEvent| {
                println!("Sell event: {:?}", e);
            },
            PumpSwapCreatePoolEvent => |e: PumpSwapCreatePoolEvent| {
                println!("CreatePool event: {:?}", e);
            },
            PumpSwapDepositEvent => |e: PumpSwapDepositEvent| {
                println!("Deposit event: {:?}", e);
            },
            PumpSwapWithdrawEvent => |e: PumpSwapWithdrawEvent| {
                println!("Withdraw event: {:?}", e);
            },
            RaydiumCpmmSwapEvent => |e: RaydiumCpmmSwapEvent| {
                println!("RaydiumCpmmSwapEvent: {:?}", e);
            },
            RaydiumClmmSwapEvent => |e: RaydiumClmmSwapEvent| {
                println!("RaydiumClmmSwapEvent: {:?}", e);
            },
            RaydiumClmmSwapV2Event => |e: RaydiumClmmSwapV2Event| {
                println!("RaydiumClmmSwapV2Event: {:?}", e);
            }
        });
    }
}
```

## 支持的协议

- **PumpFun**: 主要迷因币交易平台
- **PumpSwap**: PumpFun 的交换协议
- **Bonk**: 代币发布平台 (letsbonk.fun)
- **Raydium CPMM**: Raydium 集中池做市商协议
- **Raydium CLMM**: Raydium 集中流动性做市商协议

## 事件流服务

- **Yellowstone gRPC**: 高性能 Solana 事件流
- **ShredStream**: 替代事件流协议

## 架构特性

### 统一事件接口

- **UnifiedEvent Trait**: 所有协议事件实现通用接口
- **Protocol Enum**: 轻松识别事件来源
- **Event Factory**: 自动事件解析和分类

### 事件解析系统

- **协议特定解析器**: 每个支持协议的专用解析器
- **事件工厂**: 集中式事件创建和解析
- **可扩展设计**: 易于添加新协议和事件类型

### 流基础设施

- **Yellowstone gRPC 客户端**: 针对 Solana 事件流优化
- **ShredStream 客户端**: 替代流实现
- **异步处理**: 非阻塞事件处理

## 项目结构

```
src/
├── common/           # 通用功能和类型
├── protos/           # Protocol buffer 定义
├── streaming/        # 事件流系统
│   ├── event_parser/ # 事件解析系统
│   │   ├── common/   # 通用事件解析工具
│   │   ├── core/     # 核心解析特征和接口
│   │   ├── protocols/# 协议特定解析器
│   │   │   ├── bonk/ # Bonk 事件解析
│   │   │   ├── pumpfun/ # PumpFun 事件解析
│   │   │   ├── pumpswap/ # PumpSwap 事件解析
│   │   │   ├── raydium_cpmm/ # Raydium CPMM 事件解析
│   │   │   └── raydium_clmm/ # Raydium CLMM 事件解析
│   │   └── factory.rs # 解析器工厂
│   ├── shred_stream.rs # ShredStream 客户端
│   ├── yellowstone_grpc.rs # Yellowstone gRPC 客户端
│   └── yellowstone_sub_system.rs # Yellowstone 子系统
├── lib.rs            # 主库文件
└── main.rs           # 示例程序
```

## 性能考虑

1. **连接管理**: 正确处理连接生命周期和重连
2. **事件过滤**: 使用协议过滤减少不必要的事件处理
3. **内存管理**: 为长时间运行的流实现适当的清理
4. **错误处理**: 对网络问题和服务中断进行健壮的错误处理
5. **批处理优化**: 使用批处理减少回调开销，提高吞吐量
6. **性能监控**: 启用性能监控以识别瓶颈和优化机会

## 配置选项

### Yellowstone gRPC 配置

```rust
// 推荐：创建 gRPC 客户端并启用性能监控
let grpc = YellowstoneGrpc::new_with_config(
    "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
    None,
    true, // 启用性能监控
)?;

// 替代：基本配置（性能监控默认启用）
let grpc = YellowstoneGrpc::new(
    "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
    None,
)?;

// 最大性能：禁用性能监控
let grpc = YellowstoneGrpc::new_with_config(
    "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
    None,
    false, // 禁用性能监控
)?;
```

### ShredStream 配置

```rust
// 推荐：创建 ShredStream 客户端并启用性能监控
let shred_stream = ShredStreamGrpc::new_with_config(
    "http://127.0.0.1:10800".to_string(),
    true, // 启用性能监控
).await?;

// 替代：基本配置（性能监控默认启用）
let shred_stream = ShredStreamGrpc::new("http://127.0.0.1:10800".to_string()).await?;

// 最大性能：禁用性能监控
let shred_stream = ShredStreamGrpc::new_with_config(
    "http://127.0.0.1:10800".to_string(),
    false, // 禁用性能监控
).await?;
```

## 许可证

MIT 许可证

## 联系方式

- 项目仓库: https://github.com/0xfnzero/solana-streamer
- Telegram 群组: https://t.me/fnzero_group

## 重要注意事项

1. **网络稳定性**: 确保稳定的网络连接以进行连续的事件流传输
2. **速率限制**: 注意公共 gRPC 端点的速率限制
3. **错误恢复**: 实现适当的错误处理和重连逻辑
4. **资源管理**: 监控长时间运行流的内存和 CPU 使用情况
5. **合规性**: 确保遵守相关法律法规

## 语言版本

- [English](README.md)
- [中文](README_CN.md)