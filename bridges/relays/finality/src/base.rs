// Copyright 2019-2023 Parity Technologies (UK) Ltd.
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

use async_trait::async_trait;
use bp_header_chain::FinalityProof;
use futures::Stream;
use relay_utils::relay_loop::Client as RelayClient;
use std::fmt::Debug;

/// Base finality pipeline.
pub trait FinalityPipeline: 'static + Clone + Debug + Send + Sync {
	/// Name of the finality proofs source.
	const SOURCE_NAME: &'static str;
	/// Name of the finality proofs target.
	const TARGET_NAME: &'static str;

	/// Synced headers are identified by this hash.
	type Hash: Eq + Clone + Copy + Send + Sync + Debug;
	/// Synced headers are identified by this number.
	type Number: relay_utils::BlockNumberBase;
	/// Finality proof type.
	type FinalityProof: FinalityProof<Self::Hash, Self::Number>;
}

/// Source client used in finality related loops.
#[async_trait]
pub trait SourceClientBase<P: FinalityPipeline>: RelayClient {
	/// Stream of new finality proofs. The stream is allowed to miss proofs for some
	/// headers, even if those headers are mandatory.
	type FinalityProofsStream: Stream<Item = P::FinalityProof> + Send + Unpin;

	/// Subscribe to new finality proofs.
	async fn finality_proofs(&self) -> Result<Self::FinalityProofsStream, Self::Error>;
}

/// Target client used in finality related loops.
#[async_trait]
pub trait TargetClientBase<P: FinalityPipeline>: RelayClient {}
