use chrono::Local;
use futures::{channel::mpsc, sink::Sink, SinkExt};
use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestPing, SubscribeUpdate,
};

use super::types::{BlockMetaPretty, EventPretty, TransactionPretty};
use crate::common::AnyResult;
use crate::streaming::common::EventProcessor;
use crate::streaming::event_parser::UnifiedEvent;
use crate::streaming::grpc::AccountPretty;

/// 流消息处理器
pub struct StreamHandler;

impl StreamHandler {
    /// 处理单个流消息
    pub async fn handle_stream_message<F>(
        msg: SubscribeUpdate,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
        event_processor: EventProcessor,
        callback: &F,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync,
    {
        let created_at = msg.created_at;
        match msg.update_oneof {
            Some(UpdateOneof::Account(account)) => {
                let account_pretty = AccountPretty::from(account);
                log::debug!("Received account: {:?}", account_pretty);
                event_processor
                    .process_grpc_event_transaction_with_metrics(
                        EventPretty::Account(account_pretty),
                        callback,
                        bot_wallet,
                    )
                    .await?;
            }
            Some(UpdateOneof::BlockMeta(sut)) => {
                let block_meta_pretty = BlockMetaPretty::from((sut, created_at));
                log::debug!("Received block meta: {:?}", block_meta_pretty);
                event_processor
                    .process_grpc_event_transaction_with_metrics(
                        EventPretty::BlockMeta(block_meta_pretty),
                        callback,
                        bot_wallet,
                    )
                    .await?;
            }
            Some(UpdateOneof::Transaction(sut)) => {
                let transaction_pretty = TransactionPretty::from((sut, created_at));
                log::debug!(
                    "Received transaction: {} at slot {}",
                    transaction_pretty.signature,
                    transaction_pretty.slot
                );
                event_processor
                    .process_grpc_event_transaction_with_metrics(
                        EventPretty::Transaction(transaction_pretty),
                        callback,
                        bot_wallet,
                    )
                    .await?;
            }
            Some(UpdateOneof::Ping(_)) => {
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await?;
                log::debug!("service is ping: {}", Local::now());
            }
            Some(UpdateOneof::Pong(_)) => {
                log::debug!("service is pong: {}", Local::now());
            }
            _ => {
                log::debug!("Received other message type");
            }
        }
        Ok(())
    }

    //     /// 处理背压策略
    //     async fn handle_backpressure(
    //         tx: &mut mpsc::Sender<EventPretty>,
    //         event_pretty: EventPretty,
    //         backpressure_strategy: BackpressureStrategy,
    //     ) -> AnyResult<()> {
    //         match backpressure_strategy {
    //             BackpressureStrategy::Block => {
    //                 // 阻塞等待，直到有空间
    //                 if let Err(e) = tx.send(event_pretty).await {
    //                     log::error!("Failed to send transaction to channel: {:?}", e);
    //                     return Err(anyhow::anyhow!("Channel send failed: {:?}", e));
    //                 }
    //             }
    //             BackpressureStrategy::Drop => {
    //                 // 尝试发送，如果失败则丢弃
    //                 if let Err(e) = tx.try_send(event_pretty) {
    //                     if e.is_full() {
    //                         log::warn!("Channel is full, dropping transaction");
    //                     } else {
    //                         log::error!("Channel is closed: {:?}", e);
    //                         return Err(anyhow::anyhow!("Channel is closed: {:?}", e));
    //                     }
    //                 }
    //             }
    //             BackpressureStrategy::Retry { max_attempts, wait_ms } => {
    //                 // 重试有限次数
    //                 let mut retry_count = 0;
    //                 loop {
    //                     match tx.try_send(event_pretty.clone()) {
    //                         Ok(_) => break,
    //                         Err(e) => {
    //                             if e.is_full() {
    //                                 retry_count += 1;
    //                                 if retry_count >= max_attempts {
    //                                     log::warn!(
    //                                         "Channel is full after {} attempts, dropping transaction",
    //                                         retry_count
    //                                     );
    //                                     break;
    //                                 }
    //                                 tokio::time::sleep(tokio::time::Duration::from_millis(wait_ms))
    //                                     .await;
    //                             } else {
    //                                 log::error!("Channel is closed: {:?}", e);
    //                                 return Err(anyhow::anyhow!("Channel is closed: {:?}", e));
    //                             }
    //                         }
    //                     }
    //                 }
    //             }
    //         }
    //         Ok(())
    //     }
}
