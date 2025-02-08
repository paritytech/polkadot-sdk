// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

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
