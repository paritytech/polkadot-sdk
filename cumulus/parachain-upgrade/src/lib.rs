// Copyright 2020 Parity Technologies (UK) Ltd.
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

#![cfg_attr(not(feature = "std"), no_std)]

//! Enable Parachain validation function upgrades.
//!
//! Allow a user to determine when a parachain validation function upgrade
//! is legal, and perform the upgrade, triggering runtime events
//! for both storing and applying the new validation function.
//!
//! Depends on no external pallets or traits.
//!
//! This pallet depends on certain environmental conditions provided by
//! Cumulus. It will not work outside a Cumulus Parachain.
//!
//! Users must ensure that they register this pallet as an inherent provider.

use cumulus_primitives::{
	inherents::VALIDATION_DATA_IDENTIFIER as INHERENT_IDENTIFIER,
	well_known_keys::{NEW_VALIDATION_CODE, VALIDATION_DATA},
	OnValidationData, ValidationData,
};
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage, ensure, storage,
	weights::{DispatchClass, Weight},
};
use frame_system::{ensure_none, ensure_root};
use parachain::primitives::RelayChainBlockNumber;
use sp_core::storage::well_known_keys;
use sp_inherents::{InherentData, InherentIdentifier, ProvideInherent};
use sp_std::vec::Vec;

type System<T> = frame_system::Module<T>;

/// The pallet's configuration trait.
pub trait Trait: frame_system::Trait {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Trait>::Event>;

	/// Something which can be notified when the validation data is set.
	type OnValidationData: OnValidationData;
}

// This pallet's storage items.
decl_storage! {
	trait Store for Module<T: Trait> as ParachainUpgrade {
		// we need to store the new validation function for the span between
		// setting it and applying it.
		PendingValidationFunction get(fn new_validation_function):
			Option<(RelayChainBlockNumber, Vec<u8>)>;

		/// Were the [`ValidationData`] updated in this block?
		DidUpdateValidationData: bool;

		/// Were the validation data set to notify the relay chain?
		DidSetValidationCode: bool;
	}
}

// The pallet's dispatchable functions.
decl_module! {
	pub struct Module<T: Trait> for enum Call where origin: T::Origin {
		// Initializing events
		// this is needed only if you are using events in your pallet
		fn deposit_event() = default;

		// TODO: figure out a better weight than this
		#[weight = (0, DispatchClass::Operational)]
		pub fn schedule_upgrade(origin, validation_function: Vec<u8>) {
			ensure_root(origin)?;
			System::<T>::can_set_code(&validation_function)?;
			Self::schedule_upgrade_impl(validation_function)?;
		}

		/// Schedule a validation function upgrade without further checks.
		///
		/// Same as [`Module::schedule_upgrade`], but without checking that the new `validation_function`
		/// is correct. This makes it more flexible, but also opens the door to easily brick the chain.
		#[weight = (0, DispatchClass::Operational)]
		pub fn schedule_upgrade_without_checks(origin, validation_function: Vec<u8>) {
			ensure_root(origin)?;
			Self::schedule_upgrade_impl(validation_function)?;
		}

		/// Set the current validation data.
		///
		/// This should be invoked exactly once per block. It will panic at the finalization
		/// phase if the call was not invoked.
		///
		/// The dispatch origin for this call must be `Inherent`
		///
		/// As a side effect, this function upgrades the current validation function
		/// if the appropriate time has come.
		#[weight = (0, DispatchClass::Mandatory)]
		fn set_validation_data(origin, vfp: ValidationData) {
			ensure_none(origin)?;
			assert!(!DidUpdateValidationData::exists(), "ValidationData must be updated only once in a block");

			// initialization logic: we know that this runs exactly once every block,
			// which means we can put the initialization logic here to remove the
			// sequencing problem.
			if let Some((apply_block, validation_function)) = PendingValidationFunction::get() {
				if vfp.persisted.block_number >= apply_block {
					PendingValidationFunction::kill();
					Self::put_parachain_code(&validation_function);
					Self::deposit_event(Event::ValidationFunctionApplied(vfp.persisted.block_number));
				}
			}

			storage::unhashed::put(VALIDATION_DATA, &vfp);
			DidUpdateValidationData::put(true);
			<T::OnValidationData as OnValidationData>::on_validation_data(vfp);
		}

		fn on_finalize() {
			assert!(DidUpdateValidationData::take(), "VFPs must be updated once per block");
			DidSetValidationCode::take();
		}

		fn on_initialize(n: T::BlockNumber) -> Weight {
			// To prevent removing `NEW_VALIDATION_CODE` that was set by another `on_initialize` like
			// for example from scheduler, we only kill the storage entry if it was not yet updated
			// in the current block.
			if !DidSetValidationCode::get() {
				storage::unhashed::kill(NEW_VALIDATION_CODE);
			}

			storage::unhashed::kill(VALIDATION_DATA);

			0
		}
	}
}

