use crate::common::AnyResult;
use crate::streaming::common::{
    EventProcessor, MetricsManager, PerformanceMetrics, StreamClientConfig, SubscriptionHandle,
};
use crate::streaming::event_parser::common::filter::EventTypeFilter;
use crate::streaming::event_parser::{Protocol, UnifiedEvent};
use crate::streaming::grpc::{
    AccountPretty, BlockMetaPretty, EventPretty, SubscriptionManager, TransactionPretty,
};
use anyhow::anyhow;
use chrono::Local;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use log::error;
use solana_sdk::pubkey::Pubkey;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use yellowstone_grpc_proto::geyser::subscribe_update::UpdateOneof;
use yellowstone_grpc_proto::geyser::{CommitmentLevel, SubscribeRequest, SubscribeRequestPing};

/// 交易过滤器
#[derive(Debug, Clone)]
pub struct TransactionFilter {
    pub account_include: Vec<String>,
    pub account_exclude: Vec<String>,
    pub account_required: Vec<String>,
}

/// 账户过滤器
#[derive(Debug, Clone)]
pub struct AccountFilter {
    pub account: Vec<String>,
    pub owner: Vec<String>,
}

pub struct YellowstoneGrpc {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub config: StreamClientConfig,
    pub metrics: Arc<RwLock<PerformanceMetrics>>,
    pub subscription_manager: SubscriptionManager,
    pub metrics_manager: MetricsManager,
    pub event_processor: EventProcessor,
    pub subscription_handle: Arc<Mutex<Option<SubscriptionHandle>>>,
    // Dynamic subscription management fields
    pub active_subscription: Arc<AtomicBool>,
    pub control_tx: Arc<tokio::sync::Mutex<Option<mpsc::Sender<SubscribeRequest>>>>,
    pub current_request: Arc<tokio::sync::RwLock<Option<SubscribeRequest>>>,
}

