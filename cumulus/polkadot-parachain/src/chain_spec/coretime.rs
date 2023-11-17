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

use sc_chain_spec::ChainSpec;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq)]
pub enum CoretimeRuntimeType {
	Rococo,
	RococoLocal,
	Westend,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			rococo::CORETIME_ROCOCO => Ok(CoretimeRuntimeType::Rococo),
			rococo::CORETIME_ROCOCO_LOCAL => Ok(CoretimeRuntimeType::RococoLocal),
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
			CoretimeRuntimeType::RococoLocal =>
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
			CoretimeRuntimeType::RococoLocal =>
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

/// Sub-module for Rococo setup.
pub mod rococo {
	use crate::chain_spec::Extensions;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub(crate) const CORETIME_ROCOCO_LOCAL: &str = "coretime-rococo-local";
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
