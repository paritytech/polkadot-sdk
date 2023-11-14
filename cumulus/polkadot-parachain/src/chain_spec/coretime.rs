// Copyright Parity Technologies (UK) Ltd.
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

use cumulus_primitives_core::ParaId;
use parachains_common::Balance as CoretimeBalance;
use sc_chain_spec::ChainSpec;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq)]
pub enum CoretimeRuntimeType {
	Rococo,
	Westend,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			rococo::CORETIME_ROCOCO => Ok(CoretimeRuntimeType::Rococo),
			westend::CORETIME_WESTEND => Ok(CoretimeRuntimeType::Westend),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn chain_spec_from_json_file(&self, path: PathBuf) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo =>
				Ok(Box::new(rococo::CoretimeChainSpec::from_json_file(path)?)),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_file(path)?)),
		}
	}

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo =>
				Ok(Box::new(rococo::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-rococo.json")[..],
				)?)),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-westend.json")[..],
				)?)),
		}
	}
}

/// Check if `id` satisfies Coretime-like format.
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(CoretimeRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			CoretimeRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Rococo setup.
pub mod rococo {
	use crate::chain_spec::Extensions;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_rococo_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_rococo_runtime::RuntimeApi;
}

/// Sub-module for Westend setup.
pub mod westend {
	use crate::chain_spec::Extensions;

	pub(crate) const CORETIME_WESTEND: &str = "coretime-westend";
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_westend_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_westend_runtime::RuntimeApi;
}
