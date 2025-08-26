use crate::streaming::event_parser::core::traits::UnifiedEvent;
use crate::streaming::event_parser::protocols::block::block_meta_event::BlockMetaEvent;

pub struct CommonEventParser {}

impl CommonEventParser {
    pub fn generate_block_meta_event(
        slot: u64,
        block_hash: &str,
        block_time_ms: i64,
        program_received_time_us: i64,
    ) -> Box<dyn UnifiedEvent> {
        let mut block_meta_event = BlockMetaEvent::new(
            slot,
            block_hash.to_string(),
            block_time_ms,
            program_received_time_us,
        );
        block_meta_event.set_program_handle_time_consuming_us(
            chrono::Utc::now().timestamp_micros() - program_received_time_us,
        );
        Box::new(block_meta_event)
    }
}
