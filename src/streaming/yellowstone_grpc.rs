use std::{collections::HashMap, fmt, time::Duration};

use chrono::Local;
use futures::{channel::mpsc, sink::Sink, SinkExt, Stream, StreamExt};
use log::{error, info};
use prost_types::Timestamp;
use rustls::crypto::{ring::default_provider, CryptoProvider};
use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::{EncodedTransactionWithStatusMeta, UiTransactionEncoding};
use tonic::{transport::channel::ClientTlsConfig, Status};
use yellowstone_grpc_client::{GeyserGrpcClient, Interceptor};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, SubscribeUpdate,
    SubscribeUpdateTransaction,
};
use std::sync::Arc;
use tokio::sync::Mutex;

use crate::common::AnyResult;
use crate::streaming::event_parser::{EventParserFactory, Protocol, UnifiedEvent};

type TransactionsFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;

const CONNECT_TIMEOUT: u64 = 10;
const REQUEST_TIMEOUT: u64 = 60;
// 根据实际并发量调整通道大小，避免背压
const CHANNEL_SIZE: usize = 5000;
const MAX_DECODING_MESSAGE_SIZE: usize = 1024 * 1024 * 10;

// 批处理配置
const BATCH_SIZE: usize = 100;  // 批处理50个事件
const BATCH_TIMEOUT_MS: u64 = 10;  // 减少超时时间到10ms

// 连接池配置（为将来扩展保留）
#[allow(dead_code)]
const CONNECTION_POOL_SIZE: usize = 5;
#[allow(dead_code)]
const CONNECTION_IDLE_TIMEOUT: Duration = Duration::from_secs(300); // 5分钟

// 工作线程池配置
const WORKER_THREADS: usize = 8;
const TASK_QUEUE_SIZE: usize = 10000;

/// 工作线程池配置
pub struct WorkerPoolConfig {
    pub worker_threads: usize,
    pub task_queue_size: usize,
}

impl Default for WorkerPoolConfig {
    fn default() -> Self {
        Self {
            worker_threads: WORKER_THREADS,
            task_queue_size: TASK_QUEUE_SIZE,
        }
    }
}

/// 性能监控指标
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub events_processed: u64,
    pub events_per_second: f64,
    pub average_processing_time_ms: f64,
    pub min_processing_time_ms: f64,
    pub max_processing_time_ms: f64,
    pub cache_hit_rate: f64,
    pub memory_usage_mb: f64,
    pub last_update_time: std::time::Instant,
    pub events_in_window: u64,
    pub window_start_time: std::time::Instant,
}

impl Default for PerformanceMetrics {
    fn default() -> Self {
        Self::new()
    }
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        let now = std::time::Instant::now();
        Self {
            events_processed: 0,
            events_per_second: 0.0,
            average_processing_time_ms: 0.0,
            min_processing_time_ms: f64::MAX,
            max_processing_time_ms: 0.0,
            cache_hit_rate: 0.0,
            memory_usage_mb: 0.0,
            last_update_time: now,
            events_in_window: 0,
            window_start_time: now,
        }
    }
}

/// gRPC连接池 - 简化版本
pub struct GrpcConnectionPool {
    endpoint: String,
    x_token: Option<String>,
}

impl GrpcConnectionPool {
    pub fn new(endpoint: String, x_token: Option<String>) -> Self {
        Self {
            endpoint,
            x_token,
        }
    }

    pub async fn create_connection(&self) -> AnyResult<GeyserGrpcClient<impl Interceptor>> {
        let builder = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token(self.x_token.clone())?
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .max_decoding_message_size(MAX_DECODING_MESSAGE_SIZE)
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT));

        Ok(builder.connect().await?)
    }
}

/// 批处理事件收集器
pub struct EventBatchCollector<F>
where
    F: Fn(Vec<Box<dyn UnifiedEvent>>) + Send + Sync + 'static,
{
    callback: F,
    batch: Vec<Box<dyn UnifiedEvent>>,
    batch_size: usize,
    timeout_ms: u64,
    last_flush_time: std::time::Instant,
}

