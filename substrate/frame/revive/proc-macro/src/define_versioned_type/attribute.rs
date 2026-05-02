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

use proc_macro2::Span;
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	spanned::Spanned,
	token::{Comma, Semi},
	Attribute, Field, LitStr, Meta, Result, Token, TypePath, Variant,
};

/// The result of removing `versioned_type` attributes from an item.
///
/// Attribute parsing always has two products: the typed helper attribute used by the macro
/// implementation and every unrelated attribute that should be preserved on the generated Rust
/// item.
pub(super) struct AttributeSplit<T> {
	/// The parsed macro helper attribute for the current syntax context.
	pub(super) versioned_type: T,

	/// Every attribute that should remain attached to the Rust syntax node.
	pub(super) other_attributes: Vec<Attribute>,
}

/// The raw parsed shape of a `versioned_type` helper attribute.
///
/// This type intentionally does not decide whether `extend` or `override` is valid in a given
/// location. It only records which options were present so the context-specific attribute type can
/// perform the final validation step.
#[derive(Default)]
struct RawVersionedTypeAttribute {
	/// The span of the `extend` option when it was supplied.
	extend: Option<Span>,

	/// The span of the `override` option when it was supplied.
	r#override: Option<Span>,

	/// The types that should encode like the current versioned type.
	encode_like: Option<EncodeLikeTypes>,
}

impl RawVersionedTypeAttribute {
	/// Parses all `versioned_type` attributes and returns the rest.
	fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let mut versioned_type = Self::default();
		let mut other_attributes = Vec::<Attribute>::with_capacity(attributes.len());

		for attribute in attributes {
			if !attribute.path().is_ident("versioned_type") {
				other_attributes.push(attribute);
				continue;
			}

			versioned_type.parse_attribute(attribute)?;
		}

		Ok(AttributeSplit { versioned_type, other_attributes })
	}

	/// Parses one `versioned_type` attribute into this accumulator.
	fn parse_attribute(&mut self, attribute: Attribute) -> Result<()> {
		match &attribute.meta {
			Meta::List(_) => attribute.parse_nested_meta(|meta| {
				if meta.path.is_ident("extend") {
					Self::reject_option_arguments(&meta, "extend")?;
					self.set_extend(meta.path.span())
				} else if meta.path.is_ident("override") {
					Self::reject_option_arguments(&meta, "override")?;
					self.set_override(meta.path.span())
				} else if meta.path.is_ident("encode_like") {
					self.set_encode_like(meta.path.span(), meta.value()?.parse()?)
				} else {
					Err(meta.error(
						"unsupported versioned_type option; currently only `extend`, \
                        `override`, and `encode_like` are supported",
					))
				}
			}),
			Meta::Path(_) => Err(syn::Error::new_spanned(
				&attribute,
				"`versioned_type` requires options; use `#[versioned_type(extend)]`",
			)),
			Meta::NameValue(_) => Err(syn::Error::new_spanned(
				&attribute,
				"`versioned_type` does not support name-value syntax; use \
                `#[versioned_type(extend)]`",
			)),
		}
	}

	/// Ensures an option was written as a flag rather than a value or list.
	fn reject_option_arguments(
		meta: &syn::meta::ParseNestedMeta<'_>,
		option_name: &str,
	) -> Result<()> {
		if meta.input.peek(Token![=]) || meta.input.peek(syn::token::Paren) {
			return Err(meta.error(format!(
				"`{option_name}` does not accept arguments; use \
                `#[versioned_type({option_name})]`"
			)));
		}

		Ok(())
	}

	/// Records an `extend` option and rejects duplicate occurrences.
	fn set_extend(&mut self, span: Span) -> Result<()> {
		if let Some(first_span) = self.extend {
			return Err(Self::duplicate_option_error("extend", span, first_span));
		}

		self.extend = Some(span);
		Ok(())
	}

	/// Records an `override` option and rejects duplicate occurrences.
	fn set_override(&mut self, span: Span) -> Result<()> {
		if let Some(first_span) = self.r#override {
			return Err(Self::duplicate_option_error("override", span, first_span));
		}

		self.r#override = Some(span);
		Ok(())
	}

	/// Records an `encode_like` option and rejects duplicate occurrences.
	fn set_encode_like(&mut self, span: Span, literal: LitStr) -> Result<()> {
		if let Some(first) = &self.encode_like {
			return Err(Self::duplicate_option_error("encode_like", span, first.span));
		}

		self.encode_like = Some(EncodeLikeTypes::parse_literal(literal)?);
		Ok(())
	}

	/// Builds a diagnostic for a repeated `versioned_type` option.
	fn duplicate_option_error(
		option_name: &str,
		duplicate_span: Span,
		first_span: Span,
	) -> syn::Error {
		let mut error =
			syn::Error::new(duplicate_span, format!("`{option_name}` is specified more than once"));
		error.combine(syn::Error::new(
			first_span,
			format!("the first `{option_name}` option was specified here"),
		));
		error
	}
}

