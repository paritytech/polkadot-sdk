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

use proc_macro2::Span;
use proc_macro_crate::{crate_name, FoundCrate};
use syn::{
	parse::{Parse, ParseStream},
	spanned::Spanned,
	token, Error, Ident, Path,
};

mod keywords {
	syn::custom_keyword!(Runtime);
	syn::custom_keyword!(BlockExecutor);
	syn::custom_keyword!(CheckInherents);
}

struct Input {
	runtime: Path,
	block_executor: Path,
}

impl Parse for Input {
	fn parse(input: ParseStream) -> Result<Self, Error> {
		let mut runtime = None;
		let mut block_executor = None;

		fn parse_inner<KW: Parse + Spanned>(
			input: ParseStream,
			result: &mut Option<Path>,
		) -> Result<(), Error> {
			let kw = input.parse::<KW>()?;

			if result.is_none() {
				input.parse::<token::Eq>()?;
				*result = Some(input.parse::<Path>()?);
				if input.peek(token::Comma) {
					input.parse::<token::Comma>()?;
				}

				Ok(())
			} else {
				Err(Error::new(kw.span(), "Is only allowed to be passed once"))
			}
		}

		while !input.is_empty() || runtime.is_none() || block_executor.is_none() {
			let lookahead = input.lookahead1();

			if lookahead.peek(keywords::Runtime) {
				parse_inner::<keywords::Runtime>(input, &mut runtime)?;
			} else if lookahead.peek(keywords::BlockExecutor) {
				parse_inner::<keywords::BlockExecutor>(input, &mut block_executor)?;
			} else if lookahead.peek(keywords::CheckInherents) {
				return Err(Error::new(input.span(), "`CheckInherents` is not supported anymore!"));
			} else {
				return Err(lookahead.error())
			}
		}

		Ok(Self {
			runtime: runtime.expect("Everything is parsed before; qed"),
			block_executor: block_executor.expect("Everything is parsed before; qed"),
		})
	}
}

fn crate_() -> Result<Ident, Error> {
	match crate_name("cumulus-pallet-parachain-system") {
		Ok(FoundCrate::Itself) =>
			Ok(syn::Ident::new("cumulus_pallet_parachain_system", Span::call_site())),
		Ok(FoundCrate::Name(name)) => Ok(Ident::new(&name, Span::call_site())),
		Err(e) => Err(Error::new(Span::call_site(), e)),
	}
}

#[proc_macro]
pub fn register_validate_block(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
	let Input { runtime, block_executor } = match syn::parse(input) {
		Ok(t) => t,
		Err(e) => return e.into_compile_error().into(),
	};

	let crate_ = match crate_() {
		Ok(c) => c,
		Err(e) => return e.into_compile_error().into(),
	};

	if cfg!(not(feature = "std")) {
		quote::quote! {
			#[doc(hidden)]
			mod parachain_validate_block {
				use super::*;

				#[no_mangle]
				unsafe fn validate_block(arguments_len: usize) -> u64 {
					let mut args = #crate_::validate_block::vec![0u8; arguments_len];
					#crate_::validate_block::sp_io::input::read(&mut args[..]);

					let args = #crate_::validate_block::bytes::Bytes::from(args);

					// Then we decode from these bytes the `MemoryOptimizedValidationParams`.
					let params = #crate_::validate_block::decode_from_bytes::<
						#crate_::validate_block::MemoryOptimizedValidationParams
					>(args).expect("Invalid arguments to `validate_block`.");

					let res = #crate_::validate_block::implementation::validate_block::<
						<#runtime as #crate_::validate_block::GetRuntimeBlockType>::RuntimeBlock,
						#block_executor,
						#runtime,
					>(params);

					#crate_::validate_block::polkadot_parachain_primitives::write_result(&res)
				}
			}
		}
	} else {
		quote::quote!()
	}
	.into()
}
