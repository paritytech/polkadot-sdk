use futures::StreamExt;
use sc_client_api::BlockchainEvents;
use sc_service::SpawnTaskHandle;
use sp_consensus::BlockOrigin;
use sp_runtime::traits::{Block, Header};
use std::sync::Arc;
use stp_shield::ShieldKeystorePtr;

pub fn spawn_key_rotation_on_own_import<B, C>(
    task_spawner: &SpawnTaskHandle,
    client: Arc<C>,
    keystore: ShieldKeystorePtr,
) where
    B: Block,
    C: BlockchainEvents<B> + Send + Sync + 'static,
{
    task_spawner.spawn("mev-shield-key-rotation", None, async move {
        log::debug!(target: "mev-shield", "Key-rotation task started");
        let mut import_stream = client.import_notification_stream();

        while let Some(notif) = import_stream.next().await {
            if notif.origin != BlockOrigin::Own {
                continue;
            }

            if keystore.roll_for_next_slot().is_ok() {
                log::debug!(
                    target: "mev-shield",
                    "Rotated shield key after importing own block #{}",
                    notif.header.number()
                );
            } else {
                log::warn!(
                    target: "mev-shield",
                    "Key rotation: failed to roll for next slot"
                );
            }
        }
    });
}