/// The list of type paths passed to `versioned_type(encode_like = "...")`.
pub(super) struct EncodeLikeTypes {
	/// The span of the `encode_like` option used for diagnostics.
	span: Span,

	/// The semicolon-separated type paths parsed from the literal string.
	types: Vec<TypePath>,
}

impl EncodeLikeTypes {
	/// Parses the literal string value supplied to `encode_like`.
	fn parse_literal(literal: LitStr) -> Result<Self> {
		let span = literal.span();
		let parsed = literal.parse::<EncodeLikeTypeList>()?;
		Ok(Self { span, types: parsed.types.into_iter().collect() })
	}

	/// Returns the parsed type paths.
	#[must_use]
	pub(super) fn types(&self) -> &[TypePath] {
		&self.types
	}
}

/// Parser for semicolon-separated type paths inside an `encode_like` literal.
struct EncodeLikeTypeList {
	/// The parsed type paths.
	types: Punctuated<TypePath, Semi>,
}

impl Parse for EncodeLikeTypeList {
	/// Parses one or more type paths separated by semicolons.
	fn parse(input: ParseStream) -> Result<Self> {
		let types = Punctuated::<TypePath, Semi>::parse_separated_nonempty(input)?;

		if !input.is_empty() {
			return Err(input.error(
				"`encode_like` expects a semicolon-separated list of type paths, for example \
                `Bytes; Vec<u8>`",
			));
		}

		Ok(Self { types })
	}
}

/// The parsed helper attribute for struct and enum items.
///
/// Types support `extend` and optional `encode_like` declarations. The absence of `extend` is
/// represented as `Standalone`, which makes the item independent from the previous version.
pub(super) struct TypeVersionedTypeAttribute {
	/// The validated type-level mode requested by the user.
	mode: TypeVersionedTypeMode,

	/// The types that should encode like this versioned type.
	encode_like: Option<EncodeLikeTypes>,
}

impl TypeVersionedTypeAttribute {
	/// Parses item attributes and removes the `versioned_type` helper.
	pub(super) fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let AttributeSplit { versioned_type: raw, other_attributes } =
			RawVersionedTypeAttribute::parse_and_split(attributes)?;

		if let Some(override_span) = raw.r#override {
			return Err(syn::Error::new(
				override_span,
				"`override` is not supported on types; use \
                `#[versioned_type(extend)]` to extend a type",
			));
		}

		let mode = match raw.extend {
			Some(span) => TypeVersionedTypeMode::Extend { span },
			None => TypeVersionedTypeMode::Standalone,
		};

		Ok(AttributeSplit {
			versioned_type: Self { mode, encode_like: raw.encode_like },
			other_attributes,
		})
	}

	/// Returns the validated type-level mode.
	#[must_use]
	pub(super) fn mode(&self) -> TypeVersionedTypeMode {
		self.mode
	}

	/// Returns the type paths that should encode like this versioned type.
	#[must_use]
	pub(super) fn encode_like(&self) -> Option<&EncodeLikeTypes> {
		self.encode_like.as_ref()
	}
}

/// The type-level relationship requested by `versioned_type`.
#[derive(Clone, Copy)]
pub(super) enum TypeVersionedTypeMode {
	/// The item is defined independently from the previous version.
	Standalone,

	/// The item should extend the immediately previous version.
	Extend {
		/// The span of the `extend` option used for diagnostics.
		span: Span,
	},
}

/// The parsed helper attribute for enum variants.
///
/// Variants support both `extend` and `override`. Supplying both means that the variant replaces
/// the previous variant while also extending its fields.
pub(super) struct VariantVersionedTypeAttribute {
	/// The validated variant-level mode requested by the user.
	mode: VariantVersionedTypeMode,
}

impl VariantVersionedTypeAttribute {
	/// Parses variant attributes and removes the `versioned_type` helper.
	pub(super) fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let AttributeSplit { versioned_type: raw, other_attributes } =
			RawVersionedTypeAttribute::parse_and_split(attributes)?;

		if let Some(encode_like) = raw.encode_like {
			return Err(syn::Error::new(
				encode_like.span,
				"`encode_like` is not supported on variants; use it on a struct or enum item",
			));
		}

		let mode = match (raw.extend, raw.r#override) {
			(None, None) => VariantVersionedTypeMode::Standalone,
			(Some(span), None) => VariantVersionedTypeMode::Extend { span },
			(None, Some(span)) => VariantVersionedTypeMode::Override { span },
			(Some(extend_span), Some(override_span)) => {
				VariantVersionedTypeMode::OverrideAndExtend { override_span, extend_span }
			},
		};

