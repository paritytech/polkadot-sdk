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

//! An opt-in utility module for reporting equivocations.
//!
//! This module defines:
//! - an offence type for AURA equivocations;
//! - a system for reporting offences;
//! - a system for signing and submitting transactions;
//! - a way to get the current block author;
//! - a key ownership proof system to prove that a given authority was part of a session;
//!
//! These can be used in an offchain context in order to submit equivocation
//! reporting extrinsics (from the client that's running the AURA protocol) and
//! in a runtime context, to validate the equivocation proofs in the extrinsic
//! and report the offences.

use frame_support::traits::{EstimateNextSessionRotation, Get, KeyOwnerProofSystem};
use frame_system::{
	offchain::SubmitTransaction,
	pallet_prelude::{BlockNumberFor, HeaderFor},
};

use sp_consensus_aura::{EquivocationProof, Slot, KEY_TYPE};
use sp_runtime::{
	traits::{CheckedDiv, Header as _, Zero},
	transaction_validity::{InvalidTransaction, TransactionValidityError},
	DispatchError, KeyTypeId, Perbill, RuntimeDebug,
};
use sp_session::{GetSessionNumber, GetValidatorCount};
use sp_staking::{
	offence::{Kind, Offence, OffenceReportSystem, ReportOffence},
	SessionIndex,
};
use sp_std::prelude::*;

use log::{error, info};

use crate::{Call, Config, Error, LOG_TARGET};

/// AURA equivocation offence report.
///
/// When a validator released two or more blocks at the same slot.
#[derive(Clone, PartialEq, Eq, RuntimeDebug)]
pub struct EquivocationOffence<Offender> {
	/// The slot in which this incident happened.
	pub slot: Slot,
	/// The session index in which the incident happened.
	pub session_index: SessionIndex,
	/// The size of the validator set at the time of the offence.
	pub validator_set_count: u32,
	/// The authority that produced the equivocation.
	pub offender: Offender,
}

impl<Offender: Clone> Offence<Offender> for EquivocationOffence<Offender> {
	const ID: Kind = *b"aura:equivocatio";
	type TimeSlot = Slot;

	fn offenders(&self) -> Vec<Offender> {
		vec![self.offender.clone()]
	}

	fn session_index(&self) -> SessionIndex {
		self.session_index
	}

	fn validator_set_count(&self) -> u32 {
		self.validator_set_count
	}

	fn time_slot(&self) -> Self::TimeSlot {
		self.slot
	}

	// The formula is min((3k / n)^2, 1)
	// where k = offenders_number and n = validators_number
	fn slash_fraction(&self, offenders_count: u32) -> Perbill {
		// Perbill type domain is [0, 1] by definition
		Perbill::from_rational(3 * offenders_count, self.validator_set_count).square()
	}
}

/// AURA equivocation offence report system.
///
/// This type implements `OffenceReportSystem` trait such that:
/// - Equivocation reports are published on-chain as unsigned extrinsic via
///   `offchain::SendTransactionTypes`.
/// - On-chain validity checks and processing are mostly delegated to the user-provided generic
///   types implementing `KeyOwnerProofSystem` and `ReportOffence` traits.
/// - Offence reporter for unsigned transactions is fetched via the the authorship pallet.
///
/// Depends on:
/// - pallet-authorship: to get reporter identity when missing during offence evidence processing.
/// - pallet-session: to check the `KeyOwnerProof` validity.
///
/// In order to check for `KeyOwnerProof` validity we need a way to compare the session index
/// contained within the proof and the session index relative to the produced blocks.
/// In order to map block number to session index this implementation requires the
/// `[pallet_session::Config::NextSessionRotation]` to be `[pallet_session::PeriodicSessions]`.
/// In this way the mapping is performed by just divinding the block number by the session period
/// duration.
pub struct EquivocationReportSystem<T, R, P, L>(sp_std::marker::PhantomData<(T, R, P, L)>);

