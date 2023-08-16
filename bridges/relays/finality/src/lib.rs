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
	finality_loop::{metrics_prefix, run, FinalitySyncParams, SourceClient, TargetClient},
	sync_loop_metrics::SyncLoopMetrics,
};

use bp_header_chain::ConsensusLogReader;
use std::fmt::Debug;

mod base;
mod finality_loop;
mod finality_loop_tests;
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
