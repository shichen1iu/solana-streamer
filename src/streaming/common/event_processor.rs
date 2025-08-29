use std::sync::Arc;
use std::time::Instant;

use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use tokio::sync::Semaphore;

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
use crate::streaming::grpc::{BackpressureConfig, EventPretty};
use crate::streaming::shred::TransactionWithSlot;
use once_cell::sync::OnceCell;

/// Event processor
pub struct EventProcessor {
    pub(crate) metrics_manager: MetricsManager,
    pub(crate) config: ClientConfig,
    pub(crate) parser_cache: OnceCell<Arc<dyn EventParser>>,
    pub(crate) protocols: Vec<Protocol>,
    pub(crate) event_type_filter: Option<EventTypeFilter>,
    pub(crate) callback: Option<Arc<dyn Fn(Box<dyn UnifiedEvent>) + Send + Sync>>,
    pub(crate) backpressure_config: BackpressureConfig,
    /// Backpressure semaphore for controlling concurrent processing count
    pub(crate) backpressure_semaphore: Arc<Semaphore>,
}

impl EventProcessor {
    /// Create a new event processor
    pub fn new(metrics_manager: MetricsManager, config: ClientConfig) -> Self {
        let backpressure_config = config.backpressure.clone();
        let backpressure_semaphore = Arc::new(Semaphore::new(backpressure_config.permits));
        Self {
            metrics_manager,
            config,
            parser_cache: OnceCell::new(),
            protocols: vec![],
            event_type_filter: None,
            backpressure_config,
            callback: None,
            backpressure_semaphore,
        }
    }

    pub fn set_protocols_and_event_type_filter(
        &mut self,
        protocols: Vec<Protocol>,
        event_type_filter: Option<EventTypeFilter>,
        backpressure_config: BackpressureConfig,
        callback: Option<Arc<dyn Fn(Box<dyn UnifiedEvent>) + Send + Sync>>,
    ) {
        self.protocols = protocols.clone();
        self.event_type_filter = event_type_filter.clone();
        // Recreate semaphore if backpressure configuration changes
        if self.backpressure_config.permits != backpressure_config.permits {
            self.backpressure_semaphore = Arc::new(Semaphore::new(backpressure_config.permits));
        }
        self.backpressure_config = backpressure_config;
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
        // Backpressure control logic
        let backpressure_start = Instant::now();
        let result = self.apply_backpressure_control(event_pretty, bot_wallet).await;
        let backpressure_duration = backpressure_start.elapsed();

        // Record backpressure-related metrics
        self.metrics_manager.record_backpressure_metrics(
            backpressure_duration,
            result.is_ok(),
            self.backpressure_semaphore.available_permits(),
        );

        result
    }

    /// Apply backpressure control strategy
    async fn apply_backpressure_control(
        &self,
        event_pretty: EventPretty,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        use crate::streaming::common::BackpressureStrategy;

        match self.backpressure_config.strategy {
            BackpressureStrategy::Block => {
                // Blocking strategy: acquire semaphore permit
                let _permit =
                    self.backpressure_semaphore.acquire().await.map_err(|e| {
                        anyhow::anyhow!("Failed to acquire backpressure permit: {}", e)
                    })?;
                self.process_grpc_event_transaction(event_pretty, bot_wallet).await
            }
            BackpressureStrategy::Drop => {
                // Drop strategy: try to acquire permit, drop if failed
                match self.backpressure_semaphore.try_acquire() {
                    Ok(_permit) => {
                        let result =
                            self.process_grpc_event_transaction(event_pretty, bot_wallet).await;
                        result
                    }
                    Err(_) => {
                        // Record dropped event
                        self.metrics_manager.increment_dropped_events();
                        Ok(())
                    }
                }
            }
            BackpressureStrategy::Async => {
                // Async strategy: process asynchronously regardless of permits
                self.spawn_async_processing(event_pretty, bot_wallet).await;
                Ok(())
            }
        }
    }

    /// Process event asynchronously (without waiting for semaphore permit)
    async fn spawn_async_processing(&self, event_pretty: EventPretty, bot_wallet: Option<Pubkey>) {
        let processor = self.clone();

        tokio::spawn(async move {
            // Async strategy: no semaphore control, allow unlimited concurrency
            // Execute actual event processing directly
            if let Err(e) = processor.process_grpc_event_transaction(event_pretty, bot_wallet).await
            {
                log::error!("Error in async event processing: {}", e);
            }
        });
    }

