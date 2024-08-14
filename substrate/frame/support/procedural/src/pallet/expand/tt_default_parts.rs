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

use crate::{
	pallet::{CompositeKeyword, Def},
	COUNTER,
};
use syn::spanned::Spanned;

/// Generate the `tt_default_parts` macro.
pub fn expand_tt_default_parts(def: &mut Def) -> proc_macro2::TokenStream {
	let count = COUNTER.with(|counter| counter.borrow_mut().inc());
	let default_parts_unique_id =
		syn::Ident::new(&format!("__tt_default_parts_{}", count), def.item.span());
	let extra_parts_unique_id =
		syn::Ident::new(&format!("__tt_extra_parts_{}", count), def.item.span());
	let default_parts_unique_id_v2 =
		syn::Ident::new(&format!("__tt_default_parts_v2_{}", count), def.item.span());

	let call_part = def.call.as_ref().map(|_| quote::quote!(Call,));

	let task_part = def.task_enum.as_ref().map(|_| quote::quote!(Task,));

	let storage_part = (!def.storages.is_empty()).then(|| quote::quote!(Storage,));

	let event_part = def.event.as_ref().map(|event| {
		let gen = event.gen_kind.is_generic().then(|| quote::quote!( <T> ));
		quote::quote!( Event #gen , )
	});

	let error_part = def.error.as_ref().map(|_| quote::quote!(Error<T>,));

	let origin_part = def.origin.as_ref().map(|origin| {
		let gen = origin.is_generic.then(|| quote::quote!( <T> ));
		quote::quote!( Origin #gen , )
	});

	let config_part = def.genesis_config.as_ref().map(|genesis_config| {
		let gen = genesis_config.gen_kind.is_generic().then(|| quote::quote!( <T> ));
		quote::quote!( Config #gen , )
	});

	let inherent_part = def.inherent.as_ref().map(|_| quote::quote!(Inherent,));

	let validate_unsigned_part =
		def.validate_unsigned.as_ref().map(|_| quote::quote!(ValidateUnsigned,));

	let freeze_reason_part = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::FreezeReason(_)))
		.then_some(quote::quote!(FreezeReason,));

	let hold_reason_part = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::HoldReason(_)))
		.then_some(quote::quote!(HoldReason,));

	let lock_id_part = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::LockId(_)))
		.then_some(quote::quote!(LockId,));

	let slash_reason_part = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::SlashReason(_)))
		.then_some(quote::quote!(SlashReason,));

	let call_part_v2 = def.call.as_ref().map(|_| quote::quote!(+ Call));

	let task_part_v2 = def.task_enum.as_ref().map(|_| quote::quote!(+ Task));

	let storage_part_v2 = (!def.storages.is_empty()).then(|| quote::quote!(+ Storage));

	let event_part_v2 = def.event.as_ref().map(|event| {
		let gen = event.gen_kind.is_generic().then(|| quote::quote!(<T>));
		quote::quote!(+ Event #gen)
	});

	let error_part_v2 = def.error.as_ref().map(|_| quote::quote!(+ Error<T>));

	let origin_part_v2 = def.origin.as_ref().map(|origin| {
		let gen = origin.is_generic.then(|| quote::quote!(<T>));
		quote::quote!(+ Origin #gen)
	});

	let config_part_v2 = def.genesis_config.as_ref().map(|genesis_config| {
		let gen = genesis_config.gen_kind.is_generic().then(|| quote::quote!(<T>));
		quote::quote!(+ Config #gen)
	});

	let inherent_part_v2 = def.inherent.as_ref().map(|_| quote::quote!(+ Inherent));

	let validate_unsigned_part_v2 =
		def.validate_unsigned.as_ref().map(|_| quote::quote!(+ ValidateUnsigned));

	let freeze_reason_part_v2 = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::FreezeReason(_)))
		.then_some(quote::quote!(+ FreezeReason));

	let hold_reason_part_v2 = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::HoldReason(_)))
		.then_some(quote::quote!(+ HoldReason));

	let lock_id_part_v2 = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::LockId(_)))
		.then_some(quote::quote!(+ LockId));

	let slash_reason_part_v2 = def
		.composites
		.iter()
		.any(|c| matches!(c.composite_keyword, CompositeKeyword::SlashReason(_)))
		.then_some(quote::quote!(+ SlashReason));

	let is_part_defined_call = generate_is_part_defined(
		def.call.is_some(),
		"__substrate_call_check",
		"#[pallet::call]",
		"Call",
		"is_call_part_defined",
		false,
	);
	let is_part_defined_event = generate_is_part_defined(
		def.event.is_some(),
		"__substrate_event_check",
		"#[pallet::event]",
		"Event",
		"is_event_part_defined",
		false,
	);
	let is_part_defined_inherent = generate_is_part_defined(
		def.inherent.is_some(),
		"__substrate_inherent_check",
		"#[pallet::inherent]",
		"Inherent",
		"is_inherent_part_defined",
		false,
	);
	let is_part_defined_origin = generate_is_part_defined(
		def.origin.is_some(),
		"__substrate_origin_check",
		"#[pallet::origin]",
		"Origin",
		"is_origin_part_defined",
		false,
	);
	let is_part_defined_validate_unsigned = generate_is_part_defined(
		def.validate_unsigned.is_some(),
		"__substrate_validate_unsigned_check",
		"#[pallet::validate_unsigned]",
		"ValidateUnsigned",
		"is_validate_unsigned_part_defined",
		false,
	);
	let is_part_defined_genesis_config = generate_is_part_defined(
		def.genesis_config.is_some(),
		"__substrate_genesis_config_check",
		"#[pallet::genesis_config]",
		"Config",
		"is_genesis_config_defined",
		true,
	);

	quote::quote!(
		#is_part_defined_call
		#is_part_defined_event
		#is_part_defined_inherent
		#is_part_defined_origin
		#is_part_defined_validate_unsigned
		#is_part_defined_genesis_config

		// This macro follows the conventions as laid out by the `tt-call` crate. It does not
		// accept any arguments and simply returns the pallet parts, separated by commas, then
		// wrapped inside of braces and finally prepended with double colons, to the caller inside
		// of a key named `tokens`.
		//
		// We need to accept a path argument here, because this macro gets expanded on the
		// crate that called the `construct_runtime!` macro, and the actual path is unknown.
		#[macro_export]
		#[doc(hidden)]
		macro_rules! #default_parts_unique_id {
			{
				$caller:tt
				your_tt_return = [{ $my_tt_return:path }]
			} => {
				$my_tt_return! {
					$caller
					tokens = [{
						expanded::{
							Pallet, #call_part #storage_part #event_part #error_part #origin_part #config_part
							#inherent_part #validate_unsigned_part #freeze_reason_part #task_part
							#hold_reason_part #lock_id_part #slash_reason_part
						}
					}]
				}
			};
		}

		pub use #default_parts_unique_id as tt_default_parts;


		// This macro is similar to the `tt_default_parts!`. It expands the pallets that are declared
		// explicitly (`System: frame_system::{Pallet, Call}`) with extra parts.
		//
		// For example, after expansion an explicit pallet would look like:
		// `System: expanded::{Error} ::{Pallet, Call}`.
		//
		// The `expanded` keyword is a marker of the final state of the `construct_runtime!`.
		#[macro_export]
		#[doc(hidden)]
		macro_rules! #extra_parts_unique_id {
			{
				$caller:tt
				your_tt_return = [{ $my_tt_return:path }]
			} => {
				$my_tt_return! {
					$caller
					tokens = [{
						expanded::{
							#error_part
						}
					}]
				}
			};
		}

		pub use #extra_parts_unique_id as tt_extra_parts;

		#[macro_export]
		#[doc(hidden)]
		macro_rules! #default_parts_unique_id_v2 {
			{
				$caller:tt
				your_tt_return = [{ $my_tt_return:path }]
			} => {
				$my_tt_return! {
					$caller
					tokens = [{
						+ Pallet #call_part_v2 #storage_part_v2 #event_part_v2 #error_part_v2 #origin_part_v2 #config_part_v2
						#inherent_part_v2 #validate_unsigned_part_v2 #freeze_reason_part_v2 #task_part_v2
						#hold_reason_part_v2 #lock_id_part_v2 #slash_reason_part_v2
					}]
				}
			};
		}

		pub use #default_parts_unique_id_v2 as tt_default_parts_v2;
	)
}

