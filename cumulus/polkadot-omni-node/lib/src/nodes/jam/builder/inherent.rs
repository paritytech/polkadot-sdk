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

//! The inherent data a JAM block carries.

use sp_inherents::{InherentData, InherentDataProvider};
use sp_timestamp::Timestamp;

/// The timestamp inherent, the only inherent a JAM block carries.
///
/// The timestamp is the anchor's, which pins the block's Aura slot to the anchor; it ends up
/// *inside* the block, so importers validate it rather than recomputing it from their own clock.
pub(crate) async fn create_inherent_data(timestamp: Timestamp) -> Result<InherentData, String> {
	let mut inherent_data = InherentData::new();
	sp_timestamp::InherentDataProvider::new(timestamp)
		.provide_inherent_data(&mut inherent_data)
		.await
		.map_err(|e| format!("timestamp inherent: {e}"))?;
	Ok(inherent_data)
}
