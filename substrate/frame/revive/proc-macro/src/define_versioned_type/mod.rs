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

mod ast;
mod attr;
mod error;
mod ext;
mod fields;
mod parser;
mod patch;
mod type_def;

pub(super) use ast::*;
pub(super) use attr::*;
pub(super) use error::*;
pub(super) use ext::*;
pub(super) use fields::*;
pub(super) use parser::*;
pub(super) use patch::*;
pub(super) use type_def::*;

pub fn handle_define_versioned_type(
	input: DefineVersionedTypeInput,
) -> syn::Result<DefineVersionedTypeOutput> {
	let last_version = input
		.definitions
		.last_key_value()
		.map(|(_, def)| (def.ident_ref().clone(), def.generics_ref().clone()));

	let mut output = DefineVersionedTypeOutput {
		type_definitions: parse(input.definitions.into_values())?
			.into_iter()
			.map(Into::into)
			.collect(),
		latest_type_alias: None,
	};

	if let Some((last_item_ident, mut last_item_generics)) = last_version {
		let name = input.name.expect("qed; if latest exists then a name exists");
		let alias_ident = syn::Ident::new(&format!("Latest{name}"), last_item_ident.span());
		let doc = format!("The latest version of `{name}`.");
		last_item_generics.where_clause = None;
		for param in last_item_generics.params.iter_mut() {
			match param {
				syn::GenericParam::Lifetime(param) => {
					param.colon_token = None;
					param.bounds.clear();
				},
				syn::GenericParam::Type(param) => {
					param.colon_token = None;
					param.bounds.clear();
				},
				syn::GenericParam::Const(_) => {},
			}
		}
		let (_, type_generics, _) = last_item_generics.split_for_impl();
		output.latest_type_alias = Some(syn::parse_quote! {
			#[doc = #doc]
			pub type #alias_ident #last_item_generics = #last_item_ident #type_generics;
		});
	}

	Ok(output)
}

#[cfg(test)]
mod tests {
	use std::path::PathBuf;

	use proc_macro2::TokenStream as TokenStream2;
	use quote::{quote, ToTokens};
	use syn::{parse2, Item};

	use super::{handle_define_versioned_type, DefineVersionedTypeInput};

	#[test]
	fn generated_code_contains_expected_items() {
		let expected = parse_fixture();
		let generated = generated_file();
		let generated_items = normalized_items(&generated);
		let expected_items = normalized_items(&expected);

		assert_eq!(
			generated_items.len(),
			expected_items.len(),
			"Macro expansion and expected expansion aren't of the same length. This means that \
			there might be a change in the macro expansion code. You can regenerate the expected \
			output using the `update_define_versioned_type_expanded_fixture` test."
		);

		for expected_item in expected_items {
			assert!(
				generated_items.contains(&expected_item),
				"missing generated item: {expected_item}"
			);
		}
	}

	#[test]
	#[ignore = "Used to generate the macro-expanded code. Useful when macro logic was updated"]
	fn update_define_versioned_type_expanded_fixture() {
		let generated = expand_fixture_input().to_string();

		let fixture_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("tests")
			.join("assets")
			.join("define_versioned_type.expanded.rs");

		std::fs::create_dir_all(fixture_path.parent().unwrap()).unwrap();
		std::fs::write(fixture_path, generated).unwrap();
	}

	fn parse_fixture() -> syn::File {
		syn::parse_file(include_str!("../../tests/assets/define_versioned_type.expanded.rs"))
			.unwrap()
	}

	fn generated_file() -> syn::File {
		parse2::<syn::File>(expand_fixture_input()).unwrap()
	}

	fn expand_fixture_input() -> TokenStream2 {
		handle_define_versioned_type(parse2::<DefineVersionedTypeInput>(fixture_input()).unwrap())
			.unwrap()
			.to_token_stream()
	}

	fn fixture_input() -> TokenStream2 {
		quote! {
			#[versioned_type(extend)]
			#[derive(Clone)]
			pub struct MacroTypeV4<T: Clone>
			{
				#[doc = "overridden field"]
				#[versioned_type(override)]
				pub second: u32,
				#[versioned_type()]
				pub third: u64,
			}

			#[versioned_type()]
			#[derive(Clone)]
			pub struct MacroTypeV3<T: Clone>
			{
				#[doc = "base field"]
				first: T,
				pub second: u16,
			}

			#[versioned_type(extend)]
			#[derive(Clone)]
			pub enum MacroTypeV6<T: Clone>
			{
				#[doc = "updated variant"]
				#[versioned_type(extend)]
				FromStruct {
					#[doc = "second override"]
					#[versioned_type(override)]
					second: u64,
					#[versioned_type()]
					fifth: T,
				},
				#[versioned_type()]
				Added,
			}

			#[derive(Clone)]
			pub enum MacroTypeV5<T: Clone>
			{
				#[doc = "variant from struct"]
				#[versioned_type(extend)]
				FromStruct {
					#[versioned_type()]
					fourth: u8,
				},
				#[doc = "standalone variant"]
				#[versioned_type()]
				Standalone(#[doc = "tuple field"] u8),
			}
		}
	}

	fn normalized_items(file: &syn::File) -> Vec<String> {
		file.items.iter().map(normalized_item).collect()
	}

	fn normalized_item(item: &Item) -> String {
		item.to_token_stream().to_string()
	}
}
