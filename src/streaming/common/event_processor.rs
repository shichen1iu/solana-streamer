use std::sync::Arc;
use tokio::task::JoinHandle;

use solana_sdk::pubkey::Pubkey;

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
use crate::streaming::grpc::{BackpressureStrategy, BatchConfig, EventPretty};
use crate::streaming::shred::TransactionWithSlot;
use once_cell::sync::OnceCell;

/// 事件处理器
pub struct EventProcessor {
    pub(crate) metrics_manager: MetricsManager,
    pub(crate) config: ClientConfig,
    pub(crate) parser_cache: OnceCell<Arc<dyn EventParser>>,
    pub(crate) protocols: Vec<Protocol>,
    pub(crate) event_type_filter: Option<EventTypeFilter>,
    pub(crate) backpressure_strategy: BackpressureStrategy,
    pub(crate) batch_config: BatchConfig,
}

impl EventProcessor {
    /// 创建新的事件处理器
    pub fn new(metrics_manager: MetricsManager, config: ClientConfig) -> Self {
        Self {
            metrics_manager,
            config,
            parser_cache: OnceCell::new(),
            protocols: vec![],
            event_type_filter: None,
            backpressure_strategy: BackpressureStrategy::Block,
            batch_config: BatchConfig::default(),
        }
    }

    pub fn set_protocols_and_event_type_filter(
        &mut self,
        protocols: Vec<Protocol>,
        event_type_filter: Option<EventTypeFilter>,
        backpressure_strategy: BackpressureStrategy,
        batch_config: BatchConfig,
    ) {
        self.protocols = protocols.clone();
        self.event_type_filter = event_type_filter.clone();
        self.backpressure_strategy = backpressure_strategy;
        self.batch_config = batch_config;
        self.parser_cache
            .get_or_init(|| Arc::new(MutilEventParser::new(protocols, event_type_filter)));
    }

    pub fn get_parser(&self) -> Arc<dyn EventParser> {
        self.parser_cache.get().unwrap().clone()
    }

    pub fn get_event_handle(&self) -> Option<JoinHandle<()>> {
        return None;
    }

    pub async fn process_grpc_event_transaction_with_metrics<F>(
        &self,
        event_pretty: EventPretty,
        callback: &F,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync,
    {
        self.process_grpc_event_transaction(event_pretty, callback, bot_wallet).await?;
        Ok(())
    }

    async fn process_grpc_event_transaction<F>(
        &self,
        event_pretty: EventPretty,
        callback: &F,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync,
    {
        match event_pretty {
            EventPretty::Account(account_pretty) => {
                self.metrics_manager.add_account_process_count();
                let account_event = AccountEventParser::parse_account_event(
                    self.protocols.clone(),
                    account_pretty,
                    self.event_type_filter.clone(),
                );
                if let Some(event) = account_event {
                    let processing_time_us = event.program_handle_time_consuming_us() as f64;
                    callback(event);
                    // 更新性能指标（如果启用）
                    self.metrics_manager.update_metrics(
                        MetricsEventType::Account,
                        1,
                        processing_time_us,
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
                    )
                    .await
                    .unwrap_or_else(|_e| vec![]);

                let max_time_consuming_us = all_events
                    .iter()
                    .map(|event| event.program_handle_time_consuming_us())
                    .max()
                    .unwrap_or(0);

                // 保存事件数量用于日志记录
                let event_count = all_events.len();

                // 批量处理事件
                if !all_events.is_empty() {
                    for mut event in all_events {
                        event.set_program_handle_time_consuming_us(
                            chrono::Utc::now().timestamp_micros()
                                - event.program_received_time_us(),
                        );
                        callback(event);
                    }
                }

                // 更新性能指标
                // 更新性能指标（如果启用）
                self.metrics_manager.update_metrics(
                    MetricsEventType::Tx,
                    event_count as u64,
                    max_time_consuming_us as f64,
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
                callback(block_meta_event);
                // 更新性能指标（如果启用）
                self.metrics_manager.update_metrics(
                    MetricsEventType::BlockMeta,
                    1,
                    processing_time_us,
                );
            }
        }

        Ok(())
    }

    /// 即时处理单个交易
    pub async fn process_shred_transaction_immediate<F>(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
        callback: &F,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync,
    {
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
            )
            .await
            .unwrap_or_else(|_e| vec![]);

        let max_time_consuming_us = all_events
            .iter()
            .map(|event| event.program_handle_time_consuming_us())
            .max()
            .unwrap_or(0);

        // 保存事件数量用于日志记录
        let event_count = all_events.len();

        // 即时处理事件
        for mut event in all_events {
            event.set_program_handle_time_consuming_us(
                chrono::Utc::now().timestamp_micros() - event.program_received_time_us(),
            );
            callback(event);
        }

        // 实际调用性能指标更新
        self.metrics_manager.update_metrics(
            MetricsEventType::Tx,
            event_count as u64,
            max_time_consuming_us as f64,
        );

        Ok(())
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
            backpressure_strategy: self.backpressure_strategy.clone(),
            batch_config: self.batch_config.clone(),
        }
    }
}
