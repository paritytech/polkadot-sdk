// tests.rs - Basic test for referenda precompile

use crate::mock::*;

#[test]
fn submit_works() {
	// Use the ExtBuilder from mock.rs
	ExtBuilder::default().build().execute_with(|| {
		// Basic smoke test - just verify the test environment is set up correctly
		assert_eq!(System::block_number(), 1);
		assert_eq!(pallet_referenda::ReferendumCount::<Test>::get(), 0);
		
		// TODO: Add precompile-specific tests once the environment is validated
		println!("✅ Test environment initialized successfully");
	});
}

#[test]
fn test_balances_initialized() {
	ExtBuilder::default().build().execute_with(|| {
		// Verify our test accounts have balances
		assert_eq!(Balances::free_balance(&ALICE), 100);
		assert_eq!(Balances::free_balance(&BOB), 100);
		assert_eq!(Balances::free_balance(&CHARLIE), 100);
		
		println!("✅ Balances initialized correctly");
	});
}

// TODO: Add more tests:
// - Test submitLookup precompile call
// - Test origin encoding/decoding
// - Test referendum creation
// - Test error handling