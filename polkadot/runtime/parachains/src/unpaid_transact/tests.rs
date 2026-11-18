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

use super::*;
use crate::{
	hrmp,
	mock::{
		new_test_ext, MockGenesisConfig, RuntimeCall, RuntimeOrigin, System, Test, UnpaidTransact,
		UnpaidTransactCleanupPeriod, UnpaidTransactMaxPerWindow, UnpaidTransactMaxWeight,
		UnpaidTransactWindow,
	},
	paras::{ParaGenesisArgs, ParaKind},
};
use codec::{Decode, Encode};
use frame_support::{
	assert_noop, assert_ok, assert_storage_noop,
	traits::{Authorize, Hooks},
};
use frame_system::RawOrigin;
use polkadot_primitives::{BlockNumber, HeadData};
use polkadot_primitives_test_helpers::dummy_validation_code;
use sp_core::offchain::{
	testing::{TestOffchainExt, TestTransactionPoolExt},
	OffchainDbExt, OffchainWorkerExt, TransactionPoolExt,
};
use sp_runtime::traits::{BadOrigin, Dispatchable};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;

const PARA: u32 = 2000;

/// A call that is in `AllowedCalls`.
fn allowed_call() -> Vec<u8> {
	RuntimeCall::from(hrmp::Call::<Test>::hrmp_accept_open_channel { sender: 2001.into() }).encode()
}

fn transact(origin_kind: OriginKind, call: Vec<u8>) -> Instruction<()> {
	Transact { origin_kind, fallback_max_weight: None, call: call.into() }
}

/// `UnpaidExecution` then a native `Transact` of `call`.
fn message(call: Vec<u8>) -> Xcm<()> {
	Xcm(vec![
		UnpaidExecution { weight_limit: Unlimited, check_origin: None },
		transact(OriginKind::Native, call),
	])
}

/// Runs the barrier for `message` of `weight` from `origin`.
fn run(origin: Location, mut message: Xcm<()>, weight: Weight) -> Result<(), ProcessMessageError> {
	let mut properties = Properties { weight_credit: Weight::zero(), message_id: None };
	AllowUnpaidTransact::<Test>::should_execute(
		&origin,
		message.inner_mut(),
		weight,
		&mut properties,
	)
}

/// Runs the barrier for an allowed message of the maximum weight from `para`.
fn run_allowed(para: u32) -> Result<(), ProcessMessageError> {
	run(Parachain(para).into(), message(allowed_call()), UnpaidTransactMaxWeight::get())
}

/// The window of `para`: the block it started at and the messages counted in it.
fn window(para: u32) -> Option<(BlockNumber, u32)> {
	Windows::<Test>::get(ParaId::from(para))
}

/// A parachain gets `MaxPerWindow` messages per window, each parachain on its own.
#[test]
fn messages_of_a_para_are_bounded() {
	new_test_ext(Default::default()).execute_with(|| {
		System::set_block_number(5);
		let max = UnpaidTransactMaxPerWindow::get();

		for used in 1..=max {
			assert_ok!(run_allowed(PARA));
			assert_eq!(window(PARA), Some((5, used)));
		}
		assert_storage_noop!(assert_eq!(run_allowed(PARA), Err(ProcessMessageError::Unsupported)));

		// Another parachain is not held back.
		assert_ok!(run_allowed(PARA + 1));
		assert_eq!(window(PARA + 1), Some((5, 1)));
	});
}

/// A window lasts `Window` blocks from the first message counted in it. The next one starts at
/// the next message, not right when the previous one ends.
#[test]
fn window_restarts_at_the_next_message() {
	new_test_ext(Default::default()).execute_with(|| {
		let len = UnpaidTransactWindow::get();
		let max = UnpaidTransactMaxPerWindow::get();

		System::set_block_number(1);
		for _ in 0..max {
			assert_ok!(run_allowed(PARA));
		}

		// The window started at block 1: its last block is `len`.
		System::set_block_number(len);
		assert_eq!(run_allowed(PARA), Err(ProcessMessageError::Unsupported));
		System::set_block_number(1 + len);
		assert_ok!(run_allowed(PARA));
		assert_eq!(window(PARA), Some((1 + len, 1)));

		// Long after that window is over, the next message starts a whole new one.
		System::set_block_number(10 * len);
		assert_ok!(run_allowed(PARA));
		assert_eq!(window(PARA), Some((10 * len, 1)));
	});
}