    async fn process_grpc_event_transaction(
        &self,
        event_pretty: EventPretty,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        if self.callback.is_none() {
            return Ok(());
        }
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
                let tx = transaction_pretty.tx;
                let block_time = transaction_pretty.block_time;
                let program_received_time_us = transaction_pretty.program_received_time_us;
                let transaction_index = transaction_pretty.transaction_index;
                // Use cache to get parser
                let parser = self.get_parser();
                let callback = self.callback.clone().unwrap();
                let metrics_manager = self.metrics_manager.clone();
                let adapter_callback = Arc::new(move |event: Box<dyn UnifiedEvent>| {
                    let processing_time_us = event.program_handle_time_consuming_us() as f64;
                    callback(event);
                    metrics_manager.update_metrics(
                        MetricsEventType::Transaction,
                        1,
                        processing_time_us,
                        Some(signature),
                    );
                });
                parser
                    .parse_transaction_owned(
                        tx,
                        signature,
                        Some(slot),
                        block_time,
                        program_received_time_us,
                        bot_wallet,
                        transaction_index,
                        adapter_callback,
                    )
                    .await?;
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

    /// Process a single transaction immediately
    pub async fn process_shred_transaction_immediate(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        self.process_shred_transaction(transaction_with_slot, bot_wallet).await
    }

    /// Process shred transaction with backpressure control and performance monitoring
    pub async fn process_shred_transaction_with_metrics(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        // Backpressure control logic
        let backpressure_start = Instant::now();
        let result = self.apply_shred_backpressure_control(transaction_with_slot, bot_wallet).await;
        let backpressure_duration = backpressure_start.elapsed();

        // Record backpressure-related metrics
        self.metrics_manager.record_backpressure_metrics(
            backpressure_duration,
            result.is_ok(),
            self.backpressure_semaphore.available_permits(),
        );

        result
    }

    /// Apply shred backpressure control strategy
    async fn apply_shred_backpressure_control(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        use crate::streaming::common::BackpressureStrategy;

        match self.backpressure_config.strategy {
            BackpressureStrategy::Block => {
                // Blocking strategy: acquire semaphore permit
                let _permit =
                    self.backpressure_semaphore.acquire().await.map_err(|e| {
                        anyhow::anyhow!("Failed to acquire backpressure permit: {}", e)
                    })?;
                self.process_shred_transaction(transaction_with_slot, bot_wallet).await
            }
            BackpressureStrategy::Drop => {
                // Drop strategy: try to acquire permit, drop if failed
                match self.backpressure_semaphore.try_acquire() {
                    Ok(_permit) => {
                        let result =
                            self.process_shred_transaction(transaction_with_slot, bot_wallet).await;
                        result
                    }
                    Err(_) => {
                        // Record dropped event
                        self.metrics_manager.increment_dropped_events();
                        Ok(())
                    }
                }
            }
            BackpressureStrategy::Async => {
                // Async strategy: process asynchronously regardless of permits
                self.spawn_async_shred_processing(transaction_with_slot, bot_wallet).await;
                Ok(())
            }
        }
    }

    /// Process shred event asynchronously (without waiting for semaphore permit)
    async fn spawn_async_shred_processing(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) {
        let processor = self.clone();

        tokio::spawn(async move {
            // Async strategy: no semaphore control, allow unlimited concurrency
            // Execute actual event processing directly
            if let Err(e) =
                processor.process_shred_transaction(transaction_with_slot, bot_wallet).await
            {
                log::error!("Error in async shred event processing: {}", e);
            }
        });
    }

    pub async fn process_shred_transaction(
        &self,
        transaction_with_slot: TransactionWithSlot,
        bot_wallet: Option<Pubkey>,
    ) -> AnyResult<()> {
        if self.callback.is_none() {
            return Ok(());
        }
        self.metrics_manager.add_tx_process_count();
        let tx = transaction_with_slot.transaction;

        let slot = transaction_with_slot.slot;
        let signature = tx.signatures[0];
        let program_received_time_us = transaction_with_slot.program_received_time_us;
        // Use cache to get parser
        let parser = self.get_parser();
        let callback = self.callback.clone().unwrap();
        let metrics_manager = self.metrics_manager.clone();

        let adapter_callback = Arc::new(move |event: Box<dyn UnifiedEvent>| {
            let processing_time_us = event.program_handle_time_consuming_us() as f64;
            callback(event);
            metrics_manager.update_metrics(
                MetricsEventType::Transaction,
                1,
                processing_time_us,
                Some(signature),
            );
        });

        parser
            .parse_versioned_transaction_owned(
                tx,
                signature,
                Some(slot),
                None,
                program_received_time_us,
                bot_wallet,
                None,
                &[],
                adapter_callback,
            )
            .await?;

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

// Implement Clone trait to support sharing between modules
impl Clone for EventProcessor {
    fn clone(&self) -> Self {
        Self {
            metrics_manager: self.metrics_manager.clone(),
            config: self.config.clone(),
            parser_cache: self.parser_cache.clone(),
            protocols: self.protocols.clone(),
            event_type_filter: self.event_type_filter.clone(),
            backpressure_config: self.backpressure_config.clone(),
            callback: self.callback.clone(),
            backpressure_semaphore: self.backpressure_semaphore.clone(),
        }
    }
}
