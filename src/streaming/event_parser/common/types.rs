use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use solana_transaction_status::UiInstruction;
use std::sync::Arc;
use tokio::sync::Mutex;

// 对象池大小配置
const EVENT_METADATA_POOL_SIZE: usize = 1000;
const TRANSFER_DATA_POOL_SIZE: usize = 2000;

/// 事件元数据对象池
pub struct EventMetadataPool {
    pool: Arc<Mutex<Vec<EventMetadata>>>,
}

impl Default for EventMetadataPool {
    fn default() -> Self {
        Self::new()
    }
}

impl EventMetadataPool {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(Mutex::new(Vec::with_capacity(EVENT_METADATA_POOL_SIZE))),
        }
    }

    pub async fn acquire(&self) -> Option<EventMetadata> {
        let mut pool = self.pool.lock().await;
        pool.pop()
    }

    pub async fn release(&self, metadata: EventMetadata) {
        let mut pool = self.pool.lock().await;
        if pool.len() < EVENT_METADATA_POOL_SIZE {
            pool.push(metadata);
        }
    }
}

/// 传输数据对象池
pub struct TransferDataPool {
    pool: Arc<Mutex<Vec<TransferData>>>,
}

impl Default for TransferDataPool {
    fn default() -> Self {
        Self::new()
    }
}

impl TransferDataPool {
    pub fn new() -> Self {
        Self {
            pool: Arc::new(Mutex::new(Vec::with_capacity(TRANSFER_DATA_POOL_SIZE))),
        }
    }

    pub async fn acquire(&self) -> Option<TransferData> {
        let mut pool = self.pool.lock().await;
        pool.pop()
    }

    pub async fn release(&self, transfer_data: TransferData) {
        let mut pool = self.pool.lock().await;
        if pool.len() < TRANSFER_DATA_POOL_SIZE {
            pool.push(transfer_data);
        }
    }
}

// 全局对象池实例
lazy_static::lazy_static! {
    pub static ref EVENT_METADATA_POOL: EventMetadataPool = EventMetadataPool::new();
    pub static ref TRANSFER_DATA_POOL: TransferDataPool = TransferDataPool::new();
}

#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ProtocolType {
    #[default]
    PumpSwap,
    PumpFun,
    Bonk,
    RaydiumCpmm,
    RaydiumClmm,
    SDKSystem,
}

/// 事件类型枚举
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum EventType {
    // PumpSwap 事件
    #[default]
    PumpSwapBuy,
    PumpSwapSell,
    PumpSwapCreatePool,
    PumpSwapDeposit,
    PumpSwapWithdraw,

    // PumpFun 事件
    PumpFunCreateToken,
    PumpFunBuy,
    PumpFunSell,

    // Bonk 事件
    BonkBuyExactIn,
    BonkBuyExactOut,
    BonkSellExactIn,
    BonkSellExactOut,
    BonkInitialize,

    // Raydium CPMM 事件
    RaydiumCpmmSwapBaseInput,
    RaydiumCpmmSwapBaseOutput,

    // Raydium CLMM 事件
    RaydiumClmmSwap,
    RaydiumClmmSwapV2,

    // 通用事件
    SDKSystem,
    Unknown,
}

impl EventType {
    #[allow(clippy::inherent_to_string)]
    pub fn to_string(&self) -> String {
        match self {
            EventType::PumpSwapBuy => "PumpSwapBuy".to_string(),
            EventType::PumpSwapSell => "PumpSwapSell".to_string(),
            EventType::PumpSwapCreatePool => "PumpSwapCreatePool".to_string(),
            EventType::PumpSwapDeposit => "PumpSwapDeposit".to_string(),
            EventType::PumpSwapWithdraw => "PumpSwapWithdraw".to_string(),
            EventType::PumpFunCreateToken => "PumpFunCreateToken".to_string(),
            EventType::PumpFunBuy => "PumpFunBuy".to_string(),
            EventType::PumpFunSell => "PumpFunSell".to_string(),
            EventType::BonkBuyExactIn => "BonkBuyExactIn".to_string(),
            EventType::BonkBuyExactOut => "BonkBuyExactOut".to_string(),
            EventType::BonkSellExactIn => "BonkSellExactIn".to_string(),
            EventType::BonkSellExactOut => "BonkSellExactOut".to_string(),
            EventType::BonkInitialize => "BonkInitialize".to_string(),
            EventType::RaydiumCpmmSwapBaseInput => "RaydiumCpmmSwapBaseInput".to_string(),
            EventType::RaydiumCpmmSwapBaseOutput => "RaydiumCpmmSwapBaseOutput".to_string(),
            EventType::RaydiumClmmSwap => "RaydiumClmmSwap".to_string(),
            EventType::RaydiumClmmSwapV2 => "RaydiumClmmSwapV2".to_string(),
            EventType::SDKSystem => "SDKSystem".to_string(),
            EventType::Unknown => "Unknown".to_string(),
        }
    }
}

