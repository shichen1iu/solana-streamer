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
solana-streamer-sdk = { path = "./solana-streamer", version = "0.1.3" }
```

### 使用 crates.io

```toml
# 添加到您的 Cargo.toml
solana-streamer-sdk = "0.1.3"
```

## 使用示例

```rust
use solana_streamer_sdk::{
    match_event,
    streaming::{
        event_parser::{
            protocols::{
                bonk::{BonkPoolCreateEvent, BonkTradeEvent}, pumpfun::{PumpFunCreateTokenEvent, PumpFunTradeEvent}, pumpswap::{
                    PumpSwapBuyEvent, PumpSwapCreatePoolEvent, PumpSwapDepositEvent,
                    PumpSwapSellEvent, PumpSwapWithdrawEvent,
                }, raydium_clmm::{RaydiumClmmSwapEvent, RaydiumClmmSwapV2Event}, raydium_cpmm::RaydiumCpmmSwapEvent
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
    println!("正在订阅 GRPC 事件...");

    let grpc = YellowstoneGrpc::new(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
        None,
    )?;

    let callback = create_event_callback();
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    println!("开始监听事件，按 Ctrl+C 停止...");
    grpc.subscribe_events(protocols, None, None, None, None, None, callback)
        .await?;

    Ok(())
}

async fn test_shreds() -> Result<(), Box<dyn std::error::Error>> {
    println!("正在订阅 ShredStream 事件...");

    let shred_stream = ShredStreamGrpc::new("http://127.0.0.1:10800".to_string()).await?;
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
            BlockMetaEvent => |e: BlockMetaEvent| {
                println!("BlockMetaEvent: {:?}", e.slot);
            },
            BonkPoolCreateEvent => |e: BonkPoolCreateEvent| {
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

## 配置选项

### Yellowstone gRPC 配置

```rust
let grpc = YellowstoneGrpc::new(
    "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
    None, // 自定义配置选项
)?;
```

### ShredStream 配置

```rust
let shred_stream = ShredStreamGrpc::new("http://127.0.0.1:10800".to_string()).await?;
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