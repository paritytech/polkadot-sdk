#![no_main]
extern crate snowbridge_pallet_ethereum_client;

use libfuzzer_sys::fuzz_target;
use snowbridge_pallet_ethereum_client::{mock::*, types::CheckpointUpdate};
use snowbridge_ethereum_beacon_client_fuzz::types::FuzzCheckpointUpdate;
use std::convert::TryInto;

fuzz_target!(|input: FuzzCheckpointUpdate| {
	new_tester().execute_with(|| {
		let update: CheckpointUpdate = input.try_into().unwrap();
		let result =
			EthereumBeaconClient::force_checkpoint(RuntimeOrigin::root(), Box::new(update));
		assert!(result.is_err());
	});
});
