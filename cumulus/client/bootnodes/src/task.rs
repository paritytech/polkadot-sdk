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

//! Parachain bootnodes advertisement and discovery service.

use crate::advertisement::BootnodeAdvertisement;
use cumulus_primitives_core::ParaId;
use cumulus_relay_chain_interface::RelayChainInterface;
use sc_service::TaskManager;
use std::sync::Arc;

/// Bootnode advertisement task params.
pub struct StartBootnodeTasksParams<'a> {
	pub para_id: ParaId,
	pub task_manager: &'a mut TaskManager,
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
}

pub fn start_bootnode_tasks(
	StartBootnodeTasksParams { para_id, task_manager, relay_chain_interface }: StartBootnodeTasksParams,
) {
	let bootnode_advertisement = BootnodeAdvertisement::new(para_id, relay_chain_interface);

	task_manager.spawn_essential_handle().spawn_blocking(
		"cumulus-bootnode-advertisement",
		None,
		async {
			if let Err(e) = bootnode_advertisement.run().await {
				log::error!("Bootnode advertisement terminated with an error: {e}");
			}
		},
	);
}
