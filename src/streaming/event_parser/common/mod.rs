pub mod types;
pub mod utils;
pub mod filter;

pub const EMPTY_ID: &str = "";

/// 自动生成UnifiedEvent trait实现的宏
#[macro_export]
macro_rules! impl_unified_event {
    // 带有自定义ID表达式的版本
    ($struct_name:ident, $($field:ident),*) => {
        impl $crate::streaming::event_parser::core::traits::UnifiedEvent for $struct_name {
            fn id(&self) -> &str {
                &self.metadata.id
            }

            fn clear_id(&mut self) {
                self.metadata.id = $crate::streaming::event_parser::common::EMPTY_ID.to_string();
            }

            fn event_type(&self) -> $crate::streaming::event_parser::common::types::EventType {
                self.metadata.event_type.clone()
            }

            fn signature(&self) -> &str {
                &self.metadata.signature
            }

            fn slot(&self) -> u64 {
                self.metadata.slot
            }

            fn program_received_time_us(&self) -> i64 {
                self.metadata.program_received_time_us
            }

            fn program_handle_time_consuming_us(&self) -> i64 {
                self.metadata.program_handle_time_consuming_us
            }

            fn set_program_handle_time_consuming_us(&mut self, program_handle_time_consuming_us: i64) {
                self.metadata.program_handle_time_consuming_us = program_handle_time_consuming_us;
            }

            fn as_any(&self) -> &dyn std::any::Any {
                self
            }

            fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
                self
            }

            fn clone_boxed(&self) -> Box<dyn $crate::streaming::event_parser::core::traits::UnifiedEvent> {
                Box::new(self.clone())
            }

            fn merge(&mut self, other: &dyn $crate::streaming::event_parser::core::traits::UnifiedEvent) {
                if let Some(_e) = other.as_any().downcast_ref::<$struct_name>() {
                    $(
                        self.$field = _e.$field.clone();
                    )*
                }
            }

            fn set_swap_data(&mut self, swap_data: $crate::streaming::event_parser::common::types::SwapData) {
                self.metadata.set_swap_data(swap_data);
            }

            fn instruction_outer_index(&self) -> i64 {
                self.metadata.instruction_outer_index
            }

            fn instruction_inner_index(&self) -> Option<i64> {
                self.metadata.instruction_inner_index
            }
            fn transaction_index(&self) -> Option<u64> {
                self.metadata.transaction_index
            }

            fn set_transaction_index(&mut self, transaction_index: Option<u64>) {
                self.metadata.set_transaction_index(transaction_index);
            }
        }
    };
}

pub use types::*;
pub use utils::*;
