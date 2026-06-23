// This file is part of Substrate.
//
// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Context passed to [`CreateInherentDataProviders`](crate::CreateInherentDataProviders) when
//! building inherent data on the client.

use sp_runtime::traits::Block as BlockT;

/// Describes whether inherent data is being created for block production or import.
///
/// Block import paths can pass the verified pre-header of the block under import so that
/// inherent data providers can recreate verification data that depends on header digests.
/// Block production paths use [`Self::Proposing`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InherentContext<Block: BlockT> {
	/// Inherent data for a block being authored locally.
	Proposing,
	/// Inherent data for a block being imported.
	Verifying {
		/// Header before consensus post-digests (e.g. the seal) are applied.
		header: Block::Header,
		/// Hash of the block including post-digests not yet applied to `header`.
		post_hash: Block::Hash,
	},
}
