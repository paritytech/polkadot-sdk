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

pub mod aura;
mod manual_seal;

use crate::common::spec::{DynNodeSpec, NodeSpec as NodeSpecT};
use cumulus_primitives_core::ParaId;
use manual_seal::ManualSealNode;
use sc_service::{Configuration, TaskManager};

/// Trait that extends the `DynNodeSpec` trait with manual seal related logic.
///
/// We need it in order to be able to access both the `DynNodeSpec` and the manual seal logic
/// through dynamic dispatch.
pub trait DynNodeSpecExt: DynNodeSpec {
	fn start_manual_seal_node(
		&self,
		config: Configuration,
		para_id: ParaId,
		block_time: u64,
	) -> sc_service::error::Result<TaskManager>;
}

impl<T> DynNodeSpecExt for T
where
	T: NodeSpecT + DynNodeSpec,
{
	#[sc_tracing::logging::prefix_logs_with("Parachain")]
	fn start_manual_seal_node(
		&self,
		config: Configuration,
		para_id: ParaId,
		block_time: u64,
	) -> sc_service::error::Result<TaskManager> {
		let node = ManualSealNode::<T>::new();
		match config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				node.start_node::<sc_network::NetworkWorker<_, _>>(config, para_id, block_time),
			sc_network::config::NetworkBackendType::Litep2p =>
				node.start_node::<sc_network::Litep2pNetworkBackend>(config, para_id, block_time),
		}
	}
}