/// 解析结果
#[derive(Debug, Clone)]
pub struct ParseResult<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ParseResult<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn failure(error: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(error),
        }
    }

    pub fn is_success(&self) -> bool {
        self.success
    }

    pub fn is_failure(&self) -> bool {
        !self.success
    }
}

/// 协议信息
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtocolInfo {
    pub name: String,
    pub program_ids: Vec<Pubkey>,
}

impl ProtocolInfo {
    pub fn new(name: String, program_ids: Vec<Pubkey>) -> Self {
        Self { name, program_ids }
    }

    pub fn supports_program(&self, program_id: &Pubkey) -> bool {
        self.program_ids.contains(program_id)
    }
}

/// 交易数据
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct TransferData {
    pub token_program: Pubkey,
    pub source: Pubkey,
    pub destination: Pubkey,
    pub authority: Option<Pubkey>,
    pub amount: u64,
    pub decimals: Option<u8>,
    pub mint: Option<Pubkey>,
}

/// 事件元数据
#[derive(
    Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub struct EventMetadata {
    pub id: String,
    pub signature: String,
    pub slot: u64,
    pub block_time: i64,
    pub block_time_ms: i64,
    pub program_received_time_ms: i64,
    pub program_handle_time_consuming_ms: i64,
    pub protocol: ProtocolType,
    pub event_type: EventType,
    pub program_id: Pubkey,
    pub transfer_datas: Vec<TransferData>,
    pub index: String,
}

impl EventMetadata {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: String,
        signature: String,
        slot: u64,
        block_time: i64,
        block_time_ms: i64,
        protocol: ProtocolType,
        event_type: EventType,
        program_id: Pubkey,
        index: String,
        program_received_time_ms: i64,
    ) -> Self {
        Self {
            id,
            signature,
            slot,
            block_time,
            block_time_ms,
            program_received_time_ms,
            program_handle_time_consuming_ms: 0,
            protocol,
            event_type,
            program_id,
            transfer_datas: Vec::with_capacity(4), // 预分配容量
            index,
        }
    }

    /// 使用对象池创建EventMetadata
    #[allow(clippy::too_many_arguments)]
    pub async fn new_with_pool(
        id: String,
        signature: String,
        slot: u64,
        block_time: i64,
        block_time_ms: i64,
        protocol: ProtocolType,
        event_type: EventType,
        program_id: Pubkey,
        index: String,
        program_received_time_ms: i64,
    ) -> Self {
        // 尝试从对象池获取
        if let Some(mut metadata) = EVENT_METADATA_POOL.acquire().await {
            metadata.id = id;
            metadata.signature = signature;
            metadata.slot = slot;
            metadata.block_time = block_time;
            metadata.block_time_ms = block_time_ms;
            metadata.program_received_time_ms = program_received_time_ms;
            metadata.program_handle_time_consuming_ms = 0;
            metadata.protocol = protocol;
            metadata.event_type = event_type;
            metadata.program_id = program_id;
            metadata.index = index;
            metadata.transfer_datas.clear();
            return metadata;
        }
        
        // 如果对象池为空，创建新的
        Self::new(
            id,
            signature,
            slot,
            block_time,
            block_time_ms,
            protocol,
            event_type,
            program_id,
            index,
            program_received_time_ms,
        )
    }

    pub fn set_id(&mut self, id: String) {
        self.id = id;
    }

    pub fn set_transfer_datas(&mut self, transfer_datas: Vec<TransferData>) {
        self.transfer_datas = transfer_datas;
    }

    /// 回收EventMetadata到对象池
    pub async fn recycle(self) {
        EVENT_METADATA_POOL.release(self).await;
    }
}

