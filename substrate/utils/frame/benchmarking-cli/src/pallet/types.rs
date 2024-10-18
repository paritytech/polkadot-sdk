// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Various types used by this crate.

use sc_cli::Result;
use sp_core::traits::{RuntimeCode, WrappedRuntimeCode};
use sp_runtime::traits::Hash;

/// How the genesis state for benchmarking should be build.
#[derive(clap::ValueEnum, Debug, Eq, PartialEq, Clone, Copy)]
#[clap(rename_all = "kebab-case")]
pub enum GenesisBuilderPolicy {
	/// Do not provide any genesis state.
	///
	/// Benchmarks are advised to function with this, since they should setup their own required
	/// state. However, to keep backwards compatibility, this is not the default.
	None,
	/// Let the runtime build the genesis state through its `BuildGenesisConfig` runtime API.
	Runtime,
	// Use the runtime from the Spec file to build the genesis state.
	SpecRuntime,
	/// Use the spec file to build the genesis state. This fails when there is no spec.
	SpecGenesis,
	/// Same as `SpecGenesis` - only here for backwards compatibility.
	Spec,
}

/// A runtime blob that was either fetched from genesis storage or loaded from a file.
// NOTE: This enum is only needed for the annoying lifetime bounds on `RuntimeCode`. Otherwise we
// could just directly return the blob.
pub enum FetchedCode<'a, B, H> {
	FromGenesis { state: sp_state_machine::backend::BackendRuntimeCode<'a, B, H> },
	FromFile { wrapped_code: WrappedRuntimeCode<'a>, heap_pages: Option<u64>, hash: Vec<u8> },
}

impl<'a, B, H> FetchedCode<'a, B, H>
where
	H: Hash,
	B: sc_client_api::StateBackend<H>,
{
	/// The runtime blob.
	pub fn code(&'a self) -> Result<RuntimeCode<'a>> {
		match self {
			Self::FromGenesis { state } => state.runtime_code().map_err(Into::into),
			Self::FromFile { wrapped_code, heap_pages, hash } => Ok(RuntimeCode {
				code_fetcher: wrapped_code,
				heap_pages: *heap_pages,
				hash: hash.clone(),
			}),
		}
	}
}

/// Maps a (pallet, benchmark) to its component ranges.
pub(crate) type ComponentRangeMap =
	std::collections::HashMap<(String, String), Vec<ComponentRange>>;

/// The inclusive range of a component.
#[derive(serde::Serialize, Debug, Clone, Eq, PartialEq)]
pub(crate) struct ComponentRange {
	/// Name of the component.
	pub(crate) name: String,
	/// Minimal valid value of the component.
	pub(crate) min: u32,
	/// Maximal valid value of the component.
	pub(crate) max: u32,
}
