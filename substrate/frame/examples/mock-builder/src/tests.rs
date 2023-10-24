use super::{
	mock::{
		new_test_ext, MockAuctioneer, MockCurrency, MockTime, MyPallet, Runtime, DAY,
		EXPECTED_AMOUNT, INITIAL_TIME, PERIOD,
	},
	Error, LastDeposit,
};
use frame_support::{
	assert_noop, assert_ok, pallet_prelude::DispatchError, traits::ReservableCurrency,
};
use frame_system::RawOrigin;

const ALICE: u64 = 2;

#[test]
fn reserve() {
	new_test_ext().execute_with(|| {
		// Mock the internal call done to T::Currency::reserve() inside of
		// MyPaller:make_reserve() checking the expected inputs and returining a successfull
		// value.
		MockCurrency::mock_reserve(|account_id, amount| {
			assert_eq!(account_id, &ALICE);
			assert_eq!(amount, EXPECTED_AMOUNT);
			Ok(())
		});

		assert_ok!(MyPallet::make_reserve(RawOrigin::Signed(ALICE).into(), EXPECTED_AMOUNT));

		assert_eq!(LastDeposit::<Runtime>::get(ALICE), Some(INITIAL_TIME))
	})
}

#[test]
fn reserve_error() {
	new_test_ext().execute_with(|| {
		// Mock the internal call to T::Currency::reserve() to emulate an error
		MockCurrency::mock_reserve(|_, _| Err(DispatchError::Other("Err")));

		assert_noop!(
			MyPallet::make_reserve(RawOrigin::Signed(ALICE).into(), EXPECTED_AMOUNT),
			DispatchError::Other("Err")
		);
	})
}

/// Utility to amalgamate the pallet call with the required mocks to make them work successfull
fn do_alice_reserve(amount: u128) {
	MockCurrency::mock_reserve(|account_id, amount| {
		let previous_reserve = MockCurrency::reserved_balance(account_id);

		// Mocks can be nested.
		// Mocking a reserve implies to create a new mock for the updated reserved_balance value
		// In order to fetch later the correct updated value
		MockCurrency::mock_reserved_balance(move |_| previous_reserve + amount);

		Ok(())
	});

	MyPallet::make_reserve(RawOrigin::Signed(ALICE).into(), amount).unwrap();
}

#[test]
fn create_auction() {
	new_test_ext().execute_with(|| {
		do_alice_reserve(EXPECTED_AMOUNT);

		// Emulate advance in time to fulfill the auction conditions
		MockTime::mock_now(|| INITIAL_TIME + DAY);

		// Successfull internal call to new auction
		MockAuctioneer::mock_new_auction(|block, period| {
			assert_eq!(block, frame_system::Pallet::<Runtime>::block_number());
			assert_eq!(period, PERIOD);
			Ok(())
		});

		assert_ok!(MyPallet::create_auction(RawOrigin::Signed(ALICE).into()));
	})
}

#[test]
fn create_auction_with_several_deposits() {
	new_test_ext().execute_with(|| {
		do_alice_reserve(EXPECTED_AMOUNT / 2);
		do_alice_reserve(EXPECTED_AMOUNT / 2);

		MockTime::mock_now(|| INITIAL_TIME + DAY);
		MockAuctioneer::mock_new_auction(|_, _| Ok(()));

		assert_ok!(MyPallet::create_auction(RawOrigin::Signed(ALICE).into()));
	})
}

#[test]
fn not_enough_deposit_error() {
	new_test_ext().execute_with(|| {
		do_alice_reserve(EXPECTED_AMOUNT / 2);

		assert_noop!(
			MyPallet::create_auction(RawOrigin::Signed(ALICE).into()),
			Error::<Runtime>::NotEnoughDeposit
		);
	});
}

#[test]
fn not_enough_waiting_error() {
	new_test_ext().execute_with(|| {
		do_alice_reserve(EXPECTED_AMOUNT);

		assert_noop!(
			MyPallet::create_auction(RawOrigin::Signed(ALICE).into()),
			Error::<Runtime>::NotEnoughWaiting
		);
	});
}

#[test]
fn auction_error() {
	new_test_ext().execute_with(|| {
		do_alice_reserve(EXPECTED_AMOUNT);

		MockTime::mock_now(|| INITIAL_TIME + DAY);

		// We emulate an error in the new_auction() dependency call.
		MockAuctioneer::mock_new_auction(|_, _| Err(DispatchError::Other("Err")));

		assert_noop!(
			MyPallet::create_auction(RawOrigin::Signed(ALICE).into()),
			DispatchError::Other("Err")
		);
	});
}
