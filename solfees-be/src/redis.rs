use {
    crate::{config::ConfigRedisConsumer, grpc_geyser::GeyserMessage},
    anyhow::Context,
    redis::{streams::StreamReadReply, AsyncConnectionConfig, Client, Value as RedisValue},
    std::time::Duration,
    tokio::{sync::mpsc, time::sleep},
};

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
    tokio::spawn(async move {
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

                if tx
                    .send(bincode::deserialize(payload).context("failed to decode payload"))
                    .is_err()
                {
                    break 'outer;
                }
            }
        }
    });

    Ok(rx)
}
