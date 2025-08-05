# Solana Streamer
[中文](https://github.com/0xfnzero/solana-streamer/blob/main/README_CN.md) | [English](https://github.com/0xfnzero/solana-streamer/blob/main/README.md) | [Telegram](https://t.me/fnzero_group)

A lightweight Rust library for real-time event streaming from Solana DEX trading programs. This library provides efficient event parsing and subscription capabilities for PumpFun, PumpSwap, Bonk, and Raydium CPMM protocols.

## Project Features

1. **Real-time Event Streaming**: Subscribe to live trading events from multiple Solana DEX protocols
2. **Yellowstone gRPC Support**: High-performance event subscription using Yellowstone gRPC
3. **ShredStream Support**: Alternative event streaming using ShredStream protocol
4. **Multi-Protocol Support**: 
   - **PumpFun**: Meme coin trading platform events
   - **PumpSwap**: PumpFun's swap protocol events
   - **Bonk**: Token launch platform events (letsbonk.fun)
   - **Raydium CPMM**: Raydium's Concentrated Pool Market Maker events
   - **Raydium CLMM**: Raydium's Concentrated Liquidity Market Maker events
5. **Unified Event Interface**: Consistent event handling across all supported protocols
6. **Event Parsing System**: Automatic parsing and categorization of protocol-specific events
7. **High Performance**: Optimized for low-latency event processing
8. **Batch Processing Optimization**: Batch processing events to reduce callback overhead
9. **Performance Monitoring**: Built-in performance metrics monitoring, including event processing speed, memory usage, etc.
10. **Memory Optimization**: Object pooling and caching mechanisms to reduce memory allocations
11. **Flexible Configuration System**: Support for custom batch sizes, backpressure strategies, channel sizes, and other parameters
12. **Preset Configurations**: Provides high-performance, low-latency, ordered processing, and other preset configurations
13. **Backpressure Handling**: Supports blocking, dropping, retrying, ordered, and other backpressure strategies
14. **Runtime Configuration Updates**: Supports dynamic configuration parameter updates at runtime
15. **Full Function Performance Monitoring**: All subscribe_events functions support performance monitoring, automatically collecting and reporting performance metrics

## Installation

### Direct Clone

Clone this project to your project directory:

```bash
cd your_project_root_directory
git clone https://github.com/0xfnzero/solana-streamer
```

Add the dependency to your `Cargo.toml`:

```toml
# Add to your Cargo.toml
solana-streamer-sdk = { path = "./solana-streamer", version = "0.1.11" }
```

### Use crates.io

```toml
# Add to your Cargo.toml
solana-streamer-sdk = "0.1.11"
```

## Usage Examples

### Advanced Usage with Batch Processing and Backpressure

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
    println!("Starting Solana Streamer...");
    
    // Test Yellowstone gRPC with performance monitoring
    test_grpc().await?;
    
    // Test ShredStream with performance monitoring  
    test_shreds().await?;
    
    Ok(())
}

async fn test_grpc() -> Result<(), Box<dyn std::error::Error>> {
    println!("Subscribing to Yellowstone gRPC events...");

    // Create low-latency configuration
    let mut config = ClientConfig::low_latency();
    // Enable performance monitoring, has performance overhead, disabled by default
    config.enable_metrics = true;
    let grpc = YellowstoneGrpc::new_with_config(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
        None,
        config,
    )?;

    let callback = create_event_callback();

    // Configure protocols to monitor
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    // Configure account filtering
    let account_include = vec![
        PUMPFUN_PROGRAM_ID.to_string(),      // Monitor pumpfun program ID
        PUMPSWAP_PROGRAM_ID.to_string(),     // Monitor pumpswap program ID
        BONK_PROGRAM_ID.to_string(),         // Monitor bonk program ID
        RAYDIUM_CPMM_PROGRAM_ID.to_string(), // Monitor raydium_cpmm program ID
        RAYDIUM_CLMM_PROGRAM_ID.to_string(), // Monitor raydium_clmm program ID
        "xxxxxxxx".to_string(),              // Monitor xxxxx account
    ];
    let account_exclude = vec![];
    let account_required = vec![];

    println!("Starting to listen for events, press Ctrl+C to stop...");
    println!("Monitoring programs: {:?}", account_include);
    
    println!("Starting subscription...");
    
    // Subscribe with automatic performance monitoring
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
    println!("Subscribing to ShredStream events...");

    // Create low-latency configuration
    let mut config = ShredClientConfig::low_latency();
    // Enable performance monitoring, has performance overhead, disabled by default
    config.enable_metrics = true;
    let shred_stream =
        ShredStreamGrpc::new_with_config("http://127.0.0.1:10800".to_string(), config).await?;

    let callback = create_event_callback();
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    println!("Listening for events, press Ctrl+C to stop...");
    
    // Subscribe with automatic performance monitoring
    shred_stream
        .shredstream_subscribe(protocols, None, callback)
        .await?;

    Ok(())
}

