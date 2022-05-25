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

pub mod headers;
pub mod parachains;

/// On-demand headers relay that is relaying finalizing headers only when requested.
#[async_trait]
pub trait OnDemandRelay<SourceHeaderNumber>: Send + Sync {
	/// Ask relay to relay source header with given number  to the target chain.
	///
	/// Depending on implementation, on-demand relay may also relay `required_header` ancestors
	/// (e.g. if they're mandatory), or its descendants. The request is considered complete if
	/// the best avbailable header at the target chain has number that is larger than or equal
	/// to the `required_header`.
	async fn require_more_headers(&self, required_header: SourceHeaderNumber);
}