		Ok(AttributeSplit { versioned_type: Self { mode }, other_attributes })
	}

	/// Returns the validated variant-level mode.
	#[must_use]
	pub(super) fn mode(&self) -> VariantVersionedTypeMode {
		self.mode
	}
}

/// The variant-level relationship requested by `versioned_type`.
#[derive(Clone, Copy)]
pub(super) enum VariantVersionedTypeMode {
	/// The variant is a fresh definition in the current enum.
	Standalone,

	/// The variant should merge its fields with the previous variant.
	Extend {
		/// The span of the `extend` option used for diagnostics.
		span: Span,
	},

	/// The variant should replace a previous variant with the same name.
	Override {
		/// The span of the `override` option used for diagnostics.
		span: Span,
	},

	/// The variant should replace a previous variant and extend its fields.
	OverrideAndExtend {
		/// The span of the `override` option used for diagnostics.
		override_span: Span,

		/// The span of the `extend` option used for diagnostics.
		extend_span: Span,
	},
}

/// A variant paired with its parsed and stripped helper attribute.
pub(super) struct VariantWithVersionedTypeAttribute {
	/// The variant after removing any `versioned_type` helper attribute.
	pub(super) variant: Variant,

	/// The parsed variant-level helper attribute.
	pub(super) attribute: VariantVersionedTypeAttribute,
}

impl VariantWithVersionedTypeAttribute {
	/// Parses and strips helper attributes from every variant in a list.
	pub(super) fn parse_all(variants: Punctuated<Variant, Comma>) -> Result<Vec<Self>> {
		variants.into_iter().map(Self::parse).collect::<Result<Vec<Self>>>()
	}

	/// Parses and strips helper attributes from one variant.
	fn parse(mut variant: Variant) -> Result<Self> {
		let AttributeSplit { versioned_type: attribute, other_attributes } =
			VariantVersionedTypeAttribute::parse_and_split(core::mem::take(&mut variant.attrs))?;
		variant.attrs = other_attributes;

		Ok(Self { variant, attribute })
	}
}

/// The parsed helper attribute for struct fields and variant fields.
///
/// Fields only support `override`. Field extension is controlled by the item or variant that owns
/// the fields, not by each field independently.
pub(super) struct FieldVersionedTypeAttribute {
	/// The validated field-level mode requested by the user.
	mode: FieldVersionedTypeMode,
}

impl FieldVersionedTypeAttribute {
	/// Parses field attributes and removes the `versioned_type` helper.
	pub(super) fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let AttributeSplit { versioned_type: raw, other_attributes } =
			RawVersionedTypeAttribute::parse_and_split(attributes)?;

		if let Some(encode_like) = raw.encode_like {
			return Err(syn::Error::new(
				encode_like.span,
				"`encode_like` is not supported on fields; use it on a struct or enum item",
			));
		}

		if let Some(extend_span) = raw.extend {
			return Err(syn::Error::new(
				extend_span,
				"`extend` is not supported on fields; use \
                `#[versioned_type(override)]` to replace an existing field",
			));
		}

		let mode = match raw.r#override {
			Some(span) => FieldVersionedTypeMode::Override { span },
			None => FieldVersionedTypeMode::Inherited,
		};

		Ok(AttributeSplit { versioned_type: Self { mode }, other_attributes })
	}

	/// Returns the span of the field override option when one exists.
	#[must_use]
	pub(super) fn override_span(&self) -> Option<Span> {
		match self.mode {
			FieldVersionedTypeMode::Inherited => None,
			FieldVersionedTypeMode::Override { span } => Some(span),
		}
	}
}

/// The field-level operation requested by `versioned_type`.
#[derive(Clone, Copy)]
pub(super) enum FieldVersionedTypeMode {
	/// The field is a regular field with no helper operation.
	Inherited,

	/// The field should replace a named field from the previous version.
	Override {
		/// The span of the `override` option used for diagnostics.
		span: Span,
	},
}

/// A field paired with its parsed and stripped helper attribute.
pub(super) struct FieldWithVersionedTypeAttribute {
	/// The field after removing any `versioned_type` helper attribute.
	pub(super) field: Field,

	/// The parsed field-level helper attribute.
	pub(super) attribute: FieldVersionedTypeAttribute,
}

impl FieldWithVersionedTypeAttribute {
	/// Parses and strips helper attributes from every field in a list.
	pub(super) fn parse_all(fields: Punctuated<Field, Comma>) -> Result<Vec<Self>> {
		fields.into_iter().map(Self::parse).collect::<Result<Vec<Self>>>()
	}

	/// Parses and strips helper attributes from one field.
	fn parse(mut field: Field) -> Result<Self> {
		let AttributeSplit { versioned_type: attribute, other_attributes } =
			FieldVersionedTypeAttribute::parse_and_split(core::mem::take(&mut field.attrs))?;
		field.attrs = other_attributes;

		Ok(Self { field, attribute })
	}
}
