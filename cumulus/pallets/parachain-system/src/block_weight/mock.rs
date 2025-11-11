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

use super::{transaction_extension::DynamicMaxBlockWeight, *};
use crate as parachain_system;
use codec::Compact;
use cumulus_primitives_core::{
	BundleInfo, ClaimQueueOffset, CoreInfo, CoreSelector, CumulusDigestItem,
};
use frame_support::{
	construct_runtime, derive_impl,
	dispatch::DispatchClass,
	parameter_types,
	traits::PreInherents,
	weights::{
		constants::{BlockExecutionWeight, ExtrinsicBaseWeight},
		Weight,
	},
};
use frame_system::limits::BlockWeights;
use sp_core::ConstU32;
use sp_io;
use sp_runtime::{
	generic::{self, UncheckedExtrinsic},
	testing::UintAuthorityId,
	BuildStorage, Perbill,
};

const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(1);

/// A simple call, which one doesn't matter.
pub const CALL: &RuntimeCall =
	&RuntimeCall::System(frame_system::Call::set_heap_pages { pages: 0u64 });

pub type Extrinsic = UncheckedExtrinsic<
	UintAuthorityId,
	RuntimeCall,
	UintAuthorityId,
	DynamicMaxBlockWeight<Runtime, (), ConstU32<TARGET_BLOCK_RATE>>,
>;

pub type Block =
	generic::Block<generic::Header<u64, <Runtime as frame_system::Config>::Hashing>, Extrinsic>;

pub const TARGET_BLOCK_RATE: u32 = 12;

#[docify::export(tx_extension_setup)]
pub type TxExtension = DynamicMaxBlockWeight<
	Runtime,
	// Here you need to set the other extensions that are required by your runtime...
	(
		frame_system::AuthorizeCall<Runtime>,
		frame_system::CheckNonZeroSender<Runtime>,
		frame_system::CheckSpecVersion<Runtime>,
		frame_system::CheckGenesis<Runtime>,
		frame_system::CheckEra<Runtime>,
		frame_system::CheckNonce<Runtime>,
		frame_system::CheckWeight<Runtime>,
	),
	ConstU32<TARGET_BLOCK_RATE>,
>;

#[allow(dead_code)]
type NotDeadCode = TxExtension;

#[docify::export_content(max_block_weight_setup)]
mod max_block_weight_setup {
	use super::*;

	type MaximumBlockWeight = MaxParachainBlockWeight<Runtime, ConstU32<TARGET_BLOCK_RATE>>;

	parameter_types! {
		pub RuntimeBlockWeights: BlockWeights = BlockWeights::builder()
			.base_block(BlockExecutionWeight::get())
			.for_class(DispatchClass::all(), |weights| {
				weights.base_extrinsic = ExtrinsicBaseWeight::get();
			})
			.for_class(DispatchClass::Normal, |weights| {
				weights.max_total = Some(MaximumBlockWeight::get());
			})
			.for_class(DispatchClass::Operational, |weights| {
				weights.max_total = Some(MaximumBlockWeight::get());
			})
			.avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
			.build_or_panic();
	}
}

// Configure a mock runtime to test the functionality
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
#[docify::export(pre_inherents_setup)]
impl frame_system::Config for Runtime {
	// Setup the block weight.
	type BlockWeights = max_block_weight_setup::RuntimeBlockWeights;
	// Set the `PreInherents` hook.
	type PreInherents = DynamicMaxBlockWeightHooks<Runtime, ConstU32<TARGET_BLOCK_RATE>>;

	// Just required to make it compile, but not that important for this example here.
	type Block = Block;
	type OnSetCode = crate::ParachainSetCode<Runtime>;
	type AccountId = u64;
	type Lookup = UintAuthorityId;
	// Rest of the types are omitted here.
}

impl crate::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type OnSystemEvent = ();
	type SelfParaId = ();
	type OutboundXcmpMessageSource = ();
	type DmpQueue = ();
	type ReservedDmpWeight = ();
	type XcmpMessageHandler = ();
	type ReservedXcmpWeight = ();
	type CheckAssociatedRelayNumber = crate::RelayNumberStrictlyIncreases;
	type WeightInfo = ();
	type ConsensusHook = crate::ExpectParentIncluded;
	type RelayParentOffset = ();
}

