use {
    anyhow::Context,
    solana_client::nonblocking::rpc_client::RpcClient,
    solana_sdk::{
        clock::{Epoch, Slot},
        commitment_config::CommitmentConfig,
        epoch_schedule::EpochSchedule,
        pubkey::Pubkey,
    },
    std::{
        collections::HashMap,
        sync::{Arc, Mutex},
    },
    tokio::sync::mpsc,
    tracing::{error, info},
};

type LeaderScheduleByEpoch = HashMap<Epoch, Option<LeadersSchedule>>;

pub struct SolanaSchedule {
    rpc: Arc<RpcClient>,
    epoch_schedule: EpochSchedule,
    leader_schedule_by_epoch: Arc<Mutex<LeaderScheduleByEpoch>>,
    leader_schedule_tx: mpsc::UnboundedSender<(Epoch, LeaderScheduleRpc)>,
}

impl SolanaSchedule {
    pub fn new(
        endpoint: String,
        saved_epochs: Vec<(Epoch, LeaderScheduleRpc)>,
    ) -> (Self, mpsc::UnboundedReceiver<(Epoch, LeaderScheduleRpc)>) {
        let mut map = HashMap::new();
        for (epoch, leader_schedule_rpc) in saved_epochs {
            if let Ok(leader_schedule) = LeadersSchedule::new(&leader_schedule_rpc) {
                map.insert(epoch, Some(leader_schedule));
                info!(epoch, "use preloaded schedule");
            }
        }

        let (leader_schedule_tx, leader_schedule_rx) = mpsc::unbounded_channel();
        let schedule = Self {
            rpc: Arc::new(RpcClient::new(endpoint)),
            epoch_schedule: EpochSchedule::custom(432_000, 432_000, false),
            leader_schedule_by_epoch: Arc::new(Mutex::new(map)),
            leader_schedule_tx,
        };
        (schedule, leader_schedule_rx)
    }

    pub fn get_leader(&self, slot: Slot) -> Option<Pubkey> {
        let (epoch, index) = self.epoch_schedule.get_epoch_and_slot_index(slot);

        let mut locked = self.leader_schedule_by_epoch.lock().unwrap();
        match locked.get(&epoch) {
            Some(Some(leader_schedule)) => leader_schedule
                .get_leader(index)
                .inspect_err(
                    |error| error!(%error, slot, "failed to get leader from existed schedule"),
                )
                .ok(),
            Some(None) => None, // in progress
            None => {
                locked.insert(epoch, None);

                let rpc = Arc::clone(&self.rpc);
                let leader_schedule_by_epoch = Arc::clone(&self.leader_schedule_by_epoch);
                let leader_schedule_tx = self.leader_schedule_tx.clone();
                tokio::spawn(async move {
                    info!(epoch, "trying to fetch schedule");
                    if let Err(error) = Self::update_leader_schedule(
                        epoch,
                        slot,
                        rpc,
                        &leader_schedule_by_epoch,
                        leader_schedule_tx,
                    )
                    .await
                    {
                        error!(%error, slot, epoch, "failed to fetch leader schedule");
                        leader_schedule_by_epoch.lock().unwrap().remove(&epoch);
                    }
                });

                None
            }
        }
    }

    async fn update_leader_schedule(
        epoch: Epoch,
        slot: Slot,
        rpc: Arc<RpcClient>,
        leader_schedule_by_epoch: &Mutex<LeaderScheduleByEpoch>,
        leader_schedule_tx: mpsc::UnboundedSender<(Epoch, LeaderScheduleRpc)>,
    ) -> anyhow::Result<()> {
        let leader_schedule_rpc = rpc
            .get_leader_schedule_with_commitment(Some(slot), CommitmentConfig::confirmed())
            .await
            .context("failed to fetch schedule from RPC")?
            .context("no schedule in RPC response")?;
        let leader_schedule = LeadersSchedule::new(&leader_schedule_rpc)?;
        leader_schedule_by_epoch
            .lock()
            .unwrap()
            .insert(epoch, Some(leader_schedule));
        info!(epoch, "epoch schedule saved");
        let _ = leader_schedule_tx.send((epoch, leader_schedule_rpc));
        Ok(())
    }
}

pub type LeaderScheduleRpc = HashMap<String, Vec<usize>>;

#[derive(Debug)]
pub struct LeadersSchedule {
    leaders: Vec<Pubkey>,
    indices: Box<[u16; 432_000]>,
}

impl LeadersSchedule {
    pub fn new(schedule: &LeaderScheduleRpc) -> anyhow::Result<Self> {
        let mut map = HashMap::<Pubkey, u16>::new();

        let mut leaders = Vec::with_capacity(4096);
        let mut indices = Box::new([0; 432_000]);

        for (leader, leader_schedule) in schedule.iter() {
            let leader = leader.parse().context("failed to parse leader key")?;
            let leader_index = match map.get(&leader) {
                Some(index) => *index,
                None => {
                    let index =
                        u16::try_from(leaders.len()).context("leader index out of bounds")?;

                    leaders.push(leader);
                    map.insert(leader, index);

                    index
                }
            };
            for index in leader_schedule {
                indices[*index] = leader_index;
            }
        }

        Ok(Self { indices, leaders })
    }

    pub fn get_leader(&self, index: u64) -> anyhow::Result<Pubkey> {
        let leader_index = *self
            .indices
            .get(usize::try_from(index).context("failed convert to index")?)
            .ok_or_else(|| anyhow::anyhow!("failed to find leader index"))?;
        self.leaders
            .get(usize::from(leader_index))
            .ok_or_else(|| anyhow::anyhow!("failed to find leader by index"))
            .copied()
    }
}
