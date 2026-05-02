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
use quote::{quote, ToTokens};
use syn::{
	punctuated::Punctuated, spanned::Spanned, token::Comma, GenericParam, Generics, Ident,
	ItemEnum, ItemStruct, ItemType, Result, Variant,
};

use attribute::{
	EncodeLikeTypes,
	TypeVersionedTypeAttribute, TypeVersionedTypeMode, VariantVersionedTypeMode,
	VariantWithVersionedTypeAttribute,
};
use fields::{extend_fields, strip_field_attributes, FieldOwner};
pub use item::{DefineVersionedTypeInput, DefineVersionedTypeItem};

/// Expands every parsed versioned type item according to its extension attributes.
///
/// The input parser has already grouped the definitions by version and checked that those versions
/// are contiguous. This handler is responsible for the context-sensitive work: stripping helper
/// attributes, merging fields, merging enum variants, and producing diagnostics when an extension
/// request cannot be satisfied by the immediately previous version.
pub fn handle_define_versioned_type(
	input: DefineVersionedTypeInput,
) -> Result<DefineVersionedTypeOutput> {
	let DefineVersionedTypeInput { name, highest_version, definitions } = input;
	let latest_alias = latest_type_alias(name.as_deref(), highest_version, &definitions);
	let mut items = Vec::<DefineVersionedTypeItem>::with_capacity(definitions.len());
	let mut encode_like_impls = Vec::<EncodeLikeImpl>::new();

	for mut item in definitions.into_values() {
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(item.take_attributes())?;
		let type_attribute = attribute_split.versioned_type;
		item.set_attributes(attribute_split.other_attributes);
		encode_like_impls.extend(EncodeLikeImpl::for_item(&item, type_attribute.encode_like()));

		handle_item_extensions(&mut item, type_attribute, items.last())?;
		items.push(item);
	}

	Ok(DefineVersionedTypeOutput { items, latest_alias, encode_like_impls })
}

/// Builds the latest-version alias if the invocation contains at least one item.
fn latest_type_alias(
	name: Option<&str>,
	highest_version: Option<item::Version>,
	definitions: &std::collections::BTreeMap<item::Version, DefineVersionedTypeItem>,
) -> Option<LatestTypeAlias> {
	let name = name?;
	let latest_item = definitions.get(&highest_version?)?;
	Some(LatestTypeAlias::new(name, latest_item))
}

/// The fully processed output emitted by `define_versioned_type!`.
pub struct DefineVersionedTypeOutput {
	/// The processed versioned item definitions in ascending version order.
	items: Vec<DefineVersionedTypeItem>,

	/// The alias pointing at the highest version in this invocation.
	latest_alias: Option<LatestTypeAlias>,

	/// The generated `EncodeLike` impls requested by item-level attributes.
	encode_like_impls: Vec<EncodeLikeImpl>,
}

impl ToTokens for DefineVersionedTypeOutput {
	/// Writes the processed items, latest-version alias, and requested `EncodeLike` impls.
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		let items = &self.items;
		let latest_alias = &self.latest_alias;
		let encode_like_impls = &self.encode_like_impls;
		tokens.extend(quote! {
			#( #items )*
			#latest_alias
			#( #encode_like_impls )*
		});
	}
}

impl std::ops::Deref for DefineVersionedTypeOutput {
	type Target = [DefineVersionedTypeItem];

	fn deref(&self) -> &Self::Target {
		&self.items
	}
}

/// A generated type alias that points at the highest known version.
struct LatestTypeAlias {
	/// The underlying Rust type alias item.
	item: ItemType,
}

impl LatestTypeAlias {
	/// Builds the alias for the given base name and latest versioned item.
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
	/// Writes the wrapped type alias into the output stream.
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		self.item.to_tokens(tokens);
	}
}

/// A generated `EncodeLike` impl for a type that shares this version's SCALE representation.
struct EncodeLikeImpl {
	/// The generated impl tokens.
	tokens: proc_macro2::TokenStream,
}

impl EncodeLikeImpl {
	/// Builds all `EncodeLike` impls requested for one item.
	fn for_item(
		item: &DefineVersionedTypeItem,
		encode_like: Option<&EncodeLikeTypes>,
	) -> Vec<Self> {
		let Some(encode_like) = encode_like else { return Vec::new() };

		encode_like.types().iter().map(|source_type| Self::new(item, source_type)).collect()
	}

