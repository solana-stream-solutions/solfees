use {
    crate::grpc_geyser::{CommitmentLevel, GeyserMessage},
    hyper::body::Buf,
    jsonrpc_core::{
        Call as RpcCall, Error as RpcError, Failure as RpcFailure, Id as RpcId,
        MethodCall as RpcMethodCall, Output as RpcOutput, Response as RpcResponse,
        Success as RpcSuccess, Version as RpcVersion,
    },
    serde::{Deserialize, Serialize},
    solana_rpc_client_api::response::RpcVersionInfo,
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
            Self::run_loop(geyser_rx),
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
        enum RpcCalls {
            Single(RpcCall),
            Batch(Vec<RpcCall>),
        }

        let (batched_calls, calls) = match serde_json::from_reader(body.reader())? {
            RpcCalls::Single(call) => (false, vec![call]),
            RpcCalls::Batch(calls) => (true, calls),
        };
        let calls_total = calls.len();
        anyhow::ensure!(
            calls_total <= self.request_calls_max,
            "exceed number of allowed calls in one request ({})",
            self.request_calls_max
        );

        let mut outputs: Vec<Option<RpcOutput>> = vec![];
        let mut requests = Vec::with_capacity(calls_total);
        for call in calls {
            let call = match call {
                RpcCall::MethodCall(call) => call,
                RpcCall::Notification(_) => {
                    outputs.push(Some(Self::create_failure(
                        RpcError::invalid_request(),
                        RpcId::Null,
                    )));
                    continue;
                }
                RpcCall::Invalid { id } => {
                    outputs.push(Some(Self::create_failure(RpcError::invalid_request(), id)));
                    continue;
                }
            };

            match call.method.as_str() {
                "getVersion" => {
                    outputs.push(Some(if let Err(error) = call.params.expect_no_params() {
                        Self::create_failure(error, call.id)
                    } else {
                        let version = solana_version::Version::default();
                        Self::create_success(
                            call.jsonrpc,
                            call.id,
                            RpcVersionInfo {
                                solana_core: version.to_string(),
                                feature_set: Some(version.feature_set),
                            },
                        )?
                    }));
                }
                _ => {
                    outputs.push(Some(Self::create_failure(
                        RpcError::method_not_found(),
                        call.id,
                    )));
                }
            }
        }

        if !requests.is_empty() {
            let shutdown = Arc::new(AtomicBool::new(false));
            let (response_tx, response_rx) = oneshot::channel();

            match self.requests_tx.try_send(RpcRequests {
                mode,
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
                value = response_rx => {
                    //
                    todo!()
                }
            }
        }

        anyhow::ensure!(calls_total == outputs.len(), "invalid number of outputs");
        match outputs
            .into_iter()
            .map(|output| output.ok_or(()))
            .collect::<Result<Vec<RpcOutput>, ()>>()
        {
            Ok(mut outputs) => serde_json::to_vec(&if batched_calls {
                RpcResponse::Batch(outputs)
            } else if let Some(output) = outputs.pop() {
                RpcResponse::Single(output)
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

    fn create_success<R>(
        jsonrpc: Option<RpcVersion>,
        id: RpcId,
        result: R,
    ) -> anyhow::Result<RpcOutput>
    where
        R: Serialize,
    {
        Ok(RpcOutput::Success(RpcSuccess {
            jsonrpc,
            result: serde_json::to_value(result)?,
            id,
        }))
    }

    const fn create_failure(error: RpcError, id: RpcId) -> RpcOutput {
        RpcOutput::Failure(RpcFailure {
            jsonrpc: Some(RpcVersion::V2),
            error,
            id,
        })
    }

    async fn run_loop(
        mut rx: mpsc::UnboundedReceiver<Option<Arc<GeyserMessage>>>,
    ) -> anyhow::Result<()> {
        let mut latest_blockhash_storage = LatestBlockhashStorage::default();

        while let Some(Some(message)) = rx.recv().await {
            match message.as_ref() {
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
        }
        Ok(())
    }
}

#[derive(Debug)]
struct RpcRequests {
    mode: SolanaRpcMode,
    requests: Vec<RpcRequest>,
    shutdown: Arc<AtomicBool>,
    response_tx: oneshot::Sender<Vec<RpcOutput>>,
}

#[derive(Debug)]
enum RpcRequest {
    // GetVersion { call: RpcMethodCall },
}

#[derive(Debug, Default)]
struct LatestBlockhashStorage {
    slots: BTreeMap<Slot, LatestBlockhashSlot>,
    finalized_total: usize,
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
            if commitment == CommitmentLevel::Finalized {
                self.finalized_total += 1;
            }
        }

        while self.finalized_total > MAX_RECENT_BLOCKHASHES + 10 {
            let _ = self.slots.pop_first();
        }
    }
}

#[derive(Debug)]
struct LatestBlockhashSlot {
    hash: Hash,
    height: Slot,
    commitment: CommitmentLevel,
}
