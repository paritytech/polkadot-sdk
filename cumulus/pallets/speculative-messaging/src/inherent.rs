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

//! Client-side inherent data provider for speculative messaging.
//!
//! The [`SpecMsgInherentDataProvider`] drains queued inbound message
//! metadata (source, count, provides_root) and supplies it to the
//! runtime via the inherent data mechanism. The runtime's
//! [`ProvideInherent`] implementation then creates a
//! `receive_messages_inherent` call from this data.

use crate::pallet::{InherentType, INHERENT_IDENTIFIER};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use sp_core::H256;
use sp_inherents::{InherentData, InherentIdentifier};

/// Inherent data provider for speculative messaging.
///
/// Created by the collator's block-building pipeline. When the collator
/// is about to propose a block, it drains queued inbound message metadata
/// and wraps it in this provider so the runtime can include the
/// `receive_messages_inherent` call.
pub struct SpecMsgInherentDataProvider {
	entries: InherentType,
}

impl SpecMsgInherentDataProvider {
	/// Create a new provider with the given entries.
	///
	/// Each entry is `(source_para_id, message_count, provides_root)`.
	pub fn new(entries: Vec<(ParaId, u64, H256)>) -> Self {
		Self { entries }
	}

	/// Create an empty provider (no messages received).
	pub fn empty() -> Self {
		Self { entries: Vec::new() }
	}
}

#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for SpecMsgInherentDataProvider {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut InherentData,
	) -> Result<(), sp_inherents::Error> {
		if !self.entries.is_empty() {
			inherent_data.put_data(INHERENT_IDENTIFIER, &self.entries)?;
		}
		Ok(())
	}

	async fn try_handle_error(
		&self,
		_identifier: &InherentIdentifier,
		_error: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}
