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

use super::parse::runtime_types::RuntimeType;
use crate::{
	construct_runtime::{
		check_pallet_number, decl_all_pallets, decl_integrity_test, decl_pallet_runtime_setup,
		decl_static_assertions, expand,
	},
	runtime::{
		parse::{
			AllPalletsDeclaration, ExplicitAllPalletsDeclaration, ImplicitAllPalletsDeclaration,
		},
		Def,
	},
};
use cfg_expr::Predicate;
use frame_support_procedural_tools::{
	generate_access_from_frame_or_crate, generate_crate_access, generate_hidden_includes,
};
use proc_macro2::TokenStream as TokenStream2;
use quote::quote;
use std::collections::HashSet;
use syn::{Ident, Result};

/// The fixed name of the system pallet.
const SYSTEM_PALLET_NAME: &str = "System";

pub fn expand(def: Def, legacy_ordering: bool) -> TokenStream2 {
	let input = def.input;

	let (check_pallet_number_res, res) = match def.pallets {
		AllPalletsDeclaration::Implicit(ref decl) => (
			check_pallet_number(input.clone(), decl.pallet_count),
			construct_runtime_implicit_to_explicit(input.into(), decl.clone(), legacy_ordering),
		),
		AllPalletsDeclaration::Explicit(ref decl) => (
			check_pallet_number(input, decl.pallets.len()),
			construct_runtime_final_expansion(
				def.runtime_struct.ident.clone(),
				decl.clone(),
				def.runtime_types.clone(),
				legacy_ordering,
			),
		),
	};

	let res = res.unwrap_or_else(|e| e.to_compile_error());

	// We want to provide better error messages to the user and thus, handle the error here
	// separately. If there is an error, we print the error and still generate all of the code to
	// get in overall less errors for the user.
	let res = if let Err(error) = check_pallet_number_res {
		let error = error.to_compile_error();

		quote! {
			#error

			#res
		}
	} else {
		res
	};

	let res = expander::Expander::new("construct_runtime")
		.dry(std::env::var("FRAME_EXPAND").is_err())
		.verbose(true)
		.write_to_out_dir(res)
		.expect("Does not fail because of IO in OUT_DIR; qed");

	res.into()
}

