use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use solana_sdk::signature::Signature;

use super::constants::*;

/// 事件类型枚举
#[derive(Debug, Clone, Copy)]
pub enum EventType {
    Transaction = 0,
    Account = 1,
    BlockMeta = 2,
}

/// 兼容性别名
pub type MetricsEventType = EventType;

impl EventType {
    #[inline]
    const fn as_index(self) -> usize {
        self as usize
    }

    const fn name(self) -> &'static str {
        match self {
            EventType::Transaction => "TX",
            EventType::Account => "Account",
            EventType::BlockMeta => "Block Meta",
        }
    }

    // 兼容性常量
    pub const TX: EventType = EventType::Transaction;
}

/// 高性能原子事件指标
#[derive(Debug)]
struct AtomicEventMetrics {
    process_count: AtomicU64,
    events_processed: AtomicU64,
    events_in_window: AtomicU64,
    window_start_nanos: AtomicU64,
    events_per_second_bits: AtomicU64, // f64 的位表示
}

impl AtomicEventMetrics {
    fn new(now_nanos: u64) -> Self {
        Self {
            process_count: AtomicU64::new(0),
            events_processed: AtomicU64::new(0),
            events_in_window: AtomicU64::new(0),
            window_start_nanos: AtomicU64::new(now_nanos),
            events_per_second_bits: AtomicU64::new(0),
        }
    }

    /// 原子地增加处理计数
    #[inline]
    fn add_process_count(&self) {
        self.process_count.fetch_add(1, Ordering::Relaxed);
    }

    /// 原子地增加事件处理数量
    #[inline]
    fn add_events_processed(&self, count: u64) {
        self.events_processed.fetch_add(count, Ordering::Relaxed);
        self.events_in_window.fetch_add(count, Ordering::Relaxed);
    }

    /// 获取当前计数（非阻塞）
    #[inline]
    fn get_counts(&self) -> (u64, u64, u64) {
        (
            self.process_count.load(Ordering::Relaxed),
            self.events_processed.load(Ordering::Relaxed),
            self.events_in_window.load(Ordering::Relaxed),
        )
    }

    /// 原子地更新每秒事件数
    #[inline]
    fn update_events_per_second(&self, eps: f64) {
        self.events_per_second_bits.store(eps.to_bits(), Ordering::Relaxed);
    }

    /// 获取每秒事件数
    #[inline]
    fn get_events_per_second(&self) -> f64 {
        f64::from_bits(self.events_per_second_bits.load(Ordering::Relaxed))
    }

    /// 重置窗口计数
    #[inline]
    fn reset_window(&self, new_start_nanos: u64) {
        self.events_in_window.store(0, Ordering::Relaxed);
        self.window_start_nanos.store(new_start_nanos, Ordering::Relaxed);
    }

    #[inline]
    fn get_window_start(&self) -> u64 {
        self.window_start_nanos.load(Ordering::Relaxed)
    }
}

/// 高性能原子处理时间统计
#[derive(Debug)]
struct AtomicProcessingTimeStats {
    min_time_bits: AtomicU64,
    max_time_bits: AtomicU64,
    max_time_timestamp_nanos: AtomicU64, // 最大值更新时间戳（纳秒）
    total_time_us: AtomicU64,            // 存储微秒的整数部分
    total_events: AtomicU64,
}

impl AtomicProcessingTimeStats {
    fn new() -> Self {
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;

        Self {
            min_time_bits: AtomicU64::new(f64::INFINITY.to_bits()),
            max_time_bits: AtomicU64::new(0),
            max_time_timestamp_nanos: AtomicU64::new(now_nanos),
            total_time_us: AtomicU64::new(0),
            total_events: AtomicU64::new(0),
        }
    }

