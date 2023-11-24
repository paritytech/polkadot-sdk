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

//! # Types for the multi-block election provider pallet.

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::{BoundedVec, DebugNoBound};
use scale_info::TypeInfo;

use crate::Verifier;

use frame_election_provider_support::{NposSolution, PageIndex};

/// Supports that are returned from a given [`Verifier`].
pub type SupportsOf<V> = frame_election_provider_support::BoundedSupports<
	<V as Verifier>::AccountId,
	<V as Verifier>::MaxWinnersPerPage,
	<V as Verifier>::MaxBackersPerWinner,
>;

/// The voter index. Derived from [`SolutionOf`].
pub type SolutionVoterIndexOf<T> = <SolutionOf<T> as NposSolution>::VoterIndex;
/// The target index. Derived from [`SolutionOf`].
pub type SolutionTargetIndexOf<T> = <SolutionOf<T> as NposSolution>::TargetIndex;

/// The solution type used by this crate.
pub type SolutionOf<T> = <T as crate::Config>::Solution;

#[derive(DebugNoBound)]
pub enum ElectionError {
	/// Error returned by the election data provider.
	DataProvider,
	/// The data provider returned data that exceeded the boundaries defined in the contract with
	/// the election provider.
	DataProviderBoundariesExceeded,
}

#[derive(PartialEq, Eq, Clone, Copy, Encode, Decode, MaxEncodedLen, Debug, TypeInfo)]
pub enum Phase<Bn> {
	Halted,
	Off,
	Signed,
	SignedValidation(Bn),
	Unsigned(Bn),
	Snapshot(PageIndex),
	Export,
	Emergency,
}

impl<Bn> Default for Phase<Bn> {
	fn default() -> Self {
		Phase::Off
	}
}

/// Alias for a voter, parameterized by this crate's config.
pub(crate) type VoterOf<T> =
	frame_election_provider_support::VoterOf<<T as crate::Config>::DataProvider>;

/// Alias for a page of voters, parameterized by this crate's config.
pub(crate) type VoterPageOf<T> =
	BoundedVec<VoterOf<T>, <T as crate::Config>::VoterSnapshotPerBlock>;
