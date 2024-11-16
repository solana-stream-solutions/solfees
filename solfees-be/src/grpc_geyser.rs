use {
    crate::{
        metrics::grpc2redis as metrics,
        schedule::{LeaderScheduleRpc, SolanaSchedule},
    },
    anyhow::Context,
    futures::stream::StreamExt,
    maplit::hashmap,
    serde::{Deserialize, Serialize},
    solana_compute_budget::compute_budget_processor::process_compute_budget_instructions,
    solana_sdk::{
        clock::{Epoch, Slot, UnixTimestamp},
        commitment_config::{CommitmentConfig, CommitmentLevel as CommitmentLevelSolana},
        ed25519_program,
        hash::Hash,
        message::{AccountKeys, VersionedMessage},
        pubkey::Pubkey,
        secp256k1_program,
        signature::Signature,
        transaction::TransactionError,
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
        Status,
    },
    tracing::error,
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::{
        convert_from,
        prelude::{
            subscribe_update::UpdateOneof, CommitmentLevel as CommitmentLevelProto,
            SubscribeRequest, SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterSlots,
            SubscribeRequestFilterTransactions, SubscribeUpdate, SubscribeUpdateBlockMeta,
        },
    },
};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CommitmentLevel {
    #[default]
    Processed,
    Confirmed,
    Finalized,
}

impl From<CommitmentLevelProto> for CommitmentLevel {
    fn from(commitment: CommitmentLevelProto) -> Self {
        match commitment {
            CommitmentLevelProto::Processed => Self::Processed,
            CommitmentLevelProto::Confirmed => Self::Confirmed,
            CommitmentLevelProto::Finalized => Self::Finalized,
        }
    }
}

impl From<CommitmentConfig> for CommitmentLevel {
    fn from(commitment: CommitmentConfig) -> Self {
        commitment.commitment.into()
    }
}

