// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use crate as snowbridge_system_frontend;
use crate::mock::pallet_xcm_origin::EnsureXcm;

use core::cell::RefCell;
use codec::Encode;
use frame_support::{
	derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, Everything},
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	AccountId32, BuildStorage,
};
use xcm::prelude::*;
use xcm_executor::{
	traits::{FeeManager, FeeReason, TransactAsset},
	AssetsInHolding
};

#[cfg(feature = "runtime-benchmarks")]
use crate::BenchmarkHelper;

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = AccountId32;

// A stripped-down version of pallet-xcm that only inserts an XCM origin into the runtime
#[frame_support::pallet]
mod pallet_xcm_origin {
	use codec::{Decode, DecodeWithMemTracking, Encode};
	use frame_support::{
		pallet_prelude::*,
		traits::{Contains, OriginTrait},
	};
	use xcm::latest::prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type RuntimeOrigin: From<Origin> + From<<Self as frame_system::Config>::RuntimeOrigin>;
	}

	// Insert this custom Origin into the aggregate RuntimeOrigin
	#[pallet::origin]
	#[derive(PartialEq, Eq, Clone, Encode, Decode, DecodeWithMemTracking, RuntimeDebug, TypeInfo, MaxEncodedLen)]
	pub struct Origin(pub Location);

	impl From<Location> for Origin {
		fn from(location: Location) -> Origin {
			Origin(location)
		}
	}

	/// `EnsureOrigin` implementation succeeding with a `Location` value to recognize and
	/// filter the contained location
	pub struct EnsureXcm<F>(PhantomData<F>);
	impl<O: OriginTrait + From<Origin>, F: Contains<Location>> EnsureOrigin<O> for EnsureXcm<F>
	where
		O::PalletsOrigin: From<Origin> + TryInto<Origin, Error = O::PalletsOrigin>,
	{
		type Success = Location;

		fn try_origin(outer: O) -> Result<Self::Success, O> {
			outer.try_with_caller(|caller| {
				caller.try_into().and_then(|o| match o {
					Origin(location) if F::contains(&location) => Ok(location),
					o => Err(o.into()),
				})
			})
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin(Location::new(1, [Parachain(2000)]))))
		}
	}
}

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		XcmOrigin: pallet_xcm_origin::{Pallet, Origin},
		EthereumSystemFrontend: snowbridge_system_frontend,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u128>;
	type Nonce = u64;
	type Block = Block;
}

impl pallet_xcm_origin::Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
}

#[cfg(feature = "runtime-benchmarks")]
impl BenchmarkHelper<RuntimeOrigin> for () {
	fn make_xcm_origin(location: Location) -> RuntimeOrigin {
		RuntimeOrigin::from(pallet_xcm_origin::Origin(location))
	}
}

thread_local! {
	pub static IS_WAIVED: RefCell<Vec<FeeReason>> = RefCell::new(vec![]);
	pub static SENDER_OVERRIDE: RefCell<Option<(
		fn(
			&mut Option<Location>,
			&mut Option<Xcm<()>>,
		) -> Result<(Xcm<()>, Assets), SendError>,
		fn(
			Xcm<()>,
		) -> Result<XcmHash, SendError>,
	)>> = RefCell::new(None);
	pub static CHARGE_FEES_OVERRIDE: RefCell<Option<
		fn(Location, Assets) -> xcm::latest::Result
	>> = RefCell::new(None);
}

#[allow(dead_code)]
pub fn set_fee_waiver(waived: Vec<FeeReason>) {
	IS_WAIVED.with(|l| l.replace(waived));
}

#[allow(dead_code)]
pub fn set_sender_override(
	validate: fn(
		&mut Option<Location>,
		&mut Option<Xcm<()>>,
	) -> SendResult<Xcm<()>>,
	deliver: fn(
		Xcm<()>,
	) -> Result<XcmHash, SendError>,
) {
	SENDER_OVERRIDE.with(|x| x.replace(Some((validate, deliver))));
}

#[allow(dead_code)]
pub fn clear_sender_override() {
	SENDER_OVERRIDE.with(|x| x.replace(None));
}

