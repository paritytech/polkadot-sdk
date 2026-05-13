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

mod attribute;
mod fields;
mod item;

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
	punctuated::Punctuated, spanned::Spanned, token::Comma, GenericParam, Generics, Ident,
	ItemEnum, ItemStruct, ItemType, Result, Variant,
};

use attribute::{
	TypeVersionedTypeAttribute, TypeVersionedTypeMode, VariantVersionedTypeMode,
	VariantWithVersionedTypeAttribute,
};
use fields::{extend_fields, strip_field_attributes, FieldOwner};
pub use item::{DefineVersionedTypeInput, DefineVersionedTypeItem};

pub fn handle_define_versioned_type(
	input: DefineVersionedTypeInput,
) -> Result<DefineVersionedTypeOutput> {
	let DefineVersionedTypeInput { name, highest_version, definitions } = input;
	let latest_alias = latest_type_alias(name.as_deref(), highest_version, &definitions);
	let mut items = Vec::<DefineVersionedTypeItem>::with_capacity(definitions.len());

	for mut item in definitions.into_values() {
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(item.take_attributes())?;
		let type_attribute = attribute_split.versioned_type;
		item.set_attributes(attribute_split.other_attributes);

		handle_item_extensions(&mut item, type_attribute, items.last())?;
		items.push(item);
	}

	Ok(DefineVersionedTypeOutput { items, latest_alias })
}

fn latest_type_alias(
	name: Option<&str>,
	highest_version: Option<item::Version>,
	definitions: &std::collections::BTreeMap<item::Version, DefineVersionedTypeItem>,
) -> Option<LatestTypeAlias> {
	let name = name?;
	let latest_item = definitions.get(&highest_version?)?;
	Some(LatestTypeAlias::new(name, latest_item))
}

pub struct DefineVersionedTypeOutput {
	items: Vec<DefineVersionedTypeItem>,

	latest_alias: Option<LatestTypeAlias>,
}

impl ToTokens for DefineVersionedTypeOutput {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		let items = &self.items;
		let latest_alias = &self.latest_alias;
		tokens.extend(quote::quote! {
			#( #items )*
			#latest_alias
		});
	}
}

impl std::ops::Deref for DefineVersionedTypeOutput {
	type Target = [DefineVersionedTypeItem];

	fn deref(&self) -> &Self::Target {
		&self.items
	}
}

struct LatestTypeAlias {
	item: ItemType,
}

impl LatestTypeAlias {
	fn new(name: &str, latest_item: &DefineVersionedTypeItem) -> Self {
		let alias_ident = Ident::new(&format!("Latest{name}"), latest_item.ident().span());
		let target_ident = latest_item.ident();
		let visibility = latest_item.visibility();
		let mut alias_generics = latest_item.generics().clone();
		strip_type_alias_bounds(&mut alias_generics);
		let (_, type_generics, _) = alias_generics.split_for_impl();
		let doc = format!("The latest version of `{name}`.");

		let item = syn::parse_quote! {
			#[doc = #doc]
			#visibility type #alias_ident #alias_generics = #target_ident #type_generics;
		};

		Self { item }
	}
}

impl ToTokens for LatestTypeAlias {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		self.item.to_tokens(tokens);
	}
}

fn strip_type_alias_bounds(generics: &mut Generics) {
	generics.where_clause = None;

	for param in &mut generics.params {
		match param {
			GenericParam::Type(param) => {
				param.colon_token = None;
				param.bounds.clear();
			},
			GenericParam::Lifetime(param) => {
				param.colon_token = None;
				param.bounds.clear();
			},
			GenericParam::Const(_) => {},
		}
	}
}

fn handle_item_extensions(
	item: &mut DefineVersionedTypeItem,
	attribute: TypeVersionedTypeAttribute,
	previous_item: Option<&DefineVersionedTypeItem>,
) -> Result<()> {
	match item {
		DefineVersionedTypeItem::Struct(item_struct) => {
			handle_struct_extensions(item_struct, attribute.mode(), previous_item)
		},
		DefineVersionedTypeItem::Enum(item_enum) => {
			handle_enum_extensions(item_enum, attribute.mode(), previous_item)
		},
	}
}