impl From<CommitmentLevelSolana> for CommitmentLevel {
    fn from(commitment: CommitmentLevelSolana) -> Self {
        match commitment {
            CommitmentLevelSolana::Processed => Self::Processed,
            CommitmentLevelSolana::Confirmed => Self::Confirmed,
            CommitmentLevelSolana::Finalized => Self::Finalized,
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
    pub writable: HashSet<Pubkey>,
    pub readable: HashSet<Pubkey>,
    pub signers: HashSet<Pubkey>,
    pub fee_payer: Pubkey,
}

impl From<(&VersionedMessage, &TransactionStatusMeta)> for GeyserTransactionAccounts {
    fn from((message, meta): (&VersionedMessage, &TransactionStatusMeta)) -> Self {
        let header = message.header();
        let static_account_keys = message.static_account_keys();

        // See details in `solana_program`:
        // https://docs.rs/solana-program/2.0.8/src/solana_program/message/compiled_keys.rs.html#103-113

        let writable = meta
            .loaded_addresses
            .writable
            .iter()
            .chain(
                static_account_keys[0..(header.num_required_signatures
                    - header.num_readonly_signed_accounts)
                    as usize]
                    .iter(),
            )
            .chain(
                static_account_keys[header.num_required_signatures as usize
                    ..static_account_keys.len() - header.num_readonly_unsigned_accounts as usize]
                    .iter(),
            )
            .copied()
            .collect::<HashSet<_>>();

        let readable = meta
            .loaded_addresses
            .readonly
            .iter()
            .chain(
                static_account_keys[(header.num_required_signatures
                    - header.num_readonly_signed_accounts)
                    as usize
                    ..header.num_required_signatures as usize]
                    .iter(),
            )
            .chain(
                static_account_keys[static_account_keys.len()
                    - header.num_readonly_unsigned_accounts as usize
                    ..static_account_keys.len()]
                    .iter(),
            )
            .copied()
            .collect::<HashSet<_>>();

        let signers = static_account_keys[0..header.num_required_signatures as usize]
            .iter()
            .copied()
            .collect::<HashSet<_>>();

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

impl TryFrom<(VersionedTransactionWithStatusMeta, bool)> for GeyserTransaction {
    type Error = TransactionError;

    fn try_from(
        (
            VersionedTransactionWithStatusMeta {
                transaction: tx,
                meta,
            },
            is_vote,
        ): (VersionedTransactionWithStatusMeta, bool),
    ) -> Result<Self, Self::Error> {
        // 1 SOL = 10^9 lamports
        // lamports per signature = 5_000
        // default: 200k CU per instruction, 1.4M per tx
        // max compute per account per block: 12M
        // max compute per block: 48M

        let account_keys = AccountKeys::new(
            tx.message.static_account_keys(),
            Some(&meta.loaded_addresses),
        );

        let instructions = tx
            .message
            .instructions()
            .iter()
            .map(|ix| {
                account_keys
                    .get(ix.program_id_index as usize)
                    .ok_or(TransactionError::ProgramAccountNotFound)
                    .map(|program_id| (program_id, ix))
            })
            .collect::<Result<Vec<_>, TransactionError>>()?;

        let sigs_count = tx.signatures.len() as u32
            + instructions
                .iter()
                .map(|(program_id, ix)| {
                    match **program_id {
                        ed25519_program::ID => ix.data.first().copied(),
                        secp256k1_program::ID => ix.data.first().copied(),
                        _ => None,
                    }
                    .unwrap_or(0) as u32
                })
                .sum::<u32>();

        let computed_budget_limits = process_compute_budget_instructions(instructions.into_iter())?;

        Ok(Self {
            signature: tx.signatures[0],
            vote: is_vote,
            accounts: (&tx.message, &meta).into(),
            sigs_count,
            unit_limit: computed_budget_limits.compute_unit_limit,
            unit_price: computed_budget_limits.compute_unit_price,
            units_consumed: meta.compute_units_consumed,
            fee: meta.fee,
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
        let Some(meta) = block_info.meta else {
            anyhow::bail!("failed to get block meta");
        };

        Ok(GeyserMessage::Slot {
            leader,
            slot: meta.slot,
            hash: meta
                .blockhash
                .parse()
                .context("failed to parse block hash")?,
            time: meta
                .block_time
                .map(|ut| ut.timestamp)
                .ok_or(anyhow::anyhow!("failed to get block time"))?,
            height: meta
                .block_height
                .map(|bh| bh.block_height)
                .ok_or(anyhow::anyhow!("failed to get block height"))?,
            parent_slot: meta.parent_slot,
            parent_hash: meta
                .parent_blockhash
                .parse()
                .context("failed to parse parent block hash")?,
            transactions: Arc::new(block_info.transactions),
        })
    }
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
        .max_decoding_message_size(256 * 1024 * 1024) // 256MiB, BlockMeta with rewards can be bigger than 60MiB
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
            commitment: Some(CommitmentLevelProto::Processed as i32),
            accounts_data_slice: vec![],
            ping: None,
        })
        .await
        .context("failed to subscribe on geyser stream")?;

    let (tx, rx) = mpsc::unbounded_channel();
    let (schedule, schedule_rx) = SolanaSchedule::new(rpc_endpoint, saved_epochs);
    tokio::spawn(async move {
        let mut blocks = BTreeMap::<Slot, BlockInfo>::new();

        let mut alive = true;
        while alive {
            let msg = match parse_update_message(stream.next().await, &mut blocks, &schedule) {
                Ok(Some(msg)) => Ok(msg),
                Ok(None) => continue,
                Err(error) => Err(error),
            };

            alive = msg.is_ok();
            if tx.send(msg).is_err() {
                alive = false;
            }
        }
    });

    Ok((rx, schedule_rx))
}

fn parse_update_message(
    msg: Option<Result<SubscribeUpdate, Status>>,
    blocks: &mut BTreeMap<Slot, BlockInfo>,
    schedule: &SolanaSchedule,
) -> anyhow::Result<Option<GeyserMessage>> {
    let slot = match msg
        .map(|m| m.map(|m| m.update_oneof))
        .ok_or(anyhow::anyhow!("gRPC stream finished"))??
        .ok_or(anyhow::anyhow!("no update message"))?
    {
        UpdateOneof::Slot(info) => {
            let commitment = CommitmentLevelProto::try_from(info.status)
                .map_err(|_error| anyhow::anyhow!("failed to parse commitment level"))?
                .into();

            if commitment == CommitmentLevel::Finalized {
                loop {
                    match blocks.first_key_value() {
                        Some((block_info_slot, block_info)) if *block_info_slot < info.slot => {
                            if let Some(meta) = &block_info.meta {
                                error!(
                                    slot = block_info_slot,
                                    executed_transaction_count = meta.executed_transaction_count,
                                    transactions = block_info.transactions.len(),
                                    "failed to build block"
                                );
                                metrics::grpc_block_build_failed_inc();
                            }
                            blocks.pop_first();
                        }
                        _ => break,
                    }
                }
            }

            return Ok(Some(GeyserMessage::Status {
                slot: info.slot,
                commitment,
            }));
        }
        UpdateOneof::Transaction(info) => {
            let tx = info
                .transaction
                .ok_or(anyhow::anyhow!("failed to get transaction"))?;

            let is_vote = tx.is_vote;
            let TransactionWithStatusMeta::Complete(tx_with_meta) =
                convert_from::create_tx_with_meta(tx)
                    .map_err(|error| anyhow::anyhow!("failed to decode transaction: {error}"))?
            else {
                anyhow::bail!("failed to get transaction metadata");
            };

            blocks
                .entry(info.slot)
                .or_default()
                .transactions
                .push(GeyserTransaction::try_from((tx_with_meta, is_vote))?);
            info.slot
        }
        UpdateOneof::BlockMeta(info) => {
            let slot = info.slot;
            blocks.entry(slot).or_default().meta = Some(info);
            slot
        }
        UpdateOneof::Ping(_) => return Ok(None),
        _ => anyhow::bail!("received unexpected message"),
    };

    Ok(
        if let btree_map::Entry::Occupied(entry) = blocks.entry(slot) {
            let info = entry.get();
            if info.meta.as_ref().map(|m| m.executed_transaction_count)
                == Some(info.transactions.len() as u64)
            {
                let leader = schedule.get_leader(slot);
                Some(GeyserMessage::build_block(leader, entry.remove())?)
            } else {
                None
            }
        } else {
            None
        },
    )
}
