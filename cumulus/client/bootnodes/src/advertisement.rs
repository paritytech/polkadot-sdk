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
use cumulus_primitives_core::{
	relay_chain::{Hash, Header},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::StreamExt;
use log::{debug, warn};
use sc_network::{service::traits::NetworkService, KademliaKey};
use sp_consensus_babe::{digests::CompatibleDigestItem, Epoch, Randomness};
use sp_runtime::traits::Header as _;
use std::sync::Arc;

/// Log target for this file.
const LOG_TARGET: &str = "bootnodes::advertisement";

/// Parachain bootnode advertisement parameters.
pub struct BootnodeAdvertisementParams {
	/// Parachain ID.
	pub para_id: ParaId,
	/// Relay chain interface.
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain node network service.
	pub network_service: Arc<dyn NetworkService>,
}

/// Parachain bootnode advertisement service.
pub struct BootnodeAdvertisement {
	para_id_scale_compact: Vec<u8>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	network_service: Arc<dyn NetworkService>,
	current_epoch_key: Option<KademliaKey>,
	next_epoch_key: Option<KademliaKey>,
}

impl BootnodeAdvertisement {
	pub fn new(
		BootnodeAdvertisementParams {
			para_id,
			relay_chain_interface,
			network_service,
		}: BootnodeAdvertisementParams,
	) -> Self {
		Self {
			para_id_scale_compact: Encode::encode(&para_id),
			relay_chain_interface,
			network_service,
			current_epoch_key: None,
			next_epoch_key: None,
		}
	}

	async fn current_epoch(&self, hash: Hash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_current_epoch", hash, &[])
			.await?;
		Decode::decode(&mut &*res).map_err(Into::into)
	}

	async fn next_epoch(&self, hash: Hash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_next_epoch", hash, &[])
			.await?;
		Decode::decode(&mut &*res).map_err(Into::into)
	}

	fn epoch_key(&self, randomness: Randomness) -> KademliaKey {
		self.para_id_scale_compact
			.clone()
			.into_iter()
			.chain(randomness.into_iter())
			.collect::<Vec<_>>()
			.into()
	}

	async fn current_and_next_epoch_keys(
		&self,
		header: Header,
	) -> (Option<KademliaKey>, Option<KademliaKey>) {
		let hash = header.hash();
		let number = header.number();

		let current_epoch = match self.current_epoch(hash).await {
			Ok(epoch) => Some(epoch),
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to query current epoch for #{number} {hash:?}: {e}",
				);

				None
			},
		};

		let next_epoch = match self.next_epoch(hash).await {
			Ok(epoch) => Some(epoch),
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to query next epoch for #{number} {hash:?}: {e}",
				);

				None
			},
		};

		(
			current_epoch.map(|epoch| self.epoch_key(epoch.randomness)),
			next_epoch.map(|epoch| self.epoch_key(epoch.randomness)),
		)
	}

	async fn handle_import_notification(&mut self, header: Header) {
		if let Some(ref old_current_epoch_key) = self.current_epoch_key {
			// Readvertise on start of new epoch only.
			let is_start_of_epoch =
				header.digest().logs.iter().any(|v| v.as_next_epoch_descriptor().is_some());
			if !is_start_of_epoch {
				return;
			}

			debug!(target: LOG_TARGET, "New epoch started, readvertising parachain bootnode.");

			let (current_epoch_key, next_epoch_key) =
				self.current_and_next_epoch_keys(header).await;

			// Readvertise for current epoch.
			if let Some(ref current_epoch_key) = current_epoch_key {
				if current_epoch_key == old_current_epoch_key {
					debug!(
						target: LOG_TARGET,
						"Re-advertising bootnode for current epoch {current_epoch_key:?}",
					);
				} else {
					self.network_service.stop_providing(old_current_epoch_key.clone());
					debug!(
						target: LOG_TARGET,
						"Stopped advertising bootnode for past epoch {old_current_epoch_key:?}",
					);

					match self.next_epoch_key {
						Some(ref old_next_key) if old_next_key == current_epoch_key => debug!(
							target: LOG_TARGET,
							"Advertising bootnode for current (old next) epoch {current_epoch_key:?}",
						),
						_ => debug!(
							target: LOG_TARGET,
							"Advertising bootnode for current epoch {current_epoch_key:?}",
						),
					}
				}

				self.network_service.start_providing(current_epoch_key.clone());
				self.current_epoch_key = Some(current_epoch_key.clone());
			}

			// Readvertise for next epoch.
			if let Some(next_epoch_key) = next_epoch_key {
				match (current_epoch_key, &self.next_epoch_key) {
					(Some(current_epoch_key), Some(old_next_epoch_key)) =>
						if *old_next_epoch_key != current_epoch_key {
							self.network_service.stop_providing(old_next_epoch_key.clone());

							debug!(
								target: LOG_TARGET,
								"Stopped advertising bootnode for discarded next epoch {:?}",
								old_next_epoch_key,
							);
						},
					// In all other cases we keep the old next epoch key advertised, as it either
					// became a current epoch key, or in odd cases will just expire.
					_ => {},
				}

				self.network_service.start_providing(next_epoch_key.clone());
				self.next_epoch_key = Some(next_epoch_key.clone());

				debug!(
					target: LOG_TARGET,
					"Advertising bootnode for next epoch {next_epoch_key:?}",
				);
			}
		} else {
			// First advertisement on startup.
			let (current_epoch_key, next_epoch_key) =
				self.current_and_next_epoch_keys(header).await;
			self.current_epoch_key = current_epoch_key.clone();
			self.next_epoch_key = next_epoch_key.clone();

			if let Some(current_epoch_key) = current_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for current epoch {current_epoch_key:?}",
				);

				self.network_service.start_providing(current_epoch_key);
			} else {
				warn!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for current epoch failed: no key."
				);
			}

			if let Some(next_epoch_key) = next_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for next epoch {next_epoch_key:?}",
				);

				self.network_service.start_providing(next_epoch_key);
			} else {
				warn!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for next epoch failed: no key."
				);
			}
		}
	}

	pub async fn run(mut self) -> RelayChainResult<()> {
		let mut import_notification_stream =
			self.relay_chain_interface.import_notification_stream().await?;

		loop {
			tokio::select! {
				header = import_notification_stream.next() => match header {
					Some(header) => self.handle_import_notification(header).await,
					None => {
						debug!(
							target: LOG_TARGET,
							"Import notification stream terminated, terminating bootnode advertisement."
						);
						return Ok(());
					}
				},
				// TODO: handle requests for multiaddresses.
			}
		}
	}
}
