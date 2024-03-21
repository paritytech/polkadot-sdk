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
	check_inherents: Option<Path>,
}

impl Parse for Input {
	fn parse(input: ParseStream) -> Result<Self, Error> {
		let mut runtime = None;
		let mut block_executor = None;
		let mut check_inherents = None;

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
				parse_inner::<keywords::CheckInherents>(input, &mut check_inherents)?;
			} else {
				return Err(lookahead.error())
			}
		}

		Ok(Self {
			runtime: runtime.expect("Everything is parsed before; qed"),
			block_executor: block_executor.expect("Everything is parsed before; qed"),
			check_inherents,
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
	let Input { runtime, block_executor, check_inherents } = match syn::parse(input) {
		Ok(t) => t,
		Err(e) => return e.into_compile_error().into(),
	};

	let crate_ = match crate_() {
		Ok(c) => c,
		Err(e) => return e.into_compile_error().into(),
	};

	let check_inherents = match check_inherents {
		Some(_check_inherents) => {
			quote::quote! { #_check_inherents }
		},
		None => {
			quote::quote! {
				#crate_::DummyCheckInherents<<#runtime as #crate_::validate_block::GetRuntimeBlockType>::RuntimeBlock>
			}
		},
	};

	if cfg!(not(feature = "std")) {
		quote::quote! {
			#[doc(hidden)]
			mod parachain_validate_block {
				use super::*;

				#[no_mangle]
				unsafe fn validate_block(arguments: *mut u8, arguments_len: usize) -> u64 {
					// We convert the `arguments` into a boxed slice and then into `Bytes`.
					let args = #crate_::validate_block::sp_std::boxed::Box::from_raw(
						#crate_::validate_block::sp_std::slice::from_raw_parts_mut(
							arguments,
							arguments_len,
						)
					);
					let args = #crate_::validate_block::bytes::Bytes::from(args);

					// Then we decode from these bytes the `MemoryOptimizedValidationParams`.
					let params = #crate_::validate_block::decode_from_bytes::<
						#crate_::validate_block::MemoryOptimizedValidationParams
					>(args).expect("Invalid arguments to `validate_block`.");

					let res = #crate_::validate_block::implementation::validate_block::<
						<#runtime as #crate_::validate_block::GetRuntimeBlockType>::RuntimeBlock,
						#block_executor,
						#runtime,
						#check_inherents,
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
