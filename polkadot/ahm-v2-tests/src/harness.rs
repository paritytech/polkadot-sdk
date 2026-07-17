// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot. If not, see <http://www.gnu.org/licenses/>.

//! Test harness driving the Westend runtime in `TestExternalities`.
//!
//! Block advancement runs only the pallets relevant to parachain lifecycle. Babe/Session are
//! deliberately excluded: without real block production there are no slot digests, so sessions
//! would never rotate. Instead, session changes are injected directly through the
//! `parachains_shared` and `paras` test helpers — the same technique as the `paras_registrar`
//! mock in `polkadot-runtime-common`.

use polkadot_primitives::{
	supermajority_threshold, AccountId, Balance, BlockNumber, HeadData, PvfCheckStatement,
	SessionIndex, ValidationCode, MAX_CODE_SIZE,
};
use polkadot_runtime_parachains::{configuration, paras, shared};
use sp_io::TestExternalities;
use sp_keyring::Sr25519Keyring;
use sp_runtime::BuildStorage;
use westend_runtime::{
	Auctions, Crowdloan, Initializer, Paras as ParasPallet, Runtime, RuntimeEvent, RuntimeOrigin,
	Slots, System,
};

/// Arbitrary session length for tests; sessions are driven manually, not by Babe.
pub const BLOCKS_PER_SESSION: BlockNumber = 3;

/// Validators whose PVF pre-check votes onboard new validation code. These are keyring
/// accounts, so tests can also use them as regular (funded) user accounts; the two roles
/// don't interact.
pub const VALIDATORS: &[Sr25519Keyring] = &[
	Sr25519Keyring::Alice,
	Sr25519Keyring::Bob,
	Sr25519Keyring::Charlie,
	Sr25519Keyring::Dave,
	Sr25519Keyring::Ferdie,
];

/// Genesis state with the given account balances.
///
/// `max_code_size` matters beyond validation: the registrar charges the code part of the
/// registration deposit as if the code had maximum size.
pub fn new_test_ext(balances: Vec<(AccountId, Balance)>) -> TestExternalities {
	sp_tracing::try_init_simple();

	let mut t = frame_system::GenesisConfig::<Runtime>::default().build_storage().unwrap();

	configuration::GenesisConfig::<Runtime> {
		config: configuration::HostConfiguration {
			max_code_size: MAX_CODE_SIZE,
			max_head_data_size: 1024 * 1024,
			..Default::default()
		},
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_balances::GenesisConfig::<Runtime> { balances, ..Default::default() }
		.assimilate_storage(&mut t)
		.unwrap();

	t.into()
}

/// Advance to the given block, running the parachain-lifecycle-relevant hooks each block and
/// rotating the session every [`BLOCKS_PER_SESSION`] blocks.
pub fn run_to_block(n: BlockNumber) {
	System::run_to_block_with::<(Initializer, ParasPallet, Slots, Auctions, Crowdloan)>(
		n,
		frame_system::RunToBlockHooks::default().before_finalize(|bn| {
			if (bn + 1) % BLOCKS_PER_SESSION == 0 {
				let session_index = shared::CurrentSessionIndex::<Runtime>::get() + 1;
				let validator_keys = VALIDATORS.iter().map(|v| v.public().into()).collect();

				shared::Pallet::<Runtime>::set_session_index(session_index);
				shared::Pallet::<Runtime>::set_active_validators_ascending(validator_keys);
				paras::Pallet::<Runtime>::test_on_new_session();
			}
		}),
	);
}

pub fn run_to_session(n: SessionIndex) {
	run_to_block(n * BLOCKS_PER_SESSION);
}

/// Vote the given code through PVF pre-checking with a validator supermajority.
///
/// Mirrors `polkadot_runtime_common::mock::conclude_pvf_checking`, which is test-internal to
/// that crate and not exported.
pub fn conclude_pvf_checking(validation_code: &ValidationCode, session_index: SessionIndex) {
	let num_required = supermajority_threshold(VALIDATORS.len());
	VALIDATORS.iter().enumerate().take(num_required).for_each(|(idx, key)| {
		let statement = PvfCheckStatement {
			accept: true,
			subject: validation_code.hash(),
			session_index,
			validator_index: (idx as u32).into(),
		};
		let signature = key.sign(&statement.signing_payload());
		let _ = paras::Pallet::<Runtime>::include_pvf_check_statement(
			RuntimeOrigin::none(),
			statement,
			signature.into(),
		);
	});
}

pub fn signed(who: &AccountId) -> RuntimeOrigin {
	RuntimeOrigin::signed(who.clone())
}

pub fn test_genesis_head(size: usize) -> HeadData {
	HeadData(vec![0u8; size])
}

pub fn test_validation_code(size: usize) -> ValidationCode {
	ValidationCode(vec![0u8; size])
}

pub fn assert_has_event(event: RuntimeEvent) {
	assert!(
		System::events().iter().any(|record| record.event == event),
		"expected event {event:?} not found in {:?}",
		System::events()
	);
}
