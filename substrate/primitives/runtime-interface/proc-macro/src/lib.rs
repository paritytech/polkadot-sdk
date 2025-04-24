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

//! This crate provides procedural macros for usage within the context of the Substrate runtime
//! interface.
//!
//! It provides the [`#[runtime_interface]`](attr.runtime_interface.html) attribute macro
//! for generating the runtime interfaces.

use syn::{
	parse::{Parse, ParseStream},
	parse_macro_input, ItemTrait, Result, Token,
};

mod runtime_interface;
mod utils;

struct Options {
	wasm_only: bool,
	tracing: bool,
}

impl Options {
	fn unpack(self) -> (bool, bool) {
		(self.wasm_only, self.tracing)
	}
}
impl Default for Options {
	fn default() -> Self {
		Options { wasm_only: false, tracing: true }
	}
}

impl Parse for Options {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut res = Self::default();
		while !input.is_empty() {
			let lookahead = input.lookahead1();
			if lookahead.peek(runtime_interface::keywords::wasm_only) {
				let _ = input.parse::<runtime_interface::keywords::wasm_only>();
				res.wasm_only = true;
			} else if lookahead.peek(runtime_interface::keywords::no_tracing) {
				let _ = input.parse::<runtime_interface::keywords::no_tracing>();
				res.tracing = false;
			} else if lookahead.peek(Token![,]) {
				let _ = input.parse::<Token![,]>();
			} else {
				return Err(lookahead.error())
			}
		}
		Ok(res)
	}
}

#[proc_macro_attribute]
pub fn runtime_interface(
	attrs: proc_macro::TokenStream,
	input: proc_macro::TokenStream,
) -> proc_macro::TokenStream {
	let trait_def = parse_macro_input!(input as ItemTrait);
	let (wasm_only, tracing) = parse_macro_input!(attrs as Options).unpack();

	runtime_interface::runtime_interface_impl(trait_def, wasm_only, tracing)
		.unwrap_or_else(|e| e.to_compile_error())
		.into()
}
