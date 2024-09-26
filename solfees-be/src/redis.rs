use {
    crate::{
        config::ConfigRedisConsumer,
        grpc_geyser::{CommitmentLevel, GeyserMessage},
    },
    anyhow::Context,
    lru::LruCache,
    redis::{streams::StreamReadReply, AsyncConnectionConfig, Client, Value as RedisValue},
    std::{num::NonZeroUsize, time::Duration},
    tokio::{sync::mpsc, time::sleep},
};

#[derive(Debug, Default, Clone, Copy)]
struct SeenSlot {
    slot: bool,
    processed: bool,
    confirmed: bool,
    finalized: bool,
}

pub async fn subscribe(
    config: ConfigRedisConsumer,
) -> anyhow::Result<mpsc::UnboundedReceiver<anyhow::Result<GeyserMessage>>> {
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
        let mut finalized_slot_tip = 0;
        let mut latest_id = "0".to_owned();
        'outer: loop {
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
                    break;
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
                    break 'outer;
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
                }

                if tx.send(message).is_err() {
                    break 'outer;
                }
            }
        }
    });

    Ok(rx)
}
