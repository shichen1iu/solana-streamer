use std::sync::Arc;

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;

use crate::common::AnyResult;
use crate::streaming::common::{
    MetricsEventType, MetricsManager, StreamClientConfig as ClientConfig,
};
use crate::streaming::event_parser::common::filter::EventTypeFilter;
use crate::streaming::event_parser::core::account_event_parser::AccountEventParser;
use crate::streaming::event_parser::core::common_event_parser::CommonEventParser;
use crate::streaming::event_parser::EventParser;
use crate::streaming::event_parser::{
    core::traits::UnifiedEvent, protocols::mutil::parser::MutilEventParser, Protocol,
};
use crate::streaming::grpc::{BackpressureConfig, BatchConfig, EventPretty};
use crate::streaming::shred::TransactionWithSlot;
use once_cell::sync::OnceCell;

/// 事件处理器
pub struct EventProcessor {
    pub(crate) metrics_manager: MetricsManager,
    pub(crate) config: ClientConfig,
    pub(crate) parser_cache: OnceCell<Arc<dyn EventParser>>,
    pub(crate) protocols: Vec<Protocol>,
    pub(crate) event_type_filter: Option<EventTypeFilter>,
    pub(crate) callback: Option<Arc<dyn Fn(Box<dyn UnifiedEvent>) + Send + Sync>>,
    pub(crate) backpressure_config: BackpressureConfig,
    pub(crate) batch_config: BatchConfig,
}

impl EventProcessor {
    /// 创建新的事件处理器
    pub fn new(metrics_manager: MetricsManager, config: ClientConfig) -> Self {
        let backpressure_config = config.backpressure.clone();
        Self {
            metrics_manager,
            config,
            parser_cache: OnceCell::new(),
            protocols: vec![],
            event_type_filter: None,
            backpressure_config,
            batch_config: BatchConfig::default(),
            callback: None,
        }
    }

    pub fn set_protocols_and_event_type_filter(
        &mut self,
        protocols: Vec<Protocol>,
        event_type_filter: Option<EventTypeFilter>,
        backpressure_config: BackpressureConfig,
        batch_config: BatchConfig,
        callback: Option<Arc<dyn Fn(Box<dyn UnifiedEvent>) + Send + Sync>>,
    ) {
        self.protocols = protocols.clone();
        self.event_type_filter = event_type_filter.clone();
        self.backpressure_config = backpressure_config;
        self.batch_config = batch_config;
        self.callback = callback;
        self.parser_cache
            .get_or_init(|| Arc::new(MutilEventParser::new(protocols, event_type_filter)));
    }

    pub fn get_parser(&self) -> Arc<dyn EventParser> {
        self.parser_cache.get().unwrap().clone()
    }

    pub async fn process_grpc_event_transaction_with_metrics(
        &self,
        event_pretty: EventPretty,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        self.process_grpc_event_transaction(event_pretty, bot_wallet).await?;
        Ok(())
    }

