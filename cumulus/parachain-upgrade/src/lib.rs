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
	inherents::{ValidationDataType, VALIDATION_DATA_IDENTIFIER as INHERENT_IDENTIFIER},
	well_known_keys::{NEW_VALIDATION_CODE, VALIDATION_DATA}, AbridgedHostConfiguration,
	OnValidationData, PersistedValidationData, ParaId, relay_chain,
};
use frame_support::{
	decl_error, decl_event, decl_module, decl_storage, ensure, storage,
	weights::{DispatchClass, Weight}, dispatch::DispatchResult, traits::Get,
};
use frame_system::{ensure_none, ensure_root};
use parachain::primitives::RelayChainBlockNumber;
use sp_core::storage::well_known_keys;
use sp_inherents::{InherentData, InherentIdentifier, ProvideInherent};
use sp_std::vec::Vec;

mod relay_state_snapshot;

pub use relay_state_snapshot::MessagingStateSnapshot;

/// The pallet's configuration trait.
pub trait Config: frame_system::Config {
	/// The overarching event type.
	type Event: From<Event> + Into<<Self as frame_system::Config>::Event>;

	/// Something which can be notified when the validation data is set.
	type OnValidationData: OnValidationData;

	/// Returns the parachain ID we are running with.
	type SelfParaId: Get<ParaId>;
}

// This pallet's storage items.
decl_storage! {
	trait Store for Module<T: Config> as ParachainUpgrade {
		// we need to store the new validation function for the span between
		// setting it and applying it.
		PendingValidationFunction get(fn new_validation_function):
			Option<(RelayChainBlockNumber, Vec<u8>)>;

		/// Were the [`ValidationData`] updated in this block?
		DidUpdateValidationData: bool;

		/// Were the validation data set to notify the relay chain?
		DidSetValidationCode: bool;

		/// The last relay parent block number at which we signalled the code upgrade.
		LastUpgrade: relay_chain::BlockNumber;

		/// The snapshot of some state related to messaging relevant to the current parachain as per
		/// the relay parent.
		///
		/// This field is meant to be updated each block with the validation data inherent. Therefore,
		/// before processing of the inherent, e.g. in `on_initialize` this data may be stale.
		///
		/// This data is also absent from the genesis.
		RelevantMessagingState get(fn relevant_messaging_state): Option<MessagingStateSnapshot>;
		/// The parachain host configuration that was obtained from the relay parent.
		///
		/// This field is meant to be updated each block with the validation data inherent. Therefore,
		/// before processing of the inherent, e.g. in `on_initialize` this data may be stale.
		///
		/// This data is also absent from the genesis.
		HostConfiguration get(fn host_configuration): Option<AbridgedHostConfiguration>;
	}
}