/// Generate a macro for parts which compiles to error if not defined.
/// Used by construct runtime to ensure some consistency.
fn generate_is_part_defined(
	is_defined: bool,
	mod_name: &str,
	attr_name: &str,
	part_name: &str,
	macro_name: &str,
	add_is_std_enabled_for_genesis: bool,
) -> proc_macro2::TokenStream {
	let count = crate::COUNTER.with(|counter| counter.borrow_mut().inc());
	let macro_ident =
		syn::Ident::new(&format!("__is_part_defined_{}", count), proc_macro2::Span::call_site());
	let outer_macro_ident = syn::Ident::new(macro_name, proc_macro2::Span::call_site());
	let mod_ident = syn::Ident::new(mod_name, proc_macro2::Span::call_site());

	let maybe_compile_error = if !is_defined {
		quote::quote! {
			compile_error!(concat!(
				"`",
				stringify!($pallet_name),
				"` does not have ",
				#attr_name,
				" defined, perhaps you should remove `",
				#part_name,
				"` from construct_runtime?",
			));
		}
	} else {
		Default::default()
	};

	// Some specific special case for genesis config additional helper.
	let optional_additional_helper = if add_is_std_enabled_for_genesis {
		let std_macro_ident = syn::Ident::new(
			&format!("__is_std_enabled_for_genesis_{}", count),
			proc_macro2::Span::call_site(),
		);
		if !is_defined {
			quote::quote! {
				#[macro_export]
				#[doc(hidden)]
				macro_rules! #std_macro_ident {
					($pallet_name:ident, $pallet_path:expr) => {};
				}

				#[doc(hidden)]
				pub use #std_macro_ident as is_std_enabled_for_genesis;
			}
		} else {
			quote::quote! {
				#[cfg(not(feature = "std"))]
				#[macro_export]
				#[doc(hidden)]
				macro_rules! #std_macro_ident {
					($pallet_name:ident, $pallet_path:expr) => {
						compile_error!(concat!(
							"`",
							stringify!($pallet_name),
							"` does not have the std feature enabled, this will cause the `",
							$pallet_path,
							"::GenesisConfig` type to not implement serde traits."
						));
					};
				}

				#[cfg(feature = "std")]
				#[macro_export]
				#[doc(hidden)]
				macro_rules! #std_macro_ident {
					($pallet_name:ident, $pallet_path:expr) => {};
				}

				#[doc(hidden)]
				pub use #std_macro_ident as is_std_enabled_for_genesis;
			}
		}
	} else {
		Default::default()
	};

	quote::quote! {
		#[doc(hidden)]
		pub mod #mod_ident {
			#[macro_export]
			#[doc(hidden)]
			macro_rules! #macro_ident {
				($pallet_name:ident) => {
					#maybe_compile_error
				}
			}

			#optional_additional_helper

			#[doc(hidden)]
			pub use #macro_ident as #outer_macro_ident;
		}
	}
}
