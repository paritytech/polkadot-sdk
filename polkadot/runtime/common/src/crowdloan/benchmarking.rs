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

//! Benchmarking for crowdloan pallet

#[cfg(feature = "runtime-benchmarks")]
use super::{Pallet as Crowdloan, *};
use frame_support::{assert_ok, traits::OnInitialize};
use frame_system::RawOrigin;
use polkadot_runtime_parachains::paras;
use sp_core::crypto::UncheckedFrom;
use sp_runtime::traits::{Bounded, CheckedSub};

use frame_benchmarking::{account, benchmarks, whitelisted_caller};

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

fn create_fund<T: Config + paras::Config>(id: u32, end: BlockNumberFor<T>) -> ParaId {
	let cap = BalanceOf::<T>::max_value();
	let (_, offset) = T::Auctioneer::lease_period_length();
	// Set to the very beginning of lease period index 0.
	frame_system::Pallet::<T>::set_block_number(offset);
	let now = frame_system::Pallet::<T>::block_number();
	let (lease_period_index, _) = T::Auctioneer::lease_period_index(now).unwrap_or_default();
	let first_period = lease_period_index;
	let last_period = lease_period_index + ((SlotRange::LEASE_PERIODS_PER_SLOT as u32) - 1).into();
	let para_id = id.into();

	let caller = account("fund_creator", id, 0);
	CurrencyOf::<T>::make_free_balance_be(&caller, BalanceOf::<T>::max_value());

	// Assume ed25519 is most complex signature format
	let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());

	let head_data = T::Registrar::worst_head_data();
	let validation_code = T::Registrar::worst_validation_code();
	assert_ok!(T::Registrar::register(caller.clone(), para_id, head_data, validation_code.clone()));
	assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
		frame_system::Origin::<T>::Root.into(),
		validation_code,
	));
	T::Registrar::execute_pending_transitions();

	assert_ok!(Crowdloan::<T>::create(
		RawOrigin::Signed(caller).into(),
		para_id,
		cap,
		first_period,
		last_period,
		end,
		Some(pubkey)
	));

	para_id
}

fn contribute_fund<T: Config>(who: &T::AccountId, index: ParaId) {
	CurrencyOf::<T>::make_free_balance_be(&who, BalanceOf::<T>::max_value());
	let value = T::MinContribution::get();

	let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());
	let payload = (index, &who, BalanceOf::<T>::default(), value);
	let sig = crypto::create_ed25519_signature(&payload.encode(), pubkey);

	assert_ok!(Crowdloan::<T>::contribute(
		RawOrigin::Signed(who.clone()).into(),
		index,
		value,
		Some(sig)
	));
}