    async fn process_grpc_event_transaction(
        &self,
        event_pretty: EventPretty,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        match event_pretty {
            EventPretty::Account(account_pretty) => {
                self.metrics_manager.add_account_process_count();
                let signature = account_pretty.signature;
                let account_event = AccountEventParser::parse_account_event(
                    self.protocols.clone(),
                    account_pretty,
                    self.event_type_filter.clone(),
                );
                if let Some(event) = account_event {
                    let processing_time_us = event.program_handle_time_consuming_us() as f64;
                    self.invoke_callback(event);
                    self.update_metrics(
                        MetricsEventType::Account,
                        1,
                        processing_time_us,
                        Some(signature),
                    );
                }
            }
            EventPretty::Transaction(transaction_pretty) => {
                self.metrics_manager.add_tx_process_count();
                let slot = transaction_pretty.slot;
                let signature = transaction_pretty.signature;
                // 使用缓存获取解析器
                let parser = self.get_parser();
                let all_events = parser
                    .parse_transaction(
                        transaction_pretty.tx.clone(),
                        signature,
                        Some(slot),
                        transaction_pretty.block_time,
                        transaction_pretty.program_received_time_us,
                        bot_wallet,
                        transaction_pretty.transaction_index,
                    )
                    .await
                    .unwrap_or_else(|_e| vec![]);

                let mut all_time_consuming_us = 0;
                let event_count = all_events.len();

                // 为所有事件设置交易索引
                for mut event in all_events {
                    event.set_program_handle_time_consuming_us(
                        chrono::Utc::now().timestamp_micros() - event.program_received_time_us(),
                    );
                    all_time_consuming_us += event.program_handle_time_consuming_us();
                    self.invoke_callback(event);
                }

                // 更新性能指标
                self.update_metrics(
                    MetricsEventType::Transaction,
                    event_count as u64,
                    all_time_consuming_us as f64,
                    Some(signature),
                );
            }
            EventPretty::BlockMeta(block_meta_pretty) => {
                self.metrics_manager.add_block_meta_process_count();
                let block_time_ms = block_meta_pretty
                    .block_time
                    .map(|ts| ts.seconds * 1000 + ts.nanos as i64 / 1_000_000)
                    .unwrap_or_else(|| chrono::Utc::now().timestamp_millis());
                let block_meta_event = CommonEventParser::generate_block_meta_event(
                    block_meta_pretty.slot,
                    &block_meta_pretty.block_hash,
                    block_time_ms,
                    block_meta_pretty.program_received_time_us,
                );
                let processing_time_us = block_meta_event.program_handle_time_consuming_us() as f64;
                self.invoke_callback(block_meta_event);
                self.update_metrics(MetricsEventType::BlockMeta, 1, processing_time_us, None);
            }
        }

        Ok(())
    }

    pub fn invoke_callback(&self, event: Box<dyn UnifiedEvent>) {
        if let Some(callback) = self.callback.as_ref() {
            callback(event);
        }
    }

    /// 即时处理单个交易
    pub async fn process_shred_transaction_immediate(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        self.process_shred_transaction(transaction_with_slot, bot_wallet).await
    }

    pub async fn process_shred_transaction(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        self.metrics_manager.add_tx_process_count();
        let program_received_time_us = chrono::Utc::now().timestamp_micros();
        let slot = transaction_with_slot.slot;
        let versioned_tx = transaction_with_slot.transaction;
        let signature = versioned_tx.signatures[0];

        // 获取缓存的解析器
        let parser = self.get_parser();

        let all_events = parser
            .parse_versioned_transaction(
                &versioned_tx,
                signature,
                Some(slot),
                None,
                program_received_time_us,
                bot_wallet,
                None,
            )
            .await
            .unwrap_or_else(|_e| vec![]);

        let mut max_time_consuming_us = 0;

        // 保存事件数量用于日志记录
        let event_count = all_events.len();

        // 即时处理事件
        for mut event in all_events {
            event.set_program_handle_time_consuming_us(
                chrono::Utc::now().timestamp_micros() - event.program_received_time_us(),
            );
            max_time_consuming_us =
                max_time_consuming_us.max(event.program_handle_time_consuming_us());
            self.invoke_callback(event);
        }

        // 实际调用性能指标更新
        self.update_metrics(
            MetricsEventType::Transaction,
            event_count as u64,
            max_time_consuming_us as f64,
            Some(signature),
        );

        Ok(())
    }

    fn update_metrics(
        &self,
        ty: MetricsEventType,
        count: u64,
        time_us: f64,
        signature: Option<Signature>,
    ) {
        self.metrics_manager.update_metrics(ty, count, time_us, signature);
    }
}

// 实现 Clone trait 以支持模块间共享
impl Clone for EventProcessor {
    fn clone(&self) -> Self {
        Self {
            metrics_manager: self.metrics_manager.clone(),
            config: self.config.clone(),
            parser_cache: self.parser_cache.clone(),
            protocols: self.protocols.clone(),
            event_type_filter: self.event_type_filter.clone(),
            backpressure_config: self.backpressure_config.clone(),
            batch_config: self.batch_config.clone(),
            callback: self.callback.clone(),
        }
    }
}
