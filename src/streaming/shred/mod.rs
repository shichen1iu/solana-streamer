// ShredStream 相关模块
pub mod connection;
pub mod types;

// 重新导出主要类型
pub use connection::*;
pub use types::*;

// 从公用模块重新导出
pub use crate::streaming::common::{
    BackpressureConfig, BackpressureStrategy, BatchConfig, ConnectionConfig, MetricsEventType,
    MetricsManager, PerformanceMetrics, StreamClientConfig,
};
