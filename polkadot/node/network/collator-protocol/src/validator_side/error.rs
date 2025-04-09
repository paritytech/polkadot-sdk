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

use fatality::thiserror::Error;
use futures::channel::oneshot;

use polkadot_node_subsystem::RuntimeApiError;
use polkadot_node_subsystem_util::backing_implicit_view;
use polkadot_primitives::vstaging::CandidateDescriptorVersion;

/// General result.
pub type Result<T> = std::result::Result<T, Error>;

/// General subsystem error.
#[derive(Error, Debug)]
pub enum Error {
	#[error(transparent)]
	ImplicitViewFetchError(backing_implicit_view::FetchError),

	#[error("Response receiver for active validators request cancelled")]
	CancelledActiveValidators(oneshot::Canceled),

	#[error("Response receiver for validator groups request cancelled")]
	CancelledValidatorGroups(oneshot::Canceled),

	#[error("Response receiver for session index request cancelled")]
	CancelledSessionIndex(oneshot::Canceled),

	#[error("Response receiver for claim queue request cancelled")]
	CancelledClaimQueue(oneshot::Canceled),

	#[error("Response receiver for node features request cancelled")]
	CancelledNodeFeatures(oneshot::Canceled),

	#[error("No state for the relay parent")]
	RelayParentStateNotFound,

	#[error("Error while accessing Runtime API")]
	RuntimeApi(#[from] RuntimeApiError),
}

/// An error occurred when attempting to start seconding a candidate.
#[derive(Debug, Error)]
pub enum SecondingError {
	#[error("Error while accessing Runtime API")]
	RuntimeApi(#[from] RuntimeApiError),

	#[error("Response receiver for persisted validation data request cancelled")]
	CancelledRuntimePersistedValidationData(oneshot::Canceled),

	#[error("Response receiver for prospective validation data request cancelled")]
	CancelledProspectiveValidationData(oneshot::Canceled),

	#[error("Persisted validation data is not available")]
	PersistedValidationDataNotFound,

	#[error("Persisted validation data hash doesn't match one in the candidate receipt.")]
	PersistedValidationDataMismatch,

	#[error("Candidate hash doesn't match the advertisement")]
	CandidateHashMismatch,

	#[error("Relay parent hash doesn't match the advertisement")]
	RelayParentMismatch,

	#[error("Received duplicate collation from the peer")]
	Duplicate,

	#[error("The provided parent head data does not match the hash")]
	ParentHeadDataMismatch,

	#[error("Core index {0} present in descriptor is different than the assigned core {1}")]
	InvalidCoreIndex(u32, u32),

	#[error("Session index {0} present in descriptor is different than the expected one {1}")]
	InvalidSessionIndex(u32, u32),

	#[error("Invalid candidate receipt version {0:?}")]
	InvalidReceiptVersion(CandidateDescriptorVersion),
}

impl SecondingError {
	/// Returns true if an error indicates that a peer is malicious.
	pub fn is_malicious(&self) -> bool {
		use SecondingError::*;
		matches!(
			self,
			PersistedValidationDataMismatch |
				CandidateHashMismatch |
				RelayParentMismatch |
				ParentHeadDataMismatch |
				InvalidCoreIndex(_, _) |
				InvalidSessionIndex(_, _) |
				InvalidReceiptVersion(_)
		)
	}
}

/// Failed to request a collation due to an error.
#[derive(Debug, Error)]
pub enum FetchError {
	#[error("Collation was not previously advertised")]
	NotAdvertised,

	#[error("Peer is unknown")]
	UnknownPeer,

	#[error("Collation was already requested")]
	AlreadyRequested,

	#[error("Relay parent went out of view")]
	RelayParentOutOfView,

	#[error("Peer's protocol doesn't match the advertisement")]
	ProtocolMismatch,
}
