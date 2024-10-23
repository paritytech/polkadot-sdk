// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Runtime parameters.

use sc_chain_spec::ChainSpec;

/// The Aura ID used by the Aura consensus
#[derive(PartialEq)]
pub enum AuraConsensusId {
	/// Ed25519
	Ed25519,
	/// Sr25519
	Sr25519,
}

/// The choice of consensus for the parachain omni-node.
#[derive(PartialEq)]
pub enum Consensus {
	/// Aura consensus.
	Aura(AuraConsensusId),
}

/// The choice of block number for the parachain omni-node.
#[derive(PartialEq)]
pub enum BlockNumber {
	/// u32
	U32,
	/// u64
	U64,
}

/// Helper enum listing the supported Runtime types
#[derive(PartialEq)]
pub enum Runtime {
	/// None of the system-chain runtimes, rather the node will act agnostic to the runtime ie. be
	/// an omni-node, and simply run a node with the given consensus algorithm.
	Omni(BlockNumber, Consensus),
}

/// Helper trait used for extracting the Runtime variant from the chain spec ID.
pub trait RuntimeResolver {
	/// Extract the Runtime variant from the chain spec ID.
	fn runtime(&self, chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime>;
}

/// Default implementation for `RuntimeResolver` that just returns
/// `Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519))`.
pub struct DefaultRuntimeResolver;

impl RuntimeResolver for DefaultRuntimeResolver {
	fn runtime(&self, _chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime> {
		Ok(Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519)))
	}
}

/// Logic that inspects runtime's metadata for Omni Node compatibility.
pub mod metadata {
	use codec::Decode;
	use frame_metadata::RuntimeMetadata;

	/// Checks if pallet exists in runtime's metadata.
	///
	/// Metadata (as plain byte array) is decoded with `frame_metadata::Decode` and
	/// pallets existance is checked by looking for pallets with certain names.
	pub fn pallet_exists<'a>(
		mut metadata: &'a [u8],
		name: &str,
	) -> Result<bool, sc_service::error::Error> {
		let decoded_metadata: RuntimeMetadata = Decode::decode(&mut metadata)
			.map_err(|e| sc_service::error::Error::Application(Box::new(e) as Box<_>))?;
		match decoded_metadata {
			RuntimeMetadata::V14(inner) => Ok(inner.pallets.iter().any(|p| p.name == name)),
			RuntimeMetadata::V15(inner) => Ok(inner.pallets.iter().any(|p| p.name == name)),
			_ => Err(sc_service::error::Error::Application(
				anyhow::anyhow!("Metadata version smaller than V14 not supported.").into(),
			)),
		}
	}
}
