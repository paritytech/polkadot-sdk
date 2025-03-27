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
use polkadot_cli::service::IdentifyNetworkBackend;
use sc_service::{Configuration, TaskManager};

/// The current node version for cumulus official binaries, which takes the basic
/// SemVer form `<major>.<minor>.<patch>`. It should correspond to the latest
/// `polkadot` version of a stable release.
pub const NODE_VERSION: &'static str = "1.17.4";

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

		// If the network backend is unspecified, use the default for the given chain.
		let default_backend = config.chain_spec.network_backend();
		let network_backend = config.network.network_backend.unwrap_or(default_backend);
		match network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				node.start_node::<sc_network::NetworkWorker<_, _>>(config, para_id, block_time),
			sc_network::config::NetworkBackendType::Litep2p =>
				node.start_node::<sc_network::Litep2pNetworkBackend>(config, para_id, block_time),
		}
	}
}
