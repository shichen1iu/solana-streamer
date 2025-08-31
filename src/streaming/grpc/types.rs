use solana_sdk::{pubkey::Pubkey, signature::Signature};
use solana_transaction_status::TransactionWithStatusMeta;
use std::{collections::HashMap, fmt};
use yellowstone_grpc_proto::{
    geyser::{
        SubscribeRequestFilterAccounts, SubscribeRequestFilterTransactions, SubscribeUpdateAccount,
        SubscribeUpdateBlockMeta, SubscribeUpdateTransaction,
    },
    prost_types::Timestamp,
};

pub type TransactionsFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;
pub type AccountsFilterMap = HashMap<String, SubscribeRequestFilterAccounts>;

#[derive(Clone)]
pub enum EventPretty {
    BlockMeta(BlockMetaPretty),
    Transaction(TransactionPretty),
    Account(AccountPretty),
}

#[derive(Clone)]
pub struct AccountPretty {
    pub slot: u64,
    pub signature: Signature,
    pub pubkey: Pubkey,
    pub executable: bool,
    pub lamports: u64,
    pub owner: Pubkey,
    pub rent_epoch: u64,
    pub data: Vec<u8>,
    pub program_received_time_us: i64,
}

impl fmt::Debug for AccountPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountPretty")
            .field("slot", &self.slot)
            .field("signature", &self.signature)
            .field("pubkey", &self.pubkey)
            .field("executable", &self.executable)
            .field("lamports", &self.lamports)
            .field("owner", &self.owner)
            .field("rent_epoch", &self.rent_epoch)
            .field("data", &self.data)
            .finish()
    }
}

#[derive(Clone)]
pub struct BlockMetaPretty {
    pub slot: u64,
    pub block_hash: String,
    pub block_time: Option<Timestamp>,
    pub program_received_time_us: i64,
}

impl fmt::Debug for BlockMetaPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockMetaPretty")
            .field("slot", &self.slot)
            .field("block_hash", &self.block_hash)
            .field("block_time", &self.block_time)
            .field("program_received_time_us", &self.program_received_time_us)
            .finish()
    }
}

#[derive(Clone)]
pub struct TransactionPretty {
    pub slot: u64,
    pub transaction_index: Option<u64>, // 新增：交易在slot中的索引
    pub block_hash: String,
    pub block_time: Option<Timestamp>,
    pub signature: Signature,
    pub is_vote: bool,
    pub tx: TransactionWithStatusMeta,
    pub program_received_time_us: i64,
}

impl fmt::Debug for TransactionPretty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TransactionPretty")
            .field("slot", &self.slot)
            .field("transaction_index", &self.transaction_index)
            .field("signature", &self.signature)
            .field("is_vote", &self.is_vote)
            .field("program_received_time_us", &self.program_received_time_us)
            .finish()
    }
}

impl From<SubscribeUpdateAccount> for AccountPretty {
    fn from(account: SubscribeUpdateAccount) -> Self {
        let account_info = account.account.unwrap();
        Self {
            slot: account.slot,
            signature: if let Some(txn_signature) = account_info.txn_signature {
                Signature::try_from(txn_signature.as_slice()).expect("valid signature")
            } else {
                Signature::default()
            },
            pubkey: Pubkey::try_from(account_info.pubkey.as_slice()).expect("valid pubkey"),
            executable: account_info.executable,
            lamports: account_info.lamports,
            owner: Pubkey::try_from(account_info.owner.as_slice()).expect("valid pubkey"),
            rent_epoch: account_info.rent_epoch,
            data: account_info.data,
            program_received_time_us: chrono::Utc::now().timestamp_micros(),
        }
    }
}

impl From<(SubscribeUpdateBlockMeta, Option<Timestamp>)> for BlockMetaPretty {
    fn from(
        (SubscribeUpdateBlockMeta { slot, blockhash, .. }, block_time): (
            SubscribeUpdateBlockMeta,
            Option<Timestamp>,
        ),
    ) -> Self {
        Self {
            block_hash: blockhash.to_string(),
            block_time,
            slot,
            program_received_time_us: chrono::Utc::now().timestamp_micros(),
        }
    }
}

impl From<(SubscribeUpdateTransaction, Option<Timestamp>)> for TransactionPretty {
    fn from(
        (SubscribeUpdateTransaction { transaction, slot }, block_time): (
            SubscribeUpdateTransaction,
            Option<Timestamp>,
        ),
    ) -> Self {
        let tx = transaction.expect("should be defined");
        // 根据用户说明，交易索引在 transaction.index 中
        let transaction_index = tx.index;
        Self {
            slot,
            transaction_index: Some(transaction_index), // 提取交易索引
            block_time,
            block_hash: "".to_string(),
            signature: Signature::try_from(tx.signature.as_slice()).expect("valid signature"),
            is_vote: tx.is_vote,
            tx: yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                .expect("valid tx with meta"),
            program_received_time_us: chrono::Utc::now().timestamp_micros(),
        }
    }
}
