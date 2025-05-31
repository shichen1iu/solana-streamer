# GrpcParsed

A gRPC client implementation for subscribing to and processing Solana transaction data.

## Features

- Real-time subscription to Solana transaction data via gRPC
- Support for processing transaction entries and transactions
- Asynchronous transaction data processing
- Custom callback function support for transaction events
- Built-in error handling mechanism

## Installation

Add the following to your `Cargo.toml`:

```toml
[dependencies]
grpc-parsed = { path = ".", version = "0.1.0" }
```

## Usage Examples

### 1. Initializing the Client

```rust
use grpc_parsed::grpc::YellowstoneGrpc;

async fn setup_client() -> Result<YellowstoneGrpc, Box<dyn std::error::Error>> {
    let endpoint = "https://solana-yellowstone-grpc.publicnode.com:443";
    let client = YellowstoneGrpc::new(endpoint.to_string(), None)?;
    Ok(client)
}
```

### 2. Subscribing to Transaction Data

```rust
use grpc_parsed::common::logs_events::PumpfunEvent;
use solana_sdk::pubkey::Pubkey;

async fn subscribe_to_transactions() -> Result<(), Box<dyn std::error::Error>> {
    let client = YellowstoneGrpc::new(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
        None,
    )?;
    
    let callback = |event: PumpfunEvent| {
        match event {
            PumpfunEvent::NewToken(token_info) => {
                println!("Received new token event: {:?}", token_info);
            },
            PumpfunEvent::NewDevTrade(trade_info) => {
                println!("Received new dev trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewUserTrade(trade_info) => {
                println!("Received new trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewBotTrade(trade_info) => {
                println!("Received new bot trade event: {:?}", trade_info);
            },
            PumpfunEvent::Error(err) => {
                println!("Received error: {}", err);
            }
        }
    };

    // Optional: Specify bot wallet address for filtering
    let bot_wallet = None;
    client.subscribe_pumpfun(callback, bot_wallet).await?;
    
    Ok(())
}
```

## Error Handling

```rust
use grpc_parsed::grpc::YellowstoneGrpc;

async fn handle_errors() {
    match YellowstoneGrpc::new(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(),
        None,
    ) {
        Ok(client) => {
            println!("Client initialized successfully");
        },
        Err(e) => {
            eprintln!("Failed to initialize client: {}", e);
        }
    }
}
```

## Important Notes

- Ensure the gRPC server address is correct and accessible
- Callback functions should be thread-safe (Send + Sync)
- Implement appropriate error retry mechanisms in production environments
- Be mindful of memory usage when processing large volumes of transaction data

## License

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

### Telegram group:
https://t.me/fnzero_group
