// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Primitives of parachains module.

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]

pub use bp_header_chain::StoredHeaderData;

use bp_polkadot_core::{
	parachains::{ParaHash, ParaHead, ParaHeadsProof, ParaId},
	BlockNumber as RelayBlockNumber, Hash as RelayBlockHash,
};
use bp_runtime::{
	BlockNumberOf, Chain, HashOf, HeaderOf, Parachain, StorageDoubleMapKeyProvider,
	StorageMapKeyProvider,
};
use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{Blake2_128Concat, Twox64Concat};
use scale_info::TypeInfo;
use sp_core::storage::StorageKey;
use sp_runtime::{traits::Header as HeaderT, RuntimeDebug};
use sp_std::{marker::PhantomData, prelude::*};

/// Best known parachain head hash.
#[derive(Clone, Decode, Encode, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct BestParaHeadHash {
	/// Number of relay block where this head has been read.
	///
	/// Parachain head is opaque to relay chain. So we can't simply decode it as a header of
	/// parachains and call `block_number()` on it. Instead, we're using the fact that parachain
	/// head is always built on top of previous head (because it is blockchain) and relay chain
	/// always imports parachain heads in order. What it means for us is that at any given
	/// **finalized** relay block `B`, head of parachain will be ancestor (or the same) of all
	/// parachain heads available at descendants of `B`.
	pub at_relay_block_number: RelayBlockNumber,
	/// Hash of parachain head.
	pub head_hash: ParaHash,
}

/// Best known parachain head as it is stored in the runtime storage.
#[derive(Decode, Encode, MaxEncodedLen, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ParaInfo {
	/// Best known parachain head hash.
	pub best_head_hash: BestParaHeadHash,
	/// Current ring buffer position for this parachain.
	pub next_imported_hash_position: u32,
}

/// Returns runtime storage key of given parachain head at the source chain.
///
/// The head is stored by the `paras` pallet in the `Heads` map.
pub fn parachain_head_storage_key_at_source(
	paras_pallet_name: &str,
	para_id: ParaId,
) -> StorageKey {
	bp_runtime::storage_map_final_key::<Twox64Concat>(paras_pallet_name, "Heads", &para_id.encode())
}

/// Can be use to access the runtime storage key of the parachains info at the target chain.
///
/// The info is stored by the `pallet-bridge-parachains` pallet in the `ParasInfo` map.
pub struct ParasInfoKeyProvider;
impl StorageMapKeyProvider for ParasInfoKeyProvider {
	const MAP_NAME: &'static str = "ParasInfo";

	type Hasher = Blake2_128Concat;
	type Key = ParaId;
	type Value = ParaInfo;
}

/// Can be use to access the runtime storage key of the parachain head at the target chain.
///
/// The head is stored by the `pallet-bridge-parachains` pallet in the `ImportedParaHeads` map.
pub struct ImportedParaHeadsKeyProvider;
impl StorageDoubleMapKeyProvider for ImportedParaHeadsKeyProvider {
	const MAP_NAME: &'static str = "ImportedParaHeads";

	type Hasher1 = Blake2_128Concat;
	type Key1 = ParaId;
	type Hasher2 = Blake2_128Concat;
	type Key2 = ParaHash;
	type Value = ParaStoredHeaderData;
}

/// Stored data of the parachain head. It is encoded version of the
/// `bp_runtime::StoredHeaderData` structure.
///
/// We do not know exact structure of the parachain head, so we always store encoded version
/// of the `bp_runtime::StoredHeaderData`. It is only decoded when we talk about specific parachain.
#[derive(Clone, Decode, Encode, PartialEq, RuntimeDebug, TypeInfo)]
pub struct ParaStoredHeaderData(pub Vec<u8>);

impl ParaStoredHeaderData {
	/// Decode stored parachain head data.
	pub fn decode_parachain_head_data<C: Chain>(
		&self,
	) -> Result<StoredHeaderData<BlockNumberOf<C>, HashOf<C>>, codec::Error> {
		StoredHeaderData::<BlockNumberOf<C>, HashOf<C>>::decode(&mut &self.0[..])
	}
}

/// Stored parachain head data builder.
pub trait ParaStoredHeaderDataBuilder {
	/// Return number of parachains that are supported by this builder.
	fn supported_parachains() -> u32;

	/// Try to build head data from encoded head of parachain with given id.
	fn try_build(para_id: ParaId, para_head: &ParaHead) -> Option<ParaStoredHeaderData>;
}

/// Helper for using single parachain as `ParaStoredHeaderDataBuilder`.
pub struct SingleParaStoredHeaderDataBuilder<C: Parachain>(PhantomData<C>);

impl<C: Parachain> ParaStoredHeaderDataBuilder for SingleParaStoredHeaderDataBuilder<C> {
	fn supported_parachains() -> u32 {
		1
	}

	fn try_build(para_id: ParaId, para_head: &ParaHead) -> Option<ParaStoredHeaderData> {
		if para_id == ParaId(C::PARACHAIN_ID) {
			let header = HeaderOf::<C>::decode(&mut &para_head.0[..]).ok()?;
			return Some(ParaStoredHeaderData(
				StoredHeaderData { number: *header.number(), state_root: *header.state_root() }
					.encode(),
			))
		}
		None
	}
}

// Tries to build header data from each tuple member, short-circuiting on first successful one.
#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
#[tuple_types_custom_trait_bound(Parachain)]
impl ParaStoredHeaderDataBuilder for C {
	fn supported_parachains() -> u32 {
		let mut result = 0;
		for_tuples!( #(
			result += SingleParaStoredHeaderDataBuilder::<C>::supported_parachains();
		)* );
		result
	}

	fn try_build(para_id: ParaId, para_head: &ParaHead) -> Option<ParaStoredHeaderData> {
		for_tuples!( #(
			let maybe_para_head = SingleParaStoredHeaderDataBuilder::<C>::try_build(para_id, para_head);
			if let Some(maybe_para_head) = maybe_para_head {
				return Some(maybe_para_head);
			}
		)* );

		None
	}
}

/// A minimized version of `pallet-bridge-parachains::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeParachainCall {
	/// `pallet-bridge-parachains::Call::submit_parachain_heads`
	#[codec(index = 0)]
	submit_parachain_heads {
		/// Relay chain block, for which we have submitted the `parachain_heads_proof`.
		at_relay_block: (RelayBlockNumber, RelayBlockHash),
		/// Parachain identifiers and their head hashes.
		parachains: Vec<(ParaId, ParaHash)>,
		/// Parachain heads proof.
		parachain_heads_proof: ParaHeadsProof,
	},
}