	/// Builds one `EncodeLike` impl from a source type to the current versioned item.
	fn new(item: &DefineVersionedTypeItem, source_type: &syn::TypePath) -> Self {
		let target_ident = item.ident();
		let (_, target_generics, _) = item.generics().split_for_impl();
		let target_type: syn::Type = syn::parse_quote!(#target_ident #target_generics);
		let mut impl_generics = item.generics().clone();

		let where_clause = impl_generics.make_where_clause();
		where_clause.predicates.push(syn::parse_quote!(#source_type: ::codec::Encode));
		where_clause.predicates.push(syn::parse_quote!(#target_type: ::codec::Encode));

		let (impl_generics, _, where_clause) = impl_generics.split_for_impl();
		let tokens = quote! {
			impl #impl_generics ::codec::EncodeLike<#target_type> for #source_type #where_clause {}
		};

		Self { tokens }
	}
}

impl ToTokens for EncodeLikeImpl {
	/// Writes the generated impl into the output stream.
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		self.tokens.to_tokens(tokens);
	}
}

/// Removes bounds from alias generics to avoid unenforced type-alias bounds.
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

/// Applies the extension rules that are shared by all supported item kinds.
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

/// Applies type-level extension rules to a struct definition.
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

/// Applies type-level and variant-level extension rules to an enum definition.
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

/// Describes how an enum should relate to the immediately previous version.
#[derive(Clone, Copy)]
enum EnumMergeMode<'a> {
	/// No previous version exists, so no selective extension is possible.
	NoPrevious,

	/// The previous version is a struct and may seed individual variants.
	PreviousStruct {
		/// The struct that can be used by variant-level `extend` attributes.
		previous_struct: &'a ItemStruct,
	},

	/// The previous version is an enum and may seed variants or the enum body.
	PreviousEnum {
		/// The enum that provides the previous variant set.
		previous_enum: &'a ItemEnum,

		/// Whether the enum itself requested a full type-level extension.
		type_extension: EnumTypeExtension,
	},
}

/// Describes whether a previous enum is copied into the current enum first.
#[derive(Clone, Copy)]
enum EnumTypeExtension {
	/// The current enum starts from its own variants only.
	Standalone,

	/// The current enum starts with all variants from the previous enum.
	Extending,
}

impl EnumTypeExtension {
	/// Returns whether previous variants were copied into the output enum.
	#[must_use]
	fn is_extending(self) -> bool {
		match self {
			Self::Standalone => false,
			Self::Extending => true,
		}
	}
}

/// Builds the enum merge mode or reports invalid type-level extension usage.
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

/// Creates the initial variant list for the current enum.
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

/// Applies the current variant's requested change to the output variant list.
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

/// Applies a variant change when the previous version was also an enum.
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

/// Applies a variant change when the previous version was a struct.
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

/// Applies a variant change when there is no previous version.
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

/// Returns an error when the current enum defines a variant name twice.
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

/// Finds a variant with the provided name in a punctuated variant list.
fn find_variant<'a>(
	variants: &'a Punctuated<Variant, Comma>,
	variant_name: &str,
) -> Option<&'a Variant> {
	variants.iter().find(|variant| variant.ident == variant_name)
}

/// Replaces or appends a variant depending on the enum-level extension mode.
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

/// Builds a diagnostic for type-level `extend` without a previous version.
fn missing_type_extension_error(extend_span: Span) -> syn::Error {
	syn::Error::new(
		extend_span,
		"Using `extend` requires that there is a previous version that \
        exists which this type should extend but there is no previous version",
	)
}

/// Builds a diagnostic for extending a struct from a previous enum.
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

/// Builds a diagnostic for extending an enum from a previous struct.
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

/// Builds a diagnostic for redefining a copied enum variant without override.
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

/// Builds a diagnostic for overriding a missing previous enum variant.
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

/// Builds a diagnostic for extending a missing previous enum variant.
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
	use quote::ToTokens;
	use syn::{parse2, Fields, ItemStruct, Visibility};

	use super::{
		attribute::{
			FieldVersionedTypeAttribute, TypeVersionedTypeAttribute, TypeVersionedTypeMode,
			VariantVersionedTypeAttribute, VariantVersionedTypeMode,
		},
		fields::{extend_fields, FieldOwner},
		*,
	};

	#[test]
	fn parses_struct_when_keyword_follows_attributes_and_visibility() {
		// Arrange
		let tokens = quote::quote! {
			#[derive(Clone)]
			pub(crate) struct CallLogV1 {
				pub item: u32,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		assert!(
			matches!(input, DefineVersionedTypeItem::Struct(item) if item.ident == "CallLogV1")
		);
	}

	#[test]
	fn parses_enum_when_keyword_follows_attributes_and_visibility() {
		// Arrange
		let tokens = quote::quote! {
			#[derive(Clone)]
			pub enum CallLogV1 {
				Call {
					item: u32,
				},
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		assert!(matches!(input, DefineVersionedTypeItem::Enum(item) if item.ident == "CallLogV1"));
	}

	#[test]
	fn preserves_struct_attributes_visibility_and_doc_comments() {
		// Arrange
		let tokens = quote::quote! {
			#[doc = "Call log docs."]
			#[derive(Clone)]
			pub(crate) struct CallLogV1 {
				pub item: u32,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = input else {
			panic!("expected struct item");
		};
		assert!(matches!(item.vis, Visibility::Restricted(_)));
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("derive")));
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("doc")));
	}

	#[test]
	fn preserves_enum_attributes_visibility_and_doc_comments() {
		// Arrange
		let tokens = quote::quote! {
			#[doc = "Call log docs."]
			#[derive(Clone)]
			pub enum CallLogV1 {
				Call {
					item: u32,
				},
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = input else {
			panic!("expected enum item");
		};
		assert!(matches!(item.vis, Visibility::Public(_)));
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("derive")));
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("doc")));
	}

