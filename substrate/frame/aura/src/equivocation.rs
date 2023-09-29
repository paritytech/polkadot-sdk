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

use frame_support::traits::{Get, KeyOwnerProofSystem};
use frame_system::pallet_prelude::HeaderFor;
use log::{error, info};

use sp_consensus_aura::{EquivocationProof, Slot, KEY_TYPE};
use sp_runtime::{
	transaction_validity::{
		InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity,
		TransactionValidityError, ValidTransaction,
	},
	DispatchError, KeyTypeId, Perbill,
};
use sp_session::{GetSessionNumber, GetValidatorCount};
use sp_staking::{
	offence::{Kind, Offence, OffenceReportSystem, ReportOffence},
	SessionIndex,
};
use sp_std::prelude::*;

use crate::{Call, Config, Error, Pallet, LOG_TARGET};

/// AURA equivocation offence report.
///
/// When a validator released two or more blocks at the same slot.
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
/// This type implements `OffenceReportSystem` such that:
/// - Equivocation reports are published on-chain as unsigned extrinsic via
///   `offchain::SendTransactionTypes`.
/// - On-chain validity checks and processing are mostly delegated to the user-provided generic
///   types implementing `KeyOwnerProofSystem` and `ReportOffence` traits.
/// - Offence reporter for unsigned transactions is fetched via the the authorship pallet.
///
/// Requires the runtime to implement:
/// - pallet-authorship: to get reporter identity.
/// - pallet-session: to check the `KeyOwnerProof` validity (map block to session-id).
pub struct EquivocationReportSystem<T, R, P, L>(sp_std::marker::PhantomData<(T, R, P, L)>);

impl<T, R, P, L>
	OffenceReportSystem<
		Option<T::AccountId>,
		(EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	> for EquivocationReportSystem<T, R, P, L>
where
	T: Config
		+ pallet_authorship::Config
		+ pallet_session::Config
		+ frame_system::offchain::SendTransactionTypes<Call<T>>,
	R: ReportOffence<
		T::AccountId,
		P::IdentificationTuple,
		EquivocationOffence<P::IdentificationTuple>,
	>,
	P: KeyOwnerProofSystem<(KeyTypeId, T::AuthorityId), Proof = T::KeyOwnerProof>,
	P::IdentificationTuple: Clone,
	L: Get<u64>,
{
	type Longevity = L;

	fn publish_evidence(
		evidence: (EquivocationProof<HeaderFor<T>, T::AuthorityId>, T::KeyOwnerProof),
	) -> Result<(), ()> {
		use frame_system::offchain::SubmitTransaction;
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

		log::debug!(target: LOG_TARGET, "HELLOOOO");

		// Validate the equivocation proof (check votes are different and signatures are valid)
		if !sp_consensus_aura::check_equivocation_proof(equivocation_proof) {
			return Err(Error::<T>::InvalidEquivocationProof.into())
		}

		log::debug!(target: LOG_TARGET, "VALID EQUIVOCATION");

		// @TODO davxy

		let validator_set_count = key_owner_proof.validator_count();
		log::debug!(target: LOG_TARGET, "Validator-set count: {}", validator_set_count);
		let session_index = key_owner_proof.session();
		log::debug!(target: LOG_TARGET, "SESSION INDEX: {}", session_index);

		// TODO @davxy
		// How we can recover the session-index using the slot? We need GenesisSlot...
		// Maybe the best we can do is to depend on some external thing which is able
		// to transform slot to epoch...
		// Or report just for current session?

		// let epoch_index =
		// 	*slot.saturating_sub(crate::GenesisSlot::<T>::get()) / T::EpochDuration::get();

		// // Check that the slot number is consistent with the session index
		// // in the key ownership proof (i.e. slot is for that epoch)
		// if Pallet::<T>::session_index_for_epoch(epoch_index) != session_index {
		// 	return Err(Error::<T>::InvalidKeyOwnershipProof.into())
		// }

		// Check the membership proof and extract the offender's id
		let offender = P::check_proof((KEY_TYPE, offender), key_owner_proof)
			.ok_or(Error::<T>::InvalidKeyOwnershipProof)?;
		log::debug!(target: LOG_TARGET, "VALID KEY OWNER PROOF");

		let offence = EquivocationOffence { slot, session_index, validator_set_count, offender };
		R::report_offence(reporter.into_iter().collect(), offence)
			.map_err(|_| Error::<T>::DuplicateOffenceReport)?;

		Ok(())
	}
}
