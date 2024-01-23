#![no_main]
extern crate snowbridge_pallet_ethereum_client;

use snowbridge_pallet_ethereum_client::{mock::*, types::Update};
use snowbridge_ethereum_beacon_client_fuzz::types::FuzzUpdate;
use std::convert::TryInto;

use libfuzzer_sys::fuzz_target;

fuzz_target!(|input: FuzzUpdate| {
	new_tester().execute_with(|| {
		let update: Update = input.try_into().unwrap();
		let result = EthereumBeaconClient::submit(RuntimeOrigin::signed(1), Box::new(update));
		assert!(result.is_err());
	});
});