#[allow(dead_code)]
pub fn set_charge_fees_override(
	charge_fees: fn(Location, Assets) -> xcm::latest::Result
) {
	CHARGE_FEES_OVERRIDE.with(|x| x.replace(Some(charge_fees)));
}

#[allow(dead_code)]
pub fn clear_charge_fees_override() {
	CHARGE_FEES_OVERRIDE.with(|x| x.replace(None));
}


// Mock XCM sender that always succeeds
pub struct MockXcmSender;

impl SendXcm for MockXcmSender {
	type Ticket = Xcm<()>;

	fn validate(
		dest: &mut Option<Location>,
		xcm: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let r: SendResult<Self::Ticket> = SENDER_OVERRIDE.with(|s| {
			if let Some((ref f, _)) = &*s.borrow() {
				f(dest, xcm)
			} else {
				Ok((xcm.take().unwrap(), Assets::default()))
			}
		});
		r
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let r: Result<XcmHash, SendError> = SENDER_OVERRIDE.with(|s| {
			if let Some((_, ref f)) = &*s.borrow() {
				f(ticket)
			} else {
				let hash = ticket.using_encoded(sp_io::hashing::blake2_256);
				Ok(hash)
			}
		});
		r
	}
}

pub struct SuccessfulTransactor;
impl TransactAsset for SuccessfulTransactor {
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn can_check_out(_dest: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn deposit_asset(_what: &Asset, _who: &Location, _context: Option<&XcmContext>) -> XcmResult {
		Ok(())
	}

	fn withdraw_asset(
		_what: &Asset,
		_who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(AssetsInHolding::default())
	}

	fn internal_transfer_asset(
		_what: &Asset,
		_from: &Location,
		_to: &Location,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(AssetsInHolding::default())
	}
}

pub enum Weightless {}
impl PreparedMessage for Weightless {
	fn weight_of(&self) -> Weight {
		unreachable!();
	}
}

pub struct MockXcmExecutor;
impl<C> ExecuteXcm<C> for MockXcmExecutor {
	type Prepared = Weightless;
	fn prepare(_: Xcm<C>) -> Result<Self::Prepared, Xcm<C>> {
		unreachable!()
	}
	fn execute(_: impl Into<Location>, _: Self::Prepared, _: &mut XcmHash, _: Weight) -> Outcome {
		unreachable!()
	}
	fn charge_fees(location: impl Into<Location>, assets: Assets) -> xcm::latest::Result {
		let r: xcm::latest::Result = CHARGE_FEES_OVERRIDE.with(|s| {
			if let Some(ref f) = &*s.borrow() {
				f(location.into(), assets)
			} else {
				Ok(())
			}
		});
		r
	}
}

impl FeeManager for MockXcmExecutor {
	fn is_waived(_: Option<&Location>, r: FeeReason) -> bool {
		IS_WAIVED.with(|l| l.borrow().contains(&r))
	}

	fn handle_fee(_: Assets, _: Option<&XcmContext>, _: FeeReason) {}
}

parameter_types! {
	pub storage Ether: Location = Location::new(
				2,
				[
					GlobalConsensus(Ethereum { chain_id: 11155111 }),
				],
	);
	pub storage DeliveryFee: Asset = (Location::parent(), 80_000_000_000u128).into();
	pub BridgeHubLocation: Location = Location::new(1, [Parachain(1002)]);
	pub UniversalLocation: InteriorLocation =
		[GlobalConsensus(Polkadot), Parachain(1000)].into();
	pub PalletLocation: InteriorLocation = [PalletInstance(80)].into();
}

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RegisterTokenOrigin = AsEnsureOriginWithArg<EnsureXcm<Everything>>;
	type XcmSender = MockXcmSender;
	type AssetTransactor = SuccessfulTransactor;
	type EthereumLocation = Ether;
	type XcmExecutor = MockXcmExecutor;
	type BridgeHubLocation = BridgeHubLocation;
	type UniversalLocation = UniversalLocation;
	type PalletLocation = PalletLocation;
	type BackendWeightInfo = ();
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type Helper = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
	});
	ext
}

pub fn make_xcm_origin(location: Location) -> RuntimeOrigin {
	pallet_xcm_origin::Origin(location).into()
}