/// A message that is refused is not counted.
#[test]
fn other_messages_are_refused_and_not_counted() {
	new_test_ext(Default::default()).execute_with(|| {
		let para: Location = Parachain(PARA).into();
		let max = UnpaidTransactMaxWeight::get();
		let one = Weight::from_parts(1, 1);
		let refused = |origin: Location, message: Xcm<()>, weight, error| {
			assert_storage_noop!(assert_eq!(run(origin, message, weight), Err(error)));
		};
		use ProcessMessageError::{BadFormat, Unsupported};

		// Only a child parachain that is not a system one.
		for origin in [
			Parachain(1000).into(),
			Parachain(1999).into(),
			Location::here(),
			Location::parent(),
			Location::new(1, [Parachain(PARA)]),
			[Parachain(PARA), PalletInstance(1)].into(),
		] {
			refused(origin, message(allowed_call()), max, Unsupported);
		}

		// Heavier than allowed.
		refused(para.clone(), message(allowed_call()), max.saturating_add(one), Unsupported);
		// The origin must cover the whole message.
		let mut limited = message(allowed_call());
		limited.0[0] =
			UnpaidExecution { weight_limit: Limited(max.saturating_sub(one)), check_origin: None };
		refused(para.clone(), limited, max, Unsupported);

		// Only as the origin itself.
		let mut as_account = message(allowed_call());
		as_account.0[1] = transact(OriginKind::SovereignAccount, allowed_call());
		refused(para.clone(), as_account, max, Unsupported);
		// Only one call, and nothing else.
		let mut two_calls = message(allowed_call());
		two_calls.0.push(transact(OriginKind::Native, allowed_call()));
		refused(para.clone(), two_calls, max, Unsupported);
		let bare = Xcm(vec![transact(OriginKind::Native, allowed_call())]);
		refused(para.clone(), bare, max, Unsupported);

		// Only the allowed calls.
		let remark = RuntimeCall::from(frame_system::Call::<Test>::remark { remark: vec![] });
		refused(para.clone(), message(remark.encode()), max, Unsupported);
		// Not a call, or one followed by more bytes.
		refused(para.clone(), message(vec![]), max, BadFormat);
		refused(para.clone(), message(vec![0xff]), max, BadFormat);
		let mut trailing = allowed_call();
		trailing.push(0);
		refused(para.clone(), message(trailing), max, BadFormat);

		// A weight limit that covers the message is fine.
		let mut limited = message(allowed_call());
		limited.0[0] = UnpaidExecution { weight_limit: Limited(max), check_origin: None };
		assert_ok!(run(para, limited, max));
		assert_eq!(window(PARA), Some((System::block_number(), 1)));
	});
}

/// A genesis with `paras` registered.
fn genesis_with_paras(paras: &[u32]) -> MockGenesisConfig {
	let genesis = ParaGenesisArgs {
		para_kind: ParaKind::Parachain,
		genesis_head: HeadData(vec![4, 5, 6]),
		validation_code: dummy_validation_code(),
	};
	MockGenesisConfig {
		paras: crate::paras::GenesisConfig {
			paras: paras.iter().map(|para| ((*para).into(), genesis.clone())).collect(),
			..Default::default()
		},
		..Default::default()
	}
}

/// The call that removes the window of `para`.
fn remove_offboarded(para: u32) -> RuntimeCall {
	Call::<Test>::remove_offboarded { para: para.into() }.into()
}

/// Whether `call` is authorized.
fn authorize(call: &RuntimeCall) -> TransactionValidityWithRefund {
	call.authorize(TransactionSource::External)
		.expect("the call has authorize logic")
}