fn construct_runtime_implicit_to_explicit(
	input: TokenStream2,
	definition: ImplicitAllPalletsDeclaration,
	legacy_ordering: bool,
) -> Result<TokenStream2> {
	let frame_support = generate_access_from_frame_or_crate("frame-support")?;
	let attr = if legacy_ordering { quote!((legacy_ordering)) } else { quote!() };
	let mut expansion = quote::quote!(
		#[frame_support::runtime #attr]
		#input
	);
	for pallet in definition.pallet_decls.iter() {
		let pallet_path = &pallet.path;
		let pallet_name = &pallet.name;
		let pallet_instance = pallet.instance.as_ref().map(|instance| quote::quote!(<#instance>));
		expansion = quote::quote!(
			#frame_support::__private::tt_call! {
				macro = [{ #pallet_path::tt_default_parts_v2 }]
				frame_support = [{ #frame_support }]
				~~> #frame_support::match_and_insert! {
					target = [{ #expansion }]
					pattern = [{ #pallet_name = #pallet_path #pallet_instance  }]
				}
			}
		);
	}

	Ok(expansion)
}

fn construct_runtime_final_expansion(
	name: Ident,
	definition: ExplicitAllPalletsDeclaration,
	runtime_types: Vec<RuntimeType>,
	legacy_ordering: bool,
) -> Result<TokenStream2> {
	let ExplicitAllPalletsDeclaration { mut pallets, name: pallets_name } = definition;

	if !legacy_ordering {
		// Ensure that order of hooks is based on the pallet index
		pallets.sort_by_key(|p| p.index);
	}

	let system_pallet =
		pallets.iter().find(|decl| decl.name == SYSTEM_PALLET_NAME).ok_or_else(|| {
			syn::Error::new(
				pallets_name.span(),
				"`System` pallet declaration is missing. \
			 Please add this line: `pub type System = frame_system;`",
			)
		})?;
	if !system_pallet.cfg_pattern.is_empty() {
		return Err(syn::Error::new(
			system_pallet.name.span(),
			"`System` pallet declaration is feature gated, please remove any `#[cfg]` attributes",
		))
	}

	let features = pallets
		.iter()
		.filter_map(|decl| {
			(!decl.cfg_pattern.is_empty()).then(|| {
				decl.cfg_pattern.iter().flat_map(|attr| {
					attr.predicates().filter_map(|pred| match pred {
						Predicate::Feature(feat) => Some(feat),
						Predicate::Test => Some("test"),
						_ => None,
					})
				})
			})
		})
		.flatten()
		.collect::<HashSet<_>>();

	let hidden_crate_name = "construct_runtime";
	let scrate = generate_crate_access(hidden_crate_name, "frame-support");
	let scrate_decl = generate_hidden_includes(hidden_crate_name, "frame-support");

	let frame_system = generate_access_from_frame_or_crate("frame-system")?;
	let block = quote!(<#name as #frame_system::Config>::Block);
	let unchecked_extrinsic = quote!(<#block as #scrate::sp_runtime::traits::Block>::Extrinsic);

	let mut dispatch = None;
	let mut outer_event = None;
	let mut outer_error = None;
	let mut outer_origin = None;
	let mut freeze_reason = None;
	let mut hold_reason = None;
	let mut slash_reason = None;
	let mut lock_id = None;
	let mut task = None;

	for runtime_type in runtime_types.iter() {
		match runtime_type {
			RuntimeType::RuntimeCall(_) => {
				dispatch =
					Some(expand::expand_outer_dispatch(&name, system_pallet, &pallets, &scrate));
			},
			RuntimeType::RuntimeEvent(_) => {
				outer_event = Some(expand::expand_outer_enum(
					&name,
					&pallets,
					&scrate,
					expand::OuterEnumType::Event,
				)?);
			},
			RuntimeType::RuntimeError(_) => {
				outer_error = Some(expand::expand_outer_enum(
					&name,
					&pallets,
					&scrate,
					expand::OuterEnumType::Error,
				)?);
			},
			RuntimeType::RuntimeOrigin(_) => {
				outer_origin =
					Some(expand::expand_outer_origin(&name, system_pallet, &pallets, &scrate)?);
			},
			RuntimeType::RuntimeFreezeReason(_) => {
				freeze_reason = Some(expand::expand_outer_freeze_reason(&pallets, &scrate));
			},
			RuntimeType::RuntimeHoldReason(_) => {
				hold_reason = Some(expand::expand_outer_hold_reason(&pallets, &scrate));
			},
			RuntimeType::RuntimeSlashReason(_) => {
				slash_reason = Some(expand::expand_outer_slash_reason(&pallets, &scrate));
			},
			RuntimeType::RuntimeLockId(_) => {
				lock_id = Some(expand::expand_outer_lock_id(&pallets, &scrate));
			},
			RuntimeType::RuntimeTask(_) => {
				task = Some(expand::expand_outer_task(&name, &pallets, &scrate));
			},
		}
	}

	let all_pallets = decl_all_pallets(&name, pallets.iter(), &features);
	let pallet_to_index = decl_pallet_runtime_setup(&name, &pallets, &scrate);

	let metadata = expand::expand_runtime_metadata(
		&name,
		&pallets,
		&scrate,
		&unchecked_extrinsic,
		&system_pallet.path,
	);
	let outer_config = expand::expand_outer_config(&name, &pallets, &scrate);
	let inherent =
		expand::expand_outer_inherent(&name, &block, &unchecked_extrinsic, &pallets, &scrate);
	let validate_unsigned = expand::expand_outer_validate_unsigned(&name, &pallets, &scrate);
	let integrity_test = decl_integrity_test(&scrate);
	let static_assertions = decl_static_assertions(&name, &pallets, &scrate);

	let res = quote!(
		#scrate_decl

		// Prevent UncheckedExtrinsic to print unused warning.
		const _: () = {
			#[allow(unused)]
			type __hidden_use_of_unchecked_extrinsic = #unchecked_extrinsic;
		};

		#[derive(
			Clone, Copy, PartialEq, Eq, #scrate::sp_runtime::RuntimeDebug,
			#scrate::__private::scale_info::TypeInfo
		)]
		pub struct #name;
		impl #scrate::sp_runtime::traits::GetRuntimeBlockType for #name {
			type RuntimeBlock = #block;
		}

		// Each runtime must expose the `runtime_metadata()` to fetch the runtime API metadata.
		// The function is implemented by calling `impl_runtime_apis!`.
		//
		// However, the `runtime` may be used without calling `impl_runtime_apis!`.
		// Rely on the `Deref` trait to differentiate between a runtime that implements
		// APIs (by macro impl_runtime_apis!) and a runtime that is simply created (by macro runtime).
		//
		// Both `InternalConstructRuntime` and `InternalImplRuntimeApis` expose a `runtime_metadata()` function.
		// `InternalConstructRuntime` is implemented by the `runtime` for Runtime references (`& Runtime`),
		// while `InternalImplRuntimeApis` is implemented by the `impl_runtime_apis!` for Runtime (`Runtime`).
		//
		// Therefore, the `Deref` trait will resolve the `runtime_metadata` from `impl_runtime_apis!`
		// when both macros are called; and will resolve an empty `runtime_metadata` when only the `runtime`
		// is used.

		#[doc(hidden)]
		trait InternalConstructRuntime {
			#[inline(always)]
			fn runtime_metadata(&self) -> #scrate::__private::sp_std::vec::Vec<#scrate::__private::metadata_ir::RuntimeApiMetadataIR> {
				Default::default()
			}
		}
		#[doc(hidden)]
		impl InternalConstructRuntime for &#name {}

		#outer_event

		#outer_error

		#outer_origin

		#all_pallets

		#pallet_to_index

		#dispatch

		#task

		#metadata

		#outer_config

		#inherent

		#validate_unsigned

		#freeze_reason

		#hold_reason

		#lock_id

		#slash_reason

		#integrity_test

		#static_assertions
	);

	Ok(res)
}
