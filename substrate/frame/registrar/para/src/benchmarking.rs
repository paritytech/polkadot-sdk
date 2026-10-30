// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Benchmarks for `pallet-registrar-para`.

use super::*;
use frame_benchmarking::v2::*;
use frame_support::traits::Get;
use frame_system::RawOrigin;
use polkadot_primitives::AsyncBackingParams;
use registrar_primitives::{MessageToPara, MessageToParaV1};

/// The host configuration every benchmark runs under.
fn host_configuration() -> AbridgedHostConfiguration {
	AbridgedHostConfiguration {
		max_code_size: 3 * 1024 * 1024,
		max_head_data_size: 32 * 1024,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_upward_message_size: 4 * 1024,
		max_upward_message_num_per_candidate: 2,
		hrmp_max_message_num_per_candidate: 2,
		validation_upgrade_cooldown: 2,
		validation_upgrade_delay: 2,
		async_backing_params: AsyncBackingParams {
			allowed_ancestry_len: 0,
			max_candidate_depth: 0,
		},
	}
}

/// Put [`host_configuration`] in place and return an account able to pay every consideration.
fn funded_manager<T: Config>() -> T::AccountId {
	let config = host_configuration();
	T::BenchmarkHelper::set_host_configuration(config.clone());

	let who: T::AccountId = account("manager", 0, 0);
	T::ReservationConsideration::ensure_successful(&who, Pallet::<T>::reservation_footprint());
	T::RegistrationConsideration::ensure_successful(
		&who,
		Pallet::<T>::registration_footprint(config.max_head_data_size, &config),
	);
	who
}

/// Reserve a para id for `who` and return it.
fn reserve_for<T: Config>(who: &T::AccountId) -> Result<ParaId, BenchmarkError> {
	Pallet::<T>::reserve(RawOrigin::Signed(who.clone()).into())?;
	Ok(NextFreeParaId::<T>::get().saturating_sub(1))
}

/// Reserve a para id and put it into `Pending`.
fn make_pending<T: Config>(who: &T::AccountId) -> Result<ParaId, BenchmarkError> {
	let para_id = reserve_for::<T>(who)?;
	let config = host_configuration();
	Pallet::<T>::register(
		RawOrigin::Signed(who.clone()).into(),
		para_id,
		alloc::vec![2u8; config.max_head_data_size as usize],
		config.max_code_size,
		sp_core::H256::repeat_byte(1),
	)?;
	Ok(para_id)
}

/// Reserve a para id and put it into `Registered`.
fn make_registered<T: Config>(who: &T::AccountId) -> Result<ParaId, BenchmarkError> {
	let para_id = make_pending::<T>(who)?;
	Pallet::<T>::receive(
		RawOrigin::Root.into(),
		MessageToPara::V1(MessageToParaV1::RegisterResponse {
			para_id,
			message_id: 0,
			outcome: Ok(()),
		}),
	)?;
	Ok(para_id)
}

/// Reserve a para id, register it and ask to deregister it. Returns the id and the awaited message.
fn make_deregistering<T: Config>(who: &T::AccountId) -> Result<(ParaId, u64), BenchmarkError> {
	let para_id = make_registered::<T>(who)?;
	Pallet::<T>::deregister(RawOrigin::Signed(who.clone()).into(), para_id)?;
	match Paras::<T>::get(para_id).map(|i| i.state) {
		Some(RegistrationState::Deregistering { message_id, .. }) => Ok((para_id, message_id)),
		_ => Err(BenchmarkError::Stop("not deregistering")),
	}
}