    /// 原子地更新处理时间统计
    #[inline]
    fn update(&self, time_us: f64, event_count: u64) {
        let time_bits = time_us.to_bits();
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;

        // 更新最小值（使用 compare_exchange_weak 循环）
        let mut current_min = self.min_time_bits.load(Ordering::Relaxed);
        while time_bits < current_min {
            match self.min_time_bits.compare_exchange_weak(
                current_min,
                time_bits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // 更新最大值，检查时间差并在超过10秒时清零
        let mut current_max = self.max_time_bits.load(Ordering::Relaxed);
        let max_timestamp = self.max_time_timestamp_nanos.load(Ordering::Relaxed);

        // 检查最大值的时间戳是否超过10秒（10_000_000_000纳秒）
        let time_diff_nanos = now_nanos.saturating_sub(max_timestamp);
        if time_diff_nanos > 10_000_000_000 {
            // 超过10秒，清零最大值
            self.max_time_bits.store(0, Ordering::Relaxed);
            self.max_time_timestamp_nanos.store(now_nanos, Ordering::Relaxed);
            current_max = 0;
        }

        // 如果当前时间大于最大值，更新最大值和时间戳
        while time_bits > current_max {
            match self.max_time_bits.compare_exchange_weak(
                current_max,
                time_bits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    // 成功更新最大值，同时更新时间戳
                    self.max_time_timestamp_nanos.store(now_nanos, Ordering::Relaxed);
                    break;
                }
                Err(x) => current_max = x,
            }
        }

        // 更新累计值（将微秒转换为整数避免浮点累加问题）
        let total_time_us_int = (time_us * event_count as f64) as u64;
        self.total_time_us.fetch_add(total_time_us_int, Ordering::Relaxed);
        self.total_events.fetch_add(event_count, Ordering::Relaxed);
    }

    /// 获取统计值（非阻塞）
    #[inline]
    fn get_stats(&self) -> ProcessingTimeStats {
        let min_bits = self.min_time_bits.load(Ordering::Relaxed);
        let max_bits = self.max_time_bits.load(Ordering::Relaxed);
        let total_time_us_int = self.total_time_us.load(Ordering::Relaxed);
        let total_events = self.total_events.load(Ordering::Relaxed);

        let min_time = f64::from_bits(min_bits);
        let max_time = f64::from_bits(max_bits);
        let avg_time =
            if total_events > 0 { total_time_us_int as f64 / total_events as f64 } else { 0.0 };

        ProcessingTimeStats {
            min_us: if min_time == f64::INFINITY { 0.0 } else { min_time },
            max_us: max_time,
            avg_us: avg_time,
        }
    }
}

/// 处理时间统计结果
#[derive(Debug, Clone)]
pub struct ProcessingTimeStats {
    pub min_us: f64,
    pub max_us: f64,
    pub avg_us: f64,
}

/// 事件指标快照
#[derive(Debug, Clone)]
pub struct EventMetricsSnapshot {
    pub process_count: u64,
    pub events_processed: u64,
    pub events_per_second: f64,
}

/// 背压指标快照
#[derive(Debug, Clone)]
pub struct BackpressureMetricsSnapshot {
    pub total_duration_us: u64,
    pub success_count: u64,
    pub failure_count: u64,
    pub min_permits: u64,
    pub max_permits: u64,
    pub avg_duration_us: f64,
    pub success_rate: f64,
}

/// 兼容性结构 - 完整的性能指标
#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub uptime: std::time::Duration,
    pub tx_metrics: EventMetricsSnapshot,
    pub account_metrics: EventMetricsSnapshot,
    pub block_meta_metrics: EventMetricsSnapshot,
    pub processing_stats: ProcessingTimeStats,
    pub backpressure_metrics: BackpressureMetricsSnapshot,
    pub dropped_events_count: u64,
}

impl PerformanceMetrics {
    /// 创建默认的性能指标（兼容性方法）
    pub fn new() -> Self {
        let default_metrics =
            EventMetricsSnapshot { process_count: 0, events_processed: 0, events_per_second: 0.0 };
        let default_stats = ProcessingTimeStats { min_us: 0.0, max_us: 0.0, avg_us: 0.0 };
        let default_backpressure = BackpressureMetricsSnapshot {
            total_duration_us: 0,
            success_count: 0,
            failure_count: 0,
            min_permits: 0,
            max_permits: 0,
            avg_duration_us: 0.0,
            success_rate: 0.0,
        };

        Self {
            uptime: std::time::Duration::ZERO,
            tx_metrics: default_metrics.clone(),
            account_metrics: default_metrics.clone(),
            block_meta_metrics: default_metrics,
            processing_stats: default_stats,
            backpressure_metrics: default_backpressure,
            dropped_events_count: 0,
        }
    }
}

