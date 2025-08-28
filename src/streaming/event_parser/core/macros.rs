/// Macro to generate boilerplate EventParser implementation for protocol parsers
///
/// This macro eliminates the repetitive code where each parser simply delegates
/// all EventParser trait methods to its inner GenericEventParser.
///
/// Usage:
/// ```rust
/// impl_event_parser_delegate!(MyEventParser);
/// ```
///
/// This will generate the complete EventParser implementation that delegates
/// all methods to `self.inner`.
#[macro_export]
macro_rules! impl_event_parser_delegate {
    ($parser_type:ty) => {
        #[async_trait::async_trait]
        impl $crate::streaming::event_parser::core::traits::EventParser for $parser_type {
            fn inner_instruction_configs(
                &self,
            ) -> std::collections::HashMap<
                Vec<u8>,
                Vec<$crate::streaming::event_parser::core::traits::GenericEventParseConfig>,
            > {
                self.inner.inner_instruction_configs()
            }

            fn instruction_configs(
                &self,
            ) -> std::collections::HashMap<
                Vec<u8>,
                Vec<$crate::streaming::event_parser::core::traits::GenericEventParseConfig>,
            > {
                self.inner.instruction_configs()
            }

            fn parse_events_from_inner_instruction(
                &self,
                inner_instruction: &solana_sdk::instruction::CompiledInstruction,
                signature: solana_sdk::signature::Signature,
                slot: u64,
                block_time: Option<prost_types::Timestamp>,
                program_received_time_us: i64,
                outer_index: i64,
                inner_index: Option<i64>,
                bot_wallet: Option<solana_sdk::pubkey::Pubkey>,
                transaction_index: Option<u64>,
            ) -> Vec<Box<dyn $crate::streaming::event_parser::core::traits::UnifiedEvent>> {
                self.inner.parse_events_from_inner_instruction(
                    inner_instruction,
                    signature,
                    slot,
                    block_time,
                    program_received_time_us,
                    outer_index,
                    inner_index,
                    bot_wallet,
                    transaction_index,
                )
            }

            fn parse_events_from_instruction(
                &self,
                instruction: &solana_sdk::instruction::CompiledInstruction,
                accounts: &[solana_sdk::pubkey::Pubkey],
                signature: solana_sdk::signature::Signature,
                slot: u64,
                block_time: Option<prost_types::Timestamp>,
                program_received_time_us: i64,
                outer_index: i64,
                inner_index: Option<i64>,
                bot_wallet: Option<solana_sdk::pubkey::Pubkey>,
                transaction_index: Option<u64>,
            ) -> Vec<Box<dyn $crate::streaming::event_parser::core::traits::UnifiedEvent>> {
                self.inner.parse_events_from_instruction(
                    instruction,
                    accounts,
                    signature,
                    slot,
                    block_time,
                    program_received_time_us,
                    outer_index,
                    inner_index,
                    bot_wallet,
                    transaction_index,
                )
            }

            fn should_handle(&self, program_id: &solana_sdk::pubkey::Pubkey) -> bool {
                self.inner.should_handle(program_id)
            }

            fn supported_program_ids(&self) -> Vec<solana_sdk::pubkey::Pubkey> {
                self.inner.supported_program_ids()
            }
        }
    };
}
