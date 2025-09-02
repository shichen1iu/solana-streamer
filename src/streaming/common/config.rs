use super::constants::*;

/// 背压处理策略
#[derive(Debug, Clone, Copy)]
pub enum BackpressureStrategy {
    /// 阻塞等待（默认）
    Block,
    /// 丢弃消息
    Drop,
}

impl Default for BackpressureStrategy {
    fn default() -> Self {
        Self::Block
    }
}

/// 背压配置
#[derive(Debug, Clone)]
pub struct BackpressureConfig {
    /// 通道大小（默认：1000）
    pub permits: usize,
    /// 背压处理策略（默认：阻塞）
    pub strategy: BackpressureStrategy,
}

impl Default for BackpressureConfig {
    fn default() -> Self {
        Self { permits: 1, strategy: BackpressureStrategy::default() }
    }
}

/// 连接配置
#[derive(Debug, Clone)]
pub struct ConnectionConfig {
    /// 连接超时时间（秒）（默认：10）
    pub connect_timeout: u64,
    /// 请求超时时间（秒）（默认：60）
    pub request_timeout: u64,
    /// 最大解码消息大小（字节）（默认：10MB）
    pub max_decoding_message_size: usize,
}

impl Default for ConnectionConfig {
    fn default() -> Self {
        Self {
            connect_timeout: DEFAULT_CONNECT_TIMEOUT,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            max_decoding_message_size: DEFAULT_MAX_DECODING_MESSAGE_SIZE,
        }
    }
}

/// 通用客户端配置
#[derive(Debug, Clone)]
pub struct StreamClientConfig {
    /// 连接配置
    pub connection: ConnectionConfig,
    /// 背压配置
    pub backpressure: BackpressureConfig,
    /// 是否启用性能监控（默认：false）
    pub enable_metrics: bool,
}

impl Default for StreamClientConfig {
    fn default() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            backpressure: BackpressureConfig::default(),
            enable_metrics: false,
        }
    }
}

impl StreamClientConfig {
    /// 创建为高并发场景优化的高吞吐量配置。
    ///
    /// 此配置通过以下方式优先考虑吞吐量而不是延迟：
    /// - 实现丢弃策略以避免背压阻塞
    /// - 设置较大的许可缓冲区（20,000）以处理突发流量
    ///
    /// 适用于需要处理大量数据
    /// 并且可以容忍峰值负载期间偶尔丢弃消息的场景。
    pub fn high_throughput() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            backpressure: BackpressureConfig {
                permits: 20000,
                strategy: BackpressureStrategy::Drop,
            },
            enable_metrics: false,
        }
    }

    /// 创建为实时场景优化的低延迟配置。
    ///
    /// 此配置通过以下方式优先考虑延迟而不是吞吐量：
    /// - 立即处理事件，无需缓冲
    /// - 实现阻塞式背压策略以确保不丢失数据
    /// - 设置最佳许可（4000）以平衡吞吐量和延迟
    ///
    /// 适用于每毫秒都很重要且不能
    /// 承受任何事件丢失的场景，例如交易应用程序或实时监控。
    pub fn low_latency() -> Self {
        Self {
            connection: ConnectionConfig::default(),
            backpressure: BackpressureConfig { permits: 4000, strategy: BackpressureStrategy::Block },
            enable_metrics: false,
        }
    }
}