	#[test]
	fn reports_descriptive_error_when_item_is_not_struct_or_enum() {
		// Arrange
		let tokens = quote::quote! {
			pub fn call_log_v1() {}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeItem>(tokens) {
			Ok(_) => panic!("expected parsing to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("expects a struct or enum item"));
	}

	#[test]
	fn extracts_base_name_and_version_from_struct_name() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV12 {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let name_and_version = input.name_and_version().unwrap();

		// Assert
		assert_eq!(name_and_version.base_name(), "CallLog");
		assert_eq!(name_and_version.version().value(), 12);
	}

	#[test]
	fn extracts_base_name_and_version_from_enum_name() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV2 {
				Call,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let name_and_version = input.name_and_version().unwrap();

		// Assert
		assert_eq!(name_and_version.base_name(), "CallLog");
		assert_eq!(name_and_version.version().value(), 2);
	}

	#[test]
	fn extracts_base_name_when_name_contains_earlier_v_character() {
		// Arrange
		let tokens = quote::quote! {
			pub struct VeryVerboseCallLogV2 {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let name_and_version = input.name_and_version().unwrap();

		// Assert
		assert_eq!(name_and_version.base_name(), "VeryVerboseCallLog");
		assert_eq!(name_and_version.version().value(), 2);
	}

	#[test]
	fn preserves_struct_generics() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1<T>
			where
				T: Clone,
			{
				pub item: T,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = input else {
			panic!("expected struct item");
		};
		assert_eq!(item.generics.params.len(), 1);
		assert!(item.generics.where_clause.is_some());
	}

	#[test]
	fn preserves_enum_generics() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1<T>
			where
				T: Clone,
			{
				Call(T),
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = input else {
			panic!("expected enum item");
		};
		assert_eq!(item.generics.params.len(), 1);
		assert!(item.generics.where_clause.is_some());
	}

	#[test]
	fn parses_item_with_inherited_visibility() {
		// Arrange
		let tokens = quote::quote! {
			#[derive(Clone)]
			struct CallLogV1;
		};

		// Act
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = input else {
			panic!("expected struct item");
		};
		assert!(matches!(item.vis, Visibility::Inherited));
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("derive")));
	}

	#[test]
	fn rejects_name_with_version_but_no_base_name() {
		// Arrange
		let tokens = quote::quote! {
			pub struct V1;
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("base name before the version suffix"));
	}

	#[test]
	fn field_extensions_place_previous_named_fields_before_current_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub item3: u32,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				pub item1: u8,
				pub item2: u16,
			}
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Named(fields) = this.fields else {
			panic!("expected named fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["item1", "item2", "item3"]);
	}

	#[test]
	fn field_extensions_place_previous_unnamed_fields_before_current_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2(pub u32);
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1(pub u8, pub u16);
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Unnamed(fields) = this.fields else {
			panic!("expected unnamed fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["u8", "u16", "u32"]);
	}

	#[test]
	fn field_extensions_copy_previous_named_fields_into_current_unit_struct() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item1: u8,
				pub item2: u16,
			}

			#[versioned_type(extend)]
			pub struct CallLogV2;
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		let Fields::Named(fields) = &item.fields else {
			panic!("expected named fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["item1", "item2"]);
	}

	#[test]
	fn field_extensions_copy_previous_tuple_fields_into_current_unit_struct() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1(pub u8, pub u16);

			#[versioned_type(extend)]
			pub struct CallLogV2;
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		let Fields::Unnamed(fields) = &item.fields else {
			panic!("expected unnamed fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["u8", "u16"]);
	}

	#[test]
	fn field_extensions_name_previous_unnamed_fields_before_current_named_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub item3: u32,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1(pub u8, pub u16);
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Named(fields) = this.fields else {
			panic!("expected named fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field_0", "field_1", "item3"]);
	}

	#[test]
	fn field_extensions_place_previous_named_fields_before_current_tuple_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2(pub Type3);
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				item1: Type1,
				item2: Type2,
			}
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Unnamed(fields) = this.fields else {
			panic!("expected unnamed fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["Type1", "Type2", "Type3"]);
		assert!(matches!(fields.unnamed[0].vis, Visibility::Public(_)));
		assert!(matches!(fields.unnamed[1].vis, Visibility::Public(_)));
	}

	#[test]
	fn field_extensions_keep_current_tuple_fields_after_previous_unit_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1;

			#[versioned_type(extend)]
			pub struct CallLogV2(pub u8, pub u16);
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		let Fields::Unnamed(fields) = &item.fields else {
			panic!("expected unnamed fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["u8", "u16"]);
	}

	#[test]
	fn field_extensions_make_copied_struct_fields_public() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub item2: u16,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				item1: u8,
			}
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Named(fields) = this.fields else {
			panic!("expected named fields");
		};
		assert!(matches!(fields.named[0].vis, Visibility::Public(_)));
	}

	#[test]
	fn field_extensions_preserve_attributes_on_inherited_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				#[doc = "field docs"]
				pub item1: u8,
			}

			#[versioned_type(extend)]
			pub struct CallLogV2 {
				pub item2: u16,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		let Fields::Named(fields) = &item.fields else {
			panic!("expected named fields");
		};
		assert!(fields.named[0].attrs.iter().any(|attr| attr.path().is_ident("doc")));
	}

	#[test]
	fn field_extensions_reject_generated_name_collision_from_tuple_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub field_0: u32,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1(pub u8);
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected generated field name collision to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field `field_0` conflicts"));
		assert!(message.contains("generated from the previous tuple fields"));
	}

	#[test]
	fn field_extensions_override_named_field_in_original_position() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				#[versioned_type(override)]
				pub item2: Type4,
				pub item3: Type3,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				pub item1: Type1,
				pub item2: Type2,
			}
		);

		// Act
		extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct).unwrap();

		// Assert
		let Fields::Named(fields) = this.fields else {
			panic!("expected named fields");
		};
		let field_types = fields
			.named
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["Type1", "Type4", "Type3"]);
		assert!(fields.named[1].attrs.is_empty());
	}

	#[test]
	fn field_extensions_reject_redefined_named_field_without_override() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub item2: Type4,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				pub item1: Type1,
				pub item2: Type2,
			}
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected redefined field to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field `item2` is already defined"));
		assert!(message.contains("versioned_type(override)"));
	}

	#[test]
	fn field_extensions_reject_duplicate_current_named_fields() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				pub item3: Type3,
				pub item3: Type4,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				pub item1: Type1,
			}
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected duplicate current field to fail"),
			Err(error) => error,
		};

		// Assert
		let messages = (&error).into_iter().map(|error| error.to_string()).collect::<Vec<_>>();
		assert!(messages
			.iter()
			.any(|message| message.contains("field `item3` is defined more than once")));
		assert!(messages
			.iter()
			.any(|message| message.contains("first definition of field `item3`")));
	}

	#[test]
	fn field_extensions_reject_override_without_previous_named_field() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				#[versioned_type(override)]
				pub item3: Type3,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1 {
				pub item1: Type1,
				pub item2: Type2,
			}
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected missing override target to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field `item3` is marked as an override"));
		assert!(message.contains("no field with that name exists"));
	}

	#[test]
	fn field_extensions_reject_override_when_previous_version_has_no_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1;

			#[versioned_type(extend)]
			pub struct CallLogV2 {
				#[versioned_type(override)]
				pub item1: Type1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected field override against unit struct to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field is marked as an override"));
		assert!(message.contains("previous version has no fields"));
	}

	#[test]
	fn field_extensions_reject_override_on_tuple_field() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2(#[versioned_type(override)] pub Type3);
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1(pub Type1, pub Type2);
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected tuple field override to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("not supported on tuple fields"));
		assert!(message.contains("stable names"));
	}

	#[test]
	fn field_extensions_reject_override_when_previous_fields_are_unnamed() {
		// Arrange
		let mut this: ItemStruct = syn::parse_quote!(
			pub struct CallLogV2 {
				#[versioned_type(override)]
				pub field_0: Type3,
			}
		);
		let other: ItemStruct = syn::parse_quote!(
			pub struct CallLogV1(pub Type1, pub Type2);
		);

		// Act
		let error = match extend_fields(&mut this.fields, &other.fields, FieldOwner::Struct) {
			Ok(_) => panic!("expected override against tuple fields to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("previous version to have named fields"));
		assert!(message.contains("ambiguous"));
	}

	#[test]
	fn type_versioned_type_attribute_parses_extend_and_preserves_other_attributes() {
		// Arrange
		let attributes = vec![
			syn::parse_quote!(#[derive(Clone)]),
			syn::parse_quote!(#[versioned_type(extend)]),
			syn::parse_quote!(#[doc = "Call log docs."]),
		];

		// Act
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(matches!(
			attribute_split.versioned_type.mode(),
			TypeVersionedTypeMode::Extend { .. }
		));
		assert_eq!(attribute_split.other_attributes.len(), 2);
		assert!(attribute_split.other_attributes[0].path().is_ident("derive"));
		assert!(attribute_split.other_attributes[1].path().is_ident("doc"));
	}

	#[test]
	fn type_versioned_type_attribute_parses_encode_like_type_paths() {
		// Arrange
		let attributes = vec![
			syn::parse_quote!(#[derive(Clone)]),
			syn::parse_quote!(#[versioned_type(extend, encode_like = "Bytes; Vec<u8>")]),
		];

		// Act
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(attributes).unwrap();
		let encode_like = attribute_split.versioned_type.encode_like().unwrap();
		let type_paths =
			encode_like.types().iter().map(ToTokens::to_token_stream).collect::<Vec<_>>();

		// Assert
		assert!(matches!(
			attribute_split.versioned_type.mode(),
			TypeVersionedTypeMode::Extend { .. }
		));
		assert_eq!(type_paths[0].to_string(), "Bytes");
		assert_eq!(type_paths[1].to_string(), "Vec < u8 >");
		assert_eq!(attribute_split.other_attributes.len(), 1);
		assert!(attribute_split.other_attributes[0].path().is_ident("derive"));
	}

	#[test]
	fn type_versioned_type_attribute_defaults_when_missing() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[derive(Clone)])];

		// Act
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(matches!(attribute_split.versioned_type.mode(), TypeVersionedTypeMode::Standalone));
		assert_eq!(attribute_split.other_attributes.len(), 1);
		assert!(attribute_split.other_attributes[0].path().is_ident("derive"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_bare_attribute() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected bare versioned_type attribute to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("requires options"));
	}

	#[test]
	fn type_versioned_type_attribute_accepts_empty_options() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type()])];

		// Act
		let attribute_split = TypeVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(matches!(attribute_split.versioned_type.mode(), TypeVersionedTypeMode::Standalone));
		assert!(attribute_split.other_attributes.is_empty());
	}

	#[test]
	fn type_versioned_type_attribute_rejects_name_value_syntax() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type = "extend"])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected name-value versioned_type syntax to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("does not support name-value syntax"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_unsupported_options() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(rename)])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected unsupported versioned_type option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("currently only `extend`, `override`, and"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_extend_arguments() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(extend = true)])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected extend arguments to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`extend` does not accept arguments"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_duplicate_extend_option() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(extend, extend)])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected duplicate extend option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`extend` is specified more than once"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_duplicate_encode_like_option() {
		// Arrange
		let attributes = vec![syn::parse_quote!(
			#[versioned_type(encode_like = "Bytes", encode_like = "Vec<u8>")]
		)];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected duplicate encode_like option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`encode_like` is specified more than once"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_malformed_encode_like_literal() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(encode_like = "Bytes; ^")])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected malformed encode_like literal to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("expected identifier"));
	}

	#[test]
	fn type_versioned_type_attribute_rejects_override() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(override)])];

		// Act
		let error = match TypeVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected type override to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`override` is not supported on types"));
	}

	#[test]
	fn field_versioned_type_attribute_parses_override() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(override)])];

		// Act
		let attribute_split = FieldVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(attribute_split.versioned_type.override_span().is_some());
		assert!(attribute_split.other_attributes.is_empty());
	}

	#[test]
	fn field_versioned_type_attribute_accepts_empty_options() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type()])];

		// Act
		let attribute_split = FieldVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(attribute_split.versioned_type.override_span().is_none());
		assert!(attribute_split.other_attributes.is_empty());
	}

	#[test]
	fn field_versioned_type_attribute_rejects_extend() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(extend)])];

		// Act
		let error = match FieldVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected field extend to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`extend` is not supported on fields"));
	}

	#[test]
	fn field_versioned_type_attribute_rejects_encode_like() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(encode_like = "Bytes")])];

		// Act
		let error = match FieldVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected field encode_like to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`encode_like` is not supported on fields"));
	}

	#[test]
	fn field_versioned_type_attribute_rejects_duplicate_override_option() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(override, override)])];

		// Act
		let error = match FieldVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected duplicate override option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`override` is specified more than once"));
	}

	#[test]
	fn field_versioned_type_attribute_rejects_override_arguments() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(override(foo))])];

		// Act
		let error = match FieldVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected override arguments to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`override` does not accept arguments"));
	}

	#[test]
	fn variant_versioned_type_attribute_parses_extend_and_override() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(extend, override)])];

		// Act
		let attribute_split = VariantVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(matches!(
			attribute_split.versioned_type.mode(),
			VariantVersionedTypeMode::OverrideAndExtend { .. }
		));
		assert!(attribute_split.other_attributes.is_empty());
	}

	#[test]
	fn variant_versioned_type_attribute_accepts_empty_options() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type()])];

		// Act
		let attribute_split = VariantVersionedTypeAttribute::parse_and_split(attributes).unwrap();

		// Assert
		assert!(matches!(
			attribute_split.versioned_type.mode(),
			VariantVersionedTypeMode::Standalone
		));
		assert!(attribute_split.other_attributes.is_empty());
	}

	#[test]
	fn variant_versioned_type_attribute_rejects_encode_like() {
		// Arrange
		let attributes = vec![syn::parse_quote!(#[versioned_type(encode_like = "Bytes")])];

		// Act
		let error = match VariantVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected variant encode_like to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`encode_like` is not supported on variants"));
	}

	#[test]
	fn variant_versioned_type_attribute_rejects_duplicate_extend_option() {
		// Arrange
		let attributes = vec![
			syn::parse_quote!(#[versioned_type(extend)]),
			syn::parse_quote!(#[versioned_type(extend)]),
		];

		// Act
		let error = match VariantVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected duplicate extend option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`extend` is specified more than once"));
	}

	#[test]
	fn variant_versioned_type_attribute_rejects_duplicate_override_option() {
		// Arrange
		let attributes = vec![
			syn::parse_quote!(#[versioned_type(override)]),
			syn::parse_quote!(#[versioned_type(override)]),
		];

		// Act
		let error = match VariantVersionedTypeAttribute::parse_and_split(attributes) {
			Ok(_) => panic!("expected duplicate override option to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`override` is specified more than once"));
	}

	#[test]
	fn struct_extension_without_previous_version_errors() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(extend)]
			pub struct CallLogV1 {
				pub item1: SomeType,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected struct extension without previous version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("Using `extend` requires"));
		assert!(message.contains("there is no previous version"));
	}

	#[test]
	fn enum_extension_without_previous_version_errors() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(extend)]
			pub enum CallLogV1 {
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected enum extension without previous version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("Using `extend` requires"));
		assert!(message.contains("there is no previous version"));
	}

	#[test]
	fn enum_rejects_variant_extend_without_previous_version() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1 {
				#[versioned_type(extend)]
				Variant1 {
					field1: SomeType,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected variant extension without previous version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("this variant should extend"));
		assert!(message.contains("there is no previous version"));
	}

	#[test]
	fn enum_rejects_variant_override_without_previous_version() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1 {
				#[versioned_type(override)]
				Variant1 {
					field1: SomeType,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected variant override without previous version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("this variant should override"));
		assert!(message.contains("there is no previous version"));
	}

	#[test]
	fn enum_rejects_variant_override_and_extend_without_previous_version() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1 {
				#[versioned_type(override, extend)]
				Variant1 {
					field1: SomeType,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected variant override and extension to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("this variant should extend"));
		assert!(message.contains("there is no previous version"));
	}

	#[test]
	fn enum_extension_adds_new_variants_after_previous_variants() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				Variant2 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		assert_eq!(output.len(), 2);
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant2"]);
	}

	#[test]
	fn enum_extension_rejects_redefined_variant_without_override() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected redefined variant to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant1` is already defined"));
		assert!(message.contains("versioned_type(override)"));
	}

	#[test]
	fn enum_extension_allows_variant_override() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override)]
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		assert_eq!(item.variants.len(), 1);
		let variant = item.variants.iter().next().unwrap();
		assert_eq!(variant.ident, "Variant1");
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field2"]);
		assert!(variant.attrs.is_empty());
	}

	#[test]
	fn enum_variant_override_without_extension_rejects_field_override() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			pub enum MyEnumV2 {
				#[versioned_type(override)]
				Variant1 {
					#[versioned_type(override)]
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected field override without field extension to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("can only be used inside a type or variant"));
	}

	#[test]
	fn enum_extension_allows_variant_override_and_field_extension_together() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
					field2: SomeType2,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					#[versioned_type(override)]
					field2: SomeType4,
					field3: SomeType3,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_types = fields
			.named
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		let overridden_field = fields.named.iter().nth(1).unwrap();
		assert_eq!(field_types, vec!["SomeType", "SomeType4", "SomeType3"]);
		assert!(variant.attrs.is_empty());
		assert!(overridden_field.attrs.is_empty());
	}

	#[test]
	fn enum_extension_preserves_non_helper_variant_and_field_attributes() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[doc = "variant docs"]
				#[versioned_type(override, extend)]
				Variant1 {
					#[doc = "field docs"]
					#[versioned_type(override)]
					field1: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field = fields.named.iter().next().unwrap();
		assert!(variant.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(variant.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
		assert!(field.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(field.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
	}

	#[test]
	fn enum_extension_rejects_override_for_missing_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override)]
				Variant2 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected missing variant override target to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant2` is marked as an override"));
		assert!(message.contains("no variant with that name exists"));
	}

	#[test]
	fn enum_extension_rejects_extend_for_missing_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(extend)]
				Variant2 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected missing variant extension target to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant2` is marked as an extension"));
		assert!(message.contains("no variant with that name exists"));
	}

	#[test]
	fn enum_extension_rejects_override_and_extend_for_missing_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant2 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected missing variant override and extension target to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant2` is marked as an extension"));
		assert!(message.contains("no variant with that name exists"));
	}

	#[test]
	fn enum_extension_allows_variant_extend_without_override_for_existing_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(extend)]
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field1", "field2"]);
	}

	#[test]
	fn enum_allows_variant_override_without_enum_extension() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
				Variant2 {
					field3: SomeType3,
				},
			}

			pub enum MyEnumV2 {
				#[versioned_type(override)]
				Variant1 {
					field2: SomeType2,
				},
				Variant3 {
					field4: SomeType4,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant3"]);
	}

	#[test]
	fn enum_allows_variant_extend_without_enum_extension() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
				Variant2 {
					field3: SomeType3,
				},
			}

			pub enum MyEnumV2 {
				#[versioned_type(extend)]
				Variant1 {
					field2: SomeType2,
				},
				Variant3 {
					field4: SomeType4,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant3"]);
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field1", "field2"]);
	}

	#[test]
	fn standalone_enum_redefines_previous_variant_without_inheriting_previous_shape() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			pub enum MyEnumV2 {
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(item.variants.len(), 1);
		assert_eq!(field_names, vec!["field2"]);
	}

	#[test]
	fn enum_allows_variant_override_and_extend_without_enum_extension() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
					field2: SomeType2,
				},
				Variant2 {
					field4: SomeType4,
				},
			}

			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					#[versioned_type(override)]
					field2: SomeType5,
					field3: SomeType3,
				},
				Variant3 {
					field6: SomeType6,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_types = fields
			.named
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant3"]);
		assert_eq!(field_types, vec!["SomeType", "SomeType5", "SomeType3"]);
	}

	#[test]
	fn enum_rejects_type_level_override_attribute() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
			}

			#[versioned_type(override)]
			pub enum MyEnumV2 {
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected enum-level override to fail"),
			Err(error) => error,
		};

		// Assert
		assert!(error.to_string().contains("`override` is not supported on types"));
	}

	#[test]
	fn handler_strips_type_level_helper_attribute_from_struct_output() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item1: SomeType,
			}

			#[doc = "struct docs"]
			#[versioned_type(extend)]
			pub struct CallLogV2 {
				pub item2: SomeType2,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(item.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
	}

	#[test]
	fn handler_strips_type_level_helper_attribute_from_enum_output() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1 {
				Variant1,
			}

			#[doc = "enum docs"]
			#[versioned_type(extend)]
			pub enum CallLogV2 {
				Variant2,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		assert!(item.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(item.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
	}

	#[test]
	fn handler_strips_noop_helper_attributes_from_standalone_variant_and_field() {
		// Arrange
		let tokens = quote::quote! {
			pub enum CallLogV1 {
				#[doc = "variant docs"]
				#[versioned_type()]
				Variant1 {
					#[doc = "field docs"]
					#[versioned_type()]
					field1: SomeType,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[0] else {
			panic!("expected first item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field = fields.named.iter().next().unwrap();
		assert!(variant.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(variant.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
		assert!(field.attrs.iter().any(|attr| attr.path().is_ident("doc")));
		assert!(field.attrs.iter().all(|attr| !attr.path().is_ident("versioned_type")));
	}

	#[test]
	fn enum_rejects_duplicate_current_variants() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected duplicate variant to fail"),
			Err(error) => error,
		};

		// Assert
		let messages = (&error).into_iter().map(|error| error.to_string()).collect::<Vec<_>>();
		assert!(messages
			.iter()
			.any(|message| message.contains("variant `Variant1` is defined more than once")));
		assert!(messages
			.iter()
			.any(|message| message.contains("first definition of variant `Variant1`")));
	}

	#[test]
	fn enum_extension_variant_override_preserves_original_position() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
				Variant2 {
					field1: SomeType,
				},
				Variant3,
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override)]
				Variant2 {
					field2: SomeType2,
				},
				Variant4,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant2", "Variant3", "Variant4"]);
	}

	#[test]
	fn enum_extension_variant_extend_preserves_original_position() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
				Variant2 {
					field1: SomeType,
				},
				Variant3,
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(extend)]
				Variant2 {
					field2: SomeType2,
				},
				Variant4,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		let variant = item.variants.iter().nth(1).unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant2", "Variant3", "Variant4"]);
		assert_eq!(field_names, vec!["field1", "field2"]);
	}

	#[test]
	fn enum_extension_appends_new_variants_after_all_previous_variants() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
				Variant2,
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				Variant3,
				Variant4,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant_names = item
			.variants
			.iter()
			.map(|variant| variant.ident.to_string())
			.collect::<Vec<_>>();
		assert_eq!(variant_names, vec!["Variant1", "Variant2", "Variant3", "Variant4"]);
	}

	#[test]
	fn enum_extension_strips_variant_level_attributes() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		assert!(variant.attrs.is_empty());
	}

	#[test]
	fn enum_extension_variant_extend_rejects_duplicate_field_without_field_override() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					field1: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected duplicate variant field to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field `field1` is already defined"));
		assert!(message.contains("versioned_type(override)"));
	}

	#[test]
	fn enum_extension_variant_extend_rejects_missing_field_override_target() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					#[versioned_type(override)]
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected missing field override target to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("field `field2` is marked as an override"));
		assert!(message.contains("no field with that name exists"));
	}

	#[test]
	fn enum_extension_variant_extend_supports_tuple_variant_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1(SomeType),
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1(SomeType2),
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Unnamed(fields) = &variant.fields else {
			panic!("expected unnamed variant fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["SomeType", "SomeType2"]);
	}

	#[test]
	fn enum_extension_variant_extend_names_previous_tuple_fields_before_named_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1(SomeType),
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					field1: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field_0", "field1"]);
		assert!(matches!(fields.named[0].vis, Visibility::Inherited));
		assert!(matches!(fields.named[1].vis, Visibility::Inherited));
	}

	#[test]
	fn enum_extension_variant_extend_supports_unit_to_named_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1,
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					field1: SomeType,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field1"]);
	}

	#[test]
	fn enum_extension_variant_extend_copies_named_fields_into_current_unit_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field1"]);
	}

	#[test]
	fn enum_extension_variant_extend_copies_tuple_fields_into_current_unit_variant() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1(SomeType),
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Unnamed(fields) = &variant.fields else {
			panic!("expected unnamed variant fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["SomeType"]);
	}

	#[test]
	fn enum_extension_variant_extend_supports_named_to_tuple_fields() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1(SomeType2),
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Unnamed(fields) = &variant.fields else {
			panic!("expected unnamed variant fields");
		};
		let field_types = fields
			.unnamed
			.iter()
			.map(|field| field.ty.to_token_stream().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_types, vec!["SomeType", "SomeType2"]);
	}

	#[test]
	fn enum_extension_rejects_tuple_field_override_in_variant_extend() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyEnumV1 {
				Variant1(SomeType),
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				#[versioned_type(override, extend)]
				Variant1(#[versioned_type(override)] SomeType2),
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected tuple field override to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("not supported on tuple fields"));
		assert!(message.contains("stable names"));
	}

	#[test]
	fn enum_allows_variant_extend_from_previous_struct() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyTypeV1 {
				pub field1: SomeType,
				pub field2: SomeType2,
			}

			pub enum MyTypeV2 {
				#[versioned_type(extend)]
				Variant1 {
					field3: SomeType3,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field1", "field2", "field3"]);
	}

	#[test]
	fn enum_after_previous_struct_keeps_standalone_variant_independent() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyTypeV1 {
				pub field1: SomeType,
				pub field2: SomeType2,
			}

			pub enum MyTypeV2 {
				Variant1 {
					field3: SomeType3,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(field_names, vec!["field3"]);
	}

	#[test]
	fn enum_rejects_variant_override_from_previous_struct() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyTypeV1 {
				pub field1: SomeType,
			}

			pub enum MyTypeV2 {
				#[versioned_type(override)]
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected variant override from previous struct to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant1` is marked as an override"));
		assert!(message.contains("no variant with that name exists"));
	}

	#[test]
	fn enum_rejects_variant_override_and_extend_from_previous_struct() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyTypeV1 {
				pub field1: SomeType,
			}

			pub enum MyTypeV2 {
				#[versioned_type(override, extend)]
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected variant override from previous struct to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("variant `Variant1` is marked as an override"));
		assert!(message.contains("no variant with that name exists"));
	}

	#[test]
	fn enum_variant_extension_from_struct_removes_copied_field_visibility() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyTypeV1 {
				pub field1: SomeType,
			}

			pub enum MyTypeV2 {
				#[versioned_type(extend)]
				Variant1,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let DefineVersionedTypeItem::Enum(item) = &output[1] else {
			panic!("expected second item to be an enum");
		};
		let variant = item.variants.iter().next().unwrap();
		let Fields::Named(fields) = &variant.fields else {
			panic!("expected named variant fields");
		};
		assert!(matches!(fields.named[0].vis, Visibility::Inherited));
	}

	#[test]
	fn enum_extension_from_previous_struct_errors() {
		// Arrange
		let tokens = quote::quote! {
			pub struct MyEnumV1 {
				pub field1: SomeType,
			}

			#[versioned_type(extend)]
			pub enum MyEnumV2 {
				Variant1 {
					field2: SomeType2,
				},
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected enum extension from struct to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("enum can't be extended from a struct"));
		assert!(message.contains("previous type"));
	}

	#[test]
	fn struct_extension_from_previous_enum_errors() {
		// Arrange
		let tokens = quote::quote! {
			pub enum MyTypeV1 {
				Variant1 {
					field1: SomeType,
				},
			}

			#[versioned_type(extend)]
			pub struct MyTypeV2 {
				pub field2: SomeType2,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let error = match handle_define_versioned_type(input) {
			Ok(_) => panic!("expected struct extension from enum to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("struct can't be extended from an enum"));
		assert!(message.contains("previous type"));
	}

	#[test]
	fn rejects_name_without_version_suffix() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLog {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("must end with `V`"));
	}

	#[test]
	fn rejects_empty_version_suffix() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("positive integer after the `V`"));
	}

	#[test]
	fn rejects_non_numeric_version_suffix() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogVLatest {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("positive integer"));
	}

	#[test]
	fn rejects_zero_version_suffix() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV0 {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("must start at 1"));
	}

	#[test]
	fn rejects_version_suffix_with_leading_zero() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV01 {
				pub item: u32,
			}
		};
		let input = parse2::<DefineVersionedTypeItem>(tokens).unwrap();

		// Act
		let error = input.name_and_version().unwrap_err();

		// Assert
		assert!(error.to_string().contains("must not contain leading zeros"));
	}

	#[test]
	fn rejects_duplicate_versions_with_descriptive_error() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item: u32,
			}

			pub enum CallLogV1 {
				Call,
			}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeInput>(tokens) {
			Ok(_) => panic!("expected duplicate version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("duplicate version V1"));
		assert!(message.contains("already defined"));
	}

	#[test]
	fn rejects_duplicate_versions_when_definitions_are_not_adjacent() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item1: u8,
			}

			pub struct CallLogV2 {
				pub item2: u16,
			}

			pub enum CallLogV1 {
				Variant,
			}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeInput>(tokens) {
			Ok(_) => panic!("expected duplicate version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("duplicate version V1"));
		assert!(message.contains("already defined"));
	}

	#[test]
	fn rejects_items_with_different_base_names() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item: u32,
			}

			pub struct SomeOtherLogV2 {
				pub item: u64,
			}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeInput>(tokens) {
			Ok(_) => panic!("expected mixed type names to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("must define versions of the same type"));
		assert!(message.contains("SomeOtherLog"));
		assert!(message.contains("CallLog"));
	}

	#[test]
	fn allows_contiguous_versions_that_do_not_start_at_one() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV3 {
				pub item: u32,
			}

			pub struct CallLogV4 {
				pub item: u64,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Assert
		assert_eq!(input.definitions.len(), 2);
		assert_eq!(input.name, Some("CallLog".to_string()));
		assert_eq!(input.highest_version.map(|version| version.value()), Some(4));
	}

	#[test]
	fn allows_empty_input() {
		// Arrange
		let tokens = quote::quote! {};

		// Act
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Assert
		assert_eq!(input.name, None);
		assert!(input.definitions.is_empty());
	}

	#[test]
	fn allows_single_version_that_does_not_start_at_one() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV9 {
				pub item: u32,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Assert
		assert_eq!(input.name, Some("CallLog".to_string()));
		assert_eq!(input.highest_version.map(|version| version.value()), Some(9));
		assert_eq!(input.definitions.len(), 1);
		assert!(input.definitions.keys().any(|version| version.value() == 9));
	}

	#[test]
	fn allows_contiguous_versions_defined_out_of_source_order() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV4 {
				pub item: u64,
			}

			pub struct CallLogV3 {
				pub item: u32,
			}
		};

		// Act
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Assert
		let versions = input.definitions.keys().map(|version| version.value()).collect::<Vec<_>>();
		assert_eq!(input.highest_version.map(|version| version.value()), Some(4));
		assert_eq!(versions, vec![3, 4]);
	}

	#[test]
	fn output_emits_latest_alias_for_highest_version() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1 {
				pub item1: u8,
			}

			#[versioned_type(extend)]
			pub struct CallLogV2 {
				pub item2: u16,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();
		let output_tokens = quote::quote!(#output).to_string();

		// Assert
		assert!(output_tokens.contains("pub type LatestCallLog = CallLogV2 ;"));
	}

	#[test]
	fn output_emits_encode_like_impls_for_struct_type_paths() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(encode_like = "Bytes; Vec<u8>")]
			pub struct PristineCodeV1(pub Bytes);
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();
		let output_tokens = quote::quote!(#output).to_string();

		// Assert
		assert!(output_tokens.contains(
			"impl :: codec :: EncodeLike < PristineCodeV1 > for Bytes"
		));
		assert!(output_tokens.contains(
			"impl :: codec :: EncodeLike < PristineCodeV1 > for Vec < u8 >"
		));
		assert!(!output_tokens.contains("versioned_type"));
	}

	#[test]
	fn output_emits_encode_like_impls_for_enum_type_paths() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(encode_like = "u8")]
			pub enum StatusV1 {
				Enabled,
				Disabled,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();
		let output_tokens = quote::quote!(#output).to_string();

		// Assert
		assert!(output_tokens.contains("impl :: codec :: EncodeLike < StatusV1 > for u8"));
		assert!(!output_tokens.contains("versioned_type"));
	}

	#[test]
	fn output_emits_encode_like_impls_with_item_generics() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(encode_like = "RawWrapped<T>")]
			pub struct WrappedV1<T>(pub T);
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();
		let output_tokens = quote::quote!(#output).to_string();

		// Assert
		assert!(output_tokens.contains(
			"impl < T > :: codec :: EncodeLike < WrappedV1 < T > > for RawWrapped < T >"
		));
		assert!(output_tokens.contains("WrappedV1 < T > : :: codec :: Encode"));
		assert!(output_tokens.contains("RawWrapped < T > : :: codec :: Encode"));
	}

	#[test]
	fn latest_alias_does_not_emit_unenforced_generic_bounds() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV1<T: Clone>
			where
				T: Default,
			{
				pub item: T,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();
		let latest_alias = output.latest_alias.as_ref().unwrap();

		// Assert
		assert!(latest_alias.item.generics.where_clause.is_none());
		assert!(latest_alias.item.generics.params.iter().all(|param| match param {
			GenericParam::Type(param) => param.bounds.is_empty(),
			GenericParam::Lifetime(param) => param.bounds.is_empty(),
			GenericParam::Const(_) => true,
		}));
	}

	#[test]
	fn handler_extends_versions_in_numeric_order_when_source_order_differs() {
		// Arrange
		let tokens = quote::quote! {
			#[versioned_type(extend)]
			pub struct CallLogV4 {
				pub item2: u16,
			}

			pub struct CallLogV3 {
				pub item1: u8,
			}
		};
		let input = parse2::<DefineVersionedTypeInput>(tokens).unwrap();

		// Act
		let output = handle_define_versioned_type(input).unwrap();

		// Assert
		let item_names = output.iter().map(|item| item.ident().to_string()).collect::<Vec<_>>();
		let DefineVersionedTypeItem::Struct(item) = &output[1] else {
			panic!("expected second item to be a struct");
		};
		let Fields::Named(fields) = &item.fields else {
			panic!("expected named fields");
		};
		let field_names = fields
			.named
			.iter()
			.map(|field| field.ident.as_ref().unwrap().to_string())
			.collect::<Vec<_>>();
		assert_eq!(item_names, vec!["CallLogV3", "CallLogV4"]);
		assert_eq!(field_names, vec!["item1", "item2"]);
	}

	#[test]
	fn rejects_missing_single_version_between_definitions() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV3 {
				pub item: u32,
			}

			pub struct CallLogV5 {
				pub item: u64,
			}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeInput>(tokens) {
			Ok(_) => panic!("expected missing version to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("must be contiguous"));
		assert!(message.contains("missing version V4 before V5"));
	}

	#[test]
	fn rejects_missing_version_range_between_definitions() {
		// Arrange
		let tokens = quote::quote! {
			pub struct CallLogV3 {
				pub item: u32,
			}

			pub struct CallLogV7 {
				pub item: u64,
			}
		};

		// Act
		let error = match parse2::<DefineVersionedTypeInput>(tokens) {
			Ok(_) => panic!("expected missing version range to fail"),
			Err(error) => error,
		};

		// Assert
		let message = error.to_string();
		assert!(message.contains("must be contiguous"));
		assert!(message.contains("missing versions V4..V6 before V7"));
	}
}
