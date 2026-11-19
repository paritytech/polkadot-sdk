// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! The block executor used when validating a PoV on JAM.

use crate::{Config, Pallet};
use cumulus_primitives_core::CumulusDigestItem;
use frame_support::traits::ExecuteBlock;
use sp_consensus_aura::digests::CompatibleDigestItem;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, LazyBlock as _},
	RuntimeAppPublic,
};

/// Like aura-ext's `BlockExecutor`, but checks the Aura seal against the set JAM authorizes at the
/// block's anchor slot (its `JamParent` digest) instead of the set of the parent block.
///
/// The next set is announced a session ahead, so the first block anchored at a switch slot is
/// already sealed by the new set, exactly when JAM switches its authorizer.
pub struct BlockExecutor<T, I>(core::marker::PhantomData<(T, I)>);

impl<Block, T, I> ExecuteBlock<Block> for BlockExecutor<T, I>
where
	Block: BlockT,
	T: Config,
	I: ExecuteBlock<Block>,
{
	fn verify_and_remove_seal(block: &mut <Block as BlockT>::LazyBlock) {
		let header = block.header_mut();
		let anchor_slot = CumulusDigestItem::find_jam_parent_info(header.digest())
			.expect("a JAM block always carries its JamParent digest; qed")
			.anchor_slot;
		// Read before executing the block, from the parent's state.
		let authorities = Pallet::<T>::authorities_at(anchor_slot);

		let mut seal = None;
		header.digest_mut().logs.retain(|s| {
			let s = CompatibleDigestItem::<
				<T::AuthorityId as RuntimeAppPublic>::Signature,
			>::as_aura_seal(s);
			match (s, seal.is_some()) {
				(Some(_), true) => panic!("Found multiple AuRa seal digests"),
				(None, _) => true,
				(Some(s), false) => {
					seal = Some(s);
					false
				},
			}
		});
		let seal = seal.expect("Could not find an AuRa seal digest!");

		let slot = header
			.digest()
			.logs()
			.iter()
			.find_map(|d| CompatibleDigestItem::<()>::as_aura_pre_digest(d))
			.expect("Could not find an AuRa pre-digest!");
		if authorities.is_empty() {
			panic!("No AuRa authorities at anchor slot {anchor_slot}");
		}
		let author = (*slot % authorities.len() as u64) as usize;

		let pre_hash = header.hash();
		if !authorities
			.get(author)
			.unwrap_or_else(|| {
				panic!(
					"Invalid AuRa author index {author} for the authorities at anchor slot \
					 {anchor_slot}: {authorities:?}"
				)
			})
			.verify(&pre_hash, &seal)
		{
			panic!("Invalid AuRa seal");
		}
	}

	fn execute_verified_block(block: Block::LazyBlock) {
		I::execute_verified_block(block);
	}
}
