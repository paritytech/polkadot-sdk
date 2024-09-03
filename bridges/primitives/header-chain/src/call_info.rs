// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Defines structures related to calls of the `pallet-bridge-grandpa` pallet.

use crate::{justification, InitializationData};

use bp_runtime::HeaderOf;
use codec::{Decode, Encode};
use frame_support::{weights::Weight, RuntimeDebugNoBound};
use scale_info::TypeInfo;
use sp_consensus_grandpa::SetId;
use sp_runtime::traits::{Header as HeaderT, Zero};
use sp_std::{boxed::Box, fmt::Debug};

/// A minimized version of `pallet-bridge-grandpa::Call` that can be used without a runtime.
#[derive(Encode, Decode, Debug, PartialEq, Eq, Clone, TypeInfo)]
#[allow(non_camel_case_types)]
pub enum BridgeGrandpaCall<Header: HeaderT> {
	/// `pallet-bridge-grandpa::Call::submit_finality_proof`
	#[codec(index = 0)]
	submit_finality_proof {
		/// The header that we are going to finalize.
		finality_target: Box<Header>,
		/// Finality justification for the `finality_target`.
		justification: justification::GrandpaJustification<Header>,
	},
	/// `pallet-bridge-grandpa::Call::initialize`
	#[codec(index = 1)]
	initialize {
		/// All data, required to initialize the pallet.
		init_data: InitializationData<Header>,
	},
	/// `pallet-bridge-grandpa::Call::submit_finality_proof_ex`
	#[codec(index = 4)]
	submit_finality_proof_ex {
		/// The header that we are going to finalize.
		finality_target: Box<Header>,
		/// Finality justification for the `finality_target`.
		justification: justification::GrandpaJustification<Header>,
		/// An identifier of the validators set, that have signed the justification.
		current_set_id: SetId,
	},
}

/// The `BridgeGrandpaCall` for a pallet that bridges with given `C`;
pub type BridgeGrandpaCallOf<C> = BridgeGrandpaCall<HeaderOf<C>>;

/// A digest information on the `BridgeGrandpaCall::submit_finality_proof` call.
#[derive(Copy, Clone, PartialEq, RuntimeDebugNoBound)]
pub struct SubmitFinalityProofInfo<N: Debug> {
	/// Number of the finality target.
	pub block_number: N,
	/// An identifier of the validators set that has signed the submitted justification.
	/// It might be `None` if deprecated version of the `submit_finality_proof` is used.
	pub current_set_id: Option<SetId>,
	/// If `true`, then the call proves new **mandatory** header.
	pub is_mandatory: bool,
	/// If `true`, then the call must be free (assuming that everything else is valid) to
	/// be treated as valid.
	pub is_free_execution_expected: bool,
	/// Extra weight that we assume is included in the call.
	///
	/// We have some assumptions about headers and justifications of the bridged chain.
	/// We know that if our assumptions are correct, then the call must not have the
	/// weight above some limit. The fee paid for weight above that limit, is never refunded.
	pub extra_weight: Weight,
	/// Extra size (in bytes) that we assume are included in the call.
	///
	/// We have some assumptions about headers and justifications of the bridged chain.
	/// We know that if our assumptions are correct, then the call must not have the
	/// weight above some limit. The fee paid for bytes above that limit, is never refunded.
	pub extra_size: u32,
}

impl<N: Debug> SubmitFinalityProofInfo<N> {
	/// Returns `true` if call size/weight is below our estimations for regular calls.
	pub fn fits_limits(&self) -> bool {
		self.extra_weight.is_zero() && self.extra_size.is_zero()
	}
}
