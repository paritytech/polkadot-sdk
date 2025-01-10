// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

//! This crate has single entrypoint to run synchronization loop that is built around finality
//! proofs, as opposed to headers synchronization loop, which is built around headers. The headers
//! are still submitted to the target node, but are treated as auxiliary data as we are not trying
//! to submit all source headers to the target node.

pub use crate::{
	base::{FinalityPipeline, SourceClientBase},
	finality_loop::{
		metrics_prefix, run, FinalitySyncParams, HeadersToRelay, SourceClient, TargetClient,
	},
	finality_proofs::{FinalityProofsBuf, FinalityProofsStream},
	sync_loop_metrics::SyncLoopMetrics,
};

use bp_header_chain::ConsensusLogReader;
use relay_utils::{FailedClient, MaybeConnectionError};
use std::fmt::Debug;

mod base;
mod finality_loop;
mod finality_proofs;
mod headers;
mod mock;
mod sync_loop_metrics;

/// Finality proofs synchronization pipeline.
pub trait FinalitySyncPipeline: FinalityPipeline {
	/// A reader that can extract the consensus log from the header digest and interpret it.
	type ConsensusLogReader: ConsensusLogReader;
	/// Type of header that we're syncing.
	type Header: SourceHeader<Self::Hash, Self::Number, Self::ConsensusLogReader>;
}

/// Header that we're receiving from source node.
pub trait SourceHeader<Hash, Number, Reader>: Clone + Debug + PartialEq + Send + Sync {
	/// Returns hash of header.
	fn hash(&self) -> Hash;
	/// Returns number of header.
	fn number(&self) -> Number;
	/// Returns true if this header needs to be submitted to target node.
	fn is_mandatory(&self) -> bool;
}

/// Error that may happen inside finality synchronization loop.
#[derive(Debug)]
enum Error<P: FinalitySyncPipeline, SourceError, TargetError> {
	/// Source client request has failed with given error.
	Source(SourceError),
	/// Target client request has failed with given error.
	Target(TargetError),
	/// Finality proof for mandatory header is missing from the source node.
	MissingMandatoryFinalityProof(P::Number),
	/// `submit_finality_proof` transaction failed
	ProofSubmissionTxFailed {
		#[allow(dead_code)]
		submitted_number: P::Number,
		#[allow(dead_code)]
		best_number_at_target: P::Number,
	},
	/// `submit_finality_proof` transaction lost
	ProofSubmissionTxLost,
}

impl<P, SourceError, TargetError> Error<P, SourceError, TargetError>
where
	P: FinalitySyncPipeline,
	SourceError: MaybeConnectionError,
	TargetError: MaybeConnectionError,
{
	fn fail_if_connection_error(&self) -> Result<(), FailedClient> {
		match *self {
			Error::Source(ref error) if error.is_connection_error() => Err(FailedClient::Source),
			Error::Target(ref error) if error.is_connection_error() => Err(FailedClient::Target),
			_ => Ok(()),
		}
	}
}