// The pallet's dispatchable functions.
decl_module! {
	pub struct Module<T: Config> for enum Call where origin: T::Origin {
		// Initializing events
		// this is needed only if you are using events in your pallet
		fn deposit_event() = default;

		// TODO: figure out a better weight than this
		#[weight = (0, DispatchClass::Operational)]
		pub fn schedule_upgrade(origin, validation_function: Vec<u8>) {
			ensure_root(origin)?;
			<frame_system::Module<T>>::can_set_code(&validation_function)?;
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
		fn set_validation_data(origin, data: ValidationDataType) -> DispatchResult {
			ensure_none(origin)?;
			assert!(!DidUpdateValidationData::exists(), "ValidationData must be updated only once in a block");

			let ValidationDataType {
				validation_data: vfp,
				relay_chain_state,
			} = data;

			// initialization logic: we know that this runs exactly once every block,
			// which means we can put the initialization logic here to remove the
			// sequencing problem.
			if let Some((apply_block, validation_function)) = PendingValidationFunction::get() {
				if vfp.block_number >= apply_block {
					PendingValidationFunction::kill();
					LastUpgrade::put(&apply_block);
					Self::put_parachain_code(&validation_function);
					Self::deposit_event(Event::ValidationFunctionApplied(vfp.block_number));
				}
			}

			let (host_config, relevant_messaging_state) =
				relay_state_snapshot::extract_from_proof(
					T::SelfParaId::get(),
					vfp.relay_storage_root,
					relay_chain_state
				)
				.map_err(|err| {
					frame_support::debug::print!("invalid relay chain merkle proof: {:?}", err);
					Error::<T>::InvalidRelayChainMerkleProof
				})?;

			storage::unhashed::put(VALIDATION_DATA, &vfp);
			DidUpdateValidationData::put(true);
			RelevantMessagingState::put(relevant_messaging_state);
			HostConfiguration::put(host_config);

			<T::OnValidationData as OnValidationData>::on_validation_data(vfp);

			Ok(())
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

impl<T: Config> Module<T> {
	/// Get validation data.
	///
	/// Returns `Some(_)` after the inherent set the data for the current block.
	pub fn validation_data() -> Option<PersistedValidationData> {
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

	/// The maximum code size permitted, in bytes.
	///
	/// Returns `None` if the relay chain parachain host configuration hasn't been submitted yet.
	pub fn max_code_size() -> Option<u32> {
		HostConfiguration::get().map(|cfg| cfg.max_code_size)
	}

	/// Returns if a PVF/runtime upgrade could be signalled at the current block, and if so
	/// when the new code will take the effect.
	fn code_upgrade_allowed(
		vfp: &PersistedValidationData,
		cfg: &AbridgedHostConfiguration,
	) -> Option<relay_chain::BlockNumber> {
		if PendingValidationFunction::get().is_some() {
			// There is already upgrade scheduled. Upgrade is not allowed.
			return None;
		}

		let relay_blocks_since_last_upgrade = vfp
			.block_number
			.saturating_sub(LastUpgrade::get());

		if relay_blocks_since_last_upgrade <= cfg.validation_upgrade_frequency {
			// The cooldown after the last upgrade hasn't elapsed yet. Upgrade is not allowed.
			return None;
		}

		Some(vfp.block_number + cfg.validation_upgrade_delay)
	}

	/// The implementation of the runtime upgrade scheduling.
	fn schedule_upgrade_impl(
		validation_function: Vec<u8>,
	) -> DispatchResult {
		ensure!(
			!PendingValidationFunction::exists(),
			Error::<T>::OverlappingUpgrades
		);
		let vfp = Self::validation_data().ok_or(Error::<T>::ValidationDataNotAvailable)?;
		let cfg = Self::host_configuration().ok_or(Error::<T>::HostConfigurationNotAvailable)?;
		ensure!(
			validation_function.len() <= cfg.max_code_size as usize,
			Error::<T>::TooBig
		);
		let apply_block =
			Self::code_upgrade_allowed(&vfp, &cfg).ok_or(Error::<T>::ProhibitedByPolkadot)?;

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

impl<T: Config> ProvideInherent for Module<T> {
	type Call = Call<T>;
	type Error = sp_inherents::MakeFatalError<()>;
	const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

	fn create_inherent(data: &InherentData) -> Option<Self::Call> {
		let data: ValidationDataType = data
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
	pub enum Error for Module<T: Config> {
		/// Attempt to upgrade validation function while existing upgrade pending
		OverlappingUpgrades,
		/// Polkadot currently prohibits this parachain from upgrading its validation function
		ProhibitedByPolkadot,
		/// The supplied validation function has compiled into a blob larger than Polkadot is willing to run
		TooBig,
		/// The inherent which supplies the validation data did not run this block
		ValidationDataNotAvailable,
		/// The inherent which supplies the host configuration did not run this block
		HostConfigurationNotAvailable,
		/// Invalid relay-chain storage merkle proof
		InvalidRelayChainMerkleProof,
	}
}

/// tests for this pallet
#[cfg(test)]
mod tests {
	use super::*;

	use codec::Encode;
	use cumulus_primitives::PersistedValidationData;
	use cumulus_test_relay_sproof_builder::RelayStateSproofBuilder;
	use frame_support::{
		assert_ok,
		dispatch::UnfilteredDispatchable,
		impl_outer_event, impl_outer_origin, parameter_types,
		traits::{OnFinalize, OnInitialize},
	};
	use frame_system::{InitKind, RawOrigin};
	use sp_core::H256;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
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
		pub Version: RuntimeVersion = RuntimeVersion {
			spec_name: sp_version::create_runtime_str!("test"),
			impl_name: sp_version::create_runtime_str!("system-test"),
			authoring_version: 1,
			spec_version: 1,
			impl_version: 1,
			apis: sp_version::create_apis_vec!([]),
			transaction_version: 1,
		};
		pub const ParachainId: ParaId = ParaId::new(200);
	}
	impl frame_system::Config for Test {
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
		type BlockLength = ();
		type BlockWeights = ();
		type Version = Version;
		type PalletInfo = ();
		type AccountData = ();
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type DbWeight = ();
		type BaseCallFilter = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
	}
	impl Config for Test {
		type Event = TestEvent;
		type OnValidationData = ();
		type SelfParaId = ParachainId;
	}

	type ParachainUpgrade = Module<Test>;
	type System = frame_system::Module<Test>;

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
		n: <Test as frame_system::Config>::BlockNumber,
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
		relay_sproof_builder_hook: Option<
			Box<dyn Fn(&BlockTests, RelayChainBlockNumber, &mut RelayStateSproofBuilder)>
		>,
	}

	impl BlockTests {
		fn new() -> BlockTests {
			Default::default()
		}

		fn add_raw(mut self, test: BlockTest) -> Self {
			self.tests.push(test);
			self
		}

		fn add<F>(self, n: <Test as frame_system::Config>::BlockNumber, within_block: F) -> Self
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
			n: <Test as frame_system::Config>::BlockNumber,
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

		fn with_relay_sproof_builder<F>(mut self, f: F) -> Self
		where
			F: 'static + Fn(&BlockTests, RelayChainBlockNumber, &mut RelayStateSproofBuilder)
		{
			self.relay_sproof_builder_hook = Some(Box::new(f));
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
					System::initialize(
						&n,
						&Default::default(),
						&Default::default(),
						InitKind::Full,
					);

					// now mess with the storage the way validate_block does
					let mut sproof_builder = RelayStateSproofBuilder::default();
					if let Some(ref hook) = self.relay_sproof_builder_hook {
						hook(self, *n as RelayChainBlockNumber, &mut sproof_builder);
					}
					let (relay_storage_root, relay_chain_state) =
						sproof_builder.into_state_root_and_proof();
					let vfp = PersistedValidationData {
						block_number: *n as RelayChainBlockNumber,
						relay_storage_root,
						..Default::default()
					};

					storage::unhashed::put(VALIDATION_DATA, &vfp);
					storage::unhashed::kill(NEW_VALIDATION_CODE);

					// It is insufficient to push the validation function params
					// to storage; they must also be included in the inherent data.
					let inherent_data = {
						let mut inherent_data = InherentData::default();
						inherent_data
							.put_data(INHERENT_IDENTIFIER, &ValidationDataType {
								validation_data: vfp.clone(),
								relay_chain_state,
							})
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
					}

					// clean up
					System::finalize();
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
			.with_relay_sproof_builder(|_, _, builder| {
				builder.host_config.validation_upgrade_delay = 1000;
			})
			.add_with_post_test(
				123,
				|| {
					assert_ok!(ParachainUpgrade::schedule_upgrade(
						RawOrigin::Root.into(),
						Default::default()
					));
				},
				|| {
					let events = System::events();
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
					let events = System::events();
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
			.with_relay_sproof_builder(|_, _, builder| {
				builder.host_config.validation_upgrade_delay = 1000;
			})
			.add(123, || {
				assert_ok!(ParachainUpgrade::schedule_upgrade(
					RawOrigin::Root.into(),
					Default::default()
				));
			})
			.add(234, || {
				assert_eq!(
					ParachainUpgrade::schedule_upgrade(RawOrigin::Root.into(), Default::default()),
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
			.with_relay_sproof_builder(|_, _, builder| {
				builder.host_config.max_code_size = 8;
			})
			.add(123, || {
				assert_eq!(
					ParachainUpgrade::schedule_upgrade(RawOrigin::Root.into(), vec![0; 64]),
					Err(Error::<Test>::TooBig.into()),
				);
			});
	}
}
