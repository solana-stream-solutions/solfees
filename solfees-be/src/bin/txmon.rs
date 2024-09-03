use {
    anyhow::Context,
    borsh::de::BorshDeserialize,
    clap::Parser,
    futures::{sink::SinkExt, stream::StreamExt},
    maplit::hashmap,
    solana_sdk::{
        compute_budget::{self, ComputeBudgetInstruction},
        ed25519_program,
        message::AccountKeys,
        secp256k1_program,
    },
    solana_transaction_status::{TransactionWithStatusMeta, VersionedTransactionWithStatusMeta},
    std::collections::HashMap,
    tonic::transport::channel::ClientTlsConfig,
    tracing::error,
    yellowstone_grpc_client::GeyserGrpcClient,
    yellowstone_grpc_proto::prelude::{
        subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
        SubscribeRequestFilterBlocksMeta, SubscribeRequestFilterTransactions,
    },
};

#[derive(Debug, Clone, Parser)]
#[clap(author, version, about)]
struct Args {
    /// Service endpoint
    #[clap(short, long, default_value_t = String::from("http://127.0.0.1:10000"))]
    endpoint: String,

    /// Access token
    #[clap(long)]
    x_token: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    solfees_be::tracing_init(false)?;

    let args = Args::parse();

    let mut client = GeyserGrpcClient::build_from_shared(args.endpoint)?
        .x_token(args.x_token)?
        .tls_config(ClientTlsConfig::new().with_native_roots())?
        .connect()
        .await?;
    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    subscribe_tx
        .send(SubscribeRequest {
            slots: HashMap::new(),
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
            commitment: Some(CommitmentLevel::Processed as i32),
            accounts_data_slice: vec![],
            ping: None,
        })
        .await
        .context("failed to subscribe")?;

    while let Some(message) = stream.next().await {
        match message.context("stream error")?.update_oneof {
            Some(UpdateOneof::Transaction(info)) => {
                let tx = match info.transaction {
                    Some(tx) => tx,
                    None => {
                        error!("failed to get transaction from message");
                        continue;
                    }
                };

                // if tx.is_vote {
                //     continue;
                // }

                let (tx, meta) = match yellowstone_grpc_proto::convert_from::create_tx_with_meta(tx)
                {
                    Ok(TransactionWithStatusMeta::Complete(
                        VersionedTransactionWithStatusMeta { transaction, meta },
                    )) => (transaction, meta),
                    Ok(TransactionWithStatusMeta::MissingMetadata(_)) => {
                        error!("failed to get transaction metadata");
                        continue;
                    }
                    Err(error) => {
                        error!("failed to create transaction: {error}");
                        continue;
                    }
                };

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
                let mut sigs_count = tx.signatures.len();
                for ix in tx.message.instructions() {
                    if let Some(pubkey) = account_keys.get(ix.program_id_index as usize) {
                        match *pubkey {
                            compute_budget::ID => {
                                // We do not use `ComputeBudgetInstruction::try_from_slice(&ix.data)` because data can be longer than expected
                                match ComputeBudgetInstruction::deserialize(&mut ix.data.as_slice())
                                {
                                    Ok(ComputeBudgetInstruction::SetComputeUnitLimit(value)) => {
                                        unit_limit = Some(1_400_000.min(value));
                                    }
                                    Ok(ComputeBudgetInstruction::SetComputeUnitPrice(value)) => {
                                        unit_price = value;
                                    }
                                    _ => {}
                                }
                                continue;
                            }
                            ed25519_program::ID => {
                                sigs_count += ix.data.first().copied().unwrap_or(0) as usize;
                            }
                            secp256k1_program::ID => {
                                sigs_count += ix.data.first().copied().unwrap_or(0) as usize;
                            }
                            _ => {}
                        }
                    }
                    ixs_count += 1;
                }
                let unit_limit = unit_limit.unwrap_or_else(|| 1_400_000.min(ixs_count * 200_000));

                let extra_fee = unit_limit as u64 * unit_price;
                let expected_fee = extra_fee / 1_000_000
                    + if extra_fee % 1_000_000 > 0 { 1 } else { 0 }
                    + 5_000 * sigs_count as u64;

                if expected_fee != meta.fee {
                    error!(
                        fee = meta.fee,
                        expected_fee,
                        sigs_count,
                        unit_limit,
                        unit_price,
                        units_consumed = ?meta.compute_units_consumed,
                        "{}",
                        tx.signatures[0]
                    );
                }
            }
            Some(UpdateOneof::BlockMeta(info)) => {
                tracing::debug!("block meta received {}", info.slot);
            }
            _ => {}
        }
    }

    Ok(())
}
