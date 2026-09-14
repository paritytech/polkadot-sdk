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

//! The [`AdditionalDataFinalizer`] committing the carried JAM state proof under
//! [`JAM_PROOF_KEY`](crate::JAM_PROOF_KEY).
//!
//! The commitment is `sp_additional_data::hash_value` of the exact bytes the `JAM_PROOF_KEY`
//! entry carries in the additional-data map, so the digest recomputed from the carried map on
//! import matches the one committed at authoring — one finalizer shape for both sides of the
//! channel. The parachain runtime mirrors this shape (`validate_block_core::JamProofFinalizer`),
//! and the reader counterpart lives in this crate's [`jam_proof`] module.

use sp_additional_data::AdditionalDataFinalizer;

/// The commit side of a [`JAM_PROOF_KEY`](crate::JAM_PROOF_KEY) entry.
#[derive(Debug)]
pub struct JamProofFinalizer {
	pub commitment: [u8; 32],
}

impl AdditionalDataFinalizer for JamProofFinalizer {
	fn finalize(&self) -> Option<[u8; 32]> {
		Some(self.commitment)
	}
}
