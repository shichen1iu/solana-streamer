use crate::{
    common::AnyResult,
    streaming::{
        grpc::{EventPretty, StreamHandler},
        yellowstone_grpc::YellowstoneGrpc,
    },
};
use futures::StreamExt;
use log::error;
use solana_program::pubkey;
use solana_sdk::{pubkey::Pubkey, transaction::VersionedTransaction};
use solana_transaction_status::TransactionWithStatusMeta;

const SYSTEM_PROGRAM_ID: Pubkey = pubkey!("11111111111111111111111111111111");

#[derive(Debug)]
pub enum SystemEvent {
    NewTransfer(TransferInfo),
    Error(String),
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct TransferInfo {
    pub slot: u64,
    pub signature: String,
    pub tx: Option<VersionedTransaction>,
}

impl YellowstoneGrpc {
    pub async fn subscribe_system<F>(
        &self,
        callback: F,
        account_include: Option<Vec<String>>,
        account_exclude: Option<Vec<String>>,
    ) -> AnyResult<()>
    where
        F: Fn(SystemEvent) + Send + Sync + 'static,
    {
        let addrs = vec![SYSTEM_PROGRAM_ID.to_string()];
        let account_include = account_include.unwrap_or_default();
        let account_exclude = account_exclude.unwrap_or_default();
        let transactions = self.subscription_manager.get_subscribe_request_filter(
            account_include,
            account_exclude,
            addrs,
            None,
        );
        let (mut subscribe_tx, mut stream) = self
            .subscription_manager
            .subscribe_with_request(transactions, None, None, None)
            .await?;

        let callback = Box::new(callback);

        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Ok(event_pretty) =
                            StreamHandler::handle_stream_system_message(msg, &mut subscribe_tx)
                                .await
                        {
                            if let Some(event_pretty) = event_pretty {
                                if let Err(e) =
                                    Self::process_system_transaction(event_pretty, &*callback).await
                                {
                                    error!("Error processing transaction: {e:?}");
                                }
                            }
                        }
                    }
                    Err(error) => {
                        error!("Stream error: {error:?}");
                        break;
                    }
                }
            }
        });
        Ok(())
    }

    async fn process_system_transaction<F>(event_pretty: EventPretty, callback: &F) -> AnyResult<()>
    where
        F: Fn(SystemEvent) + Send + Sync,
    {
        match event_pretty {
            EventPretty::Transaction(transaction_pretty) => {
                let trade_raw: TransactionWithStatusMeta = transaction_pretty.tx;
                let meta = trade_raw.get_status_meta();

                if meta.is_none() {
                    return Ok(());
                }

                let transaction = trade_raw.get_transaction();

                callback(SystemEvent::NewTransfer(TransferInfo {
                    slot: transaction_pretty.slot,
                    signature: transaction_pretty.signature.to_string(),
                    tx: Some(transaction),
                }));
            }
            _ => {}
        }
        Ok(())
    }
}