impl<T: Trait> Module<T> {
	/// Get validation data.
	///
	/// Returns `Some(_)` after the inherent set the data for the current block.
	pub fn validation_data() -> Option<ValidationData> {
		storage::unhashed::get(VALIDATION_DATA)
	}

	/// Put a new validation function into a particular location where polkadot
	/// monitors for updates. Calling this function notifies polkadot that a new
	/// upgrade has been scheduled.
	fn notify_polkadot_of_pending_upgrade(code: &[u8]) {
		storage::unhashed::put_raw(NEW_VALIDATION_CODE, code);
		DidSetValidationCode::put(true);
	}

	/// Put a new validation function into a particular location where this
	/// parachain will execute it on subsequent blocks.
	fn put_parachain_code(code: &[u8]) {
		storage::unhashed::put_raw(well_known_keys::CODE, code);
	}

	/// `true` when a code upgrade is currently legal
	pub fn can_set_code() -> bool {
		Self::validation_data()
			.map(|vfp| vfp.transient.code_upgrade_allowed.is_some())
			.unwrap_or_default()
	}

	/// The maximum code size permitted, in bytes.
	pub fn max_code_size() -> Option<u32> {
		Self::validation_data().map(|vfp| vfp.transient.max_code_size)
	}

	/// The implementation of the runtime upgrade scheduling.
	fn schedule_upgrade_impl(
		validation_function: Vec<u8>,
	) -> frame_support::dispatch::DispatchResult {
		ensure!(
			!PendingValidationFunction::exists(),
			Error::<T>::OverlappingUpgrades
		);
		let vfp = Self::validation_data().ok_or(Error::<T>::ValidationDataNotAvailable)?;
		ensure!(
			validation_function.len() <= vfp.transient.max_code_size as usize,
			Error::<T>::TooBig
		);
		let apply_block = vfp
			.transient
			.code_upgrade_allowed
			.ok_or(Error::<T>::ProhibitedByPolkadot)?;

		// When a code upgrade is scheduled, it has to be applied in two
		// places, synchronized: both polkadot and the individual parachain
		// have to upgrade on the same relay chain block.
		//
		// `notify_polkadot_of_pending_upgrade` notifies polkadot; the `PendingValidationFunction`
		// storage keeps track locally for the parachain upgrade, which will
		// be applied later.
		Self::notify_polkadot_of_pending_upgrade(&validation_function);
		PendingValidationFunction::put((apply_block, validation_function));
		Self::deposit_event(Event::ValidationFunctionStored(apply_block));

		Ok(())
	}
}

impl<T: Trait> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = sp_inherents::MakeFatalError<()>;
	const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		let data: ValidationData = data
			.get_data(&INHERENT_IDENTIFIER)
			.ok()
			.flatten()
			.expect("validation function params are always injected into inherent data; qed");

		Some(Call::set_validation_data(data))
	}
}

