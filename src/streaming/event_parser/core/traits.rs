use anyhow::Result;
use prost_types::Timestamp;
use solana_sdk::signature::Signature;
use solana_sdk::{
    instruction::CompiledInstruction, pubkey::Pubkey, transaction::VersionedTransaction,
};
use solana_transaction_status::{InnerInstructions, TransactionWithStatusMeta};
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::Arc;

use crate::streaming::event_parser::common::{parse_swap_data_from_next_instructions, SwapData};
use crate::streaming::event_parser::protocols::pumpswap::{PumpSwapBuyEvent, PumpSwapSellEvent};
use crate::streaming::event_parser::{
    common::{utils::*, EventMetadata, EventType, ProtocolType},
    protocols::{
        bonk::{BonkPoolCreateEvent, BonkTradeEvent},
        pumpfun::{PumpFunCreateTokenEvent, PumpFunTradeEvent},
    },
};

/// Unified Event Interface - All protocol events must implement this trait
pub trait UnifiedEvent: Debug + Send + Sync {
    /// Get event ID
    fn id(&self) -> &str;

    /// Set event ID
    fn clear_id(&mut self);

    /// Get event type
    fn event_type(&self) -> EventType;

    /// Get transaction signature
    fn signature(&self) -> &str;

    /// Get slot number
    fn slot(&self) -> u64;

    /// Get program received timestamp (milliseconds)
    fn program_received_time_us(&self) -> i64;

    /// Processing time consumption (milliseconds)
    fn program_handle_time_consuming_us(&self) -> i64;

    /// Set processing time consumption (milliseconds)
    fn set_program_handle_time_consuming_us(&mut self, program_handle_time_consuming_us: i64);

    /// Convert event to Any for downcasting
    fn as_any(&self) -> &dyn std::any::Any;

    /// Convert event to mutable Any for downcasting
    fn as_any_mut(&mut self) -> &mut dyn std::any::Any;

    /// Clone the event
    fn clone_boxed(&self) -> Box<dyn UnifiedEvent>;

    /// Merge events (optional implementation)
    fn merge(&mut self, _other: &dyn UnifiedEvent) {
        // Default implementation: no merging operation
    }

    /// Set swap data
    fn set_swap_data(&mut self, swap_data: SwapData);

    /// Get index
    fn instruction_outer_index(&self) -> i64;
    fn instruction_inner_index(&self) -> Option<i64>;

    /// Get transaction index in slot
    fn transaction_index(&self) -> Option<u64>;
}