impl<T, R, P, L, Period, Offset>
	OffenceReportSystem<
		Option<T::AccountId>,
		(EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	> for EquivocationReportSystem<T, R, P, L>
where
	T: Config
		+ frame_system::Config
		+ pallet_authorship::Config
		+ pallet_session::Config<
			NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>,
		> + frame_system::offchain::SendTransactionTypes<Call<T>>,
	R: ReportOffence<
		T::AccountId,
		P::IdentificationTuple,
		EquivocationOffence<P::IdentificationTuple>,
	>,
	P: KeyOwnerProofSystem<(KeyTypeId, T::AuthorityId), Proof = T::KeyOwnerProof>,
	P::IdentificationTuple: Clone,
	L: Get<u64>,
	Period: Get<BlockNumberFor<T>>,
	Offset: Get<BlockNumberFor<T>>,
{
	type Longevity = L;

	fn publish_evidence(
		evidence: (EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	) -> Result<(), ()> {
		let (equivocation_proof, key_owner_proof) = evidence;

		let call = Call::report_equivocation_unsigned {
			equivocation_proof: Box::new(equivocation_proof),
			key_owner_proof,
		};
		let res = SubmitTransaction::<T, Call<T>>::submit_unsigned_transaction(call.into());
		match res {
			Ok(_) => info!(target: LOG_TARGET, "Submitted equivocation report"),
			Err(e) => error!(target: LOG_TARGET, "Error submitting equivocation report: {:?}", e),
		}
		res
	}

	fn check_evidence(
		evidence: (EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	) -> Result<(), TransactionValidityError> {
		let (equivocation_proof, key_owner_proof) = evidence;

		// Check the membership proof to extract the offender's id
		let key = (sp_consensus_aura::KEY_TYPE, equivocation_proof.offender.clone());
		let offender =
			P::check_proof(key, key_owner_proof.clone()).ok_or(InvalidTransaction::BadProof)?;

		// Check if the offence has already been reported, and if so then we can discard the report.
		if R::is_known_offence(&[offender], &equivocation_proof.slot) {
			Err(InvalidTransaction::Stale.into())
		} else {
			Ok(())
		}
	}

	fn process_evidence(
		reporter: Option<T::AccountId>,
		evidence: (EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	) -> Result<(), DispatchError> {
		let (equivocation_proof, key_owner_proof) = evidence;
		let reporter = reporter.or_else(|| <pallet_authorship::Pallet<T>>::author());

		let offender = equivocation_proof.offender.clone();
		let slot = equivocation_proof.slot;

		let block_num1 = *equivocation_proof.first_header.number();
		let block_num2 = *equivocation_proof.second_header.number();

		// Validate the equivocation proof (check votes are different and signatures are valid)
		if !sp_consensus_aura::check_equivocation_proof(equivocation_proof) {
			return Err(Error::<T>::InvalidEquivocationProof.into())
		}

		let validator_set_count = key_owner_proof.validator_count();

		// Because we are using the `PeriodicSession` type, this is the exact session duration.
		let session_len =
			<<T as pallet_session::Config>::NextSessionRotation as EstimateNextSessionRotation<
				BlockNumberFor<T>,
			>>::average_session_length();

		let session_index = key_owner_proof.session();

		let idx1 = block_num1.checked_div(&session_len).unwrap_or(Zero::zero());
		let idx2 = block_num2.checked_div(&session_len).unwrap_or(Zero::zero());

		if BlockNumberFor::<T>::from(session_index as u32) != idx1 || idx1 != idx2 {
			return Err(Error::<T>::InvalidKeyOwnershipProof.into())
		}

		// Check the membership proof and extract the offender's id
		let offender = P::check_proof((KEY_TYPE, offender), key_owner_proof)
			.ok_or(Error::<T>::InvalidKeyOwnershipProof)?;

		let offence = EquivocationOffence { slot, session_index, validator_set_count, offender };
		R::report_offence(reporter.into_iter().collect(), offence)
			.map_err(|_| Error::<T>::DuplicateOffenceReport)?;

		Ok(())
	}
}
