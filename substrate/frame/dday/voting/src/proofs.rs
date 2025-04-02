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

//! Definition and handling of proofs.

use crate::VotingPower;
use codec::MaxEncodedLen;
use frame_support::Parameter;

/// A trait representing the description of a proof used for voting.
pub trait ProofDescription {
	/// The block number at which the proof is generated.
	type BlockNumber: Parameter;
	/// The hasher used for generating the proof hash.
	type Hasher: sp_core::Hasher;
	/// The account type associated with the proof.
	type AccountId: Parameter + MaxEncodedLen;
	/// The balance type associated with the proof.
	type Balance: frame_support::traits::tokens::Balance;

	/// The proof representation itself.
	type Proof: Parameter;
}

/// A trait for verifying proofs used in voting.
pub trait VerifyProof {
	/// The associated proof description.
	type Proof: ProofDescription;

	/// Verifies the proof and extracts `VotingPower`.
	fn query_voting_power_for(
		who: &ProofAccountIdOf<Self>,
		hash: ProofHashOf<Self>,
		proof: ProofOf<Self>,
	) -> Option<VotingPower<ProofBalanceOf<Self>>>;
}

type ProofDescriptionOf<T> = <T as VerifyProof>::Proof;
/// Type alias for `VerifyProof::Proof::Balance`.
pub type ProofBalanceOf<T> = <ProofDescriptionOf<T> as ProofDescription>::Balance;
/// Type alias for `VerifyProof::Proof::AccountId`.
pub type ProofAccountIdOf<T> = <ProofDescriptionOf<T> as ProofDescription>::AccountId;
/// Type alias for `VerifyProof::Proof::Hasher`.
pub type ProofHasherOf<T> = <ProofDescriptionOf<T> as ProofDescription>::Hasher;
/// Type alias for `VerifyProof::Proof::Hasher::Out`.
pub type ProofHashOf<T> = <ProofHasherOf<T> as sp_core::Hasher>::Out;
/// Type alias for `VerifyProof::Proof::BlockNumber`.
pub type ProofBlockNumberOf<T> = <ProofDescriptionOf<T> as ProofDescription>::BlockNumber;
/// Type alias for `VerifyProof::Proof::Proof`.
pub type ProofOf<T> = <ProofDescriptionOf<T> as ProofDescription>::Proof;
