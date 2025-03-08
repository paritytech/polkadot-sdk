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
use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{Path, Result};

/// Resolve the correct path for sc_tracing:
/// - If `polkadot-sdk` is in scope, returns a Path corresponding to `polkadot_sdk::sc_tracing`
/// - Otherwise, falls back to `sc_tracing`
pub fn resolve_sc_tracing() -> Result<Path> {
	match crate_name("polkadot-sdk") {
		Ok(FoundCrate::Itself) => syn::parse_str("polkadot_sdk::sc_tracing"),
		Ok(FoundCrate::Name(sdk_name)) => syn::parse_str(&format!("{}::sc_tracing", sdk_name)),
		Err(_) => match crate_name("sc-tracing") {
			Ok(FoundCrate::Itself) => syn::parse_str("sc_tracing"),
			Ok(FoundCrate::Name(name)) => syn::parse_str(&name),
			Err(e) => Err(syn::Error::new(Span::call_site(), e)),
		},
	}
}
