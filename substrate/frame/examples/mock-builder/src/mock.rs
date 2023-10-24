mod mock_pallets;
use mock_pallets::{mock_pallet_auctioneer, mock_pallet_currency, mock_pallet_time};

use super::pallet;
use frame_support::{
	derive_impl,
	traits::{ConstU128, ConstU64},
};
use frame_system::pallet_prelude::BlockNumberFor;

pub const DAY: u64 = 24 * 3600 * 1000; // ms

pub const INITIAL_TIME: u64 = 10 * DAY;
pub const EXPECTED_AMOUNT: u128 = 100;
pub const WAITING_TIME: u64 = DAY;
pub const PERIOD: u64 = 50;

frame_support::construct_runtime!(
	pub struct Runtime {
		System: frame_system,
		MockTime: mock_pallet_time,
		MockCurrency: mock_pallet_currency,
		MockAuctioneer: mock_pallet_auctioneer,
		MyPallet: pallet,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = frame_system::mocking::MockBlock<Runtime>;
}

impl mock_pallet_time::Config for Runtime {
	type Moment = u64;
}

impl mock_pallet_currency::Config for Runtime {
	type Balance = u128;
	type PositiveImbalance = ();
	type NegativeImbalance = ();
}

impl mock_pallet_auctioneer::Config for Runtime {
	type LeasePeriod = BlockNumberFor<Runtime>;
	type Currency = mock_pallet_currency::Pallet<Runtime>;
}

impl pallet::Config for Runtime {
	type Time = mock_pallet_time::Pallet<Runtime>;
	type Currency = mock_pallet_currency::Pallet<Runtime>;
	type Auction = mock_pallet_auctioneer::Pallet<Runtime>;
	type ExpectedAmount = ConstU128<EXPECTED_AMOUNT>;
	type WaitingTime = ConstU64<WAITING_TIME>;
	type Period = ConstU64<PERIOD>;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext = sp_io::TestExternalities::new(Default::default());

	ext.execute_with(|| {
		// Initial time for all test cases
		MockTime::mock_now(|| INITIAL_TIME);

		// Initial reserved balances for all test cases
		MockCurrency::mock_reserved_balance(|_account| 0);

		// Mock calls can be later overwriten in the test cases
	});

	ext
}
