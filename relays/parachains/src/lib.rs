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

use std::fmt::Debug;

use relay_substrate_client::{Chain, Parachain};

pub mod parachains_loop;
pub mod parachains_loop_metrics;

/// Finality proofs synchronization pipeline.
pub trait ParachainsPipeline: 'static + Clone + Debug + Send + Sync {
	/// Relay chain which is storing parachain heads in its `paras` module.
	type SourceRelayChain: Chain;
	/// Parachain which headers we are syncing here.
	type SourceParachain: Parachain;
	/// Target chain (either relay or para) which wants to know about new parachain heads.
	type TargetChain: Chain;
}
