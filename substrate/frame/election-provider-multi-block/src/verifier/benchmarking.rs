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

//! # Benchmarking for the Elections Multiblock Verifier sub-pallet.

use super::*;
use crate::{
	benchmarking::helpers,
	signed::pallet::Submissions,
	unsigned::miner::OffchainWorkerMiner,
	verifier::{AsyncVerifier, Status, Verifier},
	BenchmarkingConfig, ConfigCore, ConfigSigned, ConfigUnsigned, ConfigVerifier, PalletCore,
	PalletVerifier,
};
use frame_support::assert_ok;
use frame_system::RawOrigin;

use frame_benchmarking::v2::*;

#[benchmarks(
    where T: ConfigCore + ConfigSigned + ConfigUnsigned + ConfigVerifier,
)]
mod benchmarks {
	use super::*;
	use frame_support::traits::Hooks;

	#[benchmark]
	fn on_initialize_ongoing(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		let valid_solution = true;
		let submitter = helpers::mine_and_submit::<T>(Some(PalletCore::<T>::msp()), valid_solution)
			.map_err(|err| {
				log!(error, "error mining and storing paged solutions, {:?}", err);
				BenchmarkError::Stop("mine and store error")
			})?;

		// page is ready for async verification.
		assert!(Submissions::<T>::get_page(
			&submitter,
			PalletCore::<T>::current_round(),
			PalletCore::<T>::msp()
		)
		.is_some());

		// no backings for pages yet in storage.
		assert!(PalletVerifier::<T>::pages_backed() == 0);

		// set verifier status to pick first submitted page to verify.
		<PalletVerifier<T> as AsyncVerifier>::set_status(
			Status::Ongoing(crate::Pallet::<T>::msp()),
		);

		#[block]
		{
			PalletVerifier::<T>::on_initialize(0u32.into());
		}

		// backings from submitted and verified page is in storage now
		assert!(PalletVerifier::<T>::pages_backed() == 1);

		Ok(())
	}

	#[benchmark]
	fn on_initialize_ongoing_failed(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		let valid_solution = false;
		let submitter = helpers::mine_and_submit::<T>(Some(PalletCore::<T>::msp()), valid_solution)
			.map_err(|err| {
				log!(error, "error mining and storing paged solutions, {:?}", err);
				BenchmarkError::Stop("mine and store error")
			})?;

		// page is ready for async verification.
		assert!(Submissions::<T>::get_page(
			&submitter,
			PalletCore::<T>::current_round(),
			PalletCore::<T>::msp()
		)
		.is_some());

		// no backings for pages in storage.
		assert!(PalletVerifier::<T>::pages_backed() == 0);

		// set verifier status to pick first submitted page to verify.
		<PalletVerifier<T> as AsyncVerifier>::set_status(
			Status::Ongoing(crate::Pallet::<T>::msp()),
		);

		#[block]
		{
			PalletVerifier::<T>::on_initialize(0u32.into());
		}

		// no backings for pages in storage due to failure.
		assert!(PalletVerifier::<T>::pages_backed() == 0);

		Ok(())
	}

	#[benchmark]
	fn on_initialize_ongoing_finalize(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		// submit all pages with a valid solution.
		let valid_solution = true;
		let submitter = helpers::mine_and_submit::<T>(None, valid_solution).map_err(|err| {
			log!(error, "error mining and storing paged solutions, {:?}", err);
			BenchmarkError::Stop("mine and store error")
		})?;

		// all pages are ready for async verification.
		for page in 0..T::Pages::get() {
			assert!(Submissions::<T>::get_page(&submitter, PalletCore::<T>::current_round(), page)
				.is_some());
		}

		// no backings for pages in storage.
		assert!(PalletVerifier::<T>::pages_backed() == 0);
		// no queued score yet.
		assert!(<PalletVerifier<T> as Verifier>::queued_score().is_none());

		// process all paged solutions but lsp.
		for page in (1..T::Pages::get()).rev() {
			<PalletVerifier<T> as AsyncVerifier>::set_status(Status::Ongoing(page));
			Pallet::<T>::on_initialize(0u32.into());
		}

		assert!(PalletVerifier::<T>::pages_backed() as u32 == T::Pages::get().saturating_sub(1));

		// set verifier status to pick last submitted page to verify.
		<PalletVerifier<T> as AsyncVerifier>::set_status(Status::Ongoing(PalletCore::<T>::lsp()));

		#[block]
		{
			PalletVerifier::<T>::on_initialize(0u32.into());
		}

		// OK, so score is queued.
		assert!(<PalletVerifier<T> as Verifier>::queued_score().is_some());

		Ok(())
	}

	#[benchmark]
	fn on_initialize_ongoing_finalize_failed(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		#[block]
		{
			let _ = 1 + 2;
		}

		Ok(())
	}

	#[benchmark]
	fn finalize_async_verification(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		#[block]
		{
			let _ = 1 + 2;
		}

		Ok(())
	}

	#[benchmark]
	fn verify_sync_paged(
		v: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::VOTERS_PER_PAGE[1] },
		>,
		t: Linear<
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[0] },
			{ <T as ConfigCore>::BenchmarkingConfig::TARGETS_PER_PAGE[1] },
		>,
	) -> Result<(), BenchmarkError> {
		helpers::setup_data_provider::<T>(
			<T as ConfigCore>::BenchmarkingConfig::VOTERS.max(v),
			<T as ConfigCore>::BenchmarkingConfig::TARGETS.max(t),
		);

		if let Err(err) = helpers::setup_snapshot::<T>(v, t) {
			log!(error, "error setting up snapshot: {:?}.", err);
			return Err(BenchmarkError::Stop("snapshot error"));
		}

		let (_claimed_full_score, partial_score, paged_solution) =
			OffchainWorkerMiner::<T>::mine(PalletCore::<T>::msp()).map_err(|err| {
				log!(error, "mine error: {:?}", err);
				BenchmarkError::Stop("miner error")
			})?;

		#[block]
		{
			assert_ok!(PalletVerifier::<T>::do_verify_sync(
				paged_solution,
				partial_score,
				PalletCore::<T>::msp()
			));
		}

		Ok(())
	}

	impl_benchmark_test_suite!(
		PalletVerifier,
		crate::mock::ExtBuilder::default(),
		crate::mock::Runtime,
		exec_name = build_and_execute
	);
}
