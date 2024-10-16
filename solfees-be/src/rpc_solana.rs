use {
    crate::{
        grpc_geyser::{CommitmentLevel, GeyserMessage, GeyserTransaction},
        metrics::solfees_be as metrics,
        redis::RedisMessage,
    },
    futures::{
        future::{pending, FutureExt},
        sink::SinkExt,
        stream::StreamExt,
    },
    hyper::body::Buf,
    hyper_tungstenite::HyperWebsocket,
    jsonrpc_core::{
        Call as JsonrpcCall, Error as JsonrpcError, Failure as JsonrpcFailure, Id as JsonrpcId,
        MethodCall as JsonrpcMethodCall, Output as JsonrpcOutput, Response as JsonrpcResponse,
        Success as JsonrpcSuccess, Version as JsonrpcVersion,
    },
    serde::{Deserialize, Serialize},
    solana_rpc_client_api::{
        config::{RpcContextConfig, RpcLeaderScheduleConfig, RpcLeaderScheduleConfigWrapper},
        custom_error::RpcCustomError,
        response::{
            Response as RpcResponse, RpcBlockhash, RpcPrioritizationFee, RpcResponseContext,
            RpcVersionInfo,
        },
    },
    solana_sdk::{
        clock::{Epoch, Slot, UnixTimestamp, MAX_RECENT_BLOCKHASHES},
        epoch_schedule::EpochSchedule,
        hash::Hash,
        pubkey::Pubkey,
        transaction::MAX_TX_ACCOUNT_LOCKS,
    },
    std::{
        borrow::Cow,
        collections::{BTreeMap, HashMap},
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
    tokio_tungstenite::tungstenite::protocol::{
        frame::{coding::CloseCode as WebSocketCloseCode, CloseFrame as WebSocketCloseFrame},
        Message as WebSocketMessage,
    },
    tracing::{debug, info},
};

const MAX_NUM_RECENT_SLOT_INFO: usize = 150;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolanaRpcMode {
    Solana,
    Triton,
    Solfees,
    SolfeesFrontend,
}

#[derive(Debug, Clone)]
pub struct SolanaRpc {
    request_calls_max: usize,
    request_timeout: Duration,
    redis_tx: mpsc::UnboundedSender<Option<RedisMessage>>,
    requests_tx: mpsc::Sender<RpcRequests>,
    streams_tx: broadcast::Sender<Arc<StreamsUpdateMessage>>,
}

impl SolanaRpc {
    pub fn new(
        request_calls_max: usize,
        request_timeout: Duration,
        request_queue_max: usize,
        streams_channel_capacity: usize,
    ) -> (Self, impl Future<Output = anyhow::Result<()>>) {
        let (redis_tx, redis_rx) = mpsc::unbounded_channel();
        let (streams_tx, _streams_rx) = broadcast::channel(streams_channel_capacity);
        let (requests_tx, requests_rx) = mpsc::channel(request_queue_max);

        (
            Self {
                request_calls_max,
                request_timeout,
                redis_tx,
                requests_tx,
                streams_tx: streams_tx.clone(),
            },
            Self::run_update_loop(redis_rx, streams_tx, requests_rx),
        )
    }

    pub fn shutdown(&self) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.redis_tx.send(None).is_ok(),
            "SolanaRpc update loop is dead"
        );
        Ok(())
    }

    pub fn push_redis_message(&self, message: RedisMessage) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.redis_tx.send(Some(message)).is_ok(),
            "SolanaRpc update loop is dead"
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
                "getLatestBlockhash" if mode != SolanaRpcMode::SolfeesFrontend => {
                    metrics::requests_call_inc(mode, RpcRequestType::LatestBlockhash);

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
                        SolanaRpcMode::Triton | SolanaRpcMode::Solfees => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                config: Option<RpcLatestBlockhashConfigTriton>,
                            }

                            call.params.parse().map(|ReqParams { config }| {
                                let RpcLatestBlockhashConfigTriton { context, rollback } =
                                    config.unwrap_or_default();
                                (context.commitment, rollback, context.min_context_slot)
                            })
                        }
                        SolanaRpcMode::SolfeesFrontend => unreachable!(),
                    };

                    outputs.push(match parsed_params {
                        Ok((commitment, rollback, min_context_slot)) => {
                            requests.push(RpcRequest::LatestBlockhash {
                                jsonrpc: call.jsonrpc,
                                id: call.id,
                                commitment: commitment.unwrap_or_default().into(),
                                rollback,
                                min_context_slot,
                            });
                            None
                        }
                        Err(error) => Some(Self::create_failure(call.jsonrpc, call.id, error)),
                    });
                }
                "getLeaderSchedule" => {
                    metrics::requests_call_inc(mode, RpcRequestType::LeaderSchedule);

                    let maybe_request = match mode {
                        SolanaRpcMode::Solana | SolanaRpcMode::Triton | SolanaRpcMode::Solfees => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                options: Option<RpcLeaderScheduleConfigWrapper>,
                                #[serde(default)]
                                config: Option<RpcLeaderScheduleConfig>,
                            }

                            call.params
                                .parse()
                                .and_then(|ReqParams { options, config }| {
                                    let (slot, maybe_config) =
                                        options.map(|options| options.unzip()).unwrap_or_default();
                                    let config = maybe_config.or(config).unwrap_or_default();

                                    if let Some(identity) = &config.identity {
                                        let _ = verify_pubkey(identity)?;
                                    }

                                    Ok(RpcRequest::LeaderSchedule {
                                        jsonrpc: call.jsonrpc,
                                        id: call.id.clone(),
                                        slot,
                                        epoch: None,
                                        commitment: config.commitment.unwrap_or_default().into(),
                                        identity: config.identity,
                                    })
                                })
                        }
                        SolanaRpcMode::SolfeesFrontend => {
                            #[derive(Debug, Deserialize)]
                            struct ConfigEpoch {
                                epoch: Epoch,
                            }

                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                config: ConfigEpoch,
                            }

                            call.params.parse().map(
                                |ReqParams {
                                     config: ConfigEpoch { epoch },
                                 }| {
                                    RpcRequest::LeaderSchedule {
                                        jsonrpc: call.jsonrpc,
                                        id: call.id.clone(),
                                        slot: None,
                                        epoch: Some(epoch),
                                        commitment: CommitmentLevel::default(),
                                        identity: None,
                                    }
                                },
                            )
                        }
                    };

                    outputs.push(match maybe_request {
                        Ok(request) => {
                            requests.push(request);
                            None
                        }
                        Err(error) => {
                            Some(Self::create_failure(call.jsonrpc, call.id.clone(), error))
                        }
                    });
                }
                "getRecentPrioritizationFees" => {
                    metrics::requests_call_inc(mode, RpcRequestType::RecentPrioritizationFees);

                    let maybe_parsed_params = match mode {
                        SolanaRpcMode::Solana => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                pubkey_strs: Option<Vec<String>>,
                            }

                            Some(call.params.parse().and_then(|ReqParams { pubkey_strs }| {
                                Ok((verify_pubkeys(pubkey_strs)?, None))
                            }))
                        }
                        SolanaRpcMode::Triton => {
                            #[derive(Debug, Deserialize)]
                            struct ReqParams {
                                #[serde(default)]
                                pubkey_strs: Option<Vec<String>>,
                                #[serde(default)]
                                config: Option<RpcRecentPrioritizationFeesConfigTriton>,
                            }

                            Some(call.params.parse().and_then(
                                |ReqParams {
                                     pubkey_strs,
                                     config,
                                 }| {
                                    let pubkeys = verify_pubkeys(pubkey_strs)?;

                                    let RpcRecentPrioritizationFeesConfigTriton { percentile } =
                                        config.unwrap_or_default();
                                    if let Some(percentile) = percentile {
                                        if percentile > 10_000 {
                                            return Err(JsonrpcError::invalid_params(
                                                "Percentile is too big; max value is 10000"
                                                    .to_owned(),
                                            ));
                                        }
                                    }

                                    Ok((pubkeys, percentile))
                                },
                            ))
                        }
                        SolanaRpcMode::Solfees | SolanaRpcMode::SolfeesFrontend => {
                            outputs.push(
                                match call.params.parse().and_then(
                                    |ReqParamsSlotsSubscribe { config }| {
                                        config.unwrap_or_default().try_into()
                                    },
                                ) {
                                    Ok(filter) => {
                                        requests.push(RpcRequest::SolfeesSlots {
                                            jsonrpc: call.jsonrpc,
                                            id: call.id.clone(),
                                            filter,
                                            frontend: mode == SolanaRpcMode::SolfeesFrontend,
                                        });
                                        None
                                    }
                                    Err(error) => Some(Self::create_failure(
                                        call.jsonrpc,
                                        call.id.clone(),
                                        error,
                                    )),
                                },
                            );

                            None
                        }
                    };

                    if let Some(parsed_params) = maybe_parsed_params {
                        outputs.push(match parsed_params {
                            Ok((pubkeys, percentile)) => {
                                requests.push(RpcRequest::RecentPrioritizationFees {
                                    jsonrpc: call.jsonrpc,
                                    id: call.id,
                                    pubkeys,
                                    percentile,
                                });
                                None
                            }
                            Err(error) => Some(Self::create_failure(call.jsonrpc, call.id, error)),
                        })
                    }
                }
                "getSlot" if mode != SolanaRpcMode::SolfeesFrontend => {
                    metrics::requests_call_inc(mode, RpcRequestType::Slot);

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
                                requests.push(RpcRequest::Slot {
                                    jsonrpc: call.jsonrpc,
                                    id: call.id,
                                    commitment: commitment.unwrap_or_default().into(),
                                    min_context_slot,
                                });
                                None
                            }
                            Err(error) => Some(Self::create_failure(call.jsonrpc, call.id, error)),
                        },
                    )
                }
                "getVersion" if mode != SolanaRpcMode::SolfeesFrontend => {
                    metrics::requests_call_inc(mode, RpcRequestType::Version);

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

    pub async fn on_websocket(self, mode: SolanaRpcMode, websocket: HyperWebsocket) {
        let ws_frontend = match mode {
            SolanaRpcMode::Solfees => false,
            SolanaRpcMode::SolfeesFrontend => true,
            _ => return,
        };

        let (mut websocket_tx, mut websocket_rx) = match websocket.await {
            Ok(websocket) => websocket.split(),
            Err(error) => {
                debug!(%error, "failed to update HyperWebsocket");
                return;
            }
        };
        metrics::websockets_inc(mode);

        let mut updates_rx = self.streams_tx.subscribe();
        let mut filter = None;
        let mut websocket_tx_message = None;
        let mut flush_required = false;

        let loop_close_reason = loop {
            if let Some(message) = websocket_tx_message.take() {
                if websocket_tx.feed(message).await.is_err() {
                    break None;
                }
                flush_required = true;
            }

            let websocket_tx_flush = if flush_required {
                websocket_tx.flush().boxed()
            } else {
                pending().boxed()
            };

            tokio::select! {
                flush_result = websocket_tx_flush => match flush_result {
                    Ok(()) => {
                        flush_required = false;
                        continue;
                    }
                    Err(_) => {
                        break None;
                    }
                },

                maybe_message = websocket_rx.next() => {
                    let message = match maybe_message {
                        Some(Ok(WebSocketMessage::Text(message))) => message,
                        Some(Ok(WebSocketMessage::Binary(msg))) => {
                            match String::from_utf8(msg) {
                                Ok(message) => message,
                                Err(_error) => break Some(Some("received invalid binary message")),
                            }
                        }
                        Some(Ok(WebSocketMessage::Ping(data))) => {
                            websocket_tx_message = Some(WebSocketMessage::Pong(data));
                            continue
                        }
                        Some(Ok(WebSocketMessage::Pong(_))) => continue,
                        Some(Ok(WebSocketMessage::Close(_))) => break None,
                        Some(Ok(_)) => break Some(Some("received unsupported message")),
                        Some(Err(_error)) => break None,
                        None => break None,
                    };

                    let Ok(call) = serde_json::from_str::<JsonrpcMethodCall>(&message) else {
                        break Some(Some("received invalid message"));
                    };

                    match call.method.as_str() {
                        "SlotsSubscribe" => {
                            let output = match call.params.parse().and_then(|config: ReqParamsSlotsSubscribeConfig| {
                                SlotSubscribeFilter::try_from(config)
                            }) {
                                Ok(filter_new) => {
                                    filter = Some((call.id.clone(), filter_new));
                                    Self::create_success(call.jsonrpc, call.id, "subscribed")
                                },
                                Err(error) => Self::create_failure(call.jsonrpc, call.id, error),
                            };
                            websocket_tx_message = Some(WebSocketMessage::Text(serde_json::to_string(&output).expect("failed to serialize")));
                        },
                        _ => break Some(Some("unknown subscription method")),
                    }
                },

                maybe_update = updates_rx.recv() => match maybe_update {
                    Ok(update) => if let Some((id, filter)) = filter.as_ref() {
                        let output = match update.as_ref() {
                            StreamsUpdateMessage::Status { slot, commitment } => {
                                SlotsSubscribeOutput::Status {
                                    slot: *slot,
                                    commitment: *commitment
                                }
                            },
                            StreamsUpdateMessage::Slot { info } => info.get_filtered(filter),
                        };
                        let message = if ws_frontend {
                            Self::create_success(None, id.clone(), output)
                        } else {
                            Self::create_success(None, id.clone(), SlotsSubscribeOutputSolana::from(output))
                        };
                        websocket_tx_message = Some(WebSocketMessage::Text(serde_json::to_string(&message).expect("failed to serialize")));
                    }
                    Err(broadcast::error::RecvError::Closed) => break Some(None),
                    Err(broadcast::error::RecvError::Lagged(_)) => break Some(Some("subscription lagged")),
                },
            }
        };

        if let Some(maybe_close_reason) = loop_close_reason {
            let maybe_close_frame = maybe_close_reason.map(|reason| WebSocketCloseFrame {
                code: WebSocketCloseCode::Error,
                reason: Cow::Borrowed(reason),
            });
            let _ = websocket_tx
                .feed(WebSocketMessage::Close(maybe_close_frame))
                .await;
        }
        let _ = websocket_tx.flush().await;

        metrics::websockets_dec(mode);
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

    fn internal_error_with_data<R>(data: R) -> JsonrpcError
    where
        R: Serialize,
    {
        let mut error = JsonrpcError::internal_error();
        error.data = Some(serde_json::to_value(data).expect("failed to serialize"));
        error
    }

    async fn run_update_loop(
        mut redis_rx: mpsc::UnboundedReceiver<Option<RedisMessage>>,
        streams_tx: broadcast::Sender<Arc<StreamsUpdateMessage>>,
        mut requests_rx: mpsc::Receiver<RpcRequests>,
    ) -> anyhow::Result<()> {
        let mut latest_blockhash_storage = LatestBlockhashStorage::default();
        let mut slots_info = BTreeMap::<Slot, StreamsSlotInfo>::new();

        let epoch_schedule = EpochSchedule::custom(432_000, 432_000, false);
        let mut leader_schedule_map = HashMap::new();
        let mut leader_schedule_rpc_map = HashMap::new();

        loop {
            tokio::select! {
                biased;

                maybe_message = redis_rx.recv() => match maybe_message {
                    Some(Some(RedisMessage::Geyser(message))) => match message {
                        GeyserMessage::Status { slot, commitment } => {
                            latest_blockhash_storage.update_commitment(slot, commitment);

                            if let Some(info) = slots_info.get_mut(&slot) {
                                info.commitment = commitment;
                            }

                            let _ = streams_tx.send(Arc::new(StreamsUpdateMessage::Status { slot, commitment }));

                            metrics::set_slot(commitment, slot);
                        }
                        GeyserMessage::Slot {
                            leader,
                            slot,
                            hash,
                            time,
                            height,
                            parent_slot: _parent_slot,
                            parent_hash: _parent_hash,
                            transactions,
                        } => {
                            latest_blockhash_storage.push_block(slot, height, hash);

                            let info = StreamsSlotInfo::new(leader, slot, hash, time, height, transactions);
                            slots_info.insert(slot, info.clone());
                            while slots_info.len() > MAX_NUM_RECENT_SLOT_INFO {
                                slots_info.pop_first();
                            }

                            let _ = streams_tx.send(Arc::new(StreamsUpdateMessage::Slot { info }));
                        }
                    }
                    Some(Some(RedisMessage::Epoch { epoch, leader_schedule, leader_schedule_rpc })) => {
                        info!(epoch, "epoch received");
                        leader_schedule_map.insert(epoch, leader_schedule);
                        leader_schedule_rpc_map.insert(epoch, leader_schedule_rpc);
                        continue;
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
                                    RpcRequest::LatestBlockhash { jsonrpc, id, commitment, rollback, min_context_slot } => {
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
                                            return Self::create_failure(jsonrpc, id, SolanaRpc::internal_error_with_data("no slot"));
                                        };
                                        if rollback > 0 {
                                            let Some(first_available_slot) = latest_blockhash_storage.slots.keys().next().copied() else {
                                                return Self::create_failure(jsonrpc, id, JsonrpcError::invalid_params("empty slots storage"));
                                            };

                                            for _ in 0..rollback {
                                                loop {
                                                    slot -= 1;

                                                    if let Some(prev_value) = latest_blockhash_storage.slots.get(&slot) {
                                                        if prev_value.height + 1 == value.height {
                                                            value = prev_value;
                                                            break;
                                                        }
                                                    } else if slot < first_available_slot {
                                                        return Self::create_failure(jsonrpc, id, JsonrpcError::invalid_params("not enought slots in the storage"));
                                                    }
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
                                    RpcRequest::LeaderSchedule { jsonrpc, id, slot, epoch, commitment, identity } => {
                                        if let Some(epoch) = epoch {
                                            Self::create_success(jsonrpc, id, leader_schedule_map.get(&epoch))
                                        } else {
                                            let slot = slot.unwrap_or({
                                                match commitment {
                                                    CommitmentLevel::Processed => latest_blockhash_storage.slot_processed,
                                                    CommitmentLevel::Confirmed => latest_blockhash_storage.slot_confirmed,
                                                    CommitmentLevel::Finalized => latest_blockhash_storage.slot_finalized,
                                                }
                                            });
                                            let epoch = epoch_schedule.get_epoch(slot);

                                            if let Some(identity) = identity {
                                                let mut map = HashMap::new();
                                                if let Some(slots) = leader_schedule_rpc_map.get(&epoch).and_then(|m| m.get(&identity)) {
                                                    map.insert(identity, slots);
                                                }
                                                Self::create_success(jsonrpc, id, Some(&map))
                                            } else {
                                                Self::create_success(jsonrpc, id, leader_schedule_rpc_map.get(&epoch))
                                            }
                                        }
                                    }
                                    RpcRequest::RecentPrioritizationFees { jsonrpc, id, pubkeys, percentile } => {
                                        let result = slots_info
                                            .iter()
                                            .map(|(slot, value)| RpcPrioritizationFee {
                                                slot: *slot,
                                                prioritization_fee: value.fees.get_fee(&pubkeys, percentile),
                                            })
                                            .collect::<Vec<_>>();

                                        Self::create_success(jsonrpc, id, result)
                                    }
                                    RpcRequest::Slot { jsonrpc, id, commitment, min_context_slot } => {
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
                                    RpcRequest::SolfeesSlots { jsonrpc, id, filter, frontend } => {
                                        let outputs = slots_info.values().map(|info| info.get_filtered(&filter));
                                        if frontend {
                                            Self::create_success(jsonrpc, id, outputs.collect::<Vec<_>>())
                                        } else {
                                            match outputs
                                                .map(SolfeesPrioritizationFee::try_from)
                                                .collect::<Result<Vec<_>, JsonrpcError>>()
                                            {
                                                Ok(outputs) => Self::create_success(jsonrpc, id, outputs),
                                                Err(error) => Self::create_failure(jsonrpc, id, error),
                                            }
                                        }
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
pub struct RpcLatestBlockhashConfigTriton {
    #[serde(flatten)]
    pub context: RpcContextConfig,
    #[serde(default)]
    pub rollback: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RpcRecentPrioritizationFeesConfigTriton {
    pub percentile: Option<u16>,
}

fn verify_pubkeys(pubkey_strs: Option<Vec<String>>) -> Result<Vec<Pubkey>, JsonrpcError> {
    let pubkey_strs = pubkey_strs.unwrap_or_default();
    if pubkey_strs.len() > MAX_TX_ACCOUNT_LOCKS {
        return Err(JsonrpcError::invalid_params(format!(
            "Too many inputs provided; max {MAX_TX_ACCOUNT_LOCKS}"
        )));
    }

    pubkey_strs
        .into_iter()
        .map(|pubkey_str| verify_pubkey(&pubkey_str))
        .collect::<Result<Vec<Pubkey>, JsonrpcError>>()
}

fn verify_pubkey(input: &str) -> Result<Pubkey, JsonrpcError> {
    input
        .parse()
        .map_err(|e| JsonrpcError::invalid_params(format!("Invalid param: {e:?}")))
}

#[derive(Debug)]
struct RpcRequests {
    requests: Vec<RpcRequest>,
    shutdown: Arc<AtomicBool>,
    response_tx: oneshot::Sender<Vec<JsonrpcOutput>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RpcRequestType {
    LatestBlockhash,
    LeaderSchedule,
    RecentPrioritizationFees,
    Slot,
    Version,
}

#[derive(Debug)]
enum RpcRequest {
    LatestBlockhash {
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        commitment: CommitmentLevel,
        rollback: usize,
        min_context_slot: Option<Slot>,
    },
    LeaderSchedule {
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        slot: Option<Slot>,
        epoch: Option<Epoch>,
        commitment: CommitmentLevel,
        identity: Option<String>,
    },
    RecentPrioritizationFees {
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        pubkeys: Vec<Pubkey>,
        percentile: Option<u16>,
    },
    Slot {
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        commitment: CommitmentLevel,
        min_context_slot: Option<Slot>,
    },
    SolfeesSlots {
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        filter: SlotSubscribeFilter,
        frontend: bool,
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

#[derive(Debug, Clone)]
struct StreamsSlotInfo {
    leader: Option<Pubkey>,
    slot: Slot,
    commitment: CommitmentLevel,
    hash: Hash,
    time: UnixTimestamp,
    height: Slot,
    transactions: Arc<Vec<GeyserTransaction>>,
    total_transactions_vote: usize,
    fees: Arc<RecentPrioritizationFeesSlot>, // only for solana `getRecentPrioritizationFees`
    total_fee: u64,
    total_units_consumed: u64,
}

impl StreamsSlotInfo {
    fn new(
        leader: Option<Pubkey>,
        slot: Slot,
        hash: Hash,
        time: UnixTimestamp,
        height: Slot,
        transactions: Vec<GeyserTransaction>,
    ) -> Self {
        let total_transactions_vote = transactions.iter().filter(|tx| tx.vote).count();

        let fees = Arc::new(RecentPrioritizationFeesSlot::create(&transactions));
        let total_fee = transactions.iter().map(|tx| tx.fee).sum();

        let total_units_consumed = transactions
            .iter()
            .filter(|tx| !tx.vote)
            .map(|tx| tx.units_consumed.unwrap_or_default())
            .sum::<u64>();

        Self {
            leader,
            slot,
            commitment: CommitmentLevel::Processed,
            hash,
            time,
            height,
            transactions: Arc::new(transactions),
            total_transactions_vote,
            fees,
            total_fee,
            total_units_consumed,
        }
    }

    fn get_filtered(&self, filter: &SlotSubscribeFilter) -> SlotsSubscribeOutput {
        let mut fees = Vec::with_capacity(self.transactions.len());
        for transaction in self.transactions.iter().filter(|tx| {
            !tx.vote
                && SlotSubscribeFilter::filter_pubkeys(&filter.read_write, &tx.accounts.writable)
                && SlotSubscribeFilter::filter_pubkeys(&filter.read_only, &tx.accounts.readable)
        }) {
            fees.push(transaction.unit_price);
        }
        let total_transactions_filtered = fees.len();

        let fee_average = if total_transactions_filtered == 0 {
            0f64
        } else {
            fees.iter().copied().sum::<u64>() as f64 / total_transactions_filtered as f64
        };
        let fee_levels = if filter.levels.is_empty() {
            vec![]
        } else {
            fees.sort_unstable();
            filter
                .levels
                .iter()
                .map(|percentile| RecentPrioritizationFeesSlot::get_percentile(&fees, *percentile))
                .collect()
        };

        SlotsSubscribeOutput::Slot {
            leader: self.leader.map(|pk| pk.to_string()).unwrap_or_default(),
            slot: self.slot,
            commitment: self.commitment,
            hash: self.hash.to_string(),
            time: self.time,
            height: self.height,
            total_transactions_filtered,
            total_transactions_vote: self.total_transactions_vote,
            total_transactions: self.transactions.len(),
            fee_average,
            fee_levels,
            total_fee: self.total_fee,
            total_units_consumed: self.total_units_consumed,
        }
    }
}

#[derive(Debug)]
struct RecentPrioritizationFeesSlot {
    transaction_fees: Vec<u64>,
    writable_account_fees: HashMap<Pubkey, Vec<u64>>,
}

impl RecentPrioritizationFeesSlot {
    fn create(transactions: &[GeyserTransaction]) -> Self {
        let mut transaction_fees = Vec::with_capacity(transactions.len());
        let mut writable_account_fees =
            HashMap::<Pubkey, Vec<u64>>::with_capacity(transactions.len());

        for transaction in transactions.iter().filter(|tx| !tx.vote) {
            transaction_fees.push(transaction.unit_price);
            for writable_account in transaction.accounts.writable.iter().copied() {
                writable_account_fees
                    .entry(writable_account)
                    .or_default()
                    .push(transaction.unit_price);
            }
        }

        transaction_fees.sort_unstable();
        for (_account, fees) in writable_account_fees.iter_mut() {
            fees.sort_unstable();
        }

        Self {
            transaction_fees,
            writable_account_fees,
        }
    }

    fn get_fee(&self, account_keys: &[Pubkey], percentile: Option<u16>) -> u64 {
        let mut fee =
            Self::get_with_percentile(&self.transaction_fees, percentile).unwrap_or_default();

        for account_key in account_keys {
            if let Some(fees) = self.writable_account_fees.get(account_key) {
                if let Some(account_fee) = Self::get_with_percentile(fees, percentile) {
                    fee = std::cmp::max(fee, account_fee);
                }
            }
        }

        fee
    }

    fn get_with_percentile(fees: &[u64], percentile: Option<u16>) -> Option<u64> {
        match percentile {
            Some(percentile) => Self::get_percentile(fees, percentile),
            None => fees.first().copied(),
        }
    }

    fn get_percentile(fees: &[u64], percentile: u16) -> Option<u64> {
        let index = (percentile as usize).min(9_999) * fees.len() / 10_000;
        fees.get(index).copied()
    }
}

#[derive(Debug)]
enum StreamsUpdateMessage {
    Status {
        slot: Slot,
        commitment: CommitmentLevel,
    },
    Slot {
        info: StreamsSlotInfo,
    },
}

#[derive(Debug, Deserialize)]
struct ReqParamsSlotsSubscribe {
    #[serde(default)]
    config: Option<ReqParamsSlotsSubscribeConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReqParamsSlotsSubscribeConfig {
    read_write: Vec<String>,
    read_only: Vec<String>,
    levels: Vec<u16>,
}

#[derive(Debug)]
struct SlotSubscribeFilter {
    read_write: Vec<Pubkey>,
    read_only: Vec<Pubkey>,
    levels: Vec<u16>,
}

impl TryFrom<ReqParamsSlotsSubscribeConfig> for SlotSubscribeFilter {
    type Error = JsonrpcError;

    fn try_from(config: ReqParamsSlotsSubscribeConfig) -> Result<Self, Self::Error> {
        if config.read_write.len() + config.read_only.len() > MAX_TX_ACCOUNT_LOCKS {
            return Err(JsonrpcError::invalid_params(format!(
                "read_write and read_only should contain less than {MAX_TX_ACCOUNT_LOCKS} accounts"
            )));
        }

        if config.levels.len() > 5 {
            return Err(JsonrpcError::invalid_params(
                "only max 5 percentile levels are allowed".to_owned(),
            ));
        }

        for level in config.levels.iter().copied() {
            if level > 10_000 {
                return Err(JsonrpcError::invalid_params(
                    "percentile level is too big; max value is 10000".to_owned(),
                ));
            }
        }

        let read_write = config
            .read_write
            .iter()
            .map(|pubkey| {
                pubkey.parse().map_err(|_error| {
                    JsonrpcError::invalid_params(format!("failed to parse pubkey: {pubkey}"))
                })
            })
            .collect::<Result<Vec<Pubkey>, _>>()?;

        let read_only = config
            .read_only
            .iter()
            .map(|pubkey| {
                pubkey.parse().map_err(|_error| {
                    JsonrpcError::invalid_params(format!("failed to parse pubkey: {pubkey}"))
                })
            })
            .collect::<Result<Vec<Pubkey>, _>>()?;

        Ok(Self {
            read_write,
            read_only,
            levels: config.levels,
        })
    }
}

impl SlotSubscribeFilter {
    fn filter_pubkeys(required: &[Pubkey], pubkeys: &[Pubkey]) -> bool {
        required.is_empty()
            || required
                .iter()
                .all(|pubkey| pubkeys.binary_search(pubkey).is_ok())
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum SlotsSubscribeOutput {
    #[serde(rename_all = "camelCase")]
    Status {
        slot: Slot,
        commitment: CommitmentLevel,
    },
    #[serde(rename_all = "camelCase")]
    Slot {
        leader: String,
        slot: Slot,
        commitment: CommitmentLevel,
        hash: String,
        time: UnixTimestamp,
        height: Slot,
        total_transactions_filtered: usize,
        total_transactions_vote: usize,
        total_transactions: usize,
        fee_average: f64,
        fee_levels: Vec<Option<u64>>,
        total_fee: u64,
        total_units_consumed: u64,
    },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
enum SlotsSubscribeOutputSolana {
    #[serde(rename_all = "camelCase")]
    Status {
        slot: Slot,
        commitment: CommitmentLevel,
    },
    #[serde(rename_all = "camelCase")]
    Slot {
        slot: Slot,
        commitment: CommitmentLevel,
        height: Slot,
        total_transactions_filtered: usize,
        total_transactions_vote: usize,
        total_transactions: usize,
        fee_average: f64,
        fee_levels: Vec<Option<u64>>,
    },
}

impl From<SlotsSubscribeOutput> for SlotsSubscribeOutputSolana {
    fn from(output: SlotsSubscribeOutput) -> Self {
        match output {
            SlotsSubscribeOutput::Status { slot, commitment } => {
                SlotsSubscribeOutputSolana::Status { slot, commitment }
            }
            SlotsSubscribeOutput::Slot {
                slot,
                commitment,
                height,
                total_transactions_filtered,
                total_transactions_vote,
                total_transactions,
                fee_average,
                fee_levels,
                ..
            } => SlotsSubscribeOutputSolana::Slot {
                slot,
                commitment,
                height,
                total_transactions_filtered,
                total_transactions_vote,
                total_transactions,
                fee_average,
                fee_levels,
            },
        }
    }
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct SolfeesPrioritizationFee {
    slot: Slot,
    commitment: CommitmentLevel,
    height: Slot,
    total_transactions_filtered: usize,
    total_transactions_vote: usize,
    total_transactions: usize,
    fee_average: f64,
    fee_levels: Vec<Option<u64>>,
}

impl TryFrom<SlotsSubscribeOutput> for SolfeesPrioritizationFee {
    type Error = JsonrpcError;

    fn try_from(output: SlotsSubscribeOutput) -> Result<Self, Self::Error> {
        match output {
            SlotsSubscribeOutput::Status { .. } => {
                Err(SolanaRpc::internal_error_with_data("unexpected type"))
            }
            SlotsSubscribeOutput::Slot {
                slot,
                commitment,
                height,
                total_transactions_filtered,
                total_transactions_vote,
                total_transactions,
                fee_average,
                fee_levels,
                ..
            } => Ok(SolfeesPrioritizationFee {
                slot,
                commitment,
                height,
                total_transactions_filtered,
                total_transactions_vote,
                total_transactions,
                fee_average,
                fee_levels,
            }),
        }
    }
}
