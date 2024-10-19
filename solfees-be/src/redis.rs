use {
    crate::{
        config::ConfigRedisConsumer,
        grpc_geyser::{CommitmentLevel, GeyserMessage},
        schedule::{LeaderScheduleRpc, LeadersSchedule, LeadersScheduleSolfees},
    },
    anyhow::Context,
    lru::LruCache,
    redis::{streams::StreamReadReply, AsyncConnectionConfig, Client, Value as RedisValue},
    solana_sdk::{clock::Epoch, epoch_schedule::EpochSchedule},
    std::{collections::HashSet, num::NonZeroUsize, sync::Arc, time::Duration},
    tokio::{sync::mpsc, time::sleep},
};

#[derive(Debug, Clone)]
pub enum RedisMessage {
    Geyser(GeyserMessage),
    Epoch {
        epoch: Epoch,
        leader_schedule_solfees: Arc<LeadersScheduleSolfees>,
        leader_schedule_rpc: Arc<LeaderScheduleRpc>,
    },
}

impl RedisMessage {
    fn build_epoch(epoch: Epoch, data: &[u8]) -> anyhow::Result<Self> {
        let leader_schedule_rpc: LeaderScheduleRpc = bincode::deserialize(data)
            .with_context(|| format!("failed to deserialie epoch {epoch}"))?;
        let leader_schedule = LeadersSchedule::new(&leader_schedule_rpc)
            .with_context(|| format!("failed to build schedule for epoch {epoch}"))?;

        Ok(Self::Epoch {
            epoch,
            leader_schedule_solfees: Arc::new(leader_schedule.into()),
            leader_schedule_rpc: Arc::new(leader_schedule_rpc),
        })
    }
}

#[derive(Debug, Default, Clone, Copy)]
struct SeenSlot {
    slot: bool,
    processed: bool,
    confirmed: bool,
    finalized: bool,
}

pub async fn subscribe(
    config: ConfigRedisConsumer,
) -> anyhow::Result<mpsc::UnboundedReceiver<anyhow::Result<RedisMessage>>> {
    let mut epochs = HashSet::new();

    let client = Client::open(config.endpoint).context("failed to create Redis client")?;
    let mut connection = client
        .get_multiplexed_async_connection_with_config(
            &AsyncConnectionConfig::new().set_connection_timeout(Duration::from_secs(2)),
        )
        .await
        .context("failed to get Redis connection")?;

    let (tx, rx) = mpsc::unbounded_channel();
    let mut lru = LruCache::new(
        NonZeroUsize::new(8_192).ok_or(anyhow::anyhow!("failed to create LRU capacity"))?,
    );
    tokio::spawn(async move {
        match redis::cmd("HGETALL")
            .arg(&config.epochs_key)
            .query_async::<Vec<(Epoch, Vec<u8>)>>(&mut connection)
            .await
        {
            Ok(value) => {
                for (epoch, data) in value {
                    let msg = RedisMessage::build_epoch(epoch, &data);

                    let alive = msg.is_ok();
                    if tx.send(msg).is_err() || !alive {
                        return;
                    }

                    epochs.insert(epoch);
                }
            }
            Err(error) => {
                let _ = tx.send(Err(error.into()));
                return;
            }
        };

        let epoch_schedule = EpochSchedule::custom(432_000, 432_000, false);
        let mut epochs_queue = vec![];
        let mut finalized_slot_tip = 0;
        let mut latest_id = "0".to_owned();
        loop {
            if !epochs_queue.is_empty() {
                let mut pipe = redis::pipe();
                pipe.cmd("HMGET").arg(&config.epochs_key);
                for slot in epochs_queue.iter() {
                    pipe.arg(*slot);
                }

                match pipe
                    .query_async::<Vec<Vec<Option<Vec<u8>>>>>(&mut connection)
                    .await
                {
                    Ok(value) => {
                        for (epoch, maybe_data) in epochs_queue.drain(..).zip(value.into_iter()) {
                            if let Some(Some(data)) = maybe_data.first() {
                                let msg = RedisMessage::build_epoch(epoch, data);

                                let alive = msg.is_ok();
                                if tx.send(msg).is_err() || !alive {
                                    return;
                                }

                                epochs.insert(epoch);
                            }
                        }
                    }
                    Err(error) => {
                        let _ = tx.send(Err(error.into()));
                        return;
                    }
                };
            }

            let mut stream_keys = match redis::cmd("XREAD")
                .arg("COUNT")
                .arg(5)
                .arg("STREAMS")
                .arg(&config.stream_key)
                .arg(&latest_id)
                .query_async(&mut connection)
                .await
            {
                Ok(StreamReadReply { keys }) => keys,
                Err(error) => {
                    let _ = tx.send(Err(error.into()));
                    return;
                }
            };

            let Some(stream_key) = stream_keys.pop() else {
                sleep(Duration::from_micros(500)).await;
                continue;
            };

            for stream_id in stream_key.ids {
                latest_id = stream_id.id;

                let Some(RedisValue::BulkString(payload)) =
                    stream_id.map.get(&config.stream_field_key)
                else {
                    let _ = tx.send(Err(anyhow::anyhow!("failed to get payload from StreamId")));
                    return;
                };

                let message = bincode::deserialize(payload).context("failed to decode payload");

                // deduplication
                if let Some((slot, commitment)) = match &message {
                    Ok(GeyserMessage::Status { slot, commitment }) => {
                        Some((*slot, Some(*commitment)))
                    }
                    Ok(GeyserMessage::Slot { slot, .. }) => Some((*slot, None)),
                    Err(_error) => None,
                } {
                    if slot < finalized_slot_tip {
                        continue;
                    }
                    if commitment == Some(CommitmentLevel::Finalized) {
                        finalized_slot_tip = slot;
                    }

                    let entry = lru.get_or_insert_mut(slot, SeenSlot::default);
                    let seen = match commitment {
                        Some(CommitmentLevel::Processed) => &mut entry.processed,
                        Some(CommitmentLevel::Confirmed) => &mut entry.confirmed,
                        Some(CommitmentLevel::Finalized) => &mut entry.finalized,
                        None => &mut entry.slot,
                    };
                    if *seen {
                        continue;
                    }
                    *seen = true;

                    let epoch = epoch_schedule.get_epoch(slot);
                    if commitment.is_none() && !epochs.contains(&epoch) {
                        epochs_queue.push(epoch);
                    }
                }

                if tx.send(message.map(RedisMessage::Geyser)).is_err() {
                    return;
                }
            }
        }
    });

    Ok(rx)
}
