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
use crate::configuration::HostConfiguration;
use alloc::vec;
use frame_benchmarking::v2::*;
use frame_support::traits::fungible::Mutate;
use frame_system::{pallet_prelude::BlockNumberFor, RawOrigin};
use polkadot_primitives::{
	HeadData, Id as ParaId, ValidationCode, MAX_CODE_SIZE, MAX_HEAD_DATA_SIZE,
};
use sp_runtime::traits::{One, Saturating};

pub mod mmr_setup;
mod pvf_check;

use self::pvf_check::{VoteCause, VoteOutcome};

// 2 ^ 10, because binary search time complexity is O(log(2, n)) and n = 1024 gives us a big and
// round number.
// Due to the limited number of parachains, the number of pruning, upcoming upgrades and cooldowns
// shouldn't exceed this number.
const SAMPLE_SIZE: u32 = 1024;

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	let events = frame_system::Pallet::<T>::events();
	let system_event: <T as frame_system::Config>::RuntimeEvent = generic_event.into();
	// compare to the last event record
	let frame_system::EventRecord { event, .. } = &events[events.len() - 1];
	assert_eq!(event, &system_event);
}

fn generate_disordered_pruning<T: Config>() {
	let mut needs_pruning = Vec::new();

	for i in 0..SAMPLE_SIZE {
		let id = ParaId::from(i);
		let block_number = BlockNumberFor::<T>::from(1000u32);
		needs_pruning.push((id, block_number));
	}

	PastCodePruning::<T>::put(needs_pruning);
}

pub(crate) fn generate_disordered_upgrades<T: Config>() {
	let mut upgrades = Vec::new();
	let mut cooldowns = Vec::new();

	for i in 0..SAMPLE_SIZE {
		let id = ParaId::from(i);
		let block_number = BlockNumberFor::<T>::from(1000u32);
		upgrades.push((id, block_number));
		cooldowns.push((id, block_number));
	}

	UpcomingUpgrades::<T>::put(upgrades);
	UpgradeCooldowns::<T>::put(cooldowns);
}

