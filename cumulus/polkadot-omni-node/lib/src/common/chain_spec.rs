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

//! Chain spec primitives.

pub use sc_chain_spec::ChainSpec;
use sc_chain_spec::ChainSpecExtension;
use serde::{Deserialize, Serialize};
use std::fmt::Debug;

/// Helper trait used for loading/building a chain spec starting from the chain ID.
pub trait LoadSpec {
	/// Load/Build a chain spec starting from the chain ID.
	fn load_spec(&self, id: &str) -> Result<Box<dyn ChainSpec>, String>;
}

/// Default implementation for `LoadSpec` that just reads a chain spec from the disk.
pub struct DiskChainSpecLoader;

impl LoadSpec for DiskChainSpecLoader {
	fn load_spec(&self, path: &str) -> Result<Box<dyn ChainSpec>, String> {
		Ok(Box::new(GenericChainSpec::from_json_file(path.into())?))
	}
}

/// Generic extensions for Parachain ChainSpecs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecExtension)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	#[serde(alias = "relayChain", alias = "RelayChain")]
	pub relay_chain: String,
	/// The id of the Parachain.
	#[serde(alias = "paraId", alias = "ParaId")]
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Generic chain spec for all polkadot-parachain runtimes
pub type GenericChainSpec = sc_service::GenericChainSpec<Extensions>;

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn can_decode_extension_camel_and_snake_case() {
		let camel_case = r#"{"relayChain":"relay","paraId":1}"#;
		let snake_case = r#"{"relay_chain":"relay","para_id":1}"#;
		let pascal_case = r#"{"RelayChain":"relay","ParaId":1}"#;

		let camel_case_extension: Extensions = serde_json::from_str(camel_case).unwrap();
		let snake_case_extension: Extensions = serde_json::from_str(snake_case).unwrap();
		let pascal_case_extension: Extensions = serde_json::from_str(pascal_case).unwrap();

		assert_eq!(camel_case_extension, snake_case_extension);
		assert_eq!(snake_case_extension, pascal_case_extension);
	}
}