impl YellowstoneGrpc {
    /// 创建客户端，使用默认配置
    pub fn new(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, StreamClientConfig::default())
    }

    /// 创建客户端，使用自定义配置
    pub fn new_with_config(
        endpoint: String,
        x_token: Option<String>,
        config: StreamClientConfig,
    ) -> AnyResult<Self> {
        //初始化和配置安全连接（TLS）的加密库
        //install_default(): 这行代码的作用就是告诉整个程序：“接下来所有需要 TLS 加密的安全连接，都请使用 ring 这个库来完成。” 它将 ring 设置为全局默认的加密服务提供者。
        let _ = rustls::crypto::ring::default_provider().install_default().ok();

        //性能指标
        let metrics: Arc<RwLock<PerformanceMetrics>> =
            Arc::new(RwLock::new(PerformanceMetrics::new()));

        let subscription_manager =
            SubscriptionManager::new(endpoint.clone(), x_token.clone(), config.clone());
        let metrics_manager = MetricsManager::new_with_metrics(
            metrics.clone(),
            config.enable_metrics,
            "YellowstoneGrpc".to_string(),
        );
        let event_processor = EventProcessor::new(metrics_manager.clone(), config.clone());

        Ok(Self {
            endpoint,
            x_token,
            config,
            metrics: metrics.clone(),
            subscription_manager,
            metrics_manager,
            event_processor,
            subscription_handle: Arc::new(Mutex::new(None)),
            active_subscription: Arc::new(AtomicBool::new(false)),
            control_tx: Arc::new(tokio::sync::Mutex::new(None)),
            current_request: Arc::new(tokio::sync::RwLock::new(None)),
        })
    }

    /// 创建具有高吞吐量配置的新 YellowstoneGrpcClient。
    ///
    /// 这是一个便捷方法，用于创建为高并发场景优化的客户端，
    /// 在这种场景中，吞吐量优先于延迟。有关详细的配置信息，
    /// 请参阅 `StreamClientConfig::high_throughput()`。
    pub fn new_high_throughput(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, StreamClientConfig::high_throughput())
    }

    /// 创建具有低延迟配置的新 YellowstoneGrpcClient。
    ///
    /// 这是一个便捷方法，用于创建为实时场景优化的客户端，
    /// 在这种场景中，延迟优先于吞吐量。有关详细的配置信息，
    /// 请参阅 `StreamClientConfig::low_latency()`。
    pub fn new_low_latency(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, StreamClientConfig::low_latency())
    }

    /// 获取配置
    pub fn get_config(&self) -> &StreamClientConfig {
        &self.config
    }

    /// 更新配置
    pub fn update_config(&mut self, config: StreamClientConfig) {
        self.config = config;
    }

    /// 获取性能指标
    pub fn get_metrics(&self) -> PerformanceMetrics {
        self.metrics_manager.get_metrics()
    }

    /// 打印性能指标
    pub fn print_metrics(&self) {
        self.metrics_manager.print_metrics();
    }

    /// 启用或禁用性能监控
    pub fn set_enable_metrics(&mut self, enabled: bool) {
        self.config.enable_metrics = enabled;
    }

    /// 停止当前订阅
    pub async fn stop(&self) {
        let mut handle_guard = self.subscription_handle.lock().await;
        if let Some(handle) = handle_guard.take() {
            handle.stop();
        }
        *self.control_tx.lock().await = None;
        *self.current_request.write().await = None;
        self.active_subscription.store(false, Ordering::Release);
    }

    /// 简化的即时事件订阅（推荐用于简单场景）
    ///
    /// # 参数
    /// * `protocols` - 要监控的协议列表
    /// * `bot_wallet` - 可选的机器人钱包地址，用于过滤相关交易
    /// * `transaction_filter` - 交易过滤器，指定要包含/排除的账户
    /// * `account_filter` - 账户过滤器，指定要监控的账户和所有者
    /// * `event_filter` - 可选的事件过滤器，用于进一步的事件过滤，如果为 None 则不进行过滤
    /// * `commitment` - 可选的提交级别，默认为 Confirmed
    /// * `callback` - 事件回调函数，接收已解析的统一事件
    ///
    /// # 返回
    /// 成功时返回 `AnyResult<()>`，即 `Ok(())`，失败时返回错误信息
    pub async fn subscribe_events_immediate<F>(
        &self,
        protocols: Vec<Protocol>,
        bot_wallet: Option<Pubkey>,
        transaction_filter: TransactionFilter,
        account_filter: AccountFilter,
        event_type_filter: Option<EventTypeFilter>,
        commitment: Option<CommitmentLevel>,
        callback: F,
    ) -> AnyResult<()>
    where
        F: Fn(Box<dyn UnifiedEvent>) + Send + Sync + 'static,
    {
        //current: false: 这是我们期望 active_subscription 当前的值。我们希望只有在当前没有订阅（即值为 false）时，这个操作才能成功。
        //new: true: 如果当前值确实是我们期望的 false，那么就把它原子地更新为 true。
        //success: Ordering::Acquire: 这是内存排序（Memory Ordering）参数。Acquire 保证了在这个操作之后的所有内存读写操作，都不能被重排到这个操作之前。简单来说，它建立了一个内存屏障，确保当我们成功将标志位设为 true 后，后续启动订阅流的代码能够看到一个完全一致的内存状态。
        //failure: Ordering::Relaxed: 如果操作失败（即当前值不是 false），Relaxed 意味着没有特殊的内存排序要求，这是最快的排序方式，因为我们只关心操作失败的结果，不关心失败时的内存状态。
        if self
            .active_subscription
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            return Err(anyhow!("Already subscribed. Use update_subscription() to modify filters"));
        }

        let mut metrics_handle = None;
        // 启动自动性能监控（如果启用）
        if self.config.enable_metrics {
            metrics_handle = self.metrics_manager.start_auto_monitoring().await;
        }

        let transactions = self.subscription_manager.get_subscribe_request_filter(
            transaction_filter.account_include,
            transaction_filter.account_exclude,
            transaction_filter.account_required,
            event_type_filter.as_ref(),
        );
        let accounts = self.subscription_manager.subscribe_with_account_request(
            account_filter.account,
            account_filter.owner,
            event_type_filter.as_ref(),
        );

        // 订阅事件
        let (mut subscribe_tx, mut stream, subscribe_request) = self
            .subscription_manager
            .subscribe_with_request(transactions, accounts, commitment, event_type_filter.as_ref())
            .await?;

        // 用 Arc<Mutex<>> 包装 subscribe_tx 以支持多线程共享
        let subscribe_tx = Arc::new(Mutex::new(subscribe_tx));
        *self.current_request.write().await = Some(subscribe_request);
        let (control_tx, mut control_rx) = mpsc::channel(100);
        *self.control_tx.lock().await = Some(control_tx);

        // 启动流处理任务
        let mut event_processor = self.event_processor.clone();
        event_processor.set_protocols_and_event_type_filter(
            protocols,
            event_type_filter,
            self.config.backpressure.clone(),
            Some(Arc::new(callback)),
        );
        let stream_handle = tokio::spawn(async move {
            loop {
                tokio::select! {
                    message = stream.next() => {
                        match message {
                            Some(Ok(msg)) => {
                                let created_at = msg.created_at;
                                match msg.update_oneof {
                                    Some(UpdateOneof::Account(account)) => {
                                        let account_pretty = AccountPretty::from(account);
                                        log::debug!("Received account: {:?}", account_pretty);
                                        if let Err(e) = event_processor
                                            .process_grpc_event_transaction_with_metrics(
                                                EventPretty::Account(account_pretty),
                                                bot_wallet,
                                            )
                                            .await
                                        {
                                            error!("Error processing account event: {e:?}");
                                        }
                                    }
                                    Some(UpdateOneof::BlockMeta(sut)) => {
                                        let block_meta_pretty =
                                            BlockMetaPretty::from((sut, created_at));
                                        log::debug!("Received block meta: {:?}", block_meta_pretty);
                                        if let Err(e) = event_processor
                                            .process_grpc_event_transaction_with_metrics(
                                                EventPretty::BlockMeta(block_meta_pretty),
                                                bot_wallet,
                                            )
                                            .await
                                        {
                                            error!("Error processing block meta event: {e:?}");
                                        }
                                    }
                                    Some(UpdateOneof::Transaction(sut)) => {
                                        let transaction_pretty =
                                            TransactionPretty::from((sut, created_at));
                                        log::debug!(
                                            "Received transaction: {} at slot {}",
                                            transaction_pretty.signature,
                                            transaction_pretty.slot
                                        );
                                        if let Err(e) = event_processor
                                            .process_grpc_event_transaction_with_metrics(
                                                EventPretty::Transaction(transaction_pretty),
                                                bot_wallet,
                                            )
                                            .await
                                        {
                                            error!("Error processing transaction event: {e:?}");
                                        }
                                    }
                                    Some(UpdateOneof::Ping(_)) => {
                                        // 只在需要时获取锁，并立即释放
                                        if let Ok(mut tx_guard) = subscribe_tx.try_lock() {
                                            let _ = tx_guard
                                                .send(SubscribeRequest {
                                                    ping: Some(SubscribeRequestPing { id: 1 }),
                                                    ..Default::default()
                                                })
                                                .await;
                                        }
                                        log::debug!("service is ping: {}", Local::now());
                                    }
                                    Some(UpdateOneof::Pong(_)) => {
                                        log::debug!("service is pong: {}", Local::now());
                                    }
                                    _ => {
                                        log::debug!("Received other message type");
                                    }
                                }
                            }
                            Some(Err(error)) => {
                                error!("Stream error: {error:?}");
                                break;
                            }
                            None => break,
                        }
                    }
                    Some(update) = control_rx.next() => {
                        if let Err(e) = subscribe_tx.lock().await.send(update).await {
                            error!("Failed to send subscription update: {}", e);
                            break;
                        }
                    }
                }
            }
        });

        // 保存订阅句柄
        let subscription_handle = SubscriptionHandle::new(stream_handle, None, metrics_handle);
        let mut handle_guard = self.subscription_handle.lock().await;
        *handle_guard = Some(subscription_handle);

        Ok(())
    }

    /// 在运行时更新订阅过滤器，无需重新连接
    ///
    /// # 参数
    /// * `transaction_filter` - 要应用的新交易过滤器
    /// * `account_filter` - 要应用的新账户过滤器
    ///
    /// # 返回
    /// 成功时返回 `AnyResult<()>`，失败时返回错误
    pub async fn update_subscription(
        &self,
        transaction_filter: TransactionFilter,
        account_filter: AccountFilter,
    ) -> AnyResult<()> {
        let mut control_sender = {
            let control_guard = self.control_tx.lock().await;

            if !self.active_subscription.load(Ordering::Acquire) {
                return Err(anyhow!("No active subscription to update"));
            }

            control_guard
                .as_ref()
                .ok_or_else(|| anyhow!("No active subscription to update"))?
                .clone()
        };

        let mut request = self
            .current_request
            .read()
            .await
            .as_ref()
            .ok_or_else(|| anyhow!("No active subscription"))?
            .clone();

        request.transactions = self
            .subscription_manager
            .get_subscribe_request_filter(
                transaction_filter.account_include,
                transaction_filter.account_exclude,
                transaction_filter.account_required,
                None,
            )
            .unwrap_or_default();

        request.accounts = self
            .subscription_manager
            .subscribe_with_account_request(account_filter.account, account_filter.owner, None)
            .unwrap_or_default();

        control_sender
            .send(request.clone())
            .await
            .map_err(|e| anyhow!("Failed to send update: {}", e))?;

        *self.current_request.write().await = Some(request);

        Ok(())
    }
}

// 实现 Clone trait 以支持模块间共享
impl Clone for YellowstoneGrpc {
    fn clone(&self) -> Self {
        Self {
            endpoint: self.endpoint.clone(),
            x_token: self.x_token.clone(),
            config: self.config.clone(),
            metrics: self.metrics.clone(),
            subscription_manager: self.subscription_manager.clone(),
            metrics_manager: self.metrics_manager.clone(),
            event_processor: self.event_processor.clone(),
            subscription_handle: self.subscription_handle.clone(), // 共享同一个 Arc<Mutex<>>
            active_subscription: self.active_subscription.clone(),
            control_tx: self.control_tx.clone(),
            current_request: self.current_request.clone(),
        }
    }
}
