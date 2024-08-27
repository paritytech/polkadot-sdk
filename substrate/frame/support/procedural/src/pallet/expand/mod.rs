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


/// Helper to generate better error in case of internal error.
macro_rules! msg {
	($t:literal) => {
		&format!(
			"Pallet Internal Error:{}:{}:{}: Please open an issue on polkadot-sdk github with the \
			pallet input which triggered this error",
			file!(),
			line!(),
			$t
		)
	};
}

mod call;
mod composite;
mod config;
mod constants;
mod doc_only;
mod documentation;
mod error;
mod event;
mod genesis_build;
mod genesis_config;
mod hooks;
mod instances;
mod pallet_struct;
mod storage;
mod tasks;
mod tt_default_parts;
mod type_value;
mod warnings;

use crate::pallet::Def;
use quote::ToTokens;

/// Merge where clause together, `where` token span is taken from the first not none one.
pub fn merge_where_clauses(clauses: &[&Option<syn::WhereClause>]) -> Option<syn::WhereClause> {
	let mut clauses = clauses.iter().filter_map(|f| f.as_ref());
	let mut res = clauses.next()?.clone();
	for other in clauses {
		res.predicates.extend(other.predicates.iter().cloned())
	}
	Some(res)
}

/// Expand definition, in particular:
/// * add some bounds and variants to type defined,
/// * create some new types,
/// * impl stuff on them.
pub fn expand(mut def: Def) -> proc_macro2::TokenStream {
	// Remove the `pallet_doc` attribute first.
	let metadata_docs = documentation::expand_documentation(&mut def);
	let constants = constants::expand_constants(&mut def);
	let pallet_struct = pallet_struct::expand_pallet_struct(&mut def);
	let config = config::expand_config(&mut def);
	let call = call::expand_call(&mut def);
	let tasks = tasks::expand_tasks(&mut def);
	let error = error::expand_error(&mut def);
	let event = event::expand_event(&mut def);
	let storages = storage::expand_storages(&mut def);
	let instances = instances::expand_instances(&mut def);
	let hooks = hooks::expand_hooks(&mut def);
	let genesis_build = genesis_build::expand_genesis_build(&mut def);
	genesis_config::expand_genesis_config(&mut def);
	let type_values = type_value::expand_type_values(&mut def);
	let tt_default_parts = tt_default_parts::expand_tt_default_parts(&mut def);
	let doc_only = doc_only::expand_doc_only(&mut def);
	let composites = composite::expand_composites(&mut def);

	def.item.attrs.insert(
		0,
		syn::parse_quote!(
			#[doc = r"The `pallet` module in each FRAME pallet hosts the most important items needed
to construct this pallet.

The main components of this pallet are:
- [`Pallet`], which implements all of the dispatchable extrinsics of the pallet, among
other public functions.
	- The subset of the functions that are dispatchable can be identified either in the
	[`dispatchables`] module or in the [`Call`] enum.
- [`storage_types`], which contains the list of all types that are representing a
storage item. Otherwise, all storage items are listed among [*Type Definitions*](#types).
- [`Config`], which contains the configuration trait of this pallet.
- [`Event`] and [`Error`], which are listed among the [*Enums*](#enums).
		"]
		),
	);

	let new_items = quote::quote!(
		#metadata_docs
		#constants
		#pallet_struct
		#config
		#call
		#tasks
		#error
		#event
		#storages
		#instances
		#hooks
		#genesis_build
		#type_values
		#tt_default_parts
		#doc_only
		#composites
	);

	def.item
		.content
		.as_mut()
		.expect(msg!("This is checked by parsing"))
		.1
		.push(syn::Item::Verbatim(new_items));

	def.item.into_token_stream()
}

#[cfg(test)]
mod tests {
	#[test]
	fn test_msg() {
		assert_eq!(
			msg!("my internal error"),
			"Pallet Internal Error:substrate/frame/support/procedural/src/pallet/expand/mod.rs:141:\
			my internal error: Please open an issue on polkadot-sdk github with the pallet input \
			which triggered this error"
		);
	}
}
