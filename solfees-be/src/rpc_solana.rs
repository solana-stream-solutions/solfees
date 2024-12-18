use {
    crate::{
        grpc_geyser::{CommitmentLevel, GeyserMessage, GeyserTransaction},
        metrics::solfees_be::{self as metrics, ClientId},
        redis::RedisMessage,
    },
    futures::{
        future::{join_all, pending, BoxFuture, FutureExt},
        sink::SinkExt,
        stream::StreamExt,
    },
    hyper::body::Buf,
    hyper_tungstenite::HyperWebsocket,
    jsonrpc_core::{
        Call as JsonrpcCall, Error as JsonrpcError, Failure as JsonrpcFailure, Id as JsonrpcId,
        MethodCall as JsonrpcMethodCall, Value as JsonrcpValue, Version as JsonrpcVersion,
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
        clock::{Epoch, Slot, UnixTimestamp, MAX_PROCESSING_AGE},
        epoch_schedule::EpochSchedule,
        hash::Hash,
        pubkey::Pubkey,
        transaction::MAX_TX_ACCOUNT_LOCKS,
    },
    std::{
        borrow::Cow,
        collections::{BTreeMap, BTreeSet, HashMap},
        sync::{
            atomic::{AtomicBool, Ordering},
            Arc,
        },
        time::Duration,
    },
    tokio::{
        sync::{broadcast, mpsc, oneshot, Mutex},
        time::sleep,
    },
    tokio_tungstenite::tungstenite::protocol::{
        frame::{coding::CloseCode as WebSocketCloseCode, CloseFrame as WebSocketCloseFrame},
        Message as WebSocketMessage,
    },
    tracing::{debug, info},
};

const MAX_NUM_RECENT_SLOT_INFO: usize = 150;

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(untagged)]
enum JsonrpcOutputArced {
    Success(JsonrpcSuccessArced),
    Failure(JsonrpcFailure),
}

#[derive(Debug, Serialize)]
#[serde(deny_unknown_fields)]
struct JsonrpcSuccessArced {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jsonrpc: Option<JsonrpcVersion>,
    pub result: JsonrcpValueArced,
    pub id: JsonrpcId,
}

#[derive(Debug, Serialize)]
#[serde(untagged)]
enum JsonrcpValueArced {
    Value(JsonrcpValue),
    MaybeArcedValue(Option<Arc<JsonrcpValue>>),
    RpcPrioritizationFee(Vec<RpcPrioritizationFee>),
    SolfeesSlotsFrontend(Vec<SlotsSubscribeOutput>),
    SolfeesSlots(Vec<SolfeesPrioritizationFee>),
}

impl From<Option<&Arc<JsonrcpValue>>> for JsonrcpValueArced {
    fn from(value: Option<&Arc<JsonrcpValue>>) -> Self {
        Self::MaybeArcedValue(value.cloned())
    }
}

impl From<Vec<RpcPrioritizationFee>> for JsonrcpValueArced {
    fn from(value: Vec<RpcPrioritizationFee>) -> Self {
        Self::RpcPrioritizationFee(value)
    }
}

impl From<Vec<SlotsSubscribeOutput>> for JsonrcpValueArced {
    fn from(value: Vec<SlotsSubscribeOutput>) -> Self {
        Self::SolfeesSlotsFrontend(value)
    }
}

