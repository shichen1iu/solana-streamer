use futures::StreamExt;
use log::error;
use solana_sdk::pubkey::Pubkey;
use std::sync::{Arc, RwLock};
use tokio::sync::Mutex;
use yellowstone_grpc_proto::geyser::CommitmentLevel;

use crate::common::AnyResult;
use crate::streaming::common::{
    EventProcessor, MetricsManager, PerformanceMetrics, StreamClientConfig, SubscriptionHandle,
};
use crate::streaming::event_parser::common::filter::EventTypeFilter;
use crate::streaming::event_parser::{Protocol, UnifiedEvent};
use crate::streaming::grpc::{StreamHandler, SubscriptionManager};

/// 交易过滤器
pub struct TransactionFilter {
    pub account_include: Vec<String>,
    pub account_exclude: Vec<String>,
    pub account_required: Vec<String>,
}

/// 账户过滤器
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
        let _ = rustls::crypto::ring::default_provider().install_default().ok();
        let metrics = Arc::new(RwLock::new(PerformanceMetrics::new()));
        let config_arc = Arc::new(config.clone());

        let subscription_manager =
            SubscriptionManager::new(endpoint.clone(), x_token.clone(), config.clone());
        let metrics_manager =
            MetricsManager::new(metrics.clone(), config_arc.clone(), "YellowstoneGrpc".to_string());
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
        })
    }

    /// 创建高性能客户端
    pub fn new_high_performance(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, StreamClientConfig::high_performance())
    }

    /// 创建低延迟客户端
    pub fn new_low_latency(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        Self::new_with_config(endpoint, x_token, StreamClientConfig::low_latency())
    }

    /// 创建即时处理客户端
    pub fn new_immediate(endpoint: String, x_token: Option<String>) -> AnyResult<Self> {
        let mut config = StreamClientConfig::low_latency();
        config.enable_metrics = false;
        Self::new_with_config(endpoint, x_token, config)
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
    }

    /// Simplified immediate event subscription (recommended for simple scenarios)
    ///
    /// # Parameters
    /// * `protocols` - List of protocols to monitor
    /// * `bot_wallet` - Optional bot wallet address for filtering related transactions
    /// * `transaction_filter` - Transaction filter specifying accounts to include/exclude
    /// * `account_filter` - Account filter specifying accounts and owners to monitor
    /// * `event_filter` - Optional event filter for further event filtering, no filtering if None
    /// * `commitment` - Optional commitment level, defaults to Confirmed
    /// * `callback` - Event callback function that receives parsed unified events
    ///
    /// # Returns
    /// Returns `AnyResult<()>`, `Ok(())` on success, error information on failure
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
        // 如果已有活跃订阅，先停止它
        self.stop().await;

        let mut metrics_handle = None;
        // 启动自动性能监控（如果启用）
        if self.config.enable_metrics {
            metrics_handle = self.metrics_manager.start_auto_monitoring().await;
        }

        let transactions = self.subscription_manager.get_subscribe_request_filter(
            transaction_filter.account_include,
            transaction_filter.account_exclude,
            transaction_filter.account_required,
            event_type_filter.clone(),
        );
        let accounts = self.subscription_manager.subscribe_with_account_request(
            account_filter.account,
            account_filter.owner,
            event_type_filter.clone(),
        );

        // 订阅事件
        let (mut subscribe_tx, mut stream) = self
            .subscription_manager
            .subscribe_with_request(transactions, accounts, commitment, event_type_filter.clone())
            .await?;

        // 启动流处理任务
        let mut event_processor = self.event_processor.clone();
        event_processor.set_protocols_and_event_type_filter(
            protocols,
            event_type_filter,
            self.config.backpressure.strategy,
            self.config.batch.clone(),
        );
        let stream_handle = tokio::spawn(async move {
            while let Some(message) = stream.next().await {
                match message {
                    Ok(msg) => {
                        if let Err(e) = StreamHandler::handle_stream_message(
                            msg,
                            &mut subscribe_tx,
                            event_processor.clone(),
                            &callback,
                            bot_wallet,
                        )
                        .await
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

        // 保存订阅句柄
        let subscription_handle = SubscriptionHandle::new(
            stream_handle,
            self.event_processor.get_event_handle(),
            metrics_handle,
        );
        let mut handle_guard = self.subscription_handle.lock().await;
        *handle_guard = Some(subscription_handle);

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
        }
    }
}