fn handle_struct_extensions(
	this_struct: &mut ItemStruct,
	mode: TypeVersionedTypeMode,
	previous_item: Option<&DefineVersionedTypeItem>,
) -> Result<()> {
	match (mode, previous_item) {
		(
			TypeVersionedTypeMode::Extend { .. },
			Some(DefineVersionedTypeItem::Struct(previous_struct)),
		) => extend_fields(&mut this_struct.fields, &previous_struct.fields, FieldOwner::Struct),
		(
			TypeVersionedTypeMode::Extend { span },
			Some(DefineVersionedTypeItem::Enum(previous_enum)),
		) => Err(struct_from_enum_extension_error(span, this_struct, previous_enum)),
		(TypeVersionedTypeMode::Extend { span }, None) => Err(missing_type_extension_error(span)),
		(TypeVersionedTypeMode::Standalone, None | Some(_)) => {
			strip_field_attributes(&mut this_struct.fields)
		},
	}
}

fn handle_enum_extensions(
	this_enum: &mut ItemEnum,
	mode: TypeVersionedTypeMode,
	previous_item: Option<&DefineVersionedTypeItem>,
) -> Result<()> {
	let merge_mode = enum_merge_mode(this_enum, mode, previous_item)?;
	let current_variants =
		VariantWithVersionedTypeAttribute::parse_all(core::mem::take(&mut this_enum.variants))?;
	reject_duplicate_current_variants(&current_variants)?;

	let mut output_variants = initial_enum_variants(merge_mode);
	for current_variant in current_variants {
		apply_variant_change(&mut output_variants, current_variant, merge_mode)?;
	}

	this_enum.variants = output_variants.into_iter().collect();
	Ok(())
}

#[derive(Clone, Copy)]
enum EnumMergeMode<'a> {
	NoPrevious,

	PreviousStruct { previous_struct: &'a ItemStruct },

	PreviousEnum { previous_enum: &'a ItemEnum, type_extension: EnumTypeExtension },
}

#[derive(Clone, Copy)]
enum EnumTypeExtension {
	Standalone,

	Extending,
}

impl EnumTypeExtension {
	#[must_use]
	fn is_extending(self) -> bool {
		match self {
			Self::Standalone => false,
			Self::Extending => true,
		}
	}
}

fn enum_merge_mode<'a>(
	this_enum: &ItemEnum,
	mode: TypeVersionedTypeMode,
	previous_item: Option<&'a DefineVersionedTypeItem>,
) -> Result<EnumMergeMode<'a>> {
	match (mode, previous_item) {
		(TypeVersionedTypeMode::Extend { span }, None) => Err(missing_type_extension_error(span)),
		(
			TypeVersionedTypeMode::Extend { span },
			Some(DefineVersionedTypeItem::Struct(previous_struct)),
		) => Err(enum_from_struct_extension_error(span, this_enum, previous_struct)),
		(
			TypeVersionedTypeMode::Extend { .. },
			Some(DefineVersionedTypeItem::Enum(previous_enum)),
		) => Ok(EnumMergeMode::PreviousEnum {
			previous_enum,
			type_extension: EnumTypeExtension::Extending,
		}),
		(TypeVersionedTypeMode::Standalone, Some(DefineVersionedTypeItem::Enum(previous_enum))) => {
			Ok(EnumMergeMode::PreviousEnum {
				previous_enum,
				type_extension: EnumTypeExtension::Standalone,
			})
		},
		(
			TypeVersionedTypeMode::Standalone,
			Some(DefineVersionedTypeItem::Struct(previous_struct)),
		) => Ok(EnumMergeMode::PreviousStruct { previous_struct }),
		(TypeVersionedTypeMode::Standalone, None) => Ok(EnumMergeMode::NoPrevious),
	}
}

fn initial_enum_variants(merge_mode: EnumMergeMode<'_>) -> Vec<Variant> {
	match merge_mode {
		EnumMergeMode::PreviousEnum {
			previous_enum,
			type_extension: EnumTypeExtension::Extending,
		} => previous_enum.variants.iter().cloned().collect::<Vec<_>>(),
		EnumMergeMode::NoPrevious |
		EnumMergeMode::PreviousStruct { .. } |
		EnumMergeMode::PreviousEnum { type_extension: EnumTypeExtension::Standalone, .. } => Vec::new(),
	}
}