impl From<Vec<SolfeesPrioritizationFee>> for JsonrcpValueArced {
    fn from(value: Vec<SolfeesPrioritizationFee>) -> Self {
        Self::SolfeesSlots(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SolanaRpcMode {
    Solana,
    Triton,
    Solfees,
    SolfeesFrontend,
}

impl SolanaRpcMode {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Solana => "solana",
            Self::Triton => "triton",
            Self::Solfees => "solfees",
            Self::SolfeesFrontend => "frontend",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SolanaRpc {
    request_calls_max: usize,
    request_timeout: Duration,
    redis_tx: broadcast::Sender<RedisMessage>,
    requests_tx: mpsc::Sender<RpcRequestTask>,
    streams_tx: broadcast::Sender<Arc<StreamsUpdateMessage>>,
}

impl SolanaRpc {
    pub fn new(
        request_calls_max: usize,
        request_timeout: Duration,
        calls_queue_max: usize,
        streams_channel_capacity: usize,
        pool_size: usize,
    ) -> (Self, Vec<BoxFuture<'static, anyhow::Result<()>>>) {
        const REDIS_CHANNEL_SIZE: usize = 2_048;

        let (redis_tx, _redis_rx) = broadcast::channel(REDIS_CHANNEL_SIZE);
        let (streams_tx, _streams_rx) = broadcast::channel(streams_channel_capacity);
        let (requests_tx, requests_rx) = mpsc::channel(calls_queue_max);

        let requests_rx = Arc::new(Mutex::new(requests_rx));

        let rpc = Self {
            request_calls_max,
            request_timeout,
            redis_tx: redis_tx.clone(),
            requests_tx,
            streams_tx: streams_tx.clone(),
        };

        let mut futs = vec![
            // WebSocket source
            Self::run_subscribe_update_loop(redis_tx.subscribe(), streams_tx).boxed(),
        ];
        for (index, ()) in std::iter::repeat(()).take(pool_size).enumerate() {
            futs.push(
                // RPC handler
                Self::run_request_update_loop(
                    index,
                    redis_tx.subscribe(),
                    Arc::clone(&requests_rx),
                )
                .boxed(),
            );
        }

        (rpc, futs)
    }

    pub fn shutdown(self) {
        drop(self.redis_tx)
    }

    pub fn push_redis_message(&self, message: RedisMessage) -> anyhow::Result<()> {
        anyhow::ensure!(
            self.redis_tx.send(message).is_ok(),
            "SolanaRpc update loop is dead"
        );
        Ok(())
    }

    pub async fn on_request(
        &self,
        client_id: ClientId,
        mode: SolanaRpcMode,
        body: impl Buf,
    ) -> anyhow::Result<(RpcRequestsStats, Vec<u8>)> {
        let timer = client_id.start_timer_cpu();
        let mut stats = RpcRequestsStats::default();

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
                    stats.latest_blockhash += 1;

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
                    stats.leader_schedule += 1;

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
                                        commitment: CommitmentLevel::Confirmed,
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
                    stats.recent_prioritization_fees += 1;

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
                    stats.slot += 1;

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
                    stats.version += 1;

                    outputs.push(Some(if let Err(error) = call.params.expect_no_params() {
                        Self::create_failure(call.jsonrpc, call.id, error)
                    } else {
                        let version = solana_version::Version::default();
                        Self::create_success2(
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
        timer.stop_and_record();

        if !requests.is_empty() {
            let shutdown = Arc::new(AtomicBool::new(false));

            let mut rxs = Vec::with_capacity(requests.len());
            for request in requests {
                let (tx, rx) = oneshot::channel();
                match self.requests_tx.try_send(RpcRequestTask {
                    client_id: client_id.clone(),
                    request,
                    shutdown: Arc::clone(&shutdown),
                    tx,
                }) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        shutdown.store(true, Ordering::Relaxed);
                        anyhow::bail!("requests queue is full");
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        anyhow::bail!("processed loop is closed");
                    }
                }
                metrics::requests_queue_size_inc();
                rxs.push(rx);
            }
            let response_rx = join_all(rxs);

            tokio::select! {
                () = sleep(self.request_timeout) => {
                    shutdown.store(true, Ordering::Relaxed);
                    anyhow::bail!("request timeout");
                },
                maybe_outputs = response_rx => {
                    let mut index = 0;
                    for output in maybe_outputs {
                        while index < outputs.len() && outputs[index].is_some() {
                            index += 1;
                        }
                        anyhow::ensure!(index < outputs.len(), "output index out of bounds");

                        outputs[index] = Some(output.map_err(|_error| anyhow::anyhow!("request dropped"))?);
                    }
                }
            }
        }

        anyhow::ensure!(calls_total == outputs.len(), "invalid number of outputs");
        let _timer = client_id.start_timer_cpu(); // report when dropped
        match outputs
            .into_iter()
            .map(|output| output.ok_or(()))
            .collect::<Result<Vec<JsonrpcOutputArced>, ()>>()
        {
            Ok(outputs) => if batched_calls {
                serde_json::to_vec(&outputs)
            } else if let Some(output) = outputs.last() {
                serde_json::to_vec(output)
            } else {
                anyhow::bail!("output is not defined")
            }
            .map_err(Into::into),
            Err(()) => {
                anyhow::bail!("not all outputs created")
            }
        }
        .map(|mut body| {
            body.push(b'\n');
            (stats, body)
        })
    }

    pub async fn on_websocket(
        self,
        client_id: ClientId,
        mode: SolanaRpcMode,
        websocket: HyperWebsocket,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
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
        metrics::websockets_alive_inc(mode);

        let mut updates_rx = self.streams_tx.subscribe();
        let mut filter = None;
        let mut websocket_tx_message: Option<WebSocketMessage> = None;
        let mut flush_required = false;

        let loop_close_reason = loop {
            if let Some(message) = websocket_tx_message.take() {
                client_id.observe_egress_ws(message.len() as u64);
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
                _ = shutdown_rx.recv() => break Some(None),

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

                    let timer = client_id.start_timer_cpu();
                    match call.method.as_str() {
                        "SlotsSubscribe" => {
                            let output = match call.params.parse().and_then(|config: ReqParamsSlotsSubscribeConfig| {
                                SlotSubscribeFilter::try_from(config)
                            }) {
                                Ok(filter_new) => {
                                    filter = Some((call.id.clone(), filter_new));
                                    Self::create_success2(call.jsonrpc, call.id, "subscribed")
                                },
                                Err(error) => Self::create_failure(call.jsonrpc, call.id, error),
                            };
                            websocket_tx_message = Some(WebSocketMessage::Text(serde_json::to_string(&output).expect("failed to serialize")));
                        },
                        _ => break Some(Some("unknown subscription method")),
                    }
                    timer.stop_and_record();
                },

                maybe_update = updates_rx.recv() => match maybe_update {
                    Ok(update) => if let Some((id, filter)) = filter.as_ref() {
                        let timer = client_id.start_timer_cpu();
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
                            Self::create_success2(None, id.clone(), output)
                        } else {
                            Self::create_success2(None, id.clone(), SlotsSubscribeOutputSolana::from(output))
                        };
                        websocket_tx_message = Some(WebSocketMessage::Text(serde_json::to_string(&message).expect("failed to serialize")));
                        timer.stop_and_record();
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

        metrics::websockets_alive_dec(mode);
    }

    fn create_success(
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        value: impl Into<JsonrcpValueArced>,
    ) -> JsonrpcOutputArced {
        JsonrpcOutputArced::Success(JsonrpcSuccessArced {
            jsonrpc,
            result: value.into(),
            id,
        })
    }

    fn create_success2<R>(
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        result: R,
    ) -> JsonrpcOutputArced
    where
        R: Serialize,
    {
        let value = serde_json::to_value(result).expect("failed to serialize");
        JsonrpcOutputArced::Success(JsonrpcSuccessArced {
            jsonrpc,
            result: JsonrcpValueArced::Value(value),
            id,
        })
    }

    const fn create_failure(
        jsonrpc: Option<JsonrpcVersion>,
        id: JsonrpcId,
        error: JsonrpcError,
    ) -> JsonrpcOutputArced {
        JsonrpcOutputArced::Failure(JsonrpcFailure { jsonrpc, error, id })
    }

    fn internal_error_with_data<R>(data: R) -> JsonrpcError
    where
        R: Serialize,
    {
        let mut error = JsonrpcError::internal_error();
        error.data = Some(serde_json::to_value(data).expect("failed to serialize"));
        error
    }

    async fn get_next_requests(
        rx: &Mutex<mpsc::Receiver<RpcRequestTask>>,
    ) -> Option<RpcRequestTask> {
        rx.lock().await.recv().await
    }

    async fn run_subscribe_update_loop(
        mut redis_rx: broadcast::Receiver<RedisMessage>,
        streams_tx: broadcast::Sender<Arc<StreamsUpdateMessage>>,
    ) -> anyhow::Result<()> {
        loop {
            match redis_rx.recv().await {
                Ok(RedisMessage::Geyser(message)) => match message {
                    GeyserMessage::Status { slot, commitment } => {
                        let _ = streams_tx
                            .send(Arc::new(StreamsUpdateMessage::Status { slot, commitment }));
                        metrics::set_slot(commitment, slot);
                    }
                    GeyserMessage::Slot {
                        leader,
                        slot,
                        hash,
                        time,
                        height,
                        parent_slot,
                        parent_hash: _parent_hash,
                        transactions,
                    } => {
                        let info = StreamsSlotInfo::new(
                            leader,
                            slot,
                            parent_slot,
                            hash,
                            time,
                            height,
                            transactions,
                        );
                        let _ = streams_tx.send(Arc::new(StreamsUpdateMessage::Slot { info }));
                    }
                },
                Ok(RedisMessage::Epoch { .. }) => continue,
                Err(broadcast::error::RecvError::Closed) => return Ok(()),
                Err(broadcast::error::RecvError::Lagged(_lag)) => {
                    anyhow::bail!("run_subscribe_update_loop lagged")
                }
            }
        }
    }

    async fn run_request_update_loop(
        loop_index: usize,
        mut redis_rx: broadcast::Receiver<RedisMessage>,
        requests_rx: Arc<Mutex<mpsc::Receiver<RpcRequestTask>>>,
    ) -> anyhow::Result<()> {
        let mut latest_blockhash_storage = LatestBlockhashStorage::default();
        let mut slots_info = BTreeMap::<Slot, StreamsSlotInfo>::new();

        let epoch_schedule = EpochSchedule::custom(432_000, 432_000, false);
        let mut leader_schedule_map_solfees = HashMap::<Slot, Arc<JsonrcpValue>>::new();
        let mut leader_schedule_map_rpc = HashMap::<Slot, Arc<JsonrcpValue>>::new();

        loop {
            tokio::select! {
                biased;

                maybe_message = redis_rx.recv() => match maybe_message {
                    Ok(RedisMessage::Geyser(message)) => match message {
                        GeyserMessage::Status { slot, commitment } => {
                            latest_blockhash_storage.update_commitment(slot, commitment);

                            if let Some(info) = slots_info.get_mut(&slot) {
                                info.commitment = commitment;
                            }
                        }
                        GeyserMessage::Slot {
                            leader,
                            slot,
                            hash,
                            time,
                            height,
                            parent_slot,
                            parent_hash: _parent_hash,
                            transactions,
                        } => {
                            latest_blockhash_storage.push_block(slot, parent_slot, height, hash);

                            let info = StreamsSlotInfo::new(leader, slot, parent_slot, hash, time, height, transactions);
                            slots_info.insert(slot, info.clone());
                            while slots_info.len() > MAX_NUM_RECENT_SLOT_INFO {
                                slots_info.pop_first();
                            }
                        }
                    }
                    Ok(RedisMessage::Epoch { epoch, leader_schedule_solfees, leader_schedule_rpc }) => {
                        info!(loop_index, epoch, "epoch received");
                        leader_schedule_map_solfees.insert(epoch, leader_schedule_solfees);
                        leader_schedule_map_rpc.insert(epoch, leader_schedule_rpc);
                        continue;
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(_lag)) => anyhow::bail!("run_request_update_loop#{loop_index} lagged"),
                },

                maybe_rpc_requests_task = Self::get_next_requests(&requests_rx) => match maybe_rpc_requests_task {
                    Some(task) => {
                        metrics::requests_queue_size_dec();
                        if !task.shutdown.load(Ordering::Relaxed) {
                            let timer = task.client_id.start_timer_cpu();
                            let _ = task.tx.send(Self::handle_request_task(
                                task.request,
                                &latest_blockhash_storage,
                                &slots_info,
                                &epoch_schedule,
                                &leader_schedule_map_solfees,
                                &leader_schedule_map_rpc
                            ));
                            timer.stop_and_record();
                        }
                    },
                    None => break,
                },
            }
        }

        Ok(())
    }

    fn handle_request_task(
        request: RpcRequest,
        latest_blockhash_storage: &LatestBlockhashStorage,
        slots_info: &BTreeMap<Slot, StreamsSlotInfo>,
        epoch_schedule: &EpochSchedule,
        leader_schedule_map_solfees: &HashMap<Slot, Arc<JsonrcpValue>>,
        leader_schedule_map_rpc: &HashMap<Slot, Arc<JsonrcpValue>>,
    ) -> JsonrpcOutputArced {
        match request {
            RpcRequest::LatestBlockhash {
                jsonrpc,
                id,
                commitment,
                rollback,
                min_context_slot,
            } => {
                if rollback > MAX_PROCESSING_AGE {
                    let error = JsonrpcError::invalid_params(format!(
                        "rollback exceeds {MAX_PROCESSING_AGE}"
                    ));
                    return Self::create_failure(jsonrpc, id, error);
                }

                let mut slot = match commitment {
                    CommitmentLevel::Processed => latest_blockhash_storage.slot_processed,
                    CommitmentLevel::Confirmed => latest_blockhash_storage.slot_confirmed,
                    CommitmentLevel::Finalized => latest_blockhash_storage.slot_finalized,
                };

                if let Some(min_context_slot) = min_context_slot {
                    if slot < min_context_slot {
                        let error =
                            RpcCustomError::MinContextSlotNotReached { context_slot: slot }.into();
                        return Self::create_failure(jsonrpc, id, error);
                    }
                }

                let Some(mut value) = latest_blockhash_storage.slots.get(&slot) else {
                    return Self::create_failure(
                        jsonrpc,
                        id,
                        SolanaRpc::internal_error_with_data("no slot"),
                    );
                };
                for _ in 0..rollback {
                    let Some(parent_value) = latest_blockhash_storage.slots.get(&value.parent)
                    else {
                        return Self::create_failure(
                            jsonrpc,
                            id,
                            JsonrpcError::invalid_params("not enought slots in the storage"),
                        );
                    };
                    slot = value.parent;
                    value = parent_value;
                }

                Self::create_success2(
                    jsonrpc,
                    id,
                    RpcResponse {
                        context: RpcResponseContext::new(slot),
                        value: RpcBlockhash {
                            blockhash: value.hash.to_string(),
                            last_valid_block_height: value.height + MAX_PROCESSING_AGE as u64,
                        },
                    },
                )
            }
            RpcRequest::LeaderSchedule {
                jsonrpc,
                id,
                slot,
                epoch,
                commitment,
                identity,
            } => {
                if let Some(epoch) = epoch {
                    Self::create_success(jsonrpc, id, leader_schedule_map_solfees.get(&epoch))
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
                        if let Some(slots) = leader_schedule_map_rpc
                            .get(&epoch)
                            .and_then(|m| m.get(&identity))
                        {
                            map.insert(identity, slots);
                        }
                        Self::create_success2(jsonrpc, id, Some(&map))
                    } else {
                        Self::create_success(jsonrpc, id, leader_schedule_map_rpc.get(&epoch))
                    }
                }
            }
            RpcRequest::RecentPrioritizationFees {
                jsonrpc,
                id,
                pubkeys,
                percentile,
            } => {
                let result = slots_info
                    .iter()
                    .map(|(slot, value)| RpcPrioritizationFee {
                        slot: *slot,
                        prioritization_fee: value.fees.get_fee(&pubkeys, &[], false, percentile).1,
                    })
                    .collect::<Vec<_>>();

                Self::create_success(jsonrpc, id, result)
            }
            RpcRequest::Slot {
                jsonrpc,
                id,
                commitment,
                min_context_slot,
            } => {
                let slot = match commitment {
                    CommitmentLevel::Processed => latest_blockhash_storage.slot_processed,
                    CommitmentLevel::Confirmed => latest_blockhash_storage.slot_confirmed,
                    CommitmentLevel::Finalized => latest_blockhash_storage.slot_finalized,
                };

                if let Some(min_context_slot) = min_context_slot {
                    if slot < min_context_slot {
                        let error =
                            RpcCustomError::MinContextSlotNotReached { context_slot: slot }.into();
                        return Self::create_failure(jsonrpc, id, error);
                    }
                }

                Self::create_success2(jsonrpc, id, slot)
            }
            RpcRequest::SolfeesSlots {
                jsonrpc,
                id,
                filter,
                frontend,
            } => {
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

#[derive(Debug, Default, Clone, Copy)]
pub struct RpcRequestsStats {
    pub latest_blockhash: u64,
    pub leader_schedule: u64,
    pub recent_prioritization_fees: u64,
    pub slot: u64,
    pub version: u64,
}

#[derive(Debug)]
struct RpcRequestTask {
    client_id: ClientId,
    request: RpcRequest,
    shutdown: Arc<AtomicBool>,
    tx: oneshot::Sender<JsonrpcOutputArced>,
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
    slot_processed: Slot,
    slot_confirmed: Slot,
    slot_finalized: Slot,
    confirmed: BTreeSet<Slot>,
    finalized: BTreeSet<Slot>,
    slots: BTreeMap<Slot, LatestBlockhashSlot>,
}

impl LatestBlockhashStorage {
    fn update_slots(&mut self) {
        for (slot, entry) in self.slots.iter().rev() {
            match entry.commitment {
                CommitmentLevel::Processed => {
                    self.slot_processed = self.slot_processed.max(*slot);
                }
                CommitmentLevel::Confirmed => {
                    self.slot_confirmed = self.slot_confirmed.max(*slot);
                }
                CommitmentLevel::Finalized => {
                    self.slot_finalized = self.slot_finalized.max(*slot);
                    break;
                }
            }
        }

        // in case of epoch change with a lot of forks we can receive confirmed directly
        // and processed would be behind, so we take `max(processed, confirmed)`
        self.slot_processed = self.slot_processed.max(self.slot_confirmed);
    }

    fn push_block(&mut self, slot: Slot, parent: Slot, height: Slot, hash: Hash) {
        // in case if slot status was received before block
        let mut commitment = CommitmentLevel::Processed;
        if self.confirmed.contains(&slot) {
            commitment = CommitmentLevel::Confirmed;
        }
        if self.finalized.contains(&slot) {
            commitment = CommitmentLevel::Finalized;
        }
        self.slots.insert(
            slot,
            LatestBlockhashSlot {
                commitment,
                parent,
                height,
                hash,
            },
        );

        // keep slots under the limit (based only on finalized count)
        let slots_to_remove = self
            .slots
            .values()
            .filter(|entry| entry.commitment == CommitmentLevel::Finalized)
            .count()
            .checked_sub(MAX_PROCESSING_AGE + 10)
            .unwrap_or_default();
        for _ in 0..slots_to_remove {
            while let Some((_slot, value)) = self.slots.pop_first() {
                if value.commitment == CommitmentLevel::Finalized {
                    break;
                }
            }
        }

        // update tips
        self.update_slots();
    }

    fn update_commitment(&mut self, slot: Slot, commitment: CommitmentLevel) {
        // save commitment
        if commitment == CommitmentLevel::Confirmed {
            self.confirmed.insert(slot);
        } else if commitment == CommitmentLevel::Finalized {
            self.confirmed.insert(slot);
            self.finalized.insert(slot);
        }
        while self.confirmed.len() > MAX_PROCESSING_AGE {
            self.confirmed.pop_first();
        }
        while self.finalized.len() > MAX_PROCESSING_AGE {
            self.finalized.pop_first();
        }

        // update current and all slots before
        if let Some(value) = self.slots.get_mut(&slot) {
            value.commitment = value.commitment.max(commitment);

            let mut parent_slot = value.parent;
            if commitment == CommitmentLevel::Confirmed {
                loop {
                    if let Some(value) = self.slots.get_mut(&parent_slot) {
                        if value.commitment == CommitmentLevel::Processed {
                            value.commitment = CommitmentLevel::Confirmed;
                            parent_slot = value.parent;
                            continue;
                        }
                    }
                    break;
                }
            } else if commitment == CommitmentLevel::Finalized {
                loop {
                    if let Some(value) = self.slots.get_mut(&parent_slot) {
                        if value.commitment != CommitmentLevel::Finalized {
                            value.commitment = CommitmentLevel::Finalized;
                            parent_slot = value.parent;
                            continue;
                        }
                    }
                    break;
                }
            }
        }

        // update tips
        self.update_slots();
    }
}

#[derive(Debug)]
struct LatestBlockhashSlot {
    commitment: CommitmentLevel,
    parent: Slot,
    height: Slot,
    hash: Hash,
}

#[derive(Debug, Clone)]
struct StreamsSlotInfo {
    leader: Option<Pubkey>,
    slot: Slot,
    parent_slot: Slot,
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
        parent_slot: Slot,
        hash: Hash,
        time: UnixTimestamp,
        height: Slot,
        transactions: Arc<Vec<GeyserTransaction>>,
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
            parent_slot,
            commitment: CommitmentLevel::Processed,
            hash,
            time,
            height,
            transactions,
            total_transactions_vote,
            fees,
            total_fee,
            total_units_consumed,
        }
    }

    fn get_filtered(&self, filter: &SlotSubscribeFilter) -> SlotsSubscribeOutput {
        let total_transactions_filtered = self
            .transactions
            .iter()
            .filter(|tx| {
                !tx.vote
                    && tx.unit_limit > 0
                    && filter
                        .read_write
                        .iter()
                        .all(|pubkey| tx.accounts.writable.contains(pubkey))
                    && filter
                        .read_only
                        .iter()
                        .all(|pubkey| tx.accounts.readable.contains(pubkey))
            })
            .count();

        let fee_average = self
            .fees
            .get_fee(
                &filter.read_write,
                &filter.read_only,
                filter.skip_zeros,
                None,
            )
            .0;

        let fee_levels = filter
            .levels
            .iter()
            .map(|level| {
                self.fees
                    .get_fee(
                        &filter.read_write,
                        &filter.read_only,
                        filter.skip_zeros,
                        Some(*level),
                    )
                    .1
            })
            .collect();

        SlotsSubscribeOutput::Slot {
            leader: self.leader.map(|pk| pk.to_string()).unwrap_or_default(),
            slot: self.slot,
            parent_slot: self.parent_slot,
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
    transaction_fees: CollectedFees,
    transaction_fees_nz: CollectedFees,
    writable_account_fees: HashMap<Pubkey, CollectedFees>,
    writable_account_fees_nz: HashMap<Pubkey, CollectedFees>,
    readable_account_fees: HashMap<Pubkey, CollectedFees>,
    readable_account_fees_nz: HashMap<Pubkey, CollectedFees>,
}

impl RecentPrioritizationFeesSlot {
    fn create(transactions: &[GeyserTransaction]) -> Self {
        let mut transaction_fees = Vec::with_capacity(transactions.len());
        let mut transaction_fees_nz = Vec::with_capacity(transactions.len());
        let mut writable_account_fees =
            HashMap::<Pubkey, Vec<u64>>::with_capacity(transactions.len());
        let mut writable_account_fees_nz =
            HashMap::<Pubkey, Vec<u64>>::with_capacity(transactions.len());
        let mut readable_account_fees =
            HashMap::<Pubkey, Vec<u64>>::with_capacity(transactions.len());
        let mut readable_account_fees_nz =
            HashMap::<Pubkey, Vec<u64>>::with_capacity(transactions.len());

        for transaction in transactions
            .iter()
            .filter(|tx| !tx.vote && tx.unit_limit > 0)
        {
            transaction_fees.push(transaction.unit_price);
            for account in transaction.accounts.writable.iter().copied() {
                writable_account_fees
                    .entry(account)
                    .or_default()
                    .push(transaction.unit_price);
            }
            for account in transaction.accounts.readable.iter().copied() {
                readable_account_fees
                    .entry(account)
                    .or_default()
                    .push(transaction.unit_price);
            }

            if transaction.unit_price > 0 {
                transaction_fees_nz.push(transaction.unit_price);
                for account in transaction.accounts.writable.iter().copied() {
                    writable_account_fees_nz
                        .entry(account)
                        .or_default()
                        .push(transaction.unit_price);
                }
                for account in transaction.accounts.readable.iter().copied() {
                    readable_account_fees_nz
                        .entry(account)
                        .or_default()
                        .push(transaction.unit_price);
                }
            }
        }

        let conv = |map: HashMap<Pubkey, Vec<u64>>| {
            map.into_iter()
                .map(|(account, fees)| (account, CollectedFees::new(fees)))
                .collect()
        };

        Self {
            transaction_fees: CollectedFees::new(transaction_fees),
            transaction_fees_nz: CollectedFees::new(transaction_fees_nz),
            writable_account_fees: conv(writable_account_fees),
            writable_account_fees_nz: conv(writable_account_fees_nz),
            readable_account_fees: conv(readable_account_fees),
            readable_account_fees_nz: conv(readable_account_fees_nz),
        }
    }

    fn get_fee(
        &self,
        writeable_account_keys: &[Pubkey],
        readable_account_keys: &[Pubkey],
        skip_zeros: bool,
        percentile: Option<u16>,
    ) -> (f64, u64) {
        let (txs, write_map, read_map) = if skip_zeros {
            (
                &self.transaction_fees_nz,
                &self.writable_account_fees_nz,
                &self.readable_account_fees_nz,
            )
        } else {
            (
                &self.transaction_fees,
                &self.writable_account_fees,
                &self.readable_account_fees,
            )
        };

        let (mut avg, mut fee) = txs.get_with_percentile(percentile);

        if let Some((aavg, afee)) = writeable_account_keys
            .iter()
            .zip(std::iter::repeat(write_map))
            .chain(
                readable_account_keys
                    .iter()
                    .zip(std::iter::repeat(read_map)),
            )
            .filter_map(|(account, map)| {
                map.get(account)
                    .map(|fees| fees.get_with_percentile(percentile))
            })
            .reduce(|(avg1, fee1), (avg2, fee2)| (avg1.max(avg2), fee1.max(fee2)))
        {
            avg = avg.max(aavg);
            fee = fee.max(afee);
        }

        (avg, fee)
    }
}

#[derive(Debug)]
struct CollectedFees {
    fees: Vec<u64>,
    average: f64,
}

impl From<Vec<u64>> for CollectedFees {
    fn from(fees: Vec<u64>) -> Self {
        Self::new(fees)
    }
}

impl CollectedFees {
    fn new(mut fees: Vec<u64>) -> Self {
        fees.sort_unstable();
        let average = if fees.is_empty() {
            0f64
        } else {
            fees.iter().map(|fee| *fee as f64).sum::<f64>() / fees.len() as f64
        };
        Self { fees, average }
    }

    fn get_with_percentile(&self, percentile: Option<u16>) -> (f64, u64) {
        let fee = match percentile {
            Some(percentile) => self.get_percentile(percentile),
            None => self.fees.first().copied(),
        };
        (self.average, fee.unwrap_or_default())
    }

    fn get_percentile(&self, percentile: u16) -> Option<u64> {
        let index = (percentile as usize).min(9_999) * self.fees.len() / 10_000;
        self.fees.get(index).copied()
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

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct ReqParamsSlotsSubscribe {
    config: Option<ReqParamsSlotsSubscribeConfig>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct ReqParamsSlotsSubscribeConfig {
    read_write: Vec<String>,
    read_only: Vec<String>,
    levels: Vec<u16>,
    skip_zeros: bool,
}

#[derive(Debug)]
struct SlotSubscribeFilter {
    read_write: Vec<Pubkey>,
    read_only: Vec<Pubkey>,
    levels: Vec<u16>,
    skip_zeros: bool,
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
            skip_zeros: config.skip_zeros,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SlotsSubscribeOutput {
    #[serde(rename_all = "camelCase")]
    Status {
        slot: Slot,
        commitment: CommitmentLevel,
    },
    #[serde(rename_all = "camelCase")]
    Slot {
        leader: String,
        slot: Slot,
        parent_slot: Slot,
        commitment: CommitmentLevel,
        hash: String,
        time: UnixTimestamp,
        height: Slot,
        total_transactions_filtered: usize,
        total_transactions_vote: usize,
        total_transactions: usize,
        fee_average: f64,
        fee_levels: Vec<u64>,
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
        fee_levels: Vec<u64>,
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
    fee_levels: Vec<u64>,
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