/// 高性能指标系统
#[derive(Debug)]
pub struct HighPerformanceMetrics {
    start_nanos: u64,
    event_metrics: [AtomicEventMetrics; 3],
    processing_stats: AtomicProcessingTimeStats,
    // 背压相关指标
    backpressure_total_duration_us: AtomicU64,
    backpressure_success_count: AtomicU64,
    backpressure_failure_count: AtomicU64,
    backpressure_min_permits: AtomicU64,
    backpressure_max_permits: AtomicU64,
    // 丢弃事件指标
    dropped_events_count: AtomicU64,
}

impl HighPerformanceMetrics {
    fn new() -> Self {
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;

        Self {
            start_nanos: now_nanos,
            event_metrics: [
                AtomicEventMetrics::new(now_nanos),
                AtomicEventMetrics::new(now_nanos),
                AtomicEventMetrics::new(now_nanos),
            ],
            processing_stats: AtomicProcessingTimeStats::new(),
            // 初始化背压相关指标
            backpressure_total_duration_us: AtomicU64::new(0),
            backpressure_success_count: AtomicU64::new(0),
            backpressure_failure_count: AtomicU64::new(0),
            backpressure_min_permits: AtomicU64::new(u64::MAX), // 初始化为最大值，便于后续比较
            backpressure_max_permits: AtomicU64::new(0),
            // 初始化丢弃事件指标
            dropped_events_count: AtomicU64::new(0),
        }
    }

    /// 获取运行时长（秒）
    #[inline]
    pub fn get_uptime_seconds(&self) -> f64 {
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;
        (now_nanos - self.start_nanos) as f64 / 1_000_000_000.0
    }

    /// 获取事件指标快照
    #[inline]
    pub fn get_event_metrics(&self, event_type: EventType) -> EventMetricsSnapshot {
        let index = event_type.as_index();
        let (process_count, events_processed, _) = self.event_metrics[index].get_counts();
        let events_per_second = self.calculate_real_time_eps(event_type);

        EventMetricsSnapshot { process_count, events_processed, events_per_second }
    }

    /// 获取处理时间统计
    #[inline]
    pub fn get_processing_stats(&self) -> ProcessingTimeStats {
        self.processing_stats.get_stats()
    }

    /// 获取背压指标快照
    #[inline]
    pub fn get_backpressure_metrics(&self) -> BackpressureMetricsSnapshot {
        let total_duration_us = self.backpressure_total_duration_us.load(Ordering::Relaxed);
        let success_count = self.backpressure_success_count.load(Ordering::Relaxed);
        let failure_count = self.backpressure_failure_count.load(Ordering::Relaxed);
        let min_permits = self.backpressure_min_permits.load(Ordering::Relaxed);
        let max_permits = self.backpressure_max_permits.load(Ordering::Relaxed);

        let total_count = success_count + failure_count;
        let avg_duration_us =
            if total_count > 0 { total_duration_us as f64 / total_count as f64 } else { 0.0 };
        let success_rate =
            if total_count > 0 { success_count as f64 / total_count as f64 } else { 0.0 };

        BackpressureMetricsSnapshot {
            total_duration_us,
            success_count,
            failure_count,
            min_permits: if min_permits == u64::MAX { 0 } else { min_permits },
            max_permits,
            avg_duration_us,
            success_rate,
        }
    }

    /// 获取丢弃事件计数
    #[inline]
    pub fn get_dropped_events_count(&self) -> u64 {
        self.dropped_events_count.load(Ordering::Relaxed)
    }

    /// 计算实时每秒事件数（非阻塞）
    fn calculate_real_time_eps(&self, event_type: EventType) -> f64 {
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;

        let index = event_type.as_index();
        let event_metric = &self.event_metrics[index];

        let window_start = event_metric.get_window_start();
        let current_window_duration_secs =
            (now_nanos.saturating_sub(window_start)) as f64 / 1_000_000_000.0;
        let events_in_window = event_metric.events_in_window.load(Ordering::Relaxed);

        // 优先级1: 当前窗口实时数据（≥2秒且有事件）
        if current_window_duration_secs >= 2.0 && events_in_window > 0 {
            return events_in_window as f64 / current_window_duration_secs;
        }

        // 优先级2: 上一个窗口的结果
        let stored_eps = event_metric.get_events_per_second();
        if stored_eps > 0.0 {
            return stored_eps;
        }

        // 优先级3: 总体平均值（≥3秒运行时间）
        let total_duration_secs = self.get_uptime_seconds();
        let total_events = event_metric.events_processed.load(Ordering::Relaxed);
        if total_duration_secs >= 3.0 && total_events > 0 {
            return total_events as f64 / total_duration_secs;
        }

        0.0
    }