fn apply_variant_change(
	output_variants: &mut Vec<Variant>,
	current_variant: VariantWithVersionedTypeAttribute,
	merge_mode: EnumMergeMode<'_>,
) -> Result<()> {
	match merge_mode {
		EnumMergeMode::PreviousEnum { previous_enum, type_extension } => {
			apply_variant_change_from_enum(
				output_variants,
				current_variant,
				previous_enum,
				type_extension,
			)
		},
		EnumMergeMode::PreviousStruct { previous_struct } => {
			apply_variant_change_from_struct(output_variants, current_variant, previous_struct)
		},
		EnumMergeMode::NoPrevious => {
			apply_variant_change_without_previous(output_variants, current_variant)
		},
	}
}

fn apply_variant_change_from_enum(
	output_variants: &mut Vec<Variant>,
	mut current_variant: VariantWithVersionedTypeAttribute,
	previous_enum: &ItemEnum,
	type_extension: EnumTypeExtension,
) -> Result<()> {
	let variant_name = current_variant.variant.ident.to_string();
	let previous_variant = find_variant(&previous_enum.variants, &variant_name);

	match current_variant.attribute.mode() {
		VariantVersionedTypeMode::Extend { span } |
		VariantVersionedTypeMode::OverrideAndExtend { extend_span: span, .. } => {
			let Some(previous_variant) = previous_variant else {
				return Err(missing_variant_extension_error(
					span,
					&current_variant.variant,
					&variant_name,
				));
			};

			extend_fields(
				&mut current_variant.variant.fields,
				&previous_variant.fields,
				FieldOwner::EnumVariant,
			)?;
			upsert_variant(output_variants, current_variant.variant, &variant_name, type_extension);
		},
		VariantVersionedTypeMode::Override { span } => {
			if previous_variant.is_none() {
				return Err(missing_variant_override_error(
					span,
					&current_variant.variant,
					&variant_name,
				));
			}

			strip_field_attributes(&mut current_variant.variant.fields)?;
			upsert_variant(output_variants, current_variant.variant, &variant_name, type_extension);
		},
		VariantVersionedTypeMode::Standalone => {
			if type_extension.is_extending() {
				if let Some(previous_variant) = previous_variant {
					return Err(duplicate_in_extended_enum_error(
						&current_variant.variant,
						previous_variant,
						&variant_name,
					));
				}
			}

			strip_field_attributes(&mut current_variant.variant.fields)?;
			output_variants.push(current_variant.variant);
		},
	}

	Ok(())
}

fn apply_variant_change_from_struct(
	output_variants: &mut Vec<Variant>,
	mut current_variant: VariantWithVersionedTypeAttribute,
	previous_struct: &ItemStruct,
) -> Result<()> {
	let variant_name = current_variant.variant.ident.to_string();

	match current_variant.attribute.mode() {
		VariantVersionedTypeMode::Extend { .. } => {
			extend_fields(
				&mut current_variant.variant.fields,
				&previous_struct.fields,
				FieldOwner::EnumVariant,
			)?;
			output_variants.push(current_variant.variant);
		},
		VariantVersionedTypeMode::Override { span } |
		VariantVersionedTypeMode::OverrideAndExtend { override_span: span, .. } => {
			return Err(missing_variant_override_error(
				span,
				&current_variant.variant,
				&variant_name,
			));
		},
		VariantVersionedTypeMode::Standalone => {
			strip_field_attributes(&mut current_variant.variant.fields)?;
			output_variants.push(current_variant.variant);
		},
	}

	Ok(())
}

fn apply_variant_change_without_previous(
	output_variants: &mut Vec<Variant>,
	mut current_variant: VariantWithVersionedTypeAttribute,
) -> Result<()> {
	match current_variant.attribute.mode() {
		VariantVersionedTypeMode::Extend { span } |
		VariantVersionedTypeMode::OverrideAndExtend { extend_span: span, .. } => Err(syn::Error::new(
			span,
			"Using `extend` requires that there is a previous version that exists which \
            this variant should extend but there is no previous version",
		)),
		VariantVersionedTypeMode::Override { span } => Err(syn::Error::new(
			span,
			"Using `override` requires that there is a previous version that exists which \
            this variant should override but there is no previous version",
		)),
		VariantVersionedTypeMode::Standalone => {
			strip_field_attributes(&mut current_variant.variant.fields)?;
			output_variants.push(current_variant.variant);
			Ok(())
		},
	}
}

