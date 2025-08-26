// gRPC 相关模块
pub mod connection;
pub mod stream_handler;
pub mod subscription;
pub mod types;

// 重新导出主要类型
pub use connection::*;
pub use stream_handler::*;
pub use subscription::*;
pub use types::*;

// 从公用模块重新导出
pub use crate::streaming::common::{
    BackpressureConfig, BackpressureStrategy, BatchConfig, ConnectionConfig, MetricsManager,
    PerformanceMetrics, StreamClientConfig as ClientConfig,
};
