use {
    crate::schedule::{LeaderScheduleRpc, SolanaSchedule},
    anyhow::Context,
    borsh::de::BorshDeserialize,
    futures::stream::StreamExt,
    maplit::hashmap,
    serde::{Deserialize, Serialize},
    solana_compute_budget::compute_budget_processor::{
        DEFAULT_INSTRUCTION_COMPUTE_UNIT_LIMIT, MAX_COMPUTE_UNIT_LIMIT,
    },
    solana_sdk::{
        clock::{Epoch, Slot, UnixTimestamp},
        commitment_config::{
            CommitmentConfig as SolanaCommitmentConfig, CommitmentLevel as SolanaCommitmentLevel,
        },
        compute_budget::{self, ComputeBudgetInstruction},
        ed25519_program,
        hash::Hash,
        message::{AccountKeys, VersionedMessage},
        pubkey::Pubkey,
        secp256k1_program,
        signature::Signature,
    },
    solana_transaction_status::{
        TransactionStatusMeta, TransactionWithStatusMeta, VersionedTransactionWithStatusMeta,
    },
    std::{
        collections::{btree_map, BTreeMap, HashMap, HashSet},
        sync::Arc,
    },
    tokio::sync::mpsc,
    tonic::{
        codec::CompressionEncoding,
        metadata::{errors::InvalidMetadataValue, AsciiMetadataValue},
        transport::channel::ClientTlsConfig,
    },
    tracing::{error, info},
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::prelude::{
        self as proto, subscribe_update::UpdateOneof, SubscribeRequest,
        SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots,
        SubscribeRequestFilterTransactions, SubscribeUpdateBlockMeta,
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitmentLevel {
    Processed,
    Confirmed,
    #[default]
    Finalized,
}

impl From<proto::CommitmentLevel> for CommitmentLevel {
    fn from(commitment: proto::CommitmentLevel) -> Self {
        match commitment {
            proto::CommitmentLevel::Processed => Self::Processed,
            proto::CommitmentLevel::Confirmed => Self::Confirmed,
            proto::CommitmentLevel::Finalized => Self::Finalized,
        }
    }
}

impl From<SolanaCommitmentConfig> for CommitmentLevel {
    fn from(commitment: SolanaCommitmentConfig) -> Self {
        commitment.commitment.into()
    }
}

impl From<SolanaCommitmentLevel> for CommitmentLevel {
    fn from(commitment: SolanaCommitmentLevel) -> Self {
        match commitment {
            SolanaCommitmentLevel::Processed => Self::Processed,
            SolanaCommitmentLevel::Confirmed => Self::Confirmed,
            SolanaCommitmentLevel::Finalized => Self::Finalized,
        }
    }
}

impl CommitmentLevel {
    pub const fn as_str(self) -> &'static str {
        match self {
            CommitmentLevel::Processed => "processed",
            CommitmentLevel::Confirmed => "confirmed",
            CommitmentLevel::Finalized => "finalized",
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeyserTransactionAccounts {
    pub writable: Vec<Pubkey>,
    pub readable: Vec<Pubkey>,
    pub signers: Vec<Pubkey>,
    pub fee_payer: Pubkey,
}

impl From<(VersionedMessage, TransactionStatusMeta)> for GeyserTransactionAccounts {
    fn from((message, meta): (VersionedMessage, TransactionStatusMeta)) -> Self {
        let header = message.header();
        let static_account_keys = message.static_account_keys();

        // See details in `solana_program`:
        // https://docs.rs/solana-program/2.0.8/src/solana_program/message/compiled_keys.rs.html#103-113

        let mut writable = meta
            .loaded_addresses
            .writable
            .into_iter()
            .chain(
                static_account_keys[0..(header.num_required_signatures
                    - header.num_readonly_signed_accounts)
                    as usize]
                    .iter()
                    .copied(),
            )
            .chain(
                static_account_keys[header.num_required_signatures as usize
                    ..static_account_keys.len() - header.num_readonly_unsigned_accounts as usize]
                    .iter()
                    .copied(),
            )
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        writable.sort_unstable();

        let mut readable = meta
            .loaded_addresses
            .readonly
            .into_iter()
            .chain(
                static_account_keys[(header.num_required_signatures
                    - header.num_readonly_signed_accounts)
                    as usize
                    ..header.num_required_signatures as usize]
                    .iter()
                    .copied(),
            )
            .chain(
                static_account_keys[static_account_keys.len()
                    - header.num_readonly_unsigned_accounts as usize
                    ..static_account_keys.len()]
                    .iter()
                    .copied(),
            )
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        readable.sort_unstable();

        let mut signers = static_account_keys[0..header.num_required_signatures as usize]
            .iter()
            .copied()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        signers.sort_unstable();

        Self {
            writable,
            readable,
            signers,
            fee_payer: static_account_keys[0],
        }
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GeyserTransaction {
    pub signature: Signature,
    pub vote: bool,
    pub accounts: GeyserTransactionAccounts,
    pub sigs_count: u32,
    pub unit_limit: u32,
    pub unit_price: u64,
    pub units_consumed: Option<u64>,
    pub fee: u64,
}

impl From<(VersionedTransactionWithStatusMeta, bool)> for GeyserTransaction {
    fn from(
        (
            VersionedTransactionWithStatusMeta {
                transaction: tx,
                meta,
            },
            is_vote,
        ): (VersionedTransactionWithStatusMeta, bool),
    ) -> Self {
        // 1 SOL = 10^9 lamports
        // lamports per signature = 5_000
        // default: 200k CU per instruction, 1.4M per tx
        // max compute per account per block: 12M
        // max compute per block: 48M

        let account_keys = AccountKeys::new(
            tx.message.static_account_keys(),
            Some(&meta.loaded_addresses),
        );

        let mut unit_limit = None;
        let mut unit_price = 0;
        let mut ixs_count = 0;
        let mut sigs_count = tx.signatures.len() as u32;
        for ix in tx.message.instructions() {
            if let Some(pubkey) = account_keys.get(ix.program_id_index as usize) {
                match *pubkey {
                    compute_budget::ID => {
                        // We do not use `ComputeBudgetInstruction::try_from_slice(&ix.data)` because data can be longer than expected
                        match <ComputeBudgetInstruction as BorshDeserialize>::deserialize(
                            &mut ix.data.as_slice(),
                        ) {
                            Ok(ComputeBudgetInstruction::SetComputeUnitLimit(value)) => {
                                unit_limit = Some(MAX_COMPUTE_UNIT_LIMIT.min(value));
                            }
                            Ok(ComputeBudgetInstruction::SetComputeUnitPrice(value)) => {
                                unit_price = value;
                            }
                            _ => {}
                        }
                        continue;
                    }
                    ed25519_program::ID => {
                        sigs_count += ix.data.first().copied().unwrap_or(0) as u32;
                    }
                    secp256k1_program::ID => {
                        sigs_count += ix.data.first().copied().unwrap_or(0) as u32;
                    }
                    _ => {}
                }
            }
            ixs_count += 1;
        }
        let unit_limit = unit_limit.unwrap_or_else(|| {
            MAX_COMPUTE_UNIT_LIMIT.min(ixs_count * DEFAULT_INSTRUCTION_COMPUTE_UNIT_LIMIT)
        });
        let units_consumed = meta.compute_units_consumed;
        let fee = meta.fee;

        Self {
            signature: tx.signatures[0],
            vote: is_vote,
            accounts: (tx.message, meta).into(),
            sigs_count,
            unit_limit,
            unit_price,
            units_consumed,
            fee,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeyserMessage {
    Status {
        slot: Slot,
        commitment: CommitmentLevel,
    },
    Slot {
        leader: Option<Pubkey>,
        slot: Slot,
        hash: Hash,
        time: UnixTimestamp,
        height: Slot,
        parent_slot: Slot,
        parent_hash: Hash,
        transactions: Arc<Vec<GeyserTransaction>>,
    },
}

impl GeyserMessage {
    fn build_block(leader: Option<Pubkey>, block_info: BlockInfo) -> anyhow::Result<GeyserMessage> {
        let Some(info) = block_info.meta else {
            anyhow::bail!("failed to get block meta");
        };

        Ok(GeyserMessage::Slot {
            leader,
            slot: info.slot,
            hash: info
                .blockhash
                .parse()
                .context("failed to parse block hash")?,
            time: info
                .block_time
                .map(|ut| ut.timestamp)
                .ok_or(anyhow::anyhow!("failed to get block time"))?,
            height: info
                .block_height
                .map(|bh| bh.block_height)
                .ok_or(anyhow::anyhow!("failed to get block height"))?,
            parent_slot: info.parent_slot,
            parent_hash: info
                .parent_blockhash
                .parse()
                .context("failed to parse parent block hash")?,
            transactions: Arc::new(block_info.transactions),
        })
    }
}

#[derive(Debug)]
struct BlockInfo {
    meta: Option<SubscribeUpdateBlockMeta>,
    transactions: Vec<GeyserTransaction>,
}

impl Default for BlockInfo {
    fn default() -> Self {
        Self {
            meta: None,
            transactions: Vec::with_capacity(8192),
        }
    }
}

#[derive(Debug)]
enum MaybeGeyserMessage {
    Geyser(GeyserMessage),
    Block(Slot),
}

pub async fn subscribe<T>(
    grpc_endpoint: String,
    grpc_x_token: Option<T>,
    rpc_endpoint: String,
    saved_epochs: Vec<(Epoch, LeaderScheduleRpc)>,
) -> anyhow::Result<(
    mpsc::UnboundedReceiver<anyhow::Result<GeyserMessage>>,
    mpsc::UnboundedReceiver<(Epoch, LeaderScheduleRpc)>,
)>
where
    T: TryInto<AsciiMetadataValue, Error = InvalidMetadataValue>,
{
    let mut stream = GeyserGrpcClient::build_from_shared(grpc_endpoint)?
        .x_token(grpc_x_token)?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .send_compressed(CompressionEncoding::Gzip)
        .accept_compressed(CompressionEncoding::Gzip)
        .max_decoding_message_size(128 * 1024 * 1024) // 128MiB, BlockMeta with rewards can be bigger than 60MiB
        .connect()
        .await?
        .subscribe_once(SubscribeRequest {
            slots: hashmap! { "".to_owned() => SubscribeRequestFilterSlots { filter_by_commitment: None } },
            accounts: HashMap::new(),
            transactions: hashmap! { "".to_owned() => SubscribeRequestFilterTransactions {
                vote: None,
                failed: None,
                signature: None,
                account_include: vec![],
                account_exclude: vec![],
                account_required: vec![],
            } },
            transactions_status: HashMap::new(),
            entry: HashMap::new(),
            blocks: HashMap::new(),
            blocks_meta: hashmap! { "".to_owned() => SubscribeRequestFilterBlocksMeta {} },
            commitment: Some(proto::CommitmentLevel::Processed as i32),
            accounts_data_slice: vec![],
            ping: None,
        })
        .await
        .context("failed to subscribe on geyser stream")?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (schedule, schedule_rx) = SolanaSchedule::new(rpc_endpoint, saved_epochs);
    tokio::spawn(async move {
        let mut slot_processed_first = None;
        let mut blocks = BTreeMap::<Slot, BlockInfo>::new();

        let mut alive = true;
        while alive {
            let msg = match stream.next().await.map(|m| m.map(|m| m.update_oneof)) {
                Some(Ok(Some(UpdateOneof::Slot(info)))) => {
                    proto::CommitmentLevel::try_from(info.status)
                        .map(|commitment| {
                            let commitment = CommitmentLevel::from(commitment);
                            if commitment == CommitmentLevel::Processed {
                                if slot_processed_first.is_none() {
                                    slot_processed_first = Some(info.slot);
                                    info!(slot = info.slot, "received first processed slot");
                                }
                            } else if commitment == CommitmentLevel::Finalized {
                                loop {
                                    match blocks.first_key_value() {
                                        Some((block_info_slot, block_info))
                                            if *block_info_slot < info.slot =>
                                        {
                                            if let Some(meta) = &block_info.meta {
                                                if Some(*block_info_slot) != slot_processed_first {
                                                    error!(
                                                        slot = block_info_slot,
                                                        executed_transaction_count =
                                                            meta.executed_transaction_count,
                                                        transactions =
                                                            block_info.transactions.len(),
                                                        "failed to build block"
                                                    );
                                                }
                                            }
                                            blocks.pop_first();
                                        }
                                        _ => break,
                                    }
                                }
                            }

                            MaybeGeyserMessage::Geyser(GeyserMessage::Status {
                                slot: info.slot,
                                commitment,
                            })
                        })
                        .map_err(|_error| anyhow::anyhow!("invalid commitment"))
                }
                Some(Ok(Some(UpdateOneof::Transaction(info)))) => match info.transaction.map(|tx| {
                    (
                        tx.is_vote,
                        yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx),
                    )
                }) {
                    Some((is_vote, Ok(TransactionWithStatusMeta::Complete(tx_with_meta)))) => {
                        blocks
                            .entry(info.slot)
                            .or_default()
                            .transactions
                            .push(GeyserTransaction::from((tx_with_meta, is_vote)));
                        Ok(MaybeGeyserMessage::Block(info.slot))
                    }
                    Some((_is_vote, Ok(TransactionWithStatusMeta::MissingMetadata(_)))) => {
                        Err("failed to get transaction metadata".to_owned())
                    }
                    Some((_is_vote, Err(error))) => {
                        Err(format!("failed to decode transaction: {error}"))
                    }
                    None => Err("failed to get transaction".to_owned()),
                }
                .map_err(|err| anyhow::anyhow!(err)),
                Some(Ok(Some(UpdateOneof::BlockMeta(info)))) => {
                    let slot = info.slot;
                    blocks.entry(slot).or_default().meta = Some(info);
                    Ok(MaybeGeyserMessage::Block(slot))
                }
                Some(Ok(_)) => {
                    continue;
                }
                Some(Err(error)) => Err(error.into()),
                None => Err(anyhow::anyhow!("stream finished")),
            };

            let msg = match msg {
                Ok(MaybeGeyserMessage::Geyser(msg)) => Ok(msg),
                Ok(MaybeGeyserMessage::Block(slot)) => {
                    if let btree_map::Entry::Occupied(entry) = blocks.entry(slot) {
                        let info = entry.get();
                        if info.meta.as_ref().map(|m| m.executed_transaction_count)
                            == Some(info.transactions.len() as u64)
                        {
                            let leader = schedule.get_leader(slot);
                            GeyserMessage::build_block(leader, entry.remove())
                        } else {
                            continue;
                        }
                    } else {
                        continue;
                    }
                }
                Err(error) => Err(error),
            };

            alive = msg.is_ok();
            if tx.send(msg).is_err() {
                break;
            }
        }
    });

    Ok((rx, schedule_rx))
}
