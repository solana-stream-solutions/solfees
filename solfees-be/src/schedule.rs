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
    tracing::{error, info},
};

type LeaderScheduleByEpoch = HashMap<Epoch, Option<LeadersSchedule>>;

pub struct SolanaSchedule {
    rpc: Arc<RpcClient>,
    epoch_schedule: EpochSchedule,
    leader_schedule_by_epoch: Arc<Mutex<LeaderScheduleByEpoch>>,
}

impl SolanaSchedule {
    pub fn new(endpoint: String) -> Self {
        Self {
            rpc: Arc::new(RpcClient::new(endpoint)),
            epoch_schedule: EpochSchedule::custom(432_000, 432_000, false),
            leader_schedule_by_epoch: Arc::new(Mutex::default()),
        }
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
                tokio::spawn(async move {
                    info!(epoch, "trying to fetch schedule");
                    if let Err(error) =
                        Self::update_leader_schedule(epoch, slot, rpc, &leader_schedule_by_epoch)
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
    ) -> anyhow::Result<()> {
        let response = rpc
            .get_leader_schedule_with_commitment(Some(slot), CommitmentConfig::confirmed())
            .await
            .context("failed to fetch schedule from RPC")?
            .context("no schedule in RPC response")?;
        let leader_schedule = LeadersSchedule::new(response)?;
        leader_schedule_by_epoch
            .lock()
            .unwrap()
            .insert(epoch, Some(leader_schedule));
        info!(epoch, "save schedule for epoch");
        Ok(())
    }
}

#[derive(Debug)]
struct LeadersSchedule {
    leaders: Vec<Pubkey>,
    indices: Box<[u16; 432_000]>,
}

impl LeadersSchedule {
    fn new(schedule: HashMap<String, Vec<usize>>) -> anyhow::Result<Self> {
        let mut map = HashMap::<Pubkey, u16>::new();

        let mut leaders = Vec::with_capacity(4096);
        let mut indices = Box::new([0; 432_000]);

        for (leader, leader_schedule) in schedule.into_iter() {
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
                indices[index] = leader_index;
            }
        }

        Ok(Self { indices, leaders })
    }

    fn get_leader(&self, index: u64) -> anyhow::Result<Pubkey> {
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