fn generate_disordered_actions_queue<T: Config>() {
	let mut queue = Vec::new();
	let next_session = shared::CurrentSessionIndex::<T>::get().saturating_add(One::one());

	for _ in 0..SAMPLE_SIZE {
		let id = ParaId::from(1000);
		queue.push(id);
	}

	ActionsQueue::<T>::mutate(next_session, |v| {
		*v = queue;
	});
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn force_set_current_code(c: Linear<MIN_CODE_SIZE, MAX_CODE_SIZE>) {
		let new_code = ValidationCode(vec![0; c as usize]);
		let para_id = ParaId::from(c as u32);
		CurrentCodeHash::<T>::insert(&para_id, new_code.hash());
		generate_disordered_pruning::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, new_code);

		assert_last_event::<T>(Event::CurrentCodeUpdated(para_id).into());
	}

	#[benchmark]
	fn force_set_current_head(s: Linear<MIN_CODE_SIZE, MAX_HEAD_DATA_SIZE>) {
		let new_head = HeadData(vec![0; s as usize]);
		let para_id = ParaId::from(1000);

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, new_head);

		assert_last_event::<T>(Event::CurrentHeadUpdated(para_id).into());
	}

	#[benchmark]
	fn force_set_most_recent_context() {
		let para_id = ParaId::from(1000);
		let context = BlockNumberFor::<T>::from(1000u32);

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, context);
	}

	#[benchmark]
	fn force_schedule_code_upgrade(c: Linear<MIN_CODE_SIZE, MAX_CODE_SIZE>) {
		let new_code = ValidationCode(vec![0; c as usize]);
		let para_id = ParaId::from(c as u32);
		let block = BlockNumberFor::<T>::from(c);
		generate_disordered_upgrades::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, new_code, block);

		assert_last_event::<T>(Event::CodeUpgradeScheduled(para_id).into());
	}

	#[benchmark]
	fn force_note_new_head(s: Linear<MIN_CODE_SIZE, MAX_HEAD_DATA_SIZE>) {
		let para_id = ParaId::from(1000);
		let new_head = HeadData(vec![0; s as usize]);
		let old_code_hash = ValidationCode(vec![0]).hash();
		CurrentCodeHash::<T>::insert(&para_id, old_code_hash);
		frame_system::Pallet::<T>::set_block_number(10u32.into());
		// schedule an expired code upgrade for this `para_id` so that force_note_new_head would use
		// the worst possible code path
		let expired = frame_system::Pallet::<T>::block_number().saturating_sub(One::one());
		let config = HostConfiguration::<BlockNumberFor<T>>::default();
		generate_disordered_pruning::<T>();
		Pallet::<T>::schedule_code_upgrade(
			para_id,
			ValidationCode(vec![0u8; MIN_CODE_SIZE as usize]),
			expired,
			&config,
			UpgradeStrategy::SetGoAheadSignal,
		);

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, new_head);

		assert_last_event::<T>(Event::NewHeadNoted(para_id).into());
	}

	#[benchmark]
	fn force_queue_action() {
		let para_id = ParaId::from(1000);
		generate_disordered_actions_queue::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, para_id);

		let next_session =
			crate::shared::CurrentSessionIndex::<T>::get().saturating_add(One::one());
		assert_last_event::<T>(Event::ActionQueued(para_id, next_session).into());
	}

	#[benchmark]
	fn add_trusted_validation_code(c: Linear<MIN_CODE_SIZE, MAX_CODE_SIZE>) {
		let new_code = ValidationCode(vec![0; c as usize]);

		pvf_check::prepare_bypassing_bench::<T>(new_code.clone());

		#[extrinsic_call]
		_(RawOrigin::Root, new_code);
	}

	#[benchmark]
	fn poke_unused_validation_code() {
		let code_hash = [0; 32].into();

		#[extrinsic_call]
		_(RawOrigin::Root, code_hash);
	}

	#[benchmark]
	fn include_pvf_check_statement() {
		let (stmt, signature) = pvf_check::prepare_inclusion_bench::<T>();

		#[block]
		{
			let _ =
				Pallet::<T>::include_pvf_check_statement(RawOrigin::None.into(), stmt, signature);
		}
	}

	#[benchmark]
	fn include_pvf_check_statement_finalize_upgrade_accept() {
		let (stmt, signature) =
			pvf_check::prepare_finalization_bench::<T>(VoteCause::Upgrade, VoteOutcome::Accept);

		#[block]
		{
			let _ =
				Pallet::<T>::include_pvf_check_statement(RawOrigin::None.into(), stmt, signature);
		}
	}

	#[benchmark]
	fn include_pvf_check_statement_finalize_upgrade_reject() {
		let (stmt, signature) =
			pvf_check::prepare_finalization_bench::<T>(VoteCause::Upgrade, VoteOutcome::Reject);

		#[block]
		{
			let _ =
				Pallet::<T>::include_pvf_check_statement(RawOrigin::None.into(), stmt, signature);
		}
	}

	#[benchmark]
	fn include_pvf_check_statement_finalize_onboarding_accept() {
		let (stmt, signature) =
			pvf_check::prepare_finalization_bench::<T>(VoteCause::Onboarding, VoteOutcome::Accept);

		#[block]
		{
			let _ =
				Pallet::<T>::include_pvf_check_statement(RawOrigin::None.into(), stmt, signature);
		}
	}

	#[benchmark]
	fn include_pvf_check_statement_finalize_onboarding_reject() {
		let (stmt, signature) =
			pvf_check::prepare_finalization_bench::<T>(VoteCause::Onboarding, VoteOutcome::Reject);

		#[block]
		{
			let _ =
				Pallet::<T>::include_pvf_check_statement(RawOrigin::None.into(), stmt, signature);
		}
	}

	#[benchmark]
	fn remove_upgrade_cooldown() -> Result<(), BenchmarkError> {
		let para_id = ParaId::from(1000);
		let old_code_hash = ValidationCode(vec![0]).hash();
		CurrentCodeHash::<T>::insert(&para_id, old_code_hash);
		frame_system::Pallet::<T>::set_block_number(10u32.into());
		let inclusion = frame_system::Pallet::<T>::block_number().saturating_add(10u32.into());
		let config = HostConfiguration::<BlockNumberFor<T>>::default();
		Pallet::<T>::schedule_code_upgrade(
			para_id,
			ValidationCode(vec![0u8; MIN_CODE_SIZE as usize]),
			inclusion,
			&config,
			UpgradeStrategy::SetGoAheadSignal,
		);

		let who: T::AccountId = whitelisted_caller();

		T::Fungible::mint_into(
			&who,
			T::CooldownRemovalMultiplier::get().saturating_mul(1_000_000u32.into()),
		)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(who), para_id);

		assert_last_event::<T>(Event::UpgradeCooldownRemoved { para_id }.into());

		Ok(())
	}

	#[benchmark]
	fn authorize_force_set_current_code_hash() {
		let para_id = ParaId::from(1000);
		let code = ValidationCode(vec![0; 32]);
		let new_code_hash = code.hash();
		let valid_period = BlockNumberFor::<T>::from(1_000_000_u32);
		ParaLifecycles::<T>::insert(&para_id, ParaLifecycle::Parachain);

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, new_code_hash, valid_period);

		assert_last_event::<T>(
			Event::CodeAuthorized {
				para_id,
				code_hash: new_code_hash,
				expire_at: frame_system::Pallet::<T>::block_number().saturating_add(valid_period),
			}
			.into(),
		);
	}

	#[benchmark]
	fn apply_authorized_force_set_current_code(c: Linear<MIN_CODE_SIZE, MAX_CODE_SIZE>) {
		let code = ValidationCode(vec![0; c as usize]);
		let para_id = ParaId::from(1000);
		let expire_at =
			frame_system::Pallet::<T>::block_number().saturating_add(BlockNumberFor::<T>::from(c));
		AuthorizedCodeHash::<T>::insert(
			&para_id,
			AuthorizedCodeHashAndExpiry::from((code.hash(), expire_at)),
		);
		generate_disordered_pruning::<T>();

		#[extrinsic_call]
		_(RawOrigin::Root, para_id, code);

		assert_last_event::<T>(Event::CurrentCodeUpdated(para_id).into());
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext(Default::default()),
		crate::mock::Test
	);
}
