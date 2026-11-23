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

//! Offchain runtime code: the blob a block's `:code` marker names, fetched from JAM.

use crate::{
	common::{
		types::{ParachainBackend, ParachainClient},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::LOG_TARGET,
};
use jam_interface::{HeaderHash, JamStateSource, ServiceId};
use sc_client_api::{Backend as _, StorageProvider};
use std::sync::Arc;

/// Cache the runtime code a block's `:code` marker names, fetching the preimage from JAM when it
/// is not already in the `CODE` column.
///
/// The runtime writes the marker during block execution, so the block that executes the upgrade
/// holds it and every later read of that block's runtime (`code_at`, `slot_duration_at`, a runtime
/// API) needs the bytes before the node can resolve the marker. A no-op for a block whose `:code`
/// is real code, and for a marker whose blob is already cached.
pub(crate) async fn ensure_code_blob_at<Block, RuntimeApi, Jam>(
	para_client: &Arc<ParachainClient<Block, RuntimeApi>>,
	para_backend: &ParachainBackend<Block>,
	jam: &Jam,
	service_id: ServiceId,
	block: Block::Hash,
	anchor: HeaderHash,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, ParachainClient<Block, RuntimeApi>>,
	Jam: JamStateSource,
{
	let key = sp_core::storage::StorageKey(sp_core::storage::well_known_keys::CODE.to_vec());
	let code = match para_client.storage(block, &key) {
		Ok(Some(code)) => code.0,
		Ok(None) => return,
		Err(error) => {
			tracing::warn!(target: LOG_TARGET, ?block, ?error, "Unable to read `:code`.");
			return;
		},
	};
	let Some(code_hash) = sp_code_marker::decode_marker(&code) else {
		return;
	};
	match para_backend.code_blob(&code_hash) {
		Ok(Some(_)) => return,
		Ok(None) => {},
		Err(error) => {
			tracing::warn!(
				target: LOG_TARGET,
				?code_hash,
				?error,
				"Unable to read the local runtime code blob.",
			);
			return;
		},
	}
	match jam.service_preimage(anchor, service_id, code_hash).await {
		Ok(Some(preimage)) => {
			tracing::info!(
				target: LOG_TARGET,
				?code_hash,
				"Fetched the offchain runtime code from JAM.",
			);
			if let Err(error) = para_backend.store_code_blob(&code_hash, &preimage) {
				tracing::error!(
					target: LOG_TARGET,
					?code_hash,
					?error,
					"Unable to cache the fetched runtime code; the block will be dropped.",
				);
			}
		},
		Ok(None) => {
			tracing::error!(
				target: LOG_TARGET,
				?code_hash,
				"JAM holds no preimage for the runtime code marker; the block will be dropped.",
			);
		},
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?code_hash,
				?error,
				"Failed to fetch the runtime code preimage from JAM; the block will be dropped.",
			);
		},
	}
}
