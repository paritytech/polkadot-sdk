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

//! Parachain bootnodes advertisement.

use codec::{Decode, Encode};
use cumulus_primitives_core::{relay_chain::Hash, ParaId};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::StreamExt;
use sp_consensus_babe::Epoch;
use std::sync::Arc;

pub struct BootnodeAdvertisement {
	para_id_scale_compact: Vec<u8>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
}

impl BootnodeAdvertisement {
	pub fn new(para_id: ParaId, relay_chain_interface: Arc<dyn RelayChainInterface>) -> Self {
		Self { para_id_scale_compact: Encode::encode(&para_id), relay_chain_interface }
	}

	async fn current_epoch(self: &Self, hash: Hash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_current_epoch", hash, &[])
			.await?;
		Decode::decode(&mut &*res).map_err(Into::into)
	}

	pub async fn run(self) -> RelayChainResult<()> {
		let network_service = self.relay_chain_interface.network_service()?;
		let import_notification_stream =
			self.relay_chain_interface.import_notification_stream().await?;
		let header = import_notification_stream.fuse().select_next_some().await;

		let startup_epoch = self.current_epoch(header.hash()).await?;
		let key = self
			.para_id_scale_compact
			.clone()
			.into_iter()
			.chain(startup_epoch.randomness.into_iter())
			.collect::<Vec<_>>();

		network_service.start_providing(key.into());

		// Do not terminate the essential task.
		std::future::pending().await
	}
}