impl<F> EventBatchCollector<F>
where
    F: Fn(Vec<Box<dyn UnifiedEvent>>) + Send + Sync + 'static,
{
    pub fn new(callback: F, batch_size: usize, timeout_ms: u64) -> Self {
        Self {
            callback,
            batch: Vec::with_capacity(batch_size),
            batch_size,
            timeout_ms,
            last_flush_time: std::time::Instant::now(),
        }
    }

    pub fn add_event(&mut self, event: Box<dyn UnifiedEvent>) {
        log::debug!("Adding event to batch: {} (type: {:?})", event.id(), event.event_type());
        self.batch.push(event);
        
        // 检查是否需要刷新批次
        if self.batch.len() >= self.batch_size || self.should_flush_by_timeout() {
            log::info!("Flushing batch: size={}, timeout={}", self.batch.len(), self.should_flush_by_timeout());
            self.flush();
        }
    }

    pub fn flush(&mut self) {
        if !self.batch.is_empty() {
            let events = std::mem::replace(&mut self.batch, Vec::with_capacity(self.batch_size));
            log::info!("Flushing {} events from batch processor", events.len());
            
            // 添加更详细的调试信息
            for (i, event) in events.iter().enumerate() {
                log::info!("Event {}: Type={:?}, ID={}", i, event.event_type(), event.id());
            }
            
            // 执行回调并捕获可能的错误
            log::info!("About to execute batch callback with {} events", events.len());
            match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                (self.callback)(events);
            })) {
                Ok(_) => {
                    log::info!("Batch callback executed successfully");
                }
                Err(e) => {
                    log::error!("Batch callback panicked: {:?}", e);
                }
            }
            
            self.last_flush_time = std::time::Instant::now();
        } else {
            log::debug!("No events to flush");
        }
    }

    fn should_flush_by_timeout(&self) -> bool {
        self.last_flush_time.elapsed().as_millis() >= self.timeout_ms as u128
    }
}

#[derive(Clone)]
pub struct TransactionPretty {
    pub slot: u64,
    pub block_time: Option<Timestamp>,
    pub signature: Signature,
    pub is_vote: bool,
    pub tx: EncodedTransactionWithStatusMeta,
}

impl fmt::Debug for TransactionPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct TxWrap<'a>(&'a EncodedTransactionWithStatusMeta);
        impl<'a> fmt::Debug for TxWrap<'a> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                let serialized = serde_json::to_string(self.0).expect("failed to serialize");
                fmt::Display::fmt(&serialized, f)
            }
        }

        f.debug_struct("TransactionPretty")
            .field("slot", &self.slot)
            .field("signature", &self.signature)
            .field("is_vote", &self.is_vote)
            .field("tx", &TxWrap(&self.tx))
            .finish()
    }
}

impl From<(SubscribeUpdateTransaction, Option<Timestamp>)> for TransactionPretty {
    fn from(
        (SubscribeUpdateTransaction { transaction, slot }, block_time): (
            SubscribeUpdateTransaction,
            Option<Timestamp>,
        ),
    ) -> Self {
        let tx = transaction.expect("should be defined");
        Self {
            slot,
            block_time,
            signature: Signature::try_from(tx.signature.as_slice()).expect("valid signature"),
            is_vote: tx.is_vote,
            tx: yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                .expect("valid tx with meta")
                .encode(UiTransactionEncoding::Base64, Some(u8::MAX), true)
                .expect("failed to encode"),
        }
    }
}

#[derive(Clone)]
pub struct YellowstoneGrpc {
    endpoint: String,
    x_token: Option<String>,
    metrics: Arc<Mutex<PerformanceMetrics>>,
    enable_metrics: bool, // 是否启用性能监控
}

impl YellowstoneGrpc {
    pub fn new(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, true)
    }

    pub fn new_with_config(
        endpoint: String, 
        x_token: Option<String>, 
        enable_metrics: bool,
    ) -> AnyResult<Self> {
        if CryptoProvider::get_default().is_none() {
            default_provider()
                .install_default()
                .map_err(|e| anyhow::anyhow!("Failed to install crypto provider: {:?}", e))?;
        }

        Ok(Self { 
            endpoint, 
            x_token,
            metrics: Arc::new(Mutex::new(PerformanceMetrics::new())),
            enable_metrics,
        })
    }

    /// 获取性能指标
    pub async fn get_metrics(&self) -> PerformanceMetrics {
        let metrics = self.metrics.lock().await;
        metrics.clone()
    }

    /// 启用或禁用性能监控
    pub fn set_enable_metrics(&mut self, enabled: bool) {
        self.enable_metrics = enabled;
    }



    /// 打印性能指标
    pub async fn print_metrics(&self) {
        let metrics = self.get_metrics().await;
        println!("📊 Performance Metrics:");
        println!("   Events Processed: {}", metrics.events_processed);
        println!("   Events/Second: {:.2}", metrics.events_per_second);
        println!("   Avg Processing Time: {:.2}ms", metrics.average_processing_time_ms);
        println!("   Min Processing Time: {:.2}ms", metrics.min_processing_time_ms);
        println!("   Max Processing Time: {:.2}ms", metrics.max_processing_time_ms);
        println!("   Cache Hit Rate: {:.2}%", metrics.cache_hit_rate * 100.0);
        println!("   Memory Usage: {:.2}MB", metrics.memory_usage_mb);
        println!("---");
    }

    /// 启动自动性能监控任务
    pub async fn start_auto_metrics_monitoring(&self) {
        // 检查是否启用性能监控
        if !self.enable_metrics {
            return; // 如果未启用性能监控，不启动监控任务
        }

        let grpc_clone = self.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(10));
            loop {
                interval.tick().await;
                grpc_clone.print_metrics().await;
            }
        });
    }

    /// 更新性能指标
    async fn update_metrics(&self, events_processed: u64, processing_time_ms: f64) {
        // 检查是否启用性能监控
        if !self.enable_metrics {
            return; // 如果未启用性能监控，直接返回
        }

        let mut metrics = self.metrics.lock().await;
        let now = std::time::Instant::now();
        
        metrics.events_processed += events_processed;
        metrics.events_in_window += events_processed;
        metrics.last_update_time = now;
        
        // 更新最快和最慢处理时间
        if processing_time_ms < metrics.min_processing_time_ms {
            metrics.min_processing_time_ms = processing_time_ms;
        }
        if processing_time_ms > metrics.max_processing_time_ms {
            metrics.max_processing_time_ms = processing_time_ms;
        }
        
        // 计算平均处理时间
        if metrics.events_processed > 0 {
            metrics.average_processing_time_ms = 
                (metrics.average_processing_time_ms * (metrics.events_processed - events_processed) as f64 + processing_time_ms) 
                / metrics.events_processed as f64;
        }
        
        // 基于时间窗口计算每秒处理事件数（5秒窗口）
        let window_duration = std::time::Duration::from_secs(5);
        if now.duration_since(metrics.window_start_time) >= window_duration {
            let window_seconds = now.duration_since(metrics.window_start_time).as_secs_f64();
            if window_seconds > 0.0 && metrics.events_in_window > 0 {
                metrics.events_per_second = metrics.events_in_window as f64 / window_seconds;
            } else {
                // 如果窗口内没有事件，保持之前的速率或设为0
                metrics.events_per_second = 0.0;
            }
            
            // 重置窗口
            metrics.events_in_window = 0;
            metrics.window_start_time = now;
        } else {
            // 如果窗口还没满，不更新 events_per_second，保持之前的计算值
            // 这样可以避免因为单次批处理时间波动导致的指标跳跃
        }
        
        // 估算内存使用（基于处理的事件数量）
        metrics.memory_usage_mb = metrics.events_processed as f64 * 0.001; // 每个事件约1KB
    }

    pub async fn connect(&self) -> AnyResult<GeyserGrpcClient<impl Interceptor>> {
        let builder = GeyserGrpcClient::build_from_shared(self.endpoint.clone())?
            .x_token(self.x_token.clone())?
            .tls_config(ClientTlsConfig::new().with_native_roots())?
            .max_decoding_message_size(MAX_DECODING_MESSAGE_SIZE)
            .connect_timeout(Duration::from_secs(CONNECT_TIMEOUT))
            .timeout(Duration::from_secs(REQUEST_TIMEOUT));

        Ok(builder.connect().await?)
    }

    pub async fn subscribe_with_request(
        &self,
        transactions: TransactionsFilterMap,
        commitment: Option<CommitmentLevel>,
    ) -> AnyResult<(
        impl Sink<SubscribeRequest, Error = mpsc::SendError>,
        impl Stream<Item = Result<SubscribeUpdate, Status>>,
    )> {
        let subscribe_request = SubscribeRequest {
            transactions,
            commitment: if let Some(commitment) = commitment {
                Some(commitment as i32)
            } else {
                Some(CommitmentLevel::Processed.into())
            },
            ..Default::default()
        };

        let mut client = self.connect().await?;
        let (sink, stream) = client
            .subscribe_with_request(Some(subscribe_request))
            .await?;
        Ok((sink, stream))
    }

    pub fn get_subscribe_request_filter(
        &self,
        account_include: Vec<String>,
        account_exclude: Vec<String>,
        account_required: Vec<String>,
    ) -> TransactionsFilterMap {
        let mut transactions = HashMap::new();
        transactions.insert(
            "client".to_string(),
            SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include,
                account_exclude,
                account_required,
            },
        );
        transactions
    }

    pub async fn handle_stream_message(
        msg: SubscribeUpdate,
        tx: &mut mpsc::Sender<TransactionPretty>,
        subscribe_tx: &mut (impl Sink<SubscribeRequest, Error = mpsc::SendError> + Unpin),
    ) -> AnyResult<()> {
        let created_at = msg.created_at;
        match msg.update_oneof {
            Some(UpdateOneof::Transaction(sut)) => {
                let transaction_pretty = TransactionPretty::from((sut, created_at));
                log::info!("Received transaction: {} at slot {}", transaction_pretty.signature, transaction_pretty.slot);
                tx.try_send(transaction_pretty)?;
            }
            Some(UpdateOneof::Ping(_)) => {
                subscribe_tx
                    .send(SubscribeRequest {
                        ping: Some(SubscribeRequestPing { id: 1 }),
                        ..Default::default()
                    })
                    .await?;
                info!("service is ping: {}", Local::now());
            }
            Some(UpdateOneof::Pong(_)) => {
                info!("service is pong: {}", Local::now());
            }
            _ => {
                log::debug!("Received other message type");
            }
        }
        Ok(())
    }

    /// Subscribe to Yellowstone GRPC service events with advanced filtering options
    ///
    /// This method allows subscribing to specific protocol events with more granular account filtering.
    /// It processes transactions in real-time and calls the provided callback function when matching events are found.
    ///
    /// # Parameters
    ///
    /// * `protocols` - List of protocols to parse (e.g., PumpFun, PumpSwap, Bonk, RaydiumCpmm)
    /// * `bot_wallet` - Optional bot wallet address. If passed: in PumpFunTradeEvent if user is in the address, is_bot=true will be set. In BonkTradeEvent if payer is in the address, is_bot=true will be set. Default is false.
    /// * `account_include` - List of account addresses to include in the subscription
    /// * `account_exclude` - List of account addresses to exclude from the subscription
    /// * `account_required` - List of account addresses that must be present in transactions
    /// * `commitment` - Optional commitment level for the subscription
    /// * `callback` - Function to call when matching events are found
    #[allow(clippy::too_many_arguments)]
    pub async fn subscribe_events_v2<F>(
        &self,
        protocols: Vec<Protocol>,
        bot_wallet: Option<Pubkey>,
        account_include: Vec<String>,
        account_exclude: Vec<String>,
        account_required: Vec<String>,
        commitment: Option<CommitmentLevel>,
        callback: F,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync + 'static,
    {
        
        // 启动自动性能监控
        self.start_auto_metrics_monitoring().await;
        
        if account_include.is_empty() && account_exclude.is_empty() && account_required.is_empty() {
            return Err(anyhow::anyhow!(
                "account_include or account_exclude or account_required cannot be empty"
            ));
        }

        let transactions =
            self.get_subscribe_request_filter(account_include, account_exclude, account_required);
        // Subscribe to events
        let (mut subscribe_tx, mut stream) = self
            .subscribe_with_request(transactions, commitment)
            .await?;

        // Create channel
        let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(CHANNEL_SIZE);

        // 创建批处理器，将单个事件回调转换为批量回调
        let batch_callback = move |events: Vec<Box<dyn UnifiedEvent>>| {
            for event in events {
                callback(event);
            }
        };
        
        let mut batch_processor = EventBatchCollector::new(batch_callback, BATCH_SIZE, BATCH_TIMEOUT_MS);

        // Start task to process the stream
        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Err(e) =
                            Self::handle_stream_message(msg, &mut tx, &mut subscribe_tx).await
                        {
                            error!("Error handling message: {e:?}");
                            break;
                        }
                    }
                    Err(error) => {
                        error!("Stream error: {error:?}");
                        break;
                    }
                }
            }
        });

        // Process transactions with batch processing
        let self_clone = self.clone();
        tokio::spawn(async move {
            while let Some(transaction_pretty) = rx.next().await {            
                if let Err(e) = self_clone.process_event_transaction_with_batch(
                    transaction_pretty,
                    &mut batch_processor,
                    bot_wallet,
                    protocols.clone(),
                )
                .await
                {
                    error!("Error processing transaction: {e:?}");
                }
            }
            
            // 处理剩余的事件
            batch_processor.flush();
        });

        tokio::signal::ctrl_c().await?;
        Ok(())
    }

    /// 订阅事件
    #[deprecated(
        since = "0.1.5",
        note = "This method will be removed, please use the new API: subscribe_events_v2"
    )]
    #[allow(clippy::too_many_arguments)]
    pub async fn subscribe_events<F>(
        &self,
        protocols: Vec<Protocol>,
        bot_wallet: Option<Pubkey>,
        account_include: Option<Vec<String>>,
        account_exclude: Option<Vec<String>>,
        account_required: Option<Vec<String>>,
        commitment: Option<CommitmentLevel>,
        callback: F,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync + 'static,
    {
        // 创建过滤器
        let protocol_accounts = protocols
            .iter()
            .flat_map(|p| p.get_program_id())
            .map(|p| p.to_string())
            .collect::<Vec<String>>();
        let mut account_include = account_include.unwrap_or_default();
        let account_exclude = account_exclude.unwrap_or_default();
        let account_required = account_required.unwrap_or_default();

        account_include.extend(protocol_accounts.clone());

        let transactions =
            self.get_subscribe_request_filter(account_include, account_exclude, account_required);

        // 订阅事件
        let (mut subscribe_tx, mut stream) = self
            .subscribe_with_request(transactions, commitment)
            .await?;

        // 创建通道
        let (mut tx, mut rx) = mpsc::channel::<TransactionPretty>(CHANNEL_SIZE);

        // 创建回调函数，使用 Arc 包装以便在多个任务中共享
        let callback = std::sync::Arc::new(Box::new(callback));

        // 启动处理流的任务
        tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Err(e) =
                            Self::handle_stream_message(msg, &mut tx, &mut subscribe_tx).await
                        {
                            error!("Error handling message: {e:?}");
                            break;
                        }
                    }
                    Err(error) => {
                        error!("Stream error: {error:?}");
                        break;
                    }
                }
            }
        });

        // 处理交易
        tokio::spawn(async move {
            while let Some(transaction_pretty) = rx.next().await {
                if let Err(e) = Self::process_event_transaction(
                    transaction_pretty,
                    &**callback,
                    bot_wallet,
                    protocols.clone(),
                )
                .await
                {
                    error!("Error processing transaction: {e:?}");
                }
            }
        });

        tokio::signal::ctrl_c().await?;
        Ok(())
    }

    async fn process_event_transaction<F>(
        transaction_pretty: TransactionPretty,
        callback: &F,
        bot_wallet: Option<Pubkey>,
        protocols: Vec<Protocol>,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync,
    {
        let start_time = std::time::Instant::now();
        let program_received_time_ms = chrono::Utc::now().timestamp_millis();
        let slot = transaction_pretty.slot;
        let signature = transaction_pretty.signature.to_string();
        
        // 预分配向量容量，避免动态扩容
        let mut futures = Vec::with_capacity(protocols.len());
        
        for protocol in protocols {
            let parser = EventParserFactory::create_parser(protocol);
            // 在异步任务中需要克隆值
            let tx_clone = transaction_pretty.tx.clone();
            let signature_clone = signature.clone();
            let bot_wallet_clone = bot_wallet;

            futures.push(tokio::spawn(async move {
                parser
                    .parse_transaction(
                        tx_clone,
                        &signature_clone,
                        Some(slot),
                        transaction_pretty.block_time,
                        program_received_time_ms,
                        bot_wallet_clone,
                    )
                    .await
                    .unwrap_or_else(|_e| vec![])
            }));
        }

        let results = futures::future::join_all(futures).await;
        
        // 收集所有事件
        let mut all_events = Vec::new();
        for events in results.into_iter().flatten() {
            all_events.extend(events);
        }
        
        // 保存事件数量用于日志记录
        let event_count = all_events.len();
        
        // 批量处理事件
        if !all_events.is_empty() {
            for event in all_events {
                callback(event);
            }
        }
        
        // 更新性能指标
        let processing_time = start_time.elapsed();
        let processing_time_ms = processing_time.as_millis() as f64;
        
        // 记录慢处理操作
        if processing_time_ms > 10.0 {
            log::warn!("Slow event processing: {processing_time_ms}ms for {event_count} events");
        }
        
        Ok(())
    }

    async fn process_event_transaction_with_batch<F>(
        &self,
        transaction_pretty: TransactionPretty,
        batch_processor: &mut EventBatchCollector<F>,
        bot_wallet: Option<Pubkey>,
        protocols: Vec<Protocol>,
    ) -> AnyResult<()>
    where
        F: Fn(Vec<Box<dyn UnifiedEvent>>) + Send + Sync + 'static,
    {
        let start_time = std::time::Instant::now();
        let program_received_time_ms = chrono::Utc::now().timestamp_millis();
        let slot = transaction_pretty.slot;
        let signature = transaction_pretty.signature.to_string();
        
        // 预分配向量容量，避免动态扩容
        let mut futures: Vec<tokio::task::JoinHandle<Result<Vec<Box<dyn UnifiedEvent>>, anyhow::Error>>> = Vec::with_capacity(protocols.len());
        
        for protocol in protocols {
            let parser = EventParserFactory::create_parser(protocol.clone());
            // 在异步任务中需要克隆值
            let tx_clone = transaction_pretty.tx.clone();
            let signature_clone = signature.clone();
            let bot_wallet_clone = bot_wallet;
            let protocol_clone = protocol.clone();

            futures.push(tokio::spawn(async move {
                let result = parser
                    .parse_transaction(
                        tx_clone,
                        &signature_clone,
                        Some(slot),
                        transaction_pretty.block_time,
                        program_received_time_ms,
                        bot_wallet_clone,
                    )
                    .await;
                
                match result {
                    Ok(events) => {
                        if !events.is_empty() {
                            log::info!("Parsed {} events for protocol {:?}", events.len(), protocol_clone);
                        }
                        Ok(events)
                    }
                    Err(e) => {
                        log::warn!("Failed to parse transaction for protocol {:?}: {:?}", protocol_clone, e);
                        Ok(vec![])
                    }
                }
            }));
        }

        let results = futures::future::join_all(futures).await;
        
        // 收集所有事件并使用批处理器
        let mut total_events = 0;
        for result in results {
            match result {
                Ok(parse_result) => {
                    match parse_result {
                        Ok(events) => {
                            total_events += events.len();
                            log::info!("Adding {} events to batch processor", events.len());
                            for event in events {
                                batch_processor.add_event(event);
                            }
                        }
                        Err(e) => {
                            log::warn!("Failed to parse transaction: {:?}", e);
                        }
                    }
                }
                Err(e) => {
                    log::warn!("Failed to get events from async task: {:?}", e);
                }
            }
        }
        
        // 添加调试信息
        if total_events > 0 {
            log::info!("Total events parsed: {} for transaction {}", total_events, signature);
        }
        
        // 更新性能指标
        let processing_time = start_time.elapsed();
        let processing_time_ms = processing_time.as_millis() as f64;
        
        // 实际调用性能指标更新
        self.update_metrics(total_events as u64, processing_time_ms).await;
        
        // 记录慢处理操作
        if processing_time_ms > 10.0 {
            log::warn!("Slow event processing: {processing_time_ms}ms for {total_events} events");
        }
        
        Ok(())
    }
}

