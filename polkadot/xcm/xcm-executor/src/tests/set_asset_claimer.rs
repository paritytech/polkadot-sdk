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
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Unit tests related to the `fees` register and `PayFees` instruction.
//!
//! See [Fellowship RFC 105](https://github.com/polkadot-fellows/rfCs/pull/105)
//! and the [specification](https://github.com/polkadot-fellows/xcm-format) for more information.

use codec::Encode;
use xcm::prelude::*;

use super::mock::*;
use crate::XcmExecutor;

#[test]
fn set_asset_claimer() {
	let sender = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);
	let bob = Location::new(0, [AccountId32 { id: [2; 32], network: None }]);

	// Make sure the user has enough funds to withdraw.
	add_asset(sender.clone(), (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		// if withdrawing fails we're not missing any corner case.
		.withdraw_asset((Here, 100u128))
		.clear_origin()
		.set_asset_claimer(bob.clone())
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.build();

	// We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
	// can inspect its fields.
	let mut vm =
		XcmExecutor::<XcmConfig>::new(sender, xcm.using_encoded(sp_io::hashing::blake2_256));
	vm.message_weight = XcmExecutor::<XcmConfig>::prepare(xcm.clone()).unwrap().weight_of();

	let result = vm.bench_process(xcm);
	assert!(result.is_ok());
	assert_eq!(vm.asset_claimer(), Some(bob));
}

#[test]
fn do_not_set_asset_claimer_none() {
	let sender = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);

	// Make sure the user has enough funds to withdraw.
	add_asset(sender.clone(), (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		// if withdrawing fails we're not missing any corner case.
		.withdraw_asset((Here, 100u128))
		.clear_origin()
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.build();

	// We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
	// can inspect its fields.
	let mut vm =
		XcmExecutor::<XcmConfig>::new(sender, xcm.using_encoded(sp_io::hashing::blake2_256));
	vm.message_weight = XcmExecutor::<XcmConfig>::prepare(xcm.clone()).unwrap().weight_of();

	let result = vm.bench_process(xcm);
	assert!(result.is_ok());
	assert_eq!(vm.asset_claimer(), None);
}

#[test]
fn trap_then_set_asset_claimer() {
	let sender = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);
	let bob = Location::new(0, [AccountId32 { id: [2; 32], network: None }]);

	// Make sure the user has enough funds to withdraw.
	add_asset(sender.clone(), (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		// if withdrawing fails we're not missing any corner case.
		.withdraw_asset((Here, 100u128))
		.clear_origin()
		.trap(0u64)
		.set_asset_claimer(bob)
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.build();

	// We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
	// can inspect its fields.
	let mut vm =
		XcmExecutor::<XcmConfig>::new(sender, xcm.using_encoded(sp_io::hashing::blake2_256));
	vm.message_weight = XcmExecutor::<XcmConfig>::prepare(xcm.clone()).unwrap().weight_of();

	let result = vm.bench_process(xcm);
	assert!(result.is_err());
	assert_eq!(vm.asset_claimer(), None);
}

#[test]
fn set_asset_claimer_then_trap() {
	let sender = Location::new(0, [AccountId32 { id: [0; 32], network: None }]);
	let bob = Location::new(0, [AccountId32 { id: [2; 32], network: None }]);

	// Make sure the user has enough funds to withdraw.
	add_asset(sender.clone(), (Here, 100u128));

	// Build xcm.
	let xcm = Xcm::<TestCall>::builder_unsafe()
		// if withdrawing fails we're not missing any corner case.
		.withdraw_asset((Here, 100u128))
		.clear_origin()
		.set_asset_claimer(bob.clone())
		.trap(0u64)
		.pay_fees((Here, 10u128)) // 10% destined for fees, not more.
		.build();

	// We create an XCVM instance instead of calling `XcmExecutor::<_>::prepare_and_execute` so we
	// can inspect its fields.
	let mut vm =
		XcmExecutor::<XcmConfig>::new(sender, xcm.using_encoded(sp_io::hashing::blake2_256));
	vm.message_weight = XcmExecutor::<XcmConfig>::prepare(xcm.clone()).unwrap().weight_of();

	let result = vm.bench_process(xcm);
	assert!(result.is_err());
	assert_eq!(vm.asset_claimer(), Some(bob));
}
