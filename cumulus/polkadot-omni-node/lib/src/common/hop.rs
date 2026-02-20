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
use std::sync::Arc;

/// Conditionally build the HOP data pool based on CLI flags.
pub(crate) fn build_hop_pool(
	node_extra_args: &NodeExtraArgs,
) -> sc_service::error::Result<Option<Arc<HopDataPool>>> {
	if !node_extra_args.enable_hop {
		return Ok(None);
	}

	let pool = HopDataPool::new(
		node_extra_args.hop_max_pool_size_mb * 1024 * 1024,
		node_extra_args.hop_retention_blocks,
	)
	.map_err(|e| sc_service::Error::Application(Box::new(e) as Box<_>))?;

	Ok(Some(Arc::new(pool)))
}