/// The window of a parachain stays once it is over and once the parachain is offboarded, until
/// anybody removes it.
#[test]
fn window_of_an_offboarded_para_is_removed() {
	// 2000 is registered, 2001 is not.
	new_test_ext(genesis_with_paras(&[2000])).execute_with(|| {
		System::set_block_number(1);
		assert_ok!(run_allowed(2000));
		assert_ok!(run_allowed(2001));
		System::set_block_number(1 + UnpaidTransactWindow::get());
		assert_eq!((window(2000), window(2001)), (Some((1, 1)), Some((1, 1))));

		let call = remove_offboarded(2001);
		let (validity, _) = authorize(&call).unwrap();
		assert_eq!(validity.priority, 0);
		assert_ok!(call.clone().dispatch(RawOrigin::Authorized.into()));
		assert_eq!((window(2000), window(2001)), (Some((1, 1)), None));

		// Nothing left to remove: the same call is not valid any more.
		assert_eq!(authorize(&call), Err(InvalidTransaction::Stale.into()));
	});
}

/// A removal is only valid for the window of an offboarded parachain. And it is only dispatched
/// as an authorized call.
#[test]
fn other_removals_are_refused() {
	new_test_ext(genesis_with_paras(&[2000])).execute_with(|| {
		assert_ok!(run_allowed(2000));
		assert_ok!(run_allowed(2001));
		assert_ok!(authorize(&remove_offboarded(2001)));

		let stale: TransactionValidityWithRefund = Err(InvalidTransaction::Stale.into());
		// A parachain that is registered.
		assert_eq!(authorize(&remove_offboarded(2000)), stale);
		// A parachain that has no window.
		assert_eq!(authorize(&remove_offboarded(2002)), stale);

		for origin in [RuntimeOrigin::signed(1), RuntimeOrigin::root(), RuntimeOrigin::none()] {
			assert_noop!(remove_offboarded(2001).dispatch(origin), BadOrigin);
		}
	});
}

/// Every `CleanupPeriod`, the offchain worker submits the removal of the window of each
/// offboarded parachain.
#[test]
fn offchain_worker_submits_removals() {
	let mut ext = new_test_ext(genesis_with_paras(&[2000]));
	let (offchain, _) = TestOffchainExt::new();
	let (pool, pool_state) = TestTransactionPoolExt::new();
	ext.register_extension(OffchainDbExt::new(offchain.clone()));
	ext.register_extension(OffchainWorkerExt::new(offchain));
	ext.register_extension(TransactionPoolExt::new(pool));

	ext.execute_with(|| {
		let period = UnpaidTransactCleanupPeriod::get();
		let submitted = || {
			let transactions = core::mem::take(&mut pool_state.write().transactions);
			let mut calls = transactions
				.into_iter()
				.map(|tx| UncheckedExtrinsic::decode(&mut &tx[..]).unwrap().function)
				.collect::<Vec<_>>();
			calls.sort_by_key(|call| call.encode());
			calls
		};

		// Nothing to remove.
		UnpaidTransact::offchain_worker(period);
		assert_eq!(submitted(), vec![]);

		// Three offboarded parachains, and a registered one.
		for para in [2000, 3000, 3001, 3002] {
			Windows::<Test>::insert(ParaId::from(para), (0, 1));
		}

		// Not at this block.
		UnpaidTransact::offchain_worker(period + 1);
		assert_eq!(submitted(), vec![]);

		UnpaidTransact::offchain_worker(2 * period);
		let calls = submitted();
		assert_eq!(calls, (3000..3003).map(remove_offboarded).collect::<Vec<_>>());
		for call in calls {
			assert_ok!(authorize(&call));
			assert_ok!(call.dispatch(RawOrigin::Authorized.into()));
		}
		assert_eq!(Windows::<Test>::iter_keys().collect::<Vec<_>>(), vec![ParaId::from(2000)]);
	});
}
