use {
    crate::grpc_geyser::{CommitmentLevel, GeyserMessage},
    hyper::body::Buf,
    jsonrpc_core::{
        Call as JsonrpcCall, Error as JsonrpcError, Failure as JsonrpcFailure, Id as JsonrpcId,
        Output as JsonrpcOutput, Response as JsonrpcResponse, Success as JsonrpcSuccess,
        Version as JsonrpcVersion,
    },
    serde::{Deserialize, Serialize},
    solana_rpc_client_api::{
        config::RpcContextConfig,
        custom_error::RpcCustomError,
        response::{Response as RpcResponse, RpcBlockhash, RpcResponseContext, RpcVersionInfo},
    },
    solana_sdk::{
        clock::{Slot, MAX_RECENT_BLOCKHASHES},
        hash::Hash,
    },
    std::{
        collections::BTreeMap,
        future::Future,
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::{
        sync::{broadcast, mpsc, oneshot},
        time::sleep,
    },
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolanaRpcMode {
    Solana,
    Triton,
    // Extended,
}

#[derive(Debug, Clone)]
pub struct SolanaRpc {
    request_calls_max: usize,
    request_timeout: Duration,
    geyser_tx: mpsc::UnboundedSender<Option<Arc<GeyserMessage>>>,
    requests_tx: mpsc::Sender<RpcRequests>,
}

impl SolanaRpc {
    pub fn new(
        request_calls_max: usize,
        request_timeout: Duration,
        request_queue_max: usize,
    ) -> (Self, impl Future<Output = anyhow::Result<()>>) {
        let (geyser_tx, geyser_rx) = mpsc::unbounded_channel();
        let (requests_tx, requests_rx) = mpsc::channel(request_queue_max);

        (
            Self {
                request_calls_max,
                request_timeout,
                geyser_tx,
                requests_tx,
            },
            Self::run_loop(geyser_rx, requests_rx),
        )
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        anyhow::ensure!(self.geyser_tx.send(None).is_ok(), "SolanaRpc loop is dead");
        Ok(())
    }

    pub fn push_geyser_message(&self, message: Arc<GeyserMessage>) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.geyser_tx.send(Some(message)).is_ok(),
            "SolanaRpc loop is dead"
        );
        Ok(())
    }

    pub async fn on_request(
        &self,
        mode: SolanaRpcMode,
        body: impl Buf,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> anyhow::Result<Vec<u8>> {
        #[derive(Debug, Deserialize)]
        #[serde(untagged)]
        enum JsonrpcCalls {
            Single(JsonrpcCall),
            Batch(Vec<JsonrpcCall>),
        }

        let (batched_calls, calls) = match serde_json::from_reader(body.reader())? {
            JsonrpcCalls::Single(call) => (false, vec![call]),
            JsonrpcCalls::Batch(calls) => (true, calls),
        };
        let calls_total = calls.len();
        anyhow::ensure!(
            calls_total <= self.request_calls_max,
            "exceed number of allowed calls in one request ({})",
            self.request_calls_max
        );

        let mut outputs = Vec::with_capacity(calls_total);
        let mut requests = Vec::with_capacity(calls_total);
        for call in calls {
            let call = match call {
                JsonrpcCall::MethodCall(call) => call,
                JsonrpcCall::Notification(notification) => {
                    outputs.push(Some(Self::create_failure(
                        notification.jsonrpc,
                        JsonrpcId::Null,
                        JsonrpcError::invalid_request(),
                    )));
                    continue;
                }
                JsonrpcCall::Invalid { id } => {
                    outputs.push(Some(Self::create_failure(
                        None,
                        id,
                        JsonrpcError::invalid_request(),
                    )));
                    continue;
                }
            };

            match call.method.as_str() {
                "getLatestBlockhash" => {
                    let parsed_params = match mode {
                        SolanaRpcMode::Solana => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                config: Option<RpcContextConfig>,
                            }

                            call.params.parse().map(|ReqParams { config }| {
                                let RpcContextConfig {
                                    commitment,
                                    min_context_slot,
                                } = config.unwrap_or_default();
                                (commitment, 0, min_context_slot)
                            })
                        }
                        SolanaRpcMode::Triton => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                config: Option<RpcLatestBlockhashConfig>,
                            }

                            call.params.parse().map(|ReqParams { config }| {
                                let RpcLatestBlockhashConfig { context, rollback } =
                                    config.unwrap_or_default();
                                (context.commitment, rollback, context.min_context_slot)
                            })
                        }
                    };

                    outputs.push(match parsed_params {
                        Ok((commitment, rollback, min_context_slot)) => {
                            requests.push(RpcRequest::GetLatestBlockhash {
                                id: call.id,
                                jsonrpc: call.jsonrpc,
                                commitment: commitment.unwrap_or_default().into(),
                                rollback,
                                min_context_slot,
                            });
                            None
                        }
                        Err(error) => Some(Self::create_failure(call.jsonrpc, call.id, error)),
                    });
                }
                "getSlot" => {
                    #[derive(Debug, Deserialize)]
                    struct ReqParams {
                        #[serde(default)]
                        config: Option<RpcContextConfig>,
                    }

                    outputs.push(
                        match call.params.parse().map(|ReqParams { config }| {
                            let RpcContextConfig {
                                commitment,
                                min_context_slot,
                            } = config.unwrap_or_default();
                            (commitment, min_context_slot)
                        }) {
                            Ok((commitment, min_context_slot)) => {
                                requests.push(RpcRequest::GetSlot {
                                    id: call.id,
                                    jsonrpc: call.jsonrpc,
                                    commitment: commitment.unwrap_or_default().into(),
                                    min_context_slot,
                                });
                                None
                            }
                            Err(error) => Some(Self::create_failure(call.jsonrpc, call.id, error)),
                        },
                    )
                }
                "getVersion" => {
                    outputs.push(Some(if let Err(error) = call.params.expect_no_params() {
                        Self::create_failure(call.jsonrpc, call.id, error)
                    } else {
                        let version = solana_version::Version::default();
                        Self::create_success(
                            call.jsonrpc,
                            call.id,
                            RpcVersionInfo {
                                solana_core: version.to_string(),
                                feature_set: Some(version.feature_set),
                            },
                        )
                    }));
                }
                _ => {
                    outputs.push(Some(Self::create_failure(
                        call.jsonrpc,
                        call.id,
                        JsonrpcError::method_not_found(),
                    )));
                }
            }
        }

        if !requests.is_empty() {
            let shutdown = Arc::new(AtomicBool::new(false));
            let (response_tx, response_rx) = oneshot::channel();

            match self.requests_tx.try_send(RpcRequests {
                requests,
                shutdown: Arc::clone(&shutdown),
                response_tx,
            }) {
                Ok(()) => {}
                Err(mpsc::error::TrySendError::Full(_)) => {
                    anyhow::bail!("requests queue is full");
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    anyhow::bail!("processed loop is closed");
                }
            }

            tokio::select! {
                value = shutdown_rx.recv() => match value {
                    Ok(()) => unreachable!(),
                    Err(broadcast::error::RecvError::Closed) => {
                        shutdown.store(true, Ordering::Relaxed);
                        anyhow::bail!("connection closed");
                    },
                    Err(broadcast::error::RecvError::Lagged(_)) => unreachable!(),
                },
                () = sleep(self.request_timeout) => {
                    shutdown.store(true, Ordering::Relaxed);
                    anyhow::bail!("request timeout");
                },
                maybe_outputs = response_rx => {
                    let mut index = 0;
                    for output in maybe_outputs.map_err(|_error| anyhow::anyhow!("request dropped"))? {
                        while index < outputs.len() && outputs[index].is_some() {
                            index += 1;
                        }
                        anyhow::ensure!(index < outputs.len(), "output index out of bounds");

                        outputs[index] = Some(output);
                    }
                }
            }
        }

        anyhow::ensure!(calls_total == outputs.len(), "invalid number of outputs");
        match outputs
            .into_iter()
            .map(|output| output.ok_or(()))
            .collect::<Result<Vec<JsonrpcOutput>, ()>>()
        {
            Ok(mut outputs) => serde_json::to_vec(&if batched_calls {
                JsonrpcResponse::Batch(outputs)
            } else if let Some(output) = outputs.pop() {
                JsonrpcResponse::Single(output)
            } else {
                anyhow::bail!("output is not defined")
            })
            .map_err(Into::into),
            Err(()) => {
                anyhow::bail!("not all outputs created")
            }
        }
        .map(|mut body| {
            body.push(b'\n');
            body
        })
    }

    fn create_success<R>(jsonrpc: Option<JsonrpcVersion>, id: JsonrpcId, result: R) -> JsonrpcOutput
    where
        R: Serialize,
    {
        JsonrpcOutput::Success(JsonrpcSuccess {
            jsonrpc,
            result: serde_json::to_value(result).expect("failed to serialize"),
            id,
        })
    }

    const fn create_failure(
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        error: JsonrpcError,
    ) -> JsonrpcOutput {
        JsonrpcOutput::Failure(JsonrpcFailure { jsonrpc, error, id })
    }

    async fn run_loop(
        mut geyser_rx: mpsc::UnboundedReceiver<Option<Arc<GeyserMessage>>>,
        mut requests_rx: mpsc::Receiver<RpcRequests>,
    ) -> anyhow::Result<()> {
        let mut latest_blockhash_storage = LatestBlockhashStorage::default();

        loop {
            tokio::select! {
                biased;

                maybe_message = geyser_rx.recv() => match maybe_message {
                    Some(Some(message)) => match message.as_ref() {
                        GeyserMessage::Slot { slot, commitment } => {
                            latest_blockhash_storage.update_commitment(*slot, *commitment);
                        }
                        GeyserMessage::Block {
                            slot,
                            hash,
                            // time,
                            height,
                            // parent_slot,
                            // parent_hash,
                            // transactions,
                            ..
                        } => {
                            latest_blockhash_storage.push_block(*slot, *height, *hash);
                        }
                    }
                    _ => break,
                },

                maybe_rpc_requests = requests_rx.recv() => match maybe_rpc_requests {
                    Some(rpc_requests) => {
                        if rpc_requests.shutdown.load(Ordering::Relaxed) {
                            continue;
                        }

                        let outputs = rpc_requests
                            .requests
                            .into_iter()
                            .map(|request| {
                                match request {
                                    RpcRequest::GetLatestBlockhash { id, jsonrpc, commitment, rollback, min_context_slot } => {
                                        if rollback > MAX_RECENT_BLOCKHASHES {
                                            return Self::create_failure(jsonrpc, id, JsonrpcError::invalid_params("rollback exceeds 300"));
                                        }

                                        let mut slot = match commitment {
                                            CommitmentLevel::Processed => latest_blockhash_storage.slot_processed,
                                            CommitmentLevel::Confirmed => latest_blockhash_storage.slot_confirmed,
                                            CommitmentLevel::Finalized => latest_blockhash_storage.slot_finalized,
                                        };

                                        if let Some(min_context_slot) = min_context_slot {
                                            if slot < min_context_slot {
                                                let error = RpcCustomError::MinContextSlotNotReached { context_slot: slot }.into();
                                                return Self::create_failure(jsonrpc, id, error);
                                            }
                                        }

                                        let Some(mut value) = latest_blockhash_storage.slots.get(&slot) else {
                                            return Self::create_failure(jsonrpc, id, JsonrpcError::internal_error());
                                        };
                                        for _ in 0..rollback {
                                            loop {
                                                slot -= 1;
                                                match latest_blockhash_storage.slots.get(&slot) {
                                                    Some(prev_value) => {
                                                        if prev_value.commitment == commitment {
                                                            value = prev_value;
                                                            break;
                                                        }
                                                    }
                                                    _ => return Self::create_failure(jsonrpc, id, JsonrpcError::invalid_params("failed to rollback block")),
                                                }
                                            }
                                        }

                                        Self::create_success(jsonrpc, id, RpcResponse {
                                            context: RpcResponseContext::new(slot),
                                            value: RpcBlockhash {
                                                blockhash: value.hash.to_string(),
                                                last_valid_block_height: value.height + MAX_RECENT_BLOCKHASHES as u64,
                                            },
                                        })
                                    }
                                    RpcRequest::GetSlot { id, jsonrpc, commitment, min_context_slot } => {
                                        let slot = match commitment {
                                            CommitmentLevel::Processed => latest_blockhash_storage.slot_processed,
                                            CommitmentLevel::Confirmed => latest_blockhash_storage.slot_confirmed,
                                            CommitmentLevel::Finalized => latest_blockhash_storage.slot_finalized,
                                        };

                                        if let Some(min_context_slot) = min_context_slot {
                                            if slot < min_context_slot {
                                                let error = RpcCustomError::MinContextSlotNotReached { context_slot: slot }.into();
                                                return Self::create_failure(jsonrpc, id, error);
                                            }
                                        }

                                        Self::create_success(jsonrpc, id, slot)
                                    }
                                }
                            })
                            .collect::<Vec<JsonrpcOutput>>();

                        let _ = rpc_requests.response_tx.send(outputs);
                    },
                    None => break,
                },
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcLatestBlockhashConfig {
    #[serde(flatten)]
    pub context: RpcContextConfig,
    #[serde(default)]
    pub rollback: usize,
}

#[derive(Debug)]
struct RpcRequests {
    requests: Vec<RpcRequest>,
    shutdown: Arc<AtomicBool>,
    response_tx: oneshot::Sender<Vec<JsonrpcOutput>>,
}

#[derive(Debug)]
enum RpcRequest {
    GetLatestBlockhash {
        id: JsonrpcId,
        jsonrpc: Option<JsonrpcVersion>,
        commitment: CommitmentLevel,
        rollback: usize,
        min_context_slot: Option<Slot>,
    },
    GetSlot {
        id: JsonrpcId,
        jsonrpc: Option<JsonrpcVersion>,
        commitment: CommitmentLevel,
        min_context_slot: Option<Slot>,
    },
}

#[derive(Debug, Default)]
struct LatestBlockhashStorage {
    slots: BTreeMap<Slot, LatestBlockhashSlot>,
    finalized_total: usize,
    slot_processed: Slot,
    slot_confirmed: Slot,
    slot_finalized: Slot,
}

impl LatestBlockhashStorage {
    fn push_block(&mut self, slot: Slot, height: Slot, hash: Hash) {
        self.slots.insert(
            slot,
            LatestBlockhashSlot {
                hash,
                height,
                commitment: CommitmentLevel::Processed,
            },
        );
    }

    fn update_commitment(&mut self, slot: Slot, commitment: CommitmentLevel) {
        if let Some(value) = self.slots.get_mut(&slot) {
            value.commitment = commitment;

            if commitment == CommitmentLevel::Processed && slot > self.slot_processed {
                self.slot_processed = slot;
            } else if commitment == CommitmentLevel::Confirmed {
                self.slot_confirmed = slot;
            } else if commitment == CommitmentLevel::Finalized {
                self.finalized_total += 1;
                self.slot_finalized = slot;
            }
        }

        while self.finalized_total > MAX_RECENT_BLOCKHASHES + 10 {
            if let Some((_slot, value)) = self.slots.pop_first() {
                if value.commitment == CommitmentLevel::Finalized {
                    self.finalized_total -= 1;
                }
            }
        }
    }
}

#[derive(Debug)]
struct LatestBlockhashSlot {
    hash: Hash,
    height: Slot,
    commitment: CommitmentLevel,
}
