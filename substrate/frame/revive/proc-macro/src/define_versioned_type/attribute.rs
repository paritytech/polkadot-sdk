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
	punctuated::Punctuated, spanned::Spanned, token::Comma, Attribute, Field, Meta, Result, Token,
	Variant,
};

pub(super) struct AttributeSplit<T> {
	pub(super) versioned_type: T,

	pub(super) other_attributes: Vec<Attribute>,
}

#[derive(Default)]
struct RawVersionedTypeAttribute {
	extend: Option<Span>,

	r#override: Option<Span>,
}

impl RawVersionedTypeAttribute {
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

	fn parse_attribute(&mut self, attribute: Attribute) -> Result<()> {
		match &attribute.meta {
			Meta::List(_) => attribute.parse_nested_meta(|meta| {
				if meta.path.is_ident("extend") {
					Self::reject_option_arguments(&meta, "extend")?;
					self.set_extend(meta.path.span())
				} else if meta.path.is_ident("override") {
					Self::reject_option_arguments(&meta, "override")?;
					self.set_override(meta.path.span())
				} else {
					Err(meta.error(
						"unsupported versioned_type option; currently only `extend` and \
                        `override` are supported",
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

	fn set_extend(&mut self, span: Span) -> Result<()> {
		if let Some(first_span) = self.extend {
			return Err(Self::duplicate_option_error("extend", span, first_span));
		}

		self.extend = Some(span);
		Ok(())
	}

	fn set_override(&mut self, span: Span) -> Result<()> {
		if let Some(first_span) = self.r#override {
			return Err(Self::duplicate_option_error("override", span, first_span));
		}

		self.r#override = Some(span);
		Ok(())
	}

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

pub(super) struct TypeVersionedTypeAttribute {
	mode: TypeVersionedTypeMode,
}

impl TypeVersionedTypeAttribute {
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

		Ok(AttributeSplit { versioned_type: Self { mode }, other_attributes })
	}

	#[must_use]
	pub(super) fn mode(&self) -> TypeVersionedTypeMode {
		self.mode
	}
}

#[derive(Clone, Copy)]
pub(super) enum TypeVersionedTypeMode {
	Standalone,

	Extend { span: Span },
}

pub(super) struct VariantVersionedTypeAttribute {
	mode: VariantVersionedTypeMode,
}

impl VariantVersionedTypeAttribute {
	pub(super) fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let AttributeSplit { versioned_type: raw, other_attributes } =
			RawVersionedTypeAttribute::parse_and_split(attributes)?;

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

	#[must_use]
	pub(super) fn mode(&self) -> VariantVersionedTypeMode {
		self.mode
	}
}

#[derive(Clone, Copy)]
pub(super) enum VariantVersionedTypeMode {
	Standalone,

	Extend { span: Span },

	Override { span: Span },

	OverrideAndExtend { override_span: Span, extend_span: Span },
}

pub(super) struct VariantWithVersionedTypeAttribute {
	pub(super) variant: Variant,

	pub(super) attribute: VariantVersionedTypeAttribute,
}

impl VariantWithVersionedTypeAttribute {
	pub(super) fn parse_all(variants: Punctuated<Variant, Comma>) -> Result<Vec<Self>> {
		variants.into_iter().map(Self::parse).collect::<Result<Vec<Self>>>()
	}

	fn parse(mut variant: Variant) -> Result<Self> {
		let AttributeSplit { versioned_type: attribute, other_attributes } =
			VariantVersionedTypeAttribute::parse_and_split(core::mem::take(&mut variant.attrs))?;
		variant.attrs = other_attributes;

		Ok(Self { variant, attribute })
	}
}

pub(super) struct FieldVersionedTypeAttribute {
	mode: FieldVersionedTypeMode,
}

impl FieldVersionedTypeAttribute {
	pub(super) fn parse_and_split(attributes: Vec<Attribute>) -> Result<AttributeSplit<Self>> {
		let AttributeSplit { versioned_type: raw, other_attributes } =
			RawVersionedTypeAttribute::parse_and_split(attributes)?;

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

	#[must_use]
	pub(super) fn override_span(&self) -> Option<Span> {
		match self.mode {
			FieldVersionedTypeMode::Inherited => None,
			FieldVersionedTypeMode::Override { span } => Some(span),
		}
	}
}

#[derive(Clone, Copy)]
pub(super) enum FieldVersionedTypeMode {
	Inherited,

	Override { span: Span },
}

pub(super) struct FieldWithVersionedTypeAttribute {
	pub(super) field: Field,

	pub(super) attribute: FieldVersionedTypeAttribute,
}

impl FieldWithVersionedTypeAttribute {
	pub(super) fn parse_all(fields: Punctuated<Field, Comma>) -> Result<Vec<Self>> {
		fields.into_iter().map(Self::parse).collect::<Result<Vec<Self>>>()
	}

	fn parse(mut field: Field) -> Result<Self> {
		let AttributeSplit { versioned_type: attribute, other_attributes } =
			FieldVersionedTypeAttribute::parse_and_split(core::mem::take(&mut field.attrs))?;
		field.attrs = other_attributes;

		Ok(Self { field, attribute })
	}
}