fn create_event_callback() -> impl Fn(Box<dyn UnifiedEvent>) {
    |event: Box<dyn UnifiedEvent>| {
        println!("🎉 Event received! Type: {:?}, ID: {}", event.event_type(), event.id());
        match_event!(event, {
            BonkPoolCreateEvent => |e: BonkPoolCreateEvent| {
                // When using grpc, you can get block_time from each event
                println!("block_time: {:?}, block_time_ms: {:?}", e.metadata.block_time, e.metadata.block_time_ms);
                println!("BonkPoolCreateEvent: {:?}", e.base_mint_param.symbol);
            },
            BonkTradeEvent => |e: BonkTradeEvent| {
                println!("BonkTradeEvent: {:?}", e);
            },
            BonkMigrateToAmmEvent => |e: BonkMigrateToAmmEvent| {
                println!("BonkMigrateToAmmEvent: {e:?}");
            },
            BonkMigrateToCpswapEvent => |e: BonkMigrateToCpswapEvent| {
                println!("BonkMigrateToCpswapEvent: {e:?}");
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

## Supported Protocols

- **PumpFun**: Primary meme coin trading platform
- **PumpSwap**: PumpFun's swap protocol
- **Bonk**: Token launch platform (letsbonk.fun)
- **Raydium CPMM**: Raydium's Concentrated Pool Market Maker protocol
- **Raydium CLMM**: Raydium's Concentrated Liquidity Market Maker protocol

## Event Streaming Services

- **Yellowstone gRPC**: High-performance Solana event streaming
- **ShredStream**: Alternative event streaming protocol

## Architecture Features

### Unified Event Interface

- **UnifiedEvent Trait**: All protocol events implement a common interface
- **Protocol Enum**: Easy identification of event sources
- **Event Factory**: Automatic event parsing and categorization

### Event Parsing System

- **Protocol-specific Parsers**: Dedicated parsers for each supported protocol
- **Event Factory**: Centralized event creation and parsing
- **Extensible Design**: Easy to add new protocols and event types

### Streaming Infrastructure

- **Yellowstone gRPC Client**: Optimized for Solana event streaming
- **ShredStream Client**: Alternative streaming implementation
- **Async Processing**: Non-blocking event handling

## Project Structure

```
src/
├── common/           # Common functionality and types
├── protos/           # Protocol buffer definitions
├── streaming/        # Event streaming system
│   ├── event_parser/ # Event parsing system
│   │   ├── common/   # Common event parsing tools
│   │   ├── core/     # Core parsing traits and interfaces
│   │   ├── protocols/# Protocol-specific parsers
│   │   │   ├── bonk/ # Bonk event parsing
│   │   │   ├── pumpfun/ # PumpFun event parsing
│   │   │   ├── pumpswap/ # PumpSwap event parsing
│   │   │   ├── raydium_cpmm/ # Raydium CPMM event parsing
│   │   │   └── raydium_clmm/ # Raydium CLMM event parsing
│   │   └── factory.rs # Parser factory
│   ├── shred_stream.rs # ShredStream client
│   ├── yellowstone_grpc.rs # Yellowstone gRPC client
│   └── yellowstone_sub_system.rs # Yellowstone subsystem
├── lib.rs            # Main library file
└── main.rs           # Example program
```

## License

MIT License

## Contact

- Project Repository: https://github.com/0xfnzero/solana-streamer
- Telegram Group: https://t.me/fnzero_group

## Performance Considerations

1. **Connection Management**: Properly handle connection lifecycle and reconnection
2. **Event Filtering**: Use protocol filtering to reduce unnecessary event processing
3. **Memory Management**: Implement appropriate cleanup for long-running streams
4. **Error Handling**: Robust error handling for network issues and service interruptions
5. **Batch Processing Optimization**: Use batch processing to reduce callback overhead and improve throughput
6. **Performance Monitoring**: Enable performance monitoring to identify bottlenecks and optimization opportunities

## Important Notes

1. **Network Stability**: Ensure stable network connection for continuous event streaming
2. **Rate Limiting**: Be aware of rate limits on public gRPC endpoints
3. **Error Recovery**: Implement proper error handling and reconnection logic
4. **Resource Management**: Monitor memory and CPU usage for long-running streams
5. **Compliance**: Ensure compliance with relevant laws and regulations

## Language Versions

- [English](README.md)
- [中文](README_CN.md)
