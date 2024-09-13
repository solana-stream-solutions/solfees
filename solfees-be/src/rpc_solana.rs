use {
    crate::grpc_geyser::{CommitmentLevel, GeyserMessage},
    solana_sdk::{
        clock::{Slot, MAX_RECENT_BLOCKHASHES},
        hash::Hash,
    },
    std::{collections::BTreeMap, future::Future, sync::Arc},
    tokio::sync::mpsc,
};

#[derive(Debug)]
pub struct SolanaRpc {
    tx: mpsc::UnboundedSender<Arc<GeyserMessage>>,
}

impl SolanaRpc {
    pub fn new() -> (Self, impl Future<Output = anyhow::Result<()>>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, Self::run_loop(rx))
    }

    pub fn push_geyser_message(&self, message: Arc<GeyserMessage>) -> anyhow::Result<()> {
        anyhow::ensure!(self.tx.send(message).is_ok(), "SolanaRpc loop is dead");
        Ok(())
    }

    pub fn shutdown(self) {
        drop(self.tx);
    }

    async fn run_loop(mut rx: mpsc::UnboundedReceiver<Arc<GeyserMessage>>) -> anyhow::Result<()> {
        let mut latest_blockhash_storage = LatestBlockhashStorage::default();

        while let Some(message) = rx.recv().await {
            match message.as_ref() {
                GeyserMessage::Slot { slot, commitment } => {
                    latest_blockhash_storage.update_commitment(*slot, *commitment);
                }
                GeyserMessage::Block {
                    slot,
                    hash,
                    // time,
                    // height,
                    // parent_slot,
                    // parent_hash,
                    // transactions,
                    ..
                } => {
                    latest_blockhash_storage.push_block(*slot, *hash);
                }
            }
        }
        Ok(())
    }
}

#[derive(Debug, Default)]
struct LatestBlockhashStorage {
    slots: BTreeMap<Slot, LatestBlockhashSlot>,
    finalized_total: usize,
}

impl LatestBlockhashStorage {
    fn push_block(&mut self, slot: Slot, hash: Hash) {
        self.slots.insert(
            slot,
            LatestBlockhashSlot {
                hash,
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
    commitment: CommitmentLevel,
}
