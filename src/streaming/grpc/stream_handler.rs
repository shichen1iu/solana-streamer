use chrono::Local;
use futures::{channel::mpsc, sink::Sink, SinkExt};
use solana_sdk::pubkey::Pubkey;
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, SubscribeRequest, SubscribeRequestPing, SubscribeUpdate,
};

use super::types::{BlockMetaPretty, EventPretty, TransactionPretty};
use crate::common::AnyResult;
use crate::streaming::common::EventProcessor;
use crate::streaming::grpc::AccountPretty;

/// 流消息处理器
pub struct StreamHandler;

impl StreamHandler {
    /// 处理单个流消息
    pub async fn handle_stream_message(
        msg: SubscribeUpdate,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
        event_processor: EventProcessor,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        let created_at = msg.created_at;
        match msg.update_oneof {
            Some(UpdateOneof::Account(account)) => {
                let account_pretty = AccountPretty::from(account);
                log::debug!("Received account: {:?}", account_pretty);
                event_processor
                    .process_grpc_event_transaction_with_metrics(
                        EventPretty::Account(account_pretty),
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

    pub async fn handle_stream_system_message(
        msg: SubscribeUpdate,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
    ) -> AnyResult<Option<EventPretty>> {
        let created_at = msg.created_at;
        let event_pretty = match msg.update_oneof {
            Some(UpdateOneof::Transaction(sut)) => Some(TransactionPretty::from((sut, created_at))),
            Some(UpdateOneof::Ping(_)) => {
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await?;
                None
            }
            Some(UpdateOneof::Pong(_)) => None,
            _ => None,
        };
        Ok(event_pretty.map(|e| EventPretty::Transaction(e)))
    }
}