/// 解析接下来指令中的token转账数据
pub fn parse_transfer_datas_from_next_instructions(
    inner_instruction: &solana_transaction_status::UiInnerInstructions,
    current_index: i8,
    accounts: &[Pubkey],
    event_type: EventType,
) -> Vec<TransferData> {
    let take = match event_type {
        EventType::PumpFunBuy => 4,
        EventType::PumpFunSell => 1,
        EventType::PumpSwapBuy => 3,
        EventType::PumpSwapSell => 3,
        EventType::BonkBuyExactIn
        | EventType::BonkBuyExactOut
        | EventType::BonkSellExactIn
        | EventType::BonkSellExactOut => 3,
        EventType::RaydiumCpmmSwapBaseInput
        | EventType::RaydiumCpmmSwapBaseOutput
        | EventType::RaydiumClmmSwap
        | EventType::RaydiumClmmSwapV2 => 2,
        _ => 0,
    };
    if take == 0 {
        return vec![];
    }
    let mut transfer_datas = vec![];
    // 获取当前指令之后的两个指令
    let next_instructions: Vec<&UiInstruction> = inner_instruction
        .instructions
        .iter()
        .skip((current_index + 1) as usize)
        .take(take)
        .collect();

    for instruction in next_instructions {
        if let UiInstruction::Compiled(compiled) = instruction {
            if let Ok(data) = bs58::decode(compiled.data.clone()).into_vec() {
                // Token Program: transferChecked
                // Token 2022 Program: transferChecked
                if data[0] == 12 {
                    let account_pubkeys: Vec<Pubkey> = compiled
                        .accounts
                        .iter()
                        .map(|a| accounts[*a as usize])
                        .collect();
                    if account_pubkeys.len() < 4 {
                        continue;
                    }
                    let (source, mint, destination, authority) = (
                        account_pubkeys[0],
                        account_pubkeys[1],
                        account_pubkeys[2],
                        account_pubkeys[3],
                    );
                    let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let decimals = data[9];
                    let token_program = accounts[compiled.program_id_index as usize];
                    transfer_datas.push(TransferData {
                        amount,
                        decimals: Some(decimals),
                        mint: Some(mint),
                        source,
                        destination,
                        authority: Some(authority),
                        token_program,
                    });
                }
                // Token Program: transfer
                else if data[0] == 3 {
                    let account_pubkeys: Vec<Pubkey> = compiled
                        .accounts
                        .iter()
                        .map(|a| accounts[*a as usize])
                        .collect();
                    if account_pubkeys.len() < 3 {
                        continue;
                    }
                    let (source, destination, authority) =
                        (account_pubkeys[0], account_pubkeys[1], account_pubkeys[2]);
                    let amount = u64::from_le_bytes(data[1..9].try_into().unwrap());
                    let token_program = accounts[compiled.program_id_index as usize];
                    transfer_datas.push(TransferData {
                        amount,
                        decimals: None,
                        mint: None,
                        source,
                        destination,
                        authority: Some(authority),
                        token_program,
                    });
                }
                //System Program: transfer
                else if data[0] == 2 {
                    let account_pubkeys: Vec<Pubkey> = compiled
                        .accounts
                        .iter()
                        .map(|a| accounts[*a as usize])
                        .collect();
                    if account_pubkeys.len() < 2 {
                        continue;
                    }
                    let (source, destination) = (account_pubkeys[0], account_pubkeys[1]);
                    let amount = u64::from_le_bytes(data[4..12].try_into().unwrap());
                    let token_program = accounts[compiled.program_id_index as usize];
                    transfer_datas.push(TransferData {
                        amount,
                        decimals: None,
                        mint: None,
                        source,
                        destination,
                        authority: None,
                        token_program,
                    });
                }
            }
        }
    }
    transfer_datas
}
