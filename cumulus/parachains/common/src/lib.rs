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

#![cfg_attr(not(feature = "std"), no_std)]

pub mod impls;
pub mod kusama;
pub mod polkadot;
pub mod rococo;
pub mod westend;
pub mod xcm_config;
pub use constants::*;
pub use opaque::*;
pub use types::*;

/// Common types of parachains.
mod types {
	use sp_runtime::traits::{IdentifyAccount, Verify};

	/// An index to a block.
	pub type BlockNumber = u32;

	/// Alias to 512-bit hash when used in the context of a transaction signature on the chain.
	pub type Signature = sp_runtime::MultiSignature;

	/// Some way of identifying an account on the chain. We intentionally make it equivalent
	/// to the public key of our transaction signing scheme.
	pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

	/// The type for looking up accounts. We don't expect more than 4 billion of them, but you
	/// never know...
	pub type AccountIndex = u32;

	/// Balance of an account.
	pub type Balance = u128;

	/// Index of a transaction in the chain.
	pub type Nonce = u32;

	/// A hash of some data used by the chain.
	pub type Hash = sp_core::H256;

	/// Digest item type.
	pub type DigestItem = sp_runtime::generic::DigestItem;

	// Aura consensus authority.
	pub type AuraId = sp_consensus_aura::sr25519::AuthorityId;

	// Aura consensus authority used by Asset Hub Polkadot.
	//
	// Because of registering the authorities with an ed25519 key before switching from Shell
	// to Asset Hub Polkadot, we were required to deploy a hotfix that changed Asset Hub Polkadot's
	// Aura keys to ed22519. In the future that may change again.
	pub type AssetHubPolkadotAuraId = sp_consensus_aura::ed25519::AuthorityId;

	// Id used for identifying assets.
	pub type AssetIdForTrustBackedAssets = u32;
}

/// Common constants of parachains.
mod constants {
	use super::types::BlockNumber;
	use frame_support::weights::{constants::WEIGHT_REF_TIME_PER_SECOND, Weight};
	use sp_runtime::Perbill;
	/// This determines the average expected block time that we are targeting. Blocks will be
	/// produced at a minimum duration defined by `SLOT_DURATION`. `SLOT_DURATION` is picked up by
	/// `pallet_timestamp` which is in turn picked up by `pallet_aura` to implement `fn
	/// slot_duration()`.
	///
	/// Change this to adjust the block time.
	pub const MILLISECS_PER_BLOCK: u64 = 12000;
	pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;

	// Time is measured by number of blocks.
	pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
	pub const HOURS: BlockNumber = MINUTES * 60;
	pub const DAYS: BlockNumber = HOURS * 24;

	/// We assume that ~5% of the block weight is consumed by `on_initialize` handlers. This is
	/// used to limit the maximal weight of a single extrinsic.
	pub const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(5);
	/// We allow `Normal` extrinsics to fill up the block up to 75%, the rest can be used by
	/// Operational  extrinsics.
	pub const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);

	/// We allow for 0.5 seconds of compute with a 6 second average block time.
	pub const MAXIMUM_BLOCK_WEIGHT: Weight = Weight::from_parts(
		WEIGHT_REF_TIME_PER_SECOND.saturating_div(2),
		polkadot_primitives::MAX_POV_SIZE as u64,
	);
}

/// Opaque types. These are used by the CLI to instantiate machinery that don't need to know
/// the specifics of the runtime. They can then be made to be agnostic over specific formats
/// of data like extrinsics, allowing for them to continue syncing the network through upgrades
/// to even the core data structures.
pub mod opaque {
	use super::*;
	use sp_runtime::{generic, traits::BlakeTwo256};

	pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
	/// Opaque block header type.
	pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
	/// Opaque block type.
	pub type Block = generic::Block<Header, UncheckedExtrinsic>;
	/// Opaque block identifier type.
	pub type BlockId = generic::BlockId<Block>;
}