/// Reserve a para id, register it and lock it.
fn make_locked<T: Config>(who: &T::AccountId) -> Result<ParaId, BenchmarkError> {
	let para_id = make_registered::<T>(who)?;
	Pallet::<T>::add_lock(RawOrigin::Signed(who.clone()).into(), para_id)?;
	Ok(para_id)
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn reserve() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();

		#[extrinsic_call]
		_(RawOrigin::Signed(who.clone()));

		let para_id = NextFreeParaId::<T>::get().saturating_sub(1);
		assert_eq!(Paras::<T>::get(para_id).map(|i| i.manager), Some(who));
		Ok(())
	}

	/// Requesting a registration. Dominated by shipping the head data to the relay chain.
	#[benchmark]
	fn register(
		h: Linear<0, { host_configuration().max_head_data_size }>,
	) -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = reserve_for::<T>(&who)?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(who),
			para_id,
			alloc::vec![2u8; h as usize],
			host_configuration().max_code_size,
			sp_core::H256::repeat_byte(1),
		);

		assert!(matches!(
			Paras::<T>::get(para_id).map(|i| i.state),
			Some(RegistrationState::Pending { .. })
		));
		Ok(())
	}

	/// Force-registering an id nobody reserved: the worst case, since both considerations are
	/// taken rather than just the registration's.
	#[benchmark]
	fn force_register(
		h: Linear<0, { host_configuration().max_head_data_size }>,
	) -> Result<(), BenchmarkError> {
		let origin =
			T::ForceOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let who = funded_manager::<T>();
		let para_id = T::FirstPublicParaId::get();

		#[extrinsic_call]
		_(
			origin as T::RuntimeOrigin,
			para_id,
			who,
			alloc::vec![2u8; h as usize],
			host_configuration().max_code_size,
			sp_core::H256::repeat_byte(1),
			false,
		);

		assert!(matches!(
			Paras::<T>::get(para_id).map(|i| i.state),
			Some(RegistrationState::Pending { .. })
		));
		Ok(())
	}

	/// Asking the relay chain to drop an authorization. The deposit stays held, so this is the
	/// state write plus the message.
	#[benchmark]
	fn cancel_registration() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = make_pending::<T>(&who)?;
		T::BlockNumberProvider::set_block_number(
			T::BlockNumberProvider::current_block_number()
				.saturating_add(T::PendingDeadline::get())
				.saturating_add(1u32.into()),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(who), para_id);

		assert!(matches!(
			Paras::<T>::get(para_id).map(|i| i.state),
			Some(RegistrationState::Pending { .. })
		));
		Ok(())
	}

	/// Worst case: a confirmed deregistration releases both considerations and removes the entry.
	#[benchmark]
	fn receive() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let (para_id, message_id) = make_deregistering::<T>(&who)?;
		let message = MessageToPara::V1(MessageToParaV1::DeregisterResponse {
			para_id,
			message_id,
			outcome: Ok(()),
		});

		#[extrinsic_call]
		_(RawOrigin::Root, message);

		assert!(Paras::<T>::get(para_id).is_none());
		Ok(())
	}

	/// Giving up an id the relay chain never knew: the reservation goes back here and now.
	#[benchmark]
	fn deregister_reserved() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = reserve_for::<T>(&who)?;

		#[extrinsic_call]
		deregister(RawOrigin::Signed(who), para_id);

		assert!(Paras::<T>::get(para_id).is_none());
		Ok(())
	}

	/// Asking the relay chain to drop a registered para: the state write plus the message.
	#[benchmark]
	fn deregister_registered() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = make_registered::<T>(&who)?;

		#[extrinsic_call]
		deregister(RawOrigin::Signed(who), para_id);

		assert!(matches!(
			Paras::<T>::get(para_id).map(|i| i.state),
			Some(RegistrationState::Deregistering { .. })
		));
		Ok(())
	}

	/// Asking again after the answer never came: the same, plus reporting the lost answer.
	#[benchmark]
	fn deregister_retry() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let (para_id, awaited) = make_deregistering::<T>(&who)?;
		T::BlockNumberProvider::set_block_number(
			T::BlockNumberProvider::current_block_number()
				.saturating_add(T::PendingDeadline::get()),
		);

		#[extrinsic_call]
		deregister(RawOrigin::Signed(who), para_id);

		assert!(matches!(
			Paras::<T>::get(para_id).map(|i| i.state),
			Some(RegistrationState::Deregistering { message_id, .. }) if message_id != awaited
		));
		Ok(())
	}

	/// Locking a registered para: one state read and write.
	#[benchmark]
	fn add_lock() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = make_registered::<T>(&who)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(who), para_id);

		assert!(Paras::<T>::get(para_id).map(|i| i.is_locked()).unwrap_or(false));
		Ok(())
	}

	/// Unlocking, with root standing in for the para itself: both do the same read and write.
	#[benchmark]
	fn remove_lock() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = make_locked::<T>(&who)?;

		#[extrinsic_call]
		_(RawOrigin::Root, para_id);

		assert!(!Paras::<T>::get(para_id).map(|i| i.is_locked()).unwrap_or(true));
		Ok(())
	}

	/// Asking for a code upgrade: one state read and the outgoing message.
	#[benchmark]
	fn schedule_code_upgrade() -> Result<(), BenchmarkError> {
		let who = funded_manager::<T>();
		let para_id = make_registered::<T>(&who)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(who), para_id, sp_core::H256::repeat_byte(2), T::MaxCodeSize::get());

		Ok(())
	}

	impl_benchmark_test_suite!(Pallet, crate::mock::new_test_ext(), crate::mock::Test);
}
