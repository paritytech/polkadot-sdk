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
// limitations under the License

use crate::construct_runtime::parse::PalletPath;
use proc_macro2::{Ident, TokenStream};
use quote::quote;

pub(crate) fn expand_conversion_fn(
	composite_name: &str,
	path: &PalletPath,
	instance: Option<&Ident>,
	variant_name: &Ident,
) -> TokenStream {
	let composite_name = quote::format_ident!("{}", composite_name);
	let runtime_composite_name = quote::format_ident!("Runtime{}", composite_name);

	if let Some(inst) = instance {
		quote! {
			impl From<#path::#composite_name<#path::#inst>> for #runtime_composite_name {
				fn from(hr: #path::#composite_name<#path::#inst>) -> Self {
					#runtime_composite_name::#variant_name(hr)
				}
			}
		}
	} else {
		quote! {
			impl From<#path::#composite_name> for #runtime_composite_name {
				fn from(hr: #path::#composite_name) -> Self {
					#runtime_composite_name::#variant_name(hr)
				}
			}
		}
	}
}

pub(crate) fn expand_variant(
	composite_name: &str,
	index: u8,
	path: &PalletPath,
	instance: Option<&Ident>,
	variant_name: &Ident,
) -> TokenStream {
	let composite_name = quote::format_ident!("{}", composite_name);

	if let Some(inst) = instance {
		quote! {
			#[codec(index = #index)]
			#variant_name(#path::#composite_name<#path::#inst>),
		}
	} else {
		quote! {
			#[codec(index = #index)]
			#variant_name(#path::#composite_name),
		}
	}
}

pub(crate) fn expand_variant_count(
	composite_name: &str,
	path: &PalletPath,
	instance: Option<&Ident>,
) -> TokenStream {
	let composite_name = quote::format_ident!("{}", composite_name);

	if let Some(inst) = instance {
		quote! {
			#path::#composite_name::<#path::#inst>::VARIANT_COUNT
		}
	} else {
		// Wrapped `<`..`>` means: use default type parameter for enum.
		//
		// This is used for pallets without instance support or pallets with instance support when
		// we don't specify instance:
		//
		// ```nocompile
		// pub struct Pallet<T, I = ()>{..}
		//
		// #[pallet::composite_enum]
		// pub enum HoldReason<I: 'static = ()> {..}
		//
		// Pallet1: pallet_x,  // <- default type parameter
		// ```
		quote! {
			<#path::#composite_name>::VARIANT_COUNT
		}
	}
}
