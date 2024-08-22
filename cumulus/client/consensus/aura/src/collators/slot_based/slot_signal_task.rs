// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use std::sync::Arc;
use std::time::Duration;
use codec::Codec;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use cumulus_primitives_core::{BlockT, CollectCollationInfo};
use sc_client_api::{AuxStore, BlockBackend, BlockOf, UsageProvider};
use sc_consensus_aura::SlotDuration;
use sc_utils::mpsc::TracingUnboundedSender;
use sp_api::ProvideRuntimeApi;
use sp_application_crypto::AppPublic;
use sp_blockchain::HeaderBackend;
use sp_consensus_aura::{AuraApi, Slot};
use sp_core::Pair;
use sp_runtime::traits::Member;
use sp_timestamp::Timestamp;

#[derive(Debug)]
pub struct SlotInfo {
    pub timestamp: Timestamp,
    pub slot: Slot,
    pub slot_duration: SlotDuration,
}

#[derive(Debug)]
struct SlotTimer<Block, Client, P> {
    client: Arc<Client>,
    drift: Duration,
    _marker: std::marker::PhantomData<(Block, Box<dyn Fn(P) + Send + Sync + 'static>)>,
}

/// Returns current duration since Unix epoch.
fn duration_now() -> Duration {
    use std::time::SystemTime;
    let now = SystemTime::now();
    now.duration_since(SystemTime::UNIX_EPOCH).unwrap_or_else(|e| {
        panic!("Current time {:?} is before Unix epoch. Something is wrong: {:?}", now, e)
    })
}

/// Returns the duration until the next slot from now.
fn time_until_next_slot(slot_duration: Duration, drift: Duration) -> Duration {
    let now = duration_now().as_millis() - drift.as_millis();

    let next_slot = (now + slot_duration.as_millis()) / slot_duration.as_millis();
    let remaining_millis = next_slot * slot_duration.as_millis() - now;
    Duration::from_millis(remaining_millis as u64)
}

impl<Block, Client, P> SlotTimer<Block, Client, P>
where
    Block: BlockT,
    Client: ProvideRuntimeApi<Block> + Send + Sync + 'static + UsageProvider<Block>,
    Client::Api: AuraApi<Block, P::Public>,
    P: Pair,
    P::Public: AppPublic + Member + Codec,
    P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
    pub fn new_with_drift(client: Arc<Client>, drift: Duration) -> Self {
        Self { client, drift, _marker: Default::default() }
    }

    /// Returns a future that resolves when the next slot arrives.
    pub async fn wait_until_next_slot(&self) -> Result<SlotInfo, ()> {
        let Ok(slot_duration) = crate::slot_duration(&*self.client) else {
            tracing::error!(target: crate::LOG_TARGET, "Failed to fetch slot duration from runtime.");
            return Err(())
        };

        let time_until_next_slot = time_until_next_slot(slot_duration.as_duration(), self.drift);
        tokio::time::sleep(time_until_next_slot).await;
        let timestamp = sp_timestamp::Timestamp::current();
        Ok(SlotInfo {
            slot: Slot::from_timestamp(timestamp, slot_duration),
            timestamp,
            slot_duration,
        })
    }
}

pub struct Params<Client> {
   pub signal_sender: TracingUnboundedSender<SlotInfo>,
    /// Drift every slot by this duration.
    /// This is a time quantity that is subtracted from the actual timestamp when computing
    /// the time left to enter a new slot. In practice, this *left-shifts* the clock time with the
    /// intent to keep our "clock" slightly behind the relay chain one and thus reducing the
    /// likelihood of encountering unfavorable notification arrival timings (i.e. we don't want to
    /// wait for relay chain notifications because we woke up too early).
   pub slot_drift: Duration,
   pub para_client: Arc<Client>
}

pub async fn run_signal_task<Client, Block, P>(params: Params<Client>) where
    Block: BlockT,
    Client: ProvideRuntimeApi<Block>
    + UsageProvider<Block>
    + BlockOf
    + AuxStore
    + HeaderBackend<Block>
    + BlockBackend<Block>
    + Send
    + Sync
    + 'static,
    Client::Api:
    AuraApi<Block, P::Public> + CollectCollationInfo<Block> + AuraUnincludedSegmentApi<Block>,
    P: Pair,
    P::Public: AppPublic + Member + Codec,
    P::Signature: TryFrom<Vec<u8>> + Member + Codec,
{
    let Params {
        slot_drift, para_client, signal_sender
    } = params;
    let slot_timer = SlotTimer::<_, _, P>::new_with_drift(para_client.clone(), slot_drift);
    loop {
        let Ok(para_slot) = slot_timer.wait_until_next_slot().await else {
            return;
        };

        signal_sender.unbounded_send(para_slot).expect("TODO: panic message");
    }
}