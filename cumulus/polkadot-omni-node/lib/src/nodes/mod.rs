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
use manual_seal::ManualSealNode;
use sc_service::{Configuration, TaskManager};

/// The current node version for cumulus official binaries, which takes the basic
/// SemVer form `<major>.<minor>.<patch>`. It should correspond to the latest
/// `polkadot` version of a stable release.
pub const NODE_VERSION: &'static str = "1.19.1";

/// Trait that extends the `DynNodeSpec` trait with manual seal related logic.
///
/// We need it in order to be able to access both the `DynNodeSpec` and the manual seal logic
/// through dynamic dispatch.
pub trait DynNodeSpecExt: DynNodeSpec {
	fn start_manual_seal_node(
		&self,
		config: Configuration,
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
		block_time: u64,
	) -> sc_service::error::Result<TaskManager> {
		let node = ManualSealNode::<T>::new();
		match config.network.network_backend {
			sc_network::config::NetworkBackendType::Libp2p =>
				node.start_node::<sc_network::NetworkWorker<_, _>>(config, block_time),
			sc_network::config::NetworkBackendType::Litep2p =>
				node.start_node::<sc_network::Litep2pNetworkBackend>(config, block_time),
		}
	}
}