// Include test_pallet module inline
#[frame_support::pallet(dev_mode)]
pub mod test_pallet {
	use frame_support::{
		dispatch::DispatchClass, pallet_prelude::*, weights::constants::WEIGHT_REF_TIME_PER_SECOND,
	};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config + crate::Config {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// A heavy call with Normal dispatch class that consumes significant weight.
		#[pallet::weight((Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, 1024 * 1024), DispatchClass::Normal))]
		pub fn heavy_call_normal(_: OriginFor<T>) -> DispatchResult {
			Ok(())
		}

		/// A heavy call with Operational dispatch class that consumes significant weight.
		#[pallet::weight((Weight::from_parts(WEIGHT_REF_TIME_PER_SECOND, 1024 * 1024), DispatchClass::Operational))]
		pub fn heavy_call_operational(_: OriginFor<T>) -> DispatchResult {
			Ok(())
		}
	}
}

impl test_pallet::Config for Runtime {}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		ParachainSystem: parachain_system,
		TestPallet: test_pallet,
	}
);

/// Executive: handles dispatch to the various modules.
pub type Executive =
	frame_executive::Executive<Runtime, Block, frame_system::ChainContext<Runtime>, Runtime, ()>;

/// Builder for test externalities
#[cfg(test)]
pub struct TestExtBuilder {
	num_cores: Option<u16>,
	bundle_index: Option<u8>,
	bundle_maybe_last: bool,
}

#[cfg(test)]
impl Default for TestExtBuilder {
	fn default() -> Self {
		sp_tracing::try_init_simple();

		Self { num_cores: None, bundle_index: None, bundle_maybe_last: false }
	}
}

#[cfg(test)]
impl TestExtBuilder {
	/// Create a new builder
	pub fn new() -> Self {
		Self::default()
	}

	/// Set the number of cores
	pub fn number_of_cores(mut self, num_cores: u16) -> Self {
		self.num_cores = Some(num_cores);
		self
	}

	/// Set this as the first block in the core (bundle index = 0)
	pub fn first_block_in_core(mut self, is_first: bool) -> Self {
		if is_first {
			self.bundle_index = Some(0);
		} else if self.bundle_index.is_none() {
			// If not first and no bundle index set, default to index 1
			self.bundle_index = Some(1);
		}
		self
	}

	/// Build the test externalities
	pub fn build(self) -> sp_io::TestExternalities {
		let storage = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();
		let mut ext = sp_io::TestExternalities::from(storage);

		ext.execute_with(|| {
			// Add core info if specified
			if let Some(num_cores) = self.num_cores {
				let core_info = CoreInfo {
					selector: CoreSelector(0),
					claim_queue_offset: ClaimQueueOffset(0),
					number_of_cores: Compact(num_cores),
				};
				let digest = CumulusDigestItem::CoreInfo(core_info).to_digest_item();
				frame_system::Pallet::<Runtime>::deposit_log(digest);
			}

			// Add bundle info if specified
			if let Some(bundle_index) = self.bundle_index {
				let bundle_info =
					BundleInfo { index: bundle_index, maybe_last: self.bundle_maybe_last };
				let digest = CumulusDigestItem::BundleInfo(bundle_info).to_digest_item();
				frame_system::Pallet::<Runtime>::deposit_log(digest);
			}
		});

		ext
	}
}

/// Helper to check if UseFullCore digest was deposited
pub fn has_use_full_core_digest() -> bool {
	let digest = frame_system::Pallet::<Runtime>::digest();
	CumulusDigestItem::contains_use_full_core(&digest)
}

/// Helper to register weight as consumed (simulating on_initialize)
pub fn register_weight(weight: Weight, class: DispatchClass) {
	frame_system::Pallet::<Runtime>::register_extra_weight_unchecked(weight, class);
}

/// Emulates what happes after `initialize_block` finished.
pub fn initialize_block_finished() {
	System::set_block_consumed_resources(Weight::zero(), 0);
	System::note_finished_initialize();
	<Runtime as frame_system::Config>::PreInherents::pre_inherents();
	System::note_inherents_applied();
}