    /// 更新窗口指标（后台任务调用）
    fn update_window_metrics(&self, event_type: EventType, window_duration_nanos: u64) {
        let now_nanos =
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
                as u64;

        let index = event_type.as_index();
        let event_metric = &self.event_metrics[index];

        let window_start = event_metric.get_window_start();
        if now_nanos.saturating_sub(window_start) >= window_duration_nanos {
            let events_in_window = event_metric.events_in_window.load(Ordering::Relaxed);
            let window_duration_secs = window_duration_nanos as f64 / 1_000_000_000.0;

            if window_duration_secs > 0.001 && events_in_window > 0 {
                let eps = events_in_window as f64 / window_duration_secs;
                event_metric.update_events_per_second(eps);
            }

            event_metric.reset_window(now_nanos);
        }
    }
}

/// 高性能指标管理器
pub struct MetricsManager {
    metrics: Arc<HighPerformanceMetrics>,
    enable_metrics: bool,
    stream_name: String,
    background_task_running: AtomicBool,
}

impl MetricsManager {
    /// 创建新的指标管理器
    pub fn new(enable_metrics: bool, stream_name: String) -> Self {
        let manager = Self {
            metrics: Arc::new(HighPerformanceMetrics::new()),
            enable_metrics,
            stream_name,
            background_task_running: AtomicBool::new(false),
        };

        // 启动后台任务
        manager.start_background_tasks();
        manager
    }

