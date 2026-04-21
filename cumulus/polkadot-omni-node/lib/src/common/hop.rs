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

use crate::common::NodeExtraArgs;
use sc_hop::HopDataPool;
use std::{path::PathBuf, sync::Arc};

/// Conditionally build the HOP data pool based on CLI flags.
pub(crate) fn build_hop_pool(
	node_extra_args: &NodeExtraArgs,
	database_path: Option<PathBuf>,
) -> sc_service::error::Result<Option<Arc<HopDataPool>>> {
	if !node_extra_args.hop.enabled {
		log::info!(target: "hop", "HOP data pool is disabled (use --enable-hop to enable)");
		return Ok(None);
	}

	let data_dir = match &node_extra_args.hop.data_dir {
		Some(dir) => dir.clone(),
		None => database_path
			.ok_or_else(|| {
				sc_service::Error::Application(
					"No database path available and --hop-data-dir not specified".into(),
				)
			})?
			.join("hop"),
	};

	log::info!(
		target: "hop",
		"Initializing HOP data pool: {:?} (resolved data_dir: {})",
		node_extra_args.hop,
		data_dir.display(),
	);

	let pool = HopDataPool::new(
		node_extra_args.hop.max_pool_size * 1024 * 1024,
		node_extra_args.hop.max_user_size * 1024 * 1024,
		node_extra_args.hop.retention_blocks,
		data_dir,
		node_extra_args.hop.rate_limit_config(),
	)
	.map_err(|e| sc_service::Error::Application(Box::new(e)))?;

	log::info!(
		target: "hop",
		"HOP data pool initialized: {:?}, RPC methods will be registered",
		pool.status(),
	);

	Ok(Some(Arc::new(pool)))
}
