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
solana-streamer-sdk = { path = "./solana-streamer", version = "0.1.3" }
```

### Use crates.io

```toml
# Add to your Cargo.toml
solana-streamer-sdk = "0.1.3"
```

## Usage Examples

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
    println!("Subscribing to GRPC events...");

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

    println!("Listening for events, press Ctrl+C to stop...");
    grpc.subscribe_events(protocols, None, None, None, None, None, callback)
        .await?;

    Ok(())
}

async fn test_shreds() -> Result<(), Box<dyn std::error::Error>> {
    println!("Subscribing to ShredStream events...");

    let shred_stream = ShredStreamGrpc::new("http://127.0.0.1:10800".to_string()).await?;
    let callback = create_event_callback();
    let protocols = vec![
        Protocol::PumpFun,
        Protocol::PumpSwap,
        Protocol::Bonk,
        Protocol::RaydiumCpmm,
        Protocol::RaydiumClmm,
    ];

    println!("Listening for events, press Ctrl+C to stop...");
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

## Performance Considerations

1. **Connection Management**: Properly handle connection lifecycle and reconnection
2. **Event Filtering**: Use protocol filtering to reduce unnecessary event processing
3. **Memory Management**: Implement proper cleanup for long-running streams
4. **Error Handling**: Robust error handling for network issues and service disruptions

## Configuration Options

### Yellowstone gRPC Configuration

```rust
let grpc = YellowstoneGrpc::new(
    "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
    None, // Custom configuration options
)?;
```

### ShredStream Configuration

```rust
let shred_stream = ShredStreamGrpc::new("http://127.0.0.1:10800".to_string()).await?;
```

## License

MIT License

## Contact

- Project Repository: https://github.com/0xfnzero/solana-streamer
- Telegram Group: https://t.me/fnzero_group

## Important Notes

1. **Network Stability**: Ensure stable network connection for continuous event streaming
2. **Rate Limiting**: Be aware of rate limits on public gRPC endpoints
3. **Error Recovery**: Implement proper error handling and reconnection logic
4. **Resource Management**: Monitor memory and CPU usage for long-running streams
5. **Compliance**: Ensure compliance with relevant laws and regulations

## Language Versions

- [English](README.md)
- [中文](README_CN.md)
