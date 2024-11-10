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

//! Aura-related primitives for cumulus parachain collators.

use codec::Codec;
use cumulus_primitives_aura::AuraUnincludedSegmentApi;
use sp_consensus_aura::AuraApi;
use sp_runtime::{
	app_crypto::{AppCrypto, AppPair, AppSignature, Pair},
	traits::Block as BlockT,
};

/// Convenience trait for defining the basic bounds of an `AuraId`.
pub trait AuraIdT: AppCrypto<Pair = Self::BoundedPair> + Codec + Send {
	/// Extra bounds for the `Pair`.
	type BoundedPair: AppPair + AppCrypto<Signature = Self::BoundedSignature>;

	/// Extra bounds for the `Signature`.
	type BoundedSignature: AppSignature
		+ TryFrom<Vec<u8>>
		+ std::hash::Hash
		+ sp_runtime::traits::Member
		+ Codec;
}

impl<T> AuraIdT for T
where
	T: AppCrypto + Codec + Send + Sync,
	<<T as AppCrypto>::Pair as AppCrypto>::Signature:
		TryFrom<Vec<u8>> + std::hash::Hash + sp_runtime::traits::Member + Codec,
{
	type BoundedPair = <T as AppCrypto>::Pair;
	type BoundedSignature = <<T as AppCrypto>::Pair as AppCrypto>::Signature;
}

/// Convenience trait for defining the basic bounds of a parachain runtime that supports
/// the Aura consensus.
pub trait AuraRuntimeApi<Block: BlockT, AuraId: AuraIdT>:
	sp_api::ApiExt<Block>
	+ AuraApi<Block, <AuraId::BoundedPair as Pair>::Public>
	+ AuraUnincludedSegmentApi<Block>
	+ Sized
{
	/// Check if the runtime has the Aura API.
	fn has_aura_api(&self, at: Block::Hash) -> bool {
		self.has_api::<dyn AuraApi<Block, <AuraId::BoundedPair as Pair>::Public>>(at)
			.unwrap_or(false)
	}
}

impl<T, Block: BlockT, AuraId: AuraIdT> AuraRuntimeApi<Block, AuraId> for T where
	T: sp_api::ApiExt<Block>
		+ AuraApi<Block, <AuraId::BoundedPair as Pair>::Public>
		+ AuraUnincludedSegmentApi<Block>
{
}