fn reject_duplicate_current_variants(variants: &[VariantWithVersionedTypeAttribute]) -> Result<()> {
	let mut seen_variants = std::collections::BTreeMap::<String, syn::Ident>::new();

	for variant in variants {
		let variant_name = variant.variant.ident.to_string();
		if let Some(existing_ident) = seen_variants.get(&variant_name) {
			let mut error = syn::Error::new_spanned(
				&variant.variant.ident,
				format!("variant `{variant_name}` is defined more than once"),
			);
			error.combine(syn::Error::new_spanned(
				existing_ident,
				format!("first definition of variant `{variant_name}` is here"),
			));
			return Err(error);
		}

		seen_variants.insert(variant_name, variant.variant.ident.clone());
	}

	Ok(())
}

fn find_variant<'a>(
	variants: &'a Punctuated<Variant, Comma>,
	variant_name: &str,
) -> Option<&'a Variant> {
	variants.iter().find(|variant| variant.ident == variant_name)
}

fn upsert_variant(
	output_variants: &mut Vec<Variant>,
	variant: Variant,
	variant_name: &str,
	type_extension: EnumTypeExtension,
) {
	if type_extension.is_extending() {
		if let Some(existing_variant) =
			output_variants.iter_mut().find(|candidate| candidate.ident == variant_name)
		{
			*existing_variant = variant;
			return;
		}
	}

	output_variants.push(variant);
}

fn missing_type_extension_error(extend_span: Span) -> syn::Error {
	syn::Error::new(
		extend_span,
		"Using `extend` requires that there is a previous version that \
        exists which this type should extend but there is no previous version",
	)
}

fn struct_from_enum_extension_error(
	extend_span: Span,
	this_struct: &ItemStruct,
	previous_enum: &ItemEnum,
) -> syn::Error {
	let mut error = syn::Error::new(
		extend_span,
		"A struct can't be extended from an enum; the previous type is an enum",
	);
	error.combine(syn::Error::new(extend_span, "Extend was requested here"));
	error
		.combine(syn::Error::new(previous_enum.span(), "The previous type (enum) is defined here"));
	error.combine(syn::Error::new(this_struct.span(), "This type (struct) is defined here"));
	error
}

fn enum_from_struct_extension_error(
	extend_span: Span,
	this_enum: &ItemEnum,
	previous_struct: &ItemStruct,
) -> syn::Error {
	let mut error = syn::Error::new(
		extend_span,
		"An enum can't be extended from a struct; the previous type is a struct",
	);
	error.combine(syn::Error::new(extend_span, "Extend was requested here"));
	error.combine(syn::Error::new(
		previous_struct.span(),
		"The previous type (struct) is defined here",
	));
	error.combine(syn::Error::new(this_enum.span(), "This type (enum) is defined here"));
	error
}

fn duplicate_in_extended_enum_error(
	current_variant: &Variant,
	previous_variant: &Variant,
	variant_name: &str,
) -> syn::Error {
	let mut error = syn::Error::new_spanned(
		&current_variant.ident,
		format!(
			"variant `{variant_name}` is already defined in the previous version; \
            add `#[versioned_type(override)]` to replace it"
		),
	);
	error.combine(syn::Error::new_spanned(
		&previous_variant.ident,
		format!("original variant `{variant_name}` was defined here"),
	));
	error
}

fn missing_variant_override_error(
	override_span: Span,
	variant: &Variant,
	variant_name: &str,
) -> syn::Error {
	let mut error = syn::Error::new(
		override_span,
		format!(
			"variant `{variant_name}` is marked as an override but no variant with that name \
            exists in the previous version"
		),
	);
	error.combine(syn::Error::new_spanned(
		&variant.ident,
		format!("override variant `{variant_name}` is defined here"),
	));
	error
}

fn missing_variant_extension_error(
	extend_span: Span,
	variant: &Variant,
	variant_name: &str,
) -> syn::Error {
	let mut error = syn::Error::new(
		extend_span,
		format!(
			"variant `{variant_name}` is marked as an extension but no variant with that name \
            exists in the previous version"
		),
	);
	error.combine(syn::Error::new_spanned(
		&variant.ident,
		format!("extension variant `{variant_name}` is defined here"),
	));
	error
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
				#[versioned_type(override, extend)]
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