/// 事件解析器trait - 定义了事件解析的核心方法
#[async_trait::async_trait]
pub trait EventParser: Send + Sync {
    /// 获取内联指令解析配置
    fn inner_instruction_configs(&self) -> HashMap<Vec<u8>, Vec<GenericEventParseConfig>>;
    /// 获取指令解析配置
    fn instruction_configs(&self) -> HashMap<Vec<u8>, Vec<GenericEventParseConfig>>;
    /// 从内联指令中解析事件数据
    #[allow(clippy::too_many_arguments)]
    fn parse_events_from_inner_instruction(
        &self,
        inner_instruction: &CompiledInstruction,
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Vec<Box<dyn UnifiedEvent>>;

    /// 从指令中解析事件数据
    #[allow(clippy::too_many_arguments)]
    fn parse_events_from_instruction(
        &self,
        instruction: &CompiledInstruction,
        accounts: &[Pubkey],
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Vec<Box<dyn UnifiedEvent>>;

    /// 从VersionedTransaction中解析指令事件的通用方法
    #[allow(clippy::too_many_arguments)]
    async fn parse_instruction_events_from_versioned_transaction(
        &self,
        transaction: &VersionedTransaction,
        signature: Signature,
        slot: Option<u64>,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        accounts: &[Pubkey],
        inner_instructions: &[InnerInstructions],
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Result<Vec<Box<dyn UnifiedEvent>>> {
        // 预分配容量，避免动态扩容
        let mut instruction_events = Vec::with_capacity(16);
        // 获取交易的指令和账户
        let compiled_instructions = transaction.message.instructions();
        let mut accounts: Vec<Pubkey> = accounts.to_vec();

        // 检查交易中是否包含程序
        let has_program = accounts.iter().any(|account| self.should_handle(account));
        if has_program {
            // 解析每个指令
            for (index, instruction) in compiled_instructions.iter().enumerate() {
                if let Some(program_id) = accounts.get(instruction.program_id_index as usize) {
                    if self.should_handle(program_id) {
                        let max_idx = instruction.accounts.iter().max().unwrap_or(&0);
                        // 补齐accounts(使用Pubkey::default())
                        if *max_idx as usize > accounts.len() {
                            for _i in accounts.len()..*max_idx as usize {
                                accounts.push(Pubkey::default());
                            }
                        }
                        if let Ok(mut events) = self
                            .parse_instruction(
                                instruction,
                                &accounts,
                                signature,
                                slot,
                                block_time,
                                program_received_time_us,
                                index as i64,
                                None,
                                bot_wallet,
                                Some(index as u64),
                            )
                            .await
                        {
                            if !events.is_empty() {
                                if let Some(inn) =
                                    inner_instructions.iter().find(|inner_instruction| {
                                        inner_instruction.index == index as u8
                                    })
                                {
                                    events.iter_mut().for_each(|event| {
                                        let swap_data = parse_swap_data_from_next_instructions(
                                            event.as_ref(),
                                            inn,
                                            -1_i8,
                                            &accounts,
                                        );
                                        if let Some(swap_data) = swap_data {
                                            event.set_swap_data(swap_data);
                                        }
                                    });
                                }
                                instruction_events.extend(events);
                            }
                        }
                    }
                }
            }
        }
        Ok(instruction_events)
    }

    async fn parse_versioned_transaction(
        &self,
        versioned_tx: &VersionedTransaction,
        signature: Signature,
        slot: Option<u64>,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Result<Vec<Box<dyn UnifiedEvent>>> {
        let accounts: Vec<Pubkey> = versioned_tx.message.static_account_keys().to_vec();
        let events = self
            .parse_instruction_events_from_versioned_transaction(
                versioned_tx,
                signature,
                slot,
                block_time,
                program_received_time_us,
                &accounts,
                &[],
                bot_wallet,
                transaction_index,
            )
            .await
            .unwrap_or_else(|_e| vec![]);
        Ok(self.process_events(events, bot_wallet))
    }

    async fn parse_transaction(
        &self,
        tx: TransactionWithStatusMeta,
        signature: Signature,
        slot: Option<u64>,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Result<Vec<Box<dyn UnifiedEvent>>> {
        let versioned_tx = tx.get_transaction();
        let meta = tx.get_status_meta();

        let mut address_table_lookups: Vec<Pubkey> = vec![];
        let mut inner_instructions: Vec<InnerInstructions> = vec![];
        if let Some(meta) = meta {
            inner_instructions = meta.inner_instructions.unwrap_or_default();
            address_table_lookups.reserve(
                meta.loaded_addresses.writable.len() + meta.loaded_addresses.readonly.len(),
            );
            address_table_lookups.extend(
                meta.loaded_addresses.writable.into_iter().chain(meta.loaded_addresses.readonly),
            );
        }

        let mut accounts = Vec::with_capacity(
            versioned_tx.message.static_account_keys().len() + address_table_lookups.len(),
        );
        accounts.extend_from_slice(versioned_tx.message.static_account_keys());
        accounts.extend(address_table_lookups);

        // 使用 Arc 包装共享数据，避免不必要的克隆
        let accounts_arc = Arc::new(accounts);
        let inner_instructions_arc = Arc::new(inner_instructions);

        // 预分配容量，避免动态扩容
        let mut instruction_events: Vec<Box<dyn UnifiedEvent>> = Vec::with_capacity(16);
        let mut inner_instruction_events: Vec<Box<dyn UnifiedEvent>> = Vec::with_capacity(8);

        let accounts_for_task1 = Arc::clone(&accounts_arc);
        let inner_instructions_for_task1 = Arc::clone(&inner_instructions_arc);
        let task1 = async move {
            self.parse_instruction_events_from_versioned_transaction(
                &versioned_tx,
                signature,
                slot,
                block_time,
                program_received_time_us,
                &accounts_for_task1,
                &inner_instructions_for_task1,
                bot_wallet,
                transaction_index,
            )
            .await
            .unwrap_or_else(|_e| vec![])
        };

        // 解析内联指令事件
        let inner_instructions_for_task2 = Arc::clone(&inner_instructions_arc);
        let accounts_for_task2_1 = Arc::clone(&accounts_arc);
        let accounts_for_task2_2 = Arc::clone(&accounts_arc);

        let mut task2_params = Vec::with_capacity(inner_instructions_for_task2.len() * 5);
        for inner_instruction in inner_instructions_for_task2.iter() {
            for (index, instruction) in inner_instruction.instructions.iter().enumerate() {
                task2_params.push((
                    &instruction.instruction,
                    signature,
                    slot,
                    block_time,
                    program_received_time_us,
                    inner_instruction.index as i64,
                    Some(index as i64),
                    inner_instruction,
                ));
            }
        }
        // 转换为 Arc<[T]> 更轻量
        let task2_params: Arc<[_]> = task2_params.into();
        let task2_params_clone = Arc::clone(&task2_params);
        let task2_1 = async move {
            let mut instruction_events: Vec<Box<dyn UnifiedEvent>> = Vec::with_capacity(16);
            for (
                instruction,
                signature,
                slot,
                block_time,
                program_received_time_us,
                outer_index,
                inner_index,
                inner_instruction,
            ) in task2_params_clone.iter()
            {
                if let Ok(mut events) = self
                    .parse_instruction(
                        instruction,
                        &accounts_for_task2_1,
                        *signature,
                        *slot,
                        *block_time,
                        *program_received_time_us,
                        *outer_index,
                        *inner_index,
                        bot_wallet,
                        transaction_index,
                    )
                    .await
                {
                    if !events.is_empty() {
                        events.iter_mut().for_each(|event| {
                            let swap_data = parse_swap_data_from_next_instructions(
                                event.as_ref(),
                                &inner_instruction,
                                inner_index.unwrap_or_default() as i8,
                                &accounts_for_task2_1,
                            );
                            if let Some(swap_data) = swap_data {
                                event.set_swap_data(swap_data);
                            }
                        });
                        instruction_events.extend(events);
                    }
                }
            }
            instruction_events
        };
        let task2_2 = async move {
            let mut inner_instruction_events: Vec<Box<dyn UnifiedEvent>> = Vec::with_capacity(8);
            for (
                instruction,
                signature,
                slot,
                block_time,
                program_received_time_us,
                outer_index,
                inner_index,
                inner_instruction,
            ) in task2_params.iter()
            {
                if let Ok(mut events) = self
                    .parse_inner_instruction(
                        instruction,
                        *signature,
                        *slot,
                        *block_time,
                        *program_received_time_us,
                        *outer_index,
                        *inner_index,
                        bot_wallet,
                        transaction_index,
                    )
                    .await
                {
                    if !events.is_empty() {
                        events.iter_mut().for_each(|event| {
                            let swap_data = parse_swap_data_from_next_instructions(
                                event.as_ref(),
                                &inner_instruction,
                                inner_index.unwrap_or_default() as i8,
                                &accounts_for_task2_2,
                            );
                            if let Some(swap_data) = swap_data {
                                event.set_swap_data(swap_data);
                            }
                        });
                        inner_instruction_events.extend(events);
                    }
                }
            }
            inner_instruction_events
        };

        let (r1, r2_1, r2_2) = tokio::join!(task1, task2_1, task2_2);
        instruction_events.extend(r1);
        instruction_events.extend(r2_1);
        inner_instruction_events.extend(r2_2);

        if !instruction_events.is_empty() && !inner_instruction_events.is_empty() {
            for instruction_event in &mut instruction_events {
                for inner_instruction_event in &inner_instruction_events {
                    if instruction_event.id() == inner_instruction_event.id() {
                        if instruction_event.instruction_inner_index().is_none()
                            && inner_instruction_event.instruction_inner_index().is_some()
                        {
                            if inner_instruction_event.instruction_outer_index()
                                == instruction_event.instruction_outer_index()
                            {
                                instruction_event.merge(inner_instruction_event.as_ref());
                                break;
                            }
                        } else if instruction_event.instruction_inner_index().is_some()
                            && inner_instruction_event.instruction_inner_index().is_some()
                        {
                            if instruction_event.instruction_outer_index()
                                == inner_instruction_event.instruction_outer_index()
                            {
                                if inner_instruction_event
                                    .instruction_inner_index()
                                    .unwrap_or_default()
                                    > instruction_event
                                        .instruction_inner_index()
                                        .unwrap_or_default()
                                {
                                    instruction_event.merge(inner_instruction_event.as_ref());
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        let result = self.process_events(instruction_events, bot_wallet);

        Ok(result)
    }

    fn process_events(
        &self,
        mut events: Vec<Box<dyn UnifiedEvent>>,
        bot_wallet: Option<Pubkey>,
    ) -> Vec<Box<dyn UnifiedEvent>> {
        let mut dev_address = vec![];
        let mut bonk_dev_address = None;
        for event in &mut events {
            if let Some(token_info) = event.as_any().downcast_ref::<PumpFunCreateTokenEvent>() {
                dev_address.push(token_info.user);
                if token_info.creator != Pubkey::default() && token_info.creator != token_info.user
                {
                    dev_address.push(token_info.creator);
                }
            } else if let Some(trade_info) = event.as_any_mut().downcast_mut::<PumpFunTradeEvent>()
            {
                if dev_address.contains(&trade_info.user)
                    || dev_address.contains(&trade_info.creator)
                {
                    trade_info.is_dev_create_token_trade = true;
                } else if Some(trade_info.user) == bot_wallet {
                    trade_info.is_bot = true;
                } else {
                    trade_info.is_dev_create_token_trade = false;
                }
                if trade_info.metadata.swap_data.is_some() {
                    trade_info.metadata.swap_data.as_mut().unwrap().from_amount =
                        if trade_info.is_buy {
                            trade_info.sol_amount
                        } else {
                            trade_info.token_amount
                        };
                    trade_info.metadata.swap_data.as_mut().unwrap().to_amount = if trade_info.is_buy
                    {
                        trade_info.token_amount
                    } else {
                        trade_info.sol_amount
                    };
                }
            } else if let Some(trade_info) = event.as_any_mut().downcast_mut::<PumpSwapBuyEvent>() {
                if trade_info.metadata.swap_data.is_some() {
                    trade_info.metadata.swap_data.as_mut().unwrap().from_amount =
                        trade_info.user_quote_amount_in;
                    trade_info.metadata.swap_data.as_mut().unwrap().to_amount =
                        trade_info.base_amount_out;
                }
            } else if let Some(trade_info) = event.as_any_mut().downcast_mut::<PumpSwapSellEvent>()
            {
                if trade_info.metadata.swap_data.is_some() {
                    trade_info.metadata.swap_data.as_mut().unwrap().from_amount =
                        trade_info.base_amount_in;
                    trade_info.metadata.swap_data.as_mut().unwrap().to_amount =
                        trade_info.user_quote_amount_out;
                }
            } else if let Some(pool_info) = event.as_any().downcast_ref::<BonkPoolCreateEvent>() {
                bonk_dev_address = Some(pool_info.creator);
            } else if let Some(trade_info) = event.as_any_mut().downcast_mut::<BonkTradeEvent>() {
                if Some(trade_info.payer) == bonk_dev_address {
                    trade_info.is_dev_create_token_trade = true;
                } else if Some(trade_info.payer) == bot_wallet {
                    trade_info.is_bot = true;
                } else {
                    trade_info.is_dev_create_token_trade = false;
                }
            }
            event.clear_id();
        }

        events
    }

    async fn parse_inner_instruction(
        &self,
        instruction: &CompiledInstruction,
        signature: Signature,
        slot: Option<u64>,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Result<Vec<Box<dyn UnifiedEvent>>> {
        let slot = slot.unwrap_or(0);
        let events = self.parse_events_from_inner_instruction(
            instruction,
            signature,
            slot,
            block_time,
            program_received_time_us,
            outer_index,
            inner_index,
            bot_wallet,
            transaction_index,
        );
        Ok(events)
    }

    #[allow(clippy::too_many_arguments)]
    async fn parse_instruction(
        &self,
        instruction: &CompiledInstruction,
        accounts: &[Pubkey],
        signature: Signature,
        slot: Option<u64>,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Result<Vec<Box<dyn UnifiedEvent>>> {
        let slot = slot.unwrap_or(0);
        let events = self.parse_events_from_instruction(
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
        );
        Ok(events)
    }

    /// 检查是否应该处理此程序ID
    fn should_handle(&self, program_id: &Pubkey) -> bool;

    /// 获取支持的程序ID列表
    fn supported_program_ids(&self) -> Vec<Pubkey>;
}

// 为Box<dyn UnifiedEvent>实现Clone
impl Clone for Box<dyn UnifiedEvent> {
    fn clone(&self) -> Self {
        self.clone_boxed()
    }
}

/// 通用事件解析器配置
#[derive(Debug, Clone)]
pub struct GenericEventParseConfig {
    pub program_id: Pubkey,
    pub protocol_type: ProtocolType,
    pub inner_instruction_discriminator: &'static [u8],
    pub instruction_discriminator: &'static [u8],
    pub event_type: EventType,
    pub inner_instruction_parser: Option<InnerInstructionEventParser>,
    pub instruction_parser: Option<InstructionEventParser>,
}

/// 内联指令事件解析器
pub type InnerInstructionEventParser =
    fn(data: &[u8], metadata: EventMetadata) -> Option<Box<dyn UnifiedEvent>>;

/// 指令事件解析器
pub type InstructionEventParser =
    fn(data: &[u8], accounts: &[Pubkey], metadata: EventMetadata) -> Option<Box<dyn UnifiedEvent>>;

/// 通用事件解析器基类
pub struct GenericEventParser {
    pub program_ids: Vec<Pubkey>,
    pub inner_instruction_configs: HashMap<Vec<u8>, Vec<GenericEventParseConfig>>,
    pub instruction_configs: HashMap<Vec<u8>, Vec<GenericEventParseConfig>>,
}

impl GenericEventParser {
    /// 创建新的通用事件解析器
    pub fn new(program_ids: Vec<Pubkey>, configs: Vec<GenericEventParseConfig>) -> Self {
        // 预分配容量，避免动态扩容
        let mut inner_instruction_configs = HashMap::with_capacity(configs.len());
        let mut instruction_configs = HashMap::with_capacity(configs.len());

        for config in configs {
            if config.inner_instruction_discriminator.len() > 0 {
                inner_instruction_configs
                    .entry(config.inner_instruction_discriminator.to_vec())
                    .or_insert_with(Vec::new)
                    .push(config.clone());
            }
            instruction_configs
                .entry(config.instruction_discriminator.to_vec())
                .or_insert_with(Vec::new)
                .push(config.clone());
        }

        Self { program_ids, inner_instruction_configs, instruction_configs }
    }

    /// 通用的内联指令解析方法
    #[allow(clippy::too_many_arguments)]
    fn parse_inner_instruction_event(
        &self,
        config: &GenericEventParseConfig,
        data: &[u8],
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        transaction_index: Option<u64>,
    ) -> Option<Box<dyn UnifiedEvent>> {
        if let Some(parser) = config.inner_instruction_parser {
            let timestamp = block_time.unwrap_or(Timestamp { seconds: 0, nanos: 0 });
            let block_time_ms = timestamp.seconds * 1000 + (timestamp.nanos as i64) / 1_000_000;
            let metadata = EventMetadata::new(
                signature.to_string(),
                signature.to_string(),
                slot,
                timestamp.seconds,
                block_time_ms,
                config.protocol_type.clone(),
                config.event_type.clone(),
                config.program_id,
                outer_index,
                inner_index,
                program_received_time_us,
                transaction_index,
            );
            parser(data, metadata)
        } else {
            None
        }
    }

    /// 通用的指令解析方法
    #[allow(clippy::too_many_arguments)]
    fn parse_instruction_event(
        &self,
        config: &GenericEventParseConfig,
        data: &[u8],
        account_pubkeys: &[Pubkey],
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        transaction_index: Option<u64>,
    ) -> Option<Box<dyn UnifiedEvent>> {
        if let Some(parser) = config.instruction_parser {
            let timestamp = block_time.unwrap_or(Timestamp { seconds: 0, nanos: 0 });
            let block_time_ms = timestamp.seconds * 1000 + (timestamp.nanos as i64) / 1_000_000;
            let metadata = EventMetadata::new(
                signature.to_string(),
                signature.to_string(),
                slot,
                timestamp.seconds,
                block_time_ms,
                config.protocol_type.clone(),
                config.event_type.clone(),
                config.program_id,
                outer_index,
                inner_index,
                program_received_time_us,
                transaction_index,
            );
            parser(data, account_pubkeys, metadata)
        } else {
            None
        }
    }
}

#[async_trait::async_trait]
impl EventParser for GenericEventParser {
    fn inner_instruction_configs(&self) -> HashMap<Vec<u8>, Vec<GenericEventParseConfig>> {
        // 返回引用而非克隆，减少内存分配
        self.inner_instruction_configs.clone()
    }
    fn instruction_configs(&self) -> HashMap<Vec<u8>, Vec<GenericEventParseConfig>> {
        // 返回引用而非克隆，减少内存分配
        self.instruction_configs.clone()
    }
    /// 从内联指令中解析事件数据
    #[allow(clippy::too_many_arguments)]
    fn parse_events_from_inner_instruction(
        &self,
        inner_instruction: &CompiledInstruction,
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Vec<Box<dyn UnifiedEvent>> {
        if inner_instruction.data.len() < 16 {
            return Vec::new();
        }
        let data = &inner_instruction.data[16..];
        let mut events = Vec::new();
        for (disc, configs) in &self.inner_instruction_configs {
            if data == disc {
                for config in configs {
                    if let Some(event) = self.parse_inner_instruction_event(
                        config,
                        data,
                        signature,
                        slot,
                        block_time,
                        program_received_time_us,
                        outer_index,
                        inner_index,
                        transaction_index,
                    ) {
                        events.push(event);
                    }
                }
            }
        }
        events
    }

    /// 从指令中解析事件
    #[allow(clippy::too_many_arguments)]
    fn parse_events_from_instruction(
        &self,
        instruction: &CompiledInstruction,
        accounts: &[Pubkey],
        signature: Signature,
        slot: u64,
        block_time: Option<Timestamp>,
        program_received_time_us: i64,
        outer_index: i64,
        inner_index: Option<i64>,
        bot_wallet: Option<Pubkey>,
        transaction_index: Option<u64>,
    ) -> Vec<Box<dyn UnifiedEvent>> {
        let program_id = accounts[instruction.program_id_index as usize];
        if !self.should_handle(&program_id) {
            return Vec::new();
        }
        let mut events = Vec::new();
        for (disc, configs) in &self.instruction_configs {
            if instruction.data.len() < disc.len() {
                continue;
            }
            let discriminator = &instruction.data[..disc.len()];
            let data = &instruction.data[disc.len()..];
            if discriminator == disc {
                // 验证账户索引
                if !validate_account_indices(&instruction.accounts, accounts.len()) {
                    continue;
                }

                let account_pubkeys: Vec<Pubkey> =
                    instruction.accounts.iter().map(|&idx| accounts[idx as usize]).collect();
                for config in configs {
                    if config.program_id != program_id {
                        continue;
                    }
                    if let Some(event) = self.parse_instruction_event(
                        config,
                        data,
                        &account_pubkeys,
                        signature,
                        slot,
                        block_time,
                        program_received_time_us,
                        outer_index,
                        inner_index,
                        transaction_index,
                    ) {
                        events.push(event);
                    }
                }
            }
        }

        events
    }

    fn should_handle(&self, program_id: &Pubkey) -> bool {
        self.program_ids.contains(program_id)
    }

    fn supported_program_ids(&self) -> Vec<Pubkey> {
        self.program_ids.clone()
    }
}
