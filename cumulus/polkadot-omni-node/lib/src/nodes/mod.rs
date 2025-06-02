// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
pub const NODE_VERSION: &'static str = "1.18.5";

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
