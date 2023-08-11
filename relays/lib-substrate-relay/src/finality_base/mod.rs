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

//! Types and functions intended to ease adding of new Substrate -> Substrate
//! finality pipelines.

pub mod engine;

use async_trait::async_trait;
use relay_substrate_client::{Chain, ChainWithTransactions};
use std::fmt::Debug;

/// Substrate -> Substrate finality related pipeline.
#[async_trait]
pub trait SubstrateFinalityPipeline: 'static + Clone + Debug + Send + Sync {
	/// Headers of this chain are submitted to the `TargetChain`.
	type SourceChain: Chain;
	/// Headers of the `SourceChain` are submitted to this chain.
	type TargetChain: ChainWithTransactions;
	/// Finality engine.
	type FinalityEngine: engine::Engine<Self::SourceChain>;
}