    /// 启动后台任务
    fn start_background_tasks(&self) {
        if self
            .background_task_running
            .compare_exchange(false, true, Ordering::Relaxed, Ordering::Relaxed)
            .is_ok()
        {
            if !self.enable_metrics {
                return;
            }

            let metrics = self.metrics.clone();

            tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_millis(500));

                loop {
                    interval.tick().await;

                    let window_duration_nanos = DEFAULT_METRICS_WINDOW_SECONDS * 1_000_000_000;

                    // 更新所有事件类型的窗口指标
                    metrics.update_window_metrics(EventType::Transaction, window_duration_nanos);
                    metrics.update_window_metrics(EventType::Account, window_duration_nanos);
                    metrics.update_window_metrics(EventType::BlockMeta, window_duration_nanos);
                }
            });
        }
    }

    /// 记录处理次数（非阻塞）
    #[inline]
    pub fn record_process(&self, event_type: EventType) {
        if self.enable_metrics {
            self.metrics.event_metrics[event_type.as_index()].add_process_count();
        }
    }

    /// 记录事件处理（非阻塞）
    #[inline]
    pub fn record_events(&self, event_type: EventType, count: u64, processing_time_us: f64) {
        if !self.enable_metrics {
            return;
        }

        // 原子更新事件计数
        self.metrics.event_metrics[event_type.as_index()].add_events_processed(count);

        // 原子更新处理时间统计
        self.metrics.processing_stats.update(processing_time_us, count);
    }

    /// 记录慢处理操作
    #[inline]
    pub fn log_slow_processing(
        &self,
        processing_time_us: f64,
        event_count: usize,
        signature: Option<Signature>,
    ) {
        if processing_time_us > SLOW_PROCESSING_THRESHOLD_US {
            log::warn!(
                "{} slow processing: {:.2}us for {} events, signature: {:?}",
                self.stream_name,
                processing_time_us,
                event_count,
                signature
            );
        }
    }

    /// 获取运行时长
    pub fn get_uptime(&self) -> std::time::Duration {
        std::time::Duration::from_secs_f64(self.metrics.get_uptime_seconds())
    }

    /// 获取事件指标
    pub fn get_event_metrics(&self, event_type: EventType) -> EventMetricsSnapshot {
        self.metrics.get_event_metrics(event_type)
    }

    /// 获取处理时间统计
    pub fn get_processing_stats(&self) -> ProcessingTimeStats {
        self.metrics.get_processing_stats()
    }

    /// 获取背压指标
    pub fn get_backpressure_metrics(&self) -> BackpressureMetricsSnapshot {
        self.metrics.get_backpressure_metrics()
    }

    /// 获取丢弃事件计数
    pub fn get_dropped_events_count(&self) -> u64 {
        self.metrics.get_dropped_events_count()
    }

    /// 打印性能指标（非阻塞）
    pub fn print_metrics(&self) {
        println!("\n📊 {} Performance Metrics", self.stream_name);
        println!("   Run Time: {:?}", self.get_uptime());

        // 打印背压指标表格
        let backpressure = self.get_backpressure_metrics();
        if backpressure.success_count > 0 || backpressure.failure_count > 0 {
            println!("\n🚦 Backpressure Metrics");
            println!("┌──────────────────────┬─────────────┐");
            println!("│ Metric               │ Value       │");
            println!("├──────────────────────┼─────────────┤");
            println!("│ Success Count        │ {:11} │", backpressure.success_count);
            println!("│ Failure Count        │ {:11} │", backpressure.failure_count);
            println!("│ Success Rate         │ {:11.2} │", backpressure.success_rate * 100.0);
            println!("│ Avg Duration (ms)    │ {:11.2} │", backpressure.avg_duration_us / 1000.0);
            println!("│ Min Permits          │ {:11} │", backpressure.min_permits);
            println!("│ Max Permits          │ {:11} │", backpressure.max_permits);
            println!("└──────────────────────┴─────────────┘");
        }

        // 打印丢弃事件指标
        let dropped_count = self.get_dropped_events_count();
        if dropped_count > 0 {
            println!("\n⚠️  Dropped Events: {}", dropped_count);
        }

        // 打印事件指标表格
        println!("┌─────────────┬──────────────┬──────────────────┬─────────────────┐");
        println!("│ Event Type  │ Process Count│ Events Processed │ Events/Second   │");
        println!("├─────────────┼──────────────┼──────────────────┼─────────────────┤");

        for event_type in [EventType::Transaction, EventType::Account, EventType::BlockMeta] {
            let metrics = self.get_event_metrics(event_type);
            println!(
                "│ {:11} │ {:12} │ {:16} │ {:13.2}   │",
                event_type.name(),
                metrics.process_count,
                metrics.events_processed,
                metrics.events_per_second
            );
        }

        println!("└─────────────┴──────────────┴──────────────────┴─────────────────┘");

        // 打印处理时间统计表格
        let stats = self.get_processing_stats();
        println!("\n⏱️  Processing Time Statistics");
        println!("┌───────────────────────┬─────────────┐");
        println!("│ Metric                │ Value (us)  │");
        println!("├───────────────────────┼─────────────┤");
        println!("│ Average               │ {:9.2}   │", stats.avg_us);
        println!("│ Minimum               │ {:9.2}   │", stats.min_us);
        println!("│ Maximum within 10s    │ {:9.2}   │", stats.max_us);
        println!("└───────────────────────┴─────────────┘");

        println!();
    }

    /// 启动自动性能监控任务
    pub async fn start_auto_monitoring(&self) -> Option<tokio::task::JoinHandle<()>> {
        if !self.enable_metrics {
            return None;
        }

        let manager = self.clone();
        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(
                DEFAULT_METRICS_PRINT_INTERVAL_SECONDS,
            ));
            loop {
                interval.tick().await;
                manager.print_metrics();
            }
        });
        Some(handle)
    }

    // === 兼容性方法 ===

    /// 兼容性构造函数
    pub fn new_with_metrics(
        _metrics: Arc<std::sync::RwLock<PerformanceMetrics>>,
        enable_metrics: bool,
        stream_name: String,
    ) -> Self {
        Self::new(enable_metrics, stream_name)
    }

    /// 获取完整的性能指标（兼容性方法）
    pub fn get_metrics(&self) -> PerformanceMetrics {
        PerformanceMetrics {
            uptime: self.get_uptime(),
            tx_metrics: self.get_event_metrics(EventType::Transaction),
            account_metrics: self.get_event_metrics(EventType::Account),
            block_meta_metrics: self.get_event_metrics(EventType::BlockMeta),
            processing_stats: self.get_processing_stats(),
            backpressure_metrics: self.metrics.get_backpressure_metrics(),
            dropped_events_count: self.metrics.get_dropped_events_count(),
        }
    }

    /// 兼容性方法 - 添加交易处理计数
    #[inline]
    pub fn add_tx_process_count(&self) {
        self.record_process(EventType::Transaction);
    }

    /// 兼容性方法 - 添加账户处理计数
    #[inline]
    pub fn add_account_process_count(&self) {
        self.record_process(EventType::Account);
    }

    /// 兼容性方法 - 添加区块元数据处理计数
    #[inline]
    pub fn add_block_meta_process_count(&self) {
        self.record_process(EventType::BlockMeta);
    }

    /// 兼容性方法 - 更新指标
    #[inline]
    pub fn update_metrics(
        &self,
        event_type: MetricsEventType,
        events_processed: u64,
        processing_time_us: f64,
        signature: Option<Signature>,
    ) {
        self.record_events(event_type, events_processed, processing_time_us);
        self.log_slow_processing(processing_time_us, events_processed as usize, signature);
    }

    /// 记录背压相关的metrics
    #[inline]
    pub fn record_backpressure_metrics(
        &self,
        backpressure_duration: std::time::Duration,
        success: bool,
        available_permits: usize,
    ) {
        if !self.enable_metrics {
            return;
        }

        let duration_us = backpressure_duration.as_micros() as u64;
        let permits = available_permits as u64;

        // 记录总持续时间
        self.metrics.backpressure_total_duration_us.fetch_add(duration_us, Ordering::Relaxed);

        // 记录成功/失败计数
        if success {
            self.metrics.backpressure_success_count.fetch_add(1, Ordering::Relaxed);
        } else {
            self.metrics.backpressure_failure_count.fetch_add(1, Ordering::Relaxed);
        }

        // 更新最小许可数（使用 compare_exchange_weak 循环）
        let mut current_min = self.metrics.backpressure_min_permits.load(Ordering::Relaxed);
        while permits < current_min {
            match self.metrics.backpressure_min_permits.compare_exchange_weak(
                current_min,
                permits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_min = x,
            }
        }

        // 更新最大许可数
        let mut current_max = self.metrics.backpressure_max_permits.load(Ordering::Relaxed);
        while permits > current_max {
            match self.metrics.backpressure_max_permits.compare_exchange_weak(
                current_max,
                permits,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => break,
                Err(x) => current_max = x,
            }
        }

        // 记录慢背压操作的日志
        if duration_us > 10_000 {
            // 超过10ms的背压认为是慢操作
            log::warn!(
                "{} slow backpressure: {:.2}ms, success: {}, available_permits: {}",
                self.stream_name,
                duration_us as f64 / 1000.0,
                success,
                available_permits
            );
        }
    }

    /// 增加丢弃事件计数
    #[inline]
    pub fn increment_dropped_events(&self) {
        if !self.enable_metrics {
            return;
        }

        // 原子地增加丢弃事件计数
        let new_count = self.metrics.dropped_events_count.fetch_add(1, Ordering::Relaxed) + 1;

        // 每丢弃1000个事件记录一次警告日志
        if new_count % 1000 == 0 {
            log::warn!("{} dropped events count reached: {}", self.stream_name, new_count);
        }
    }

    /// 批量增加丢弃事件计数
    #[inline]
    pub fn increment_dropped_events_by(&self, count: u64) {
        if !self.enable_metrics || count == 0 {
            return;
        }

        // 原子地增加丢弃事件计数
        let new_count = self.metrics.dropped_events_count.fetch_add(count, Ordering::Relaxed) + count;

        // 记录批量丢弃事件的日志
        if count > 1 {
            log::warn!("{} dropped batch of {} events, total dropped: {}", 
                self.stream_name, count, new_count);
        }

        // 每丢弃1000个事件记录一次警告日志
        if new_count % 1000 == 0 || (new_count / 1000) != ((new_count - count) / 1000) {
            log::warn!("{} dropped events count reached: {}", self.stream_name, new_count);
        }
    }
}

impl Clone for MetricsManager {
    fn clone(&self) -> Self {
        Self {
            metrics: self.metrics.clone(),
            enable_metrics: self.enable_metrics,
            stream_name: self.stream_name.clone(),
            background_task_running: AtomicBool::new(false), // 新实例不自动启动后台任务
        }
    }
}