decl_event! {
	pub enum Event {
		// The validation function has been scheduled to apply as of the contained relay chain block number.
		ValidationFunctionStored(RelayChainBlockNumber),
		// The validation function was applied as of the contained relay chain block number.
		ValidationFunctionApplied(RelayChainBlockNumber),
	}
}

decl_error! {
	pub enum Error for Module<T: Trait> {
		/// Attempt to upgrade validation function while existing upgrade pending
		OverlappingUpgrades,
		/// Polkadot currently prohibits this parachain from upgrading its validation function
		ProhibitedByPolkadot,
		/// The supplied validation function has compiled into a blob larger than Polkadot is willing to run
		TooBig,
		/// The inherent which supplies the validation data did not run this block
		ValidationDataNotAvailable,
	}
}

/// tests for this pallet
#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use cumulus_primitives::{PersistedValidationData, TransientValidationData};
	use frame_support::{
		assert_ok,
		dispatch::UnfilteredDispatchable,
		impl_outer_event, impl_outer_origin, parameter_types,
		traits::{OnFinalize, OnInitialize},
		weights::Weight,
	};
	use frame_system::{InitKind, RawOrigin};
	use sp_core::H256;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
		Perbill,
	};
	use sp_version::RuntimeVersion;

	impl_outer_origin! {
		pub enum Origin for Test where system = frame_system {}
	}

	mod parachain_upgrade {
		pub use crate::Event;
	}

	impl_outer_event! {
		pub enum TestEvent for Test {
			frame_system<T>,
			parachain_upgrade,
		}
	}

	// For testing the pallet, we construct most of a mock runtime. This means
	// first constructing a configuration type (`Test`) which `impl`s each of the
	// configuration traits of modules we want to use.
	#[derive(Clone, Eq, PartialEq)]
	pub struct Test;
	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub const MaximumBlockWeight: Weight = 1024;
		pub const MaximumBlockLength: u32 = 2 * 1024;
		pub const AvailableBlockRatio: Perbill = Perbill::from_percent(75);
		pub Version: RuntimeVersion = RuntimeVersion {
			spec_name: sp_version::create_runtime_str!("test"),
			impl_name: sp_version::create_runtime_str!("system-test"),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: sp_version::create_apis_vec!([]),
			transaction_version: 1,
		};
	}
	impl frame_system::Trait for Test {
		type Origin = Origin;
		type Call = ();
		type Index = u64;
		type BlockNumber = u64;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = u64;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = TestEvent;
		type BlockHashCount = BlockHashCount;
		type MaximumBlockWeight = MaximumBlockWeight;
		type MaximumExtrinsicWeight = MaximumBlockWeight;
		type MaximumBlockLength = MaximumBlockLength;
		type AvailableBlockRatio = AvailableBlockRatio;
		type Version = Version;
		type PalletInfo = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type DbWeight = ();
		type BlockExecutionWeight = ();
		type ExtrinsicBaseWeight = ();
		type BaseCallFilter = ();
		type SystemWeightInfo = ();
	}
	impl Trait for Test {
		type Event = TestEvent;
		type OnValidationData = ();
	}

	type ParachainUpgrade = Module<Test>;

	// This function basically just builds a genesis storage key/value store according to
	// our desired mockup.
	fn new_test_ext() -> sp_io::TestExternalities {
		frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap()
			.into()
	}

	struct CallInWasm(Vec<u8>);

	impl sp_core::traits::CallInWasm for CallInWasm {
		fn call_in_wasm(
			&self,
			_wasm_code: &[u8],
			_code_hash: Option<Vec<u8>>,
			_method: &str,
			_call_data: &[u8],
			_ext: &mut dyn sp_externalities::Externalities,
			_missing_host_functions: sp_core::traits::MissingHostFunctions,
		) -> Result<Vec<u8>, String> {
			Ok(self.0.clone())
		}
	}

	fn wasm_ext() -> sp_io::TestExternalities {
		let version = RuntimeVersion {
			spec_name: "test".into(),
			spec_version: 2,
			impl_version: 1,
			..Default::default()
		};
		let call_in_wasm = CallInWasm(version.encode());

		let mut ext = new_test_ext();
		ext.register_extension(sp_core::traits::CallInWasmExt::new(call_in_wasm));
		ext
	}

	struct BlockTest {
		n: <Test as frame_system::Trait>::BlockNumber,
		within_block: Box<dyn Fn()>,
		after_block: Option<Box<dyn Fn()>>,
	}

	/// BlockTests exist to test blocks with some setup: we have to assume that
	/// `validate_block` will mutate and check storage in certain predictable
	/// ways, for example, and we want to always ensure that tests are executed
	/// in the context of some particular block number.
	#[derive(Default)]
	struct BlockTests {
		tests: Vec<BlockTest>,
		pending_upgrade: Option<RelayChainBlockNumber>,
		ran: bool,
		vfp_maker: Option<Box<dyn Fn(&BlockTests, RelayChainBlockNumber) -> ValidationData>>,
	}

	impl BlockTests {
		fn new() -> BlockTests {
			Default::default()
		}

		fn add_raw(mut self, test: BlockTest) -> Self {
			self.tests.push(test);
			self
		}

		fn add<F>(self, n: <Test as frame_system::Trait>::BlockNumber, within_block: F) -> Self
		where
			F: 'static + Fn(),
		{
			self.add_raw(BlockTest {
				n,
				within_block: Box::new(within_block),
				after_block: None,
			})
		}

		fn add_with_post_test<F1, F2>(
			self,
			n: <Test as frame_system::Trait>::BlockNumber,
			within_block: F1,
			after_block: F2,
		) -> Self
		where
			F1: 'static + Fn(),
			F2: 'static + Fn(),
		{
			self.add_raw(BlockTest {
				n,
				within_block: Box::new(within_block),
				after_block: Some(Box::new(after_block)),
			})
		}

		fn with_validation_data<F>(mut self, f: F) -> Self
		where
			F: 'static + Fn(&BlockTests, RelayChainBlockNumber) -> ValidationData,
		{
			self.vfp_maker = Some(Box::new(f));
			self
		}

		fn run(&mut self) {
			self.ran = true;
			wasm_ext().execute_with(|| {
				for BlockTest {
					n,
					within_block,
					after_block,
				} in self.tests.iter()
				{
					// clear pending updates, as applicable
					if let Some(upgrade_block) = self.pending_upgrade {
						if n >= &upgrade_block.into() {
							self.pending_upgrade = None;
						}
					}

					// begin initialization
					System::<Test>::initialize(
						&n,
						&Default::default(),
						&Default::default(),
						&Default::default(),
						InitKind::Full,
					);

					// now mess with the storage the way validate_block does
					let vfp = match self.vfp_maker {
						None => ValidationData {
							persisted: PersistedValidationData {
								block_number: *n as RelayChainBlockNumber,
								..Default::default()
							},
							transient: TransientValidationData {
								max_code_size: 10 * 1024 * 1024, // 10 mb
								code_upgrade_allowed: if self.pending_upgrade.is_some() {
									None
								} else {
									Some(*n as RelayChainBlockNumber + 1000)
								},
								..Default::default()
							},
						},
						Some(ref maker) => maker(self, *n as RelayChainBlockNumber),
					};
					storage::unhashed::put(VALIDATION_DATA, &vfp);
					storage::unhashed::kill(NEW_VALIDATION_CODE);

					// It is insufficient to push the validation function params
					// to storage; they must also be included in the inherent data.
					let inherent_data = {
						let mut inherent_data = InherentData::default();
						inherent_data
							.put_data(INHERENT_IDENTIFIER, &vfp)
							.expect("failed to put VFP inherent");
						inherent_data
					};

					// execute the block
					ParachainUpgrade::on_initialize(*n);
					ParachainUpgrade::create_inherent(&inherent_data)
						.expect("got an inherent")
						.dispatch_bypass_filter(RawOrigin::None.into())
						.expect("dispatch succeeded");
					within_block();
					ParachainUpgrade::on_finalize(*n);

					// did block execution set new validation code?
					if storage::unhashed::exists(NEW_VALIDATION_CODE) {
						if self.pending_upgrade.is_some() {
							panic!("attempted to set validation code while upgrade was pending");
						}
						self.pending_upgrade = vfp.transient.code_upgrade_allowed;
					}

					// clean up
					System::<Test>::finalize();
					if let Some(after_block) = after_block {
						after_block();
					}
				}
			});
		}
	}

	impl Drop for BlockTests {
		fn drop(&mut self) {
			if !self.ran {
				self.run();
			}
		}
	}

	#[test]
	#[should_panic]
	fn block_tests_run_on_drop() {
		BlockTests::new().add(123, || {
			panic!("if this test passes, block tests run properly")
		});
	}

	#[test]
	fn requires_root() {
		BlockTests::new().add(123, || {
			assert_eq!(
				ParachainUpgrade::schedule_upgrade(Origin::signed(1), Default::default()),
				Err(sp_runtime::DispatchError::BadOrigin),
			);
		});
	}

	#[test]
	fn requires_root_2() {
		BlockTests::new().add(123, || {
			assert_ok!(ParachainUpgrade::schedule_upgrade(
				RawOrigin::Root.into(),
				Default::default()
			));
		});
	}

	#[test]
	fn events() {
		BlockTests::new()
			.add_with_post_test(
				123,
				|| {
					assert_ok!(ParachainUpgrade::schedule_upgrade(
						RawOrigin::Root.into(),
						Default::default()
					));
				},
				|| {
					let events = System::<Test>::events();
					assert_eq!(
						events[0].event,
						TestEvent::parachain_upgrade(Event::ValidationFunctionStored(1123))
					);
				},
			)
			.add_with_post_test(
				1234,
				|| {},
				|| {
					let events = System::<Test>::events();
					assert_eq!(
						events[0].event,
						TestEvent::parachain_upgrade(Event::ValidationFunctionApplied(1234))
					);
				},
			);
	}

	#[test]
	fn non_overlapping() {
		BlockTests::new()
			.add(123, || {
				assert_ok!(ParachainUpgrade::schedule_upgrade(
					RawOrigin::Root.into(),
					Default::default()
				));
			})
			.add(234, || {
				assert_eq!(
					ParachainUpgrade::schedule_upgrade(RawOrigin::Root.into(), Default::default(),),
					Err(Error::<Test>::OverlappingUpgrades.into()),
				)
			});
	}

	#[test]
	fn manipulates_storage() {
		BlockTests::new()
			.add(123, || {
				assert!(
					!PendingValidationFunction::exists(),
					"validation function must not exist yet"
				);
				assert_ok!(ParachainUpgrade::schedule_upgrade(
					RawOrigin::Root.into(),
					Default::default()
				));
				assert!(
					PendingValidationFunction::exists(),
					"validation function must now exist"
				);
			})
			.add_with_post_test(
				1234,
				|| {},
				|| {
					assert!(
						!PendingValidationFunction::exists(),
						"validation function must have been unset"
					);
				},
			);
	}

	#[test]
	fn checks_size() {
		BlockTests::new()
			.with_validation_data(|_, n| ValidationData {
				persisted: PersistedValidationData {
					block_number: n,
					..Default::default()
				},
				transient: TransientValidationData {
					max_code_size: 32,
					code_upgrade_allowed: Some(n + 1000),
					..Default::default()
				},
			})
			.add(123, || {
				assert_eq!(
					ParachainUpgrade::schedule_upgrade(RawOrigin::Root.into(), vec![0; 64]),
					Err(Error::<Test>::TooBig.into()),
				);
			});
	}
}
