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

//! The block seal on JAM: the JAM authorizer authorizes the block's author.
//!
//! The parachain has no authority set of its own that could drift away from the authorizers JAM
//! runs. Instead the seal carries a [`BlockAuthorization`], and the block is valid if its
//! authorizer is in an auth pool at the block's anchor and authorizes the seal-less header hash.
//!
//! A JAM runtime uses [`BlockExecutor`] in place of `cumulus_pallet_aura_ext::BlockExecutor`:
//!
//! ```ignore
//! cumulus_pallet_parachain_system::register_validate_block! {
//!     Runtime = Runtime,
//!     BlockExecutor = cumulus_jam_block_authorization::BlockExecutor::<Runtime, Executive>,
//! }
//! ```

#![cfg(jam)]
#![no_std]

use codec::DecodeAll;
use cumulus_jam_state_reader::jam_state::jam_state_read;
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::traits::{ExecuteBlock, Get};
use parachain_service_core::{
	authorize_block::authorize_block,
	block_authorization::{BlockAuthorization, BLOCK_AUTHORIZATION_ENGINE_ID},
	types::ParaId,
};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, LazyBlock},
	DigestItem,
};

/// The block executor used when validating a block on JAM.
///
/// Verifies the block's JAM block authorization seal, then hands the block to `I`.
pub struct BlockExecutor<T, I>(core::marker::PhantomData<(T, I)>);

impl<Block, T, I> ExecuteBlock<Block> for BlockExecutor<T, I>
where
	Block: BlockT,
	T: cumulus_pallet_parachain_system::Config,
	I: ExecuteBlock<Block>,
{
	fn verify_and_remove_seal(block: &mut <Block as BlockT>::LazyBlock) {
		verify_and_remove_seal::<T, _>(block.header_mut());
	}

	fn execute_verified_block(block: Block::LazyBlock) {
		I::execute_verified_block(block);
	}
}

/// Remove the JAM block authorization seal from `header` and verify it.
///
/// Must run with the block's JAM state reader armed: the auth pools and the authorizer code are
/// read from JAM state at the block's anchor.
fn verify_and_remove_seal<T: cumulus_pallet_parachain_system::Config, H: HeaderT>(header: &mut H) {
	let mut seal = None;
	header.digest_mut().logs.retain(|item| match item {
		DigestItem::Seal(id, data) if *id == BLOCK_AUTHORIZATION_ENGINE_ID => {
			assert!(
				seal.replace(data.clone()).is_none(),
				"Found multiple JAM block authorization seals"
			);
			false
		},
		_ => true,
	});
	let seal = seal.expect("Could not find the JAM block authorization seal");
	let authorization = BlockAuthorization::decode_all(&mut &seal[..])
		.expect("Could not decode the JAM block authorization seal");

	let slot = CumulusDigestItem::find_jam_parent_info(header.digest())
		.expect("A JAM block carries its JamParent digest")
		.lookup_anchor_slot;
	let para_id = ParaId::from(u32::from(T::SelfParaId::get()));
	let pre_hash: [u8; 32] = header
		.hash()
		.as_ref()
		.try_into()
		.expect("Parachain block hashes are 32 bytes; qed");

	if let Err(error) =
		authorize_block(|key| jam_state_read(key), para_id, &authorization, pre_hash, slot)
	{
		panic!("Invalid JAM block authorization: {error:?}");
	}
}