benchmarks! {
	where_clause { where T: paras::Config }

	create {
		let para_id = ParaId::from(1_u32);
		let cap = BalanceOf::<T>::max_value();
		let first_period = 0u32.into();
		let last_period = 3u32.into();
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;

		let caller: T::AccountId = whitelisted_caller();
		let head_data = T::Registrar::worst_head_data();
		let validation_code = T::Registrar::worst_validation_code();

		let verifier = MultiSigner::unchecked_from(account::<[u8; 32]>("verifier", 0, 0));

		CurrencyOf::<T>::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		T::Registrar::register(caller.clone(), para_id, head_data, validation_code.clone())?;
		assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
			frame_system::Origin::<T>::Root.into(),
			validation_code,
		));

		T::Registrar::execute_pending_transitions();

	}: _(RawOrigin::Signed(caller), para_id, cap, first_period, last_period, end, Some(verifier))
	verify {
		assert_last_event::<T>(Event::<T>::Created { para_id }.into())
	}

	// Contribute has two arms: PreEnding and Ending, but both are equal complexity.
	contribute {
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1, end);
		let caller: T::AccountId = whitelisted_caller();
		let contribution = T::MinContribution::get();
		CurrencyOf::<T>::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		assert!(NewRaise::<T>::get().is_empty());

		let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());
		let payload = (fund_index, &caller, BalanceOf::<T>::default(), contribution);
		let sig = crypto::create_ed25519_signature(&payload.encode(), pubkey);

	}: _(RawOrigin::Signed(caller.clone()), fund_index, contribution, Some(sig))
	verify {
		// NewRaise is appended to, so we don't need to fill it up for worst case scenario.
		assert!(!NewRaise::<T>::get().is_empty());
		assert_last_event::<T>(Event::<T>::Contributed { who: caller, fund_index, amount: contribution }.into());
	}

	withdraw {
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1337, end);
		let caller: T::AccountId = whitelisted_caller();
		let contributor = account("contributor", 0, 0);
		contribute_fund::<T>(&contributor, fund_index);
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
	}: _(RawOrigin::Signed(caller), contributor.clone(), fund_index)
	verify {
		assert_last_event::<T>(Event::<T>::Withdrew { who: contributor, fund_index, amount: T::MinContribution::get() }.into());
	}

	// Worst case: Refund removes `RemoveKeysLimit` keys, and is fully refunded.
	#[skip_meta]
	refund {
		let k in 0 .. T::RemoveKeysLimit::get();
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1337, end);

		// Dissolve will remove at most `RemoveKeysLimit` at once.
		for i in 0 .. k {
			contribute_fund::<T>(&account("contributor", i, 0), fund_index);
		}

		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
	}: _(RawOrigin::Signed(caller), fund_index)
	verify {
		assert_last_event::<T>(Event::<T>::AllRefunded { para_id: fund_index }.into());
	}

	dissolve {
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1337, end);
		let caller: T::AccountId = whitelisted_caller();
		frame_system::Pallet::<T>::set_block_number(BlockNumberFor::<T>::max_value());
	}: _(RawOrigin::Signed(caller.clone()), fund_index)
	verify {
		assert_last_event::<T>(Event::<T>::Dissolved { para_id: fund_index }.into());
	}

	edit {
		let para_id = ParaId::from(1_u32);
		let cap = BalanceOf::<T>::max_value();
		let first_period = 0u32.into();
		let last_period = 3u32.into();
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;

		let caller: T::AccountId = whitelisted_caller();
		let head_data = T::Registrar::worst_head_data();
		let validation_code = T::Registrar::worst_validation_code();

		let verifier = MultiSigner::unchecked_from(account::<[u8; 32]>("verifier", 0, 0));

		CurrencyOf::<T>::make_free_balance_be(&caller, BalanceOf::<T>::max_value());
		T::Registrar::register(caller.clone(), para_id, head_data, validation_code.clone())?;
		assert_ok!(paras::Pallet::<T>::add_trusted_validation_code(
			frame_system::Origin::<T>::Root.into(),
			validation_code,
		));

		T::Registrar::execute_pending_transitions();

		Crowdloan::<T>::create(
			RawOrigin::Signed(caller).into(),
			para_id, cap, first_period, last_period, end, Some(verifier.clone()),
		)?;

		// Doesn't matter what we edit to, so use the same values.
	}: _(RawOrigin::Root, para_id, cap, first_period, last_period, end, Some(verifier))
	verify {
		assert_last_event::<T>(Event::<T>::Edited { para_id }.into())
	}

	add_memo {
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1, end);
		let caller: T::AccountId = whitelisted_caller();
		contribute_fund::<T>(&caller, fund_index);
		let worst_memo = vec![42; T::MaxMemoLength::get().into()];
	}: _(RawOrigin::Signed(caller.clone()), fund_index, worst_memo.clone())
	verify {
		let fund = Funds::<T>::get(fund_index).expect("fund was created...");
		assert_eq!(
			Crowdloan::<T>::contribution_get(fund.fund_index, &caller),
			(T::MinContribution::get(), worst_memo),
		);
	}

	poke {
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end = lpl + offset;
		let fund_index = create_fund::<T>(1, end);
		let caller: T::AccountId = whitelisted_caller();
		contribute_fund::<T>(&caller, fund_index);
		NewRaise::<T>::kill();
		assert!(NewRaise::<T>::get().is_empty());
	}: _(RawOrigin::Signed(caller), fund_index)
	verify {
		assert!(!NewRaise::<T>::get().is_empty());
		assert_last_event::<T>(Event::<T>::AddedToNewRaise { para_id: fund_index }.into())
	}

	// Worst case scenario: N funds are all in the `NewRaise` list, we are
	// in the beginning of the ending period, and each fund outbids the next
	// over the same periods.
	on_initialize {
		// We test the complexity over different number of new raise
		let n in 2 .. 100;
		let (lpl, offset) = T::Auctioneer::lease_period_length();
		let end_block = lpl + offset - 1u32.into();

		let pubkey = crypto::create_ed25519_pubkey(b"//verifier".to_vec());

		for i in 0 .. n {
			let fund_index = create_fund::<T>(i, end_block);
			let contributor: T::AccountId = account("contributor", i, 0);
			let contribution = T::MinContribution::get() * (i + 1).into();
			let payload = (fund_index, &contributor, BalanceOf::<T>::default(), contribution);
			let sig = crypto::create_ed25519_signature(&payload.encode(), pubkey.clone());

			CurrencyOf::<T>::make_free_balance_be(&contributor, BalanceOf::<T>::max_value());
			Crowdloan::<T>::contribute(RawOrigin::Signed(contributor).into(), fund_index, contribution, Some(sig))?;
		}

		let now = frame_system::Pallet::<T>::block_number();
		let (lease_period_index, _) = T::Auctioneer::lease_period_index(now).unwrap_or_default();
		let duration = end_block
			.checked_sub(&frame_system::Pallet::<T>::block_number())
			.ok_or("duration of auction less than zero")?;
		T::Auctioneer::new_auction(duration, lease_period_index)?;

		assert_eq!(T::Auctioneer::auction_status(end_block).is_ending(), Some((0u32.into(), 0u32.into())));
		assert_eq!(NewRaise::<T>::get().len(), n as usize);
		let old_endings_count = EndingsCount::<T>::get();
	}: {
		Crowdloan::<T>::on_initialize(end_block);
	} verify {
		assert_eq!(EndingsCount::<T>::get(), old_endings_count + 1);
		assert_last_event::<T>(Event::<T>::HandleBidResult { para_id: (n - 1).into(), result: Ok(()) }.into());
	}

	impl_benchmark_test_suite!(
		Crowdloan,
		crate::integration_tests::new_test_ext_with_offset(10),
		crate::integration_tests::Test,
	);
}
