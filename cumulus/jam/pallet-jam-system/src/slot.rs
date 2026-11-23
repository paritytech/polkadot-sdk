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

//! The JAM slots a block is anchored at, and the JAM slot as the parachain's block-number clock.
//!
//! A JAM parachain has no relay block number. Its clock is the JAM slot named by the block's own
//! `JamParent` digest: [`JamSlotNumber`] is a [`BlockNumberProvider`] whose current block number
//! is that digest's lookup-anchor slot, for the places a relay parachain uses the relay block
//! number.
//!
//! The digest carries the anchors as hashes, and a runtime cannot turn a hash back into a slot.
//! The collator knows both slots when it builds the block and carries them in the digest next to
//! the hashes; `jam_validate_block` checks the whole digest against the refine context the service
//! serves, so a collator that lies about a slot is rejected at refine rather than silently
//! steering the clock.

use cumulus_primitives_core::{CumulusDigestItem, JamParent};
use sp_runtime::traits::BlockNumberProvider;

/// The `JamParent` digest carried by this block, if any.
pub(crate) fn jam_parent<T: frame_system::Config>() -> Option<JamParent> {
	CumulusDigestItem::find_jam_parent_info(&frame_system::Pallet::<T>::digest())
}

/// The refine lookup-anchor slot named by this block's `JamParent` digest.
///
/// This is the slot that names the collator's round-robin position and the one [`JamSlotNumber`]
/// reports as the block number. `None` when the block carries no `JamParent` digest.
pub fn lookup_anchor_slot<T: frame_system::Config>() -> Option<u32> {
	jam_parent::<T>().map(|parent| parent.lookup_anchor_slot)
}

/// The refine anchor slot named by this block's `JamParent` digest.
///
/// This is the clock the upgrade bookkeeping must use. [`lookup_anchor_slot`] deliberately returns
/// the other one: the lookup anchor trails the anchor and is not the right clock for a deadline
/// measured in anchor slots. `None` when the block carries no `JamParent` digest.
pub fn anchor_slot<T: frame_system::Config>() -> Option<u32> {
	jam_parent::<T>().map(|parent| parent.anchor_slot)
}

/// A [`BlockNumberProvider`] returning the JAM lookup-anchor slot during JAM block execution.
///
/// A validated JAM block always carries its `JamParent` digest — `jam_validate_block` aborts a
/// candidate without one — so a read outside block execution, where the digest is absent, is a
/// protocol error.
pub struct JamSlotNumber<T>(core::marker::PhantomData<T>);

impl<T: frame_system::Config> BlockNumberProvider for JamSlotNumber<T> {
	type BlockNumber = u32;

	fn current_block_number() -> u32 {
		lookup_anchor_slot::<T>().expect("a JAM block always carries its JamParent digest; qed")
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{new_test_ext, Test};

	fn push_jam_parent(anchor_slot: u32, lookup_anchor_slot: u32) {
		frame_system::Pallet::<Test>::deposit_log(
			CumulusDigestItem::JamParent {
				anchor: [1u8; 32].into(),
				anchor_slot,
				lookup_anchor: [2u8; 32].into(),
				lookup_anchor_slot,
			}
			.to_digest_item(),
		);
	}

	#[test]
	fn jam_parent_digest_yields_the_lookup_anchor_slot() {
		let mut ext = new_test_ext();
		ext.execute_with(|| {
			push_jam_parent(111, 222);

			assert_eq!(lookup_anchor_slot::<Test>(), Some(222));
			assert_eq!(anchor_slot::<Test>(), Some(111));
		});
	}

	#[test]
	fn missing_jam_parent_digest_yields_no_slot() {
		let mut ext = new_test_ext();
		ext.execute_with(|| {
			assert_eq!(lookup_anchor_slot::<Test>(), None);
			assert_eq!(anchor_slot::<Test>(), None);
		});
	}
}
