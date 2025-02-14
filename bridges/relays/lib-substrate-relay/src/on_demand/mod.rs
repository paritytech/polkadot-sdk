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

//! Types and functions intended to ease adding of new Substrate -> Substrate
//! on-demand pipelines.

use async_trait::async_trait;
use relay_substrate_client::{BlockNumberOf, CallOf, Chain, Error as SubstrateError, HeaderIdOf};

pub mod headers;
pub mod parachains;

/// On-demand headers relay that is relaying finalizing headers only when requested.
#[async_trait]
pub trait OnDemandRelay<SourceChain: Chain, TargetChain: Chain>: Send + Sync {
	/// Reconnect to source and target nodes.
	async fn reconnect(&self) -> Result<(), SubstrateError>;

	/// Ask relay to relay source header with given number  to the target chain.
	///
	/// Depending on implementation, on-demand relay may also relay `required_header` ancestors
	/// (e.g. if they're mandatory), or its descendants. The request is considered complete if
	/// the best avbailable header at the target chain has number that is larger than or equal
	/// to the `required_header`.
	async fn require_more_headers(&self, required_header: BlockNumberOf<SourceChain>);

	/// Ask relay to prove source `required_header` to the `TargetChain`.
	///
	/// Returns number of header that is proved (it may be the `required_header` or one of its
	/// descendants) and calls for delivering the proof.
	async fn prove_header(
		&self,
		required_header: BlockNumberOf<SourceChain>,
	) -> Result<(HeaderIdOf<SourceChain>, Vec<CallOf<TargetChain>>), SubstrateError>;
}
