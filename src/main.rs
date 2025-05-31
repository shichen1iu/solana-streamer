use grpc_parsed::{common::{
    logs_events::PumpfunEvent,
}, grpc::YellowstoneGrpc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let grpc = YellowstoneGrpc::new(
        "https://solana-yellowstone-grpc.publicnode.com:443".to_string(), 
        None,
    )?;

    println!("Connected to the network");

    let callback = |event: PumpfunEvent| {

        match event {
            PumpfunEvent::NewDevTrade(trade_info) => {
                println!("Received new dev trade event: {:?}", trade_info);
            },
            PumpfunEvent::NewToken(token_info) => {
                println!("Received new token event: {:?}", token_info);
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

    grpc.subscribe_pumpfun(callback, None).await?;

    Ok(())  
}