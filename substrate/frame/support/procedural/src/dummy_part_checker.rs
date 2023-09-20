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

use crate::COUNTER;
use proc_macro::TokenStream;

pub fn generate_dummy_part_checker(input: TokenStream) -> TokenStream {
	if !input.is_empty() {
		return syn::Error::new(proc_macro2::Span::call_site(), "No arguments expected")
			.to_compile_error()
			.into()
	}

	let count = COUNTER.with(|counter| counter.borrow_mut().inc());

	let no_op_macro_ident =
		syn::Ident::new(&format!("__dummy_part_checker_{}", count), proc_macro2::Span::call_site());

	quote::quote!(
		#[macro_export]
		#[doc(hidden)]
		macro_rules! #no_op_macro_ident {
			( $( $tt:tt )* ) => {};
		}

		#[doc(hidden)]
		pub mod __substrate_genesis_config_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_genesis_config_defined;
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_std_enabled_for_genesis;
		}

		#[doc(hidden)]
		pub mod __substrate_event_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_event_part_defined;
		}

		#[doc(hidden)]
		pub mod __substrate_inherent_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_inherent_part_defined;
		}

		#[doc(hidden)]
		pub mod __substrate_validate_unsigned_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_validate_unsigned_part_defined;
		}

		#[doc(hidden)]
		pub mod __substrate_call_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_call_part_defined;
		}

		#[doc(hidden)]
		pub mod __substrate_origin_check {
			#[doc(hidden)]
			pub use #no_op_macro_ident as is_origin_part_defined;
		}
	)
	.into()
}
