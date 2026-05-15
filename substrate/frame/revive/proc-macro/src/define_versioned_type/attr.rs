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
	ext::IdentExt,
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	spanned::Spanned,
	Attribute, Ident, Meta, Result, Token,
};

use super::*;

#[derive(Clone, Default)]
pub struct RawVersioningAttribute {
	pub extend: Option<Span>,
	pub r#override: Option<Span>,
}

impl RawVersioningAttribute {
	pub fn take<T: TryFrom<Self, Error = syn::Error>>(
		attributes: &mut Vec<Attribute>,
	) -> Result<Vec<T>> {
		let mut kept = Vec::with_capacity(attributes.len());
		let mut versioned = Vec::new();

		for attribute in core::mem::take(attributes) {
			match Self::from_attribute(&attribute).transpose()? {
				Some(attribute) => versioned.push(attribute.try_into()?),
				None => kept.push(attribute),
			}
		}

		*attributes = kept;
		Ok(versioned)
	}
}

#[derive(Default)]
pub struct FieldVersioningAttribute {
	pub r#override: Option<Span>,
}

impl FieldVersioningAttribute {
	pub fn take(attributes: &mut Vec<Attribute>) -> Result<Self> {
		RawVersioningAttribute::take::<Self>(attributes).map(|attributes| {
			attributes.into_iter().fold(Self::default(), |mut folded, attribute| {
				if let Some(span) = attribute.r#override {
					folded.r#override.get_or_insert(span);
				}

				folded
			})
		})
	}
}

#[derive(Default)]
pub struct StructVersioningAttribute {
	pub extend: Option<Span>,
}

impl StructVersioningAttribute {
	pub fn take(attributes: &mut Vec<Attribute>) -> Result<Self> {
		RawVersioningAttribute::take::<Self>(attributes).map(|attributes| {
			attributes.into_iter().fold(Self::default(), |mut folded, attribute| {
				if let Some(span) = attribute.extend {
					folded.extend.get_or_insert(span);
				}

				folded
			})
		})
	}
}

#[derive(Default)]
pub struct EnumVersioningAttribute {
	pub extend: Option<Span>,
}

impl EnumVersioningAttribute {
	pub fn take(attributes: &mut Vec<Attribute>) -> Result<Self> {
		RawVersioningAttribute::take::<Self>(attributes).map(|attributes| {
			attributes.into_iter().fold(Self::default(), |mut folded, attribute| {
				if let Some(span) = attribute.extend {
					folded.extend.get_or_insert(span);
				}

				folded
			})
		})
	}
}

#[derive(Clone, Copy, Default)]
pub enum VariantVersioningAttribute {
	#[default]
	None,
	Extend(Span),
	Override(Span),
}

impl VariantVersioningAttribute {
	pub fn take(attributes: &mut Vec<Attribute>) -> Result<Self> {
		RawVersioningAttribute::take::<Self>(attributes).and_then(|attributes| {
			attributes.into_iter().try_fold(Self::None, |folded, attribute| {
				match (folded, attribute) {
					(Self::None, attribute) | (attribute, Self::None) => Ok(attribute),
					(Self::Extend(span), Self::Extend(_)) => Ok(Self::Extend(span)),
					(Self::Override(span), Self::Override(_)) => Ok(Self::Override(span)),
					(Self::Extend(extend_span), Self::Override(override_span)) => {
						bail! {
							override_span => "`override` can't be combined with `extend` on \
								the same variant",
							extend_span => "This variant was already marked as `extend` here"
						}
					},
					(Self::Override(override_span), Self::Extend(extend_span)) => {
						bail! {
							extend_span => "`extend` can't be combined with `override` on \
								the same variant",
							override_span => "This variant was already marked as `override` here"
						}
					},
				}
			})
		})
	}
}

impl RawVersioningAttribute {
	pub fn from_attribute(attribute: &Attribute) -> Option<Result<Self>> {
		if !attribute.path().is_ident("versioned_type") {
			return None;
		}

		Some(if let Meta::List(ref meta) = attribute.meta {
			meta.parse_args()
		} else {
			Err(syn_error! {
				attribute.span() => "`versioned_type` requires options. E.g, use \
					`#[versioned_type(extend)]`",
			})
		})
	}
}

impl Parse for RawVersioningAttribute {
	fn parse(input: ParseStream) -> Result<Self> {
		Punctuated::<VersioningOption, Token![,]>::parse_terminated(input).map(|punct| {
			punct.into_iter().fold(Self::default(), |mut attribute, option| {
				match option {
					VersioningOption::Extend(span) => {
						attribute.extend.get_or_insert(span);
					},
					VersioningOption::Override(span) => {
						attribute.r#override.get_or_insert(span);
					},
				}

				attribute
			})
		})
	}
}

enum VersioningOption {
	Extend(Span),
	Override(Span),
}

impl Parse for VersioningOption {
	fn parse(input: ParseStream) -> Result<Self> {
		let ident = input.call(Ident::parse_any)?;
		let name = ident.to_string();
		let span = ident.span();

		if input.peek(Token![=]) || input.peek(syn::token::Paren) {
			bail! {
				span => format!("`{name}` does not accept arguments; use `#[versioned_type({name})]`")
			}
		}

		match name.as_str() {
			"extend" => Ok(Self::Extend(span)),
			"override" => Ok(Self::Override(span)),
			_ => Err(syn::Error::new(
				span,
				"unsupported versioned option; currently only `extend` and \
				`override` are supported",
			)),
		}
	}
}

impl TryFrom<RawVersioningAttribute> for FieldVersioningAttribute {
	type Error = syn::Error;

	fn try_from(value: RawVersioningAttribute) -> core::result::Result<Self, Self::Error> {
		if let Some(span) = value.extend {
			bail! {
				span => "`extend` is not supported on fields; use `#[versioned_type(override)]` \
					to replace an existing field"
			}
		}

		Ok(Self { r#override: value.r#override })
	}
}

impl TryFrom<RawVersioningAttribute> for StructVersioningAttribute {
	type Error = syn::Error;

	fn try_from(value: RawVersioningAttribute) -> core::result::Result<Self, Self::Error> {
		if let Some(span) = value.r#override {
			bail! {
				span => "`override` is not supported on field groups; use \
					`#[versioned_type(extend)]` to extend a field group"
			}
		}

		Ok(Self { extend: value.extend })
	}
}

impl TryFrom<RawVersioningAttribute> for EnumVersioningAttribute {
	type Error = syn::Error;

	fn try_from(value: RawVersioningAttribute) -> core::result::Result<Self, Self::Error> {
		if let Some(span) = value.r#override {
			bail! {
				span => "`override` is not supported on enums; use `#[versioned_type(extend)]` \
					to extend an enum"
			}
		}

		Ok(Self { extend: value.extend })
	}
}

impl TryFrom<RawVersioningAttribute> for VariantVersioningAttribute {
	type Error = syn::Error;

	fn try_from(value: RawVersioningAttribute) -> core::result::Result<Self, Self::Error> {
		match (value.extend, value.r#override) {
			(None, None) => Ok(Self::None),
			(Some(span), None) => Ok(Self::Extend(span)),
			(None, Some(span)) => Ok(Self::Override(span)),
			(Some(extend_span), Some(override_span)) => {
				bail! {
					override_span => "`override` can't be combined with `extend` on the same \
						variant",
					extend_span => "This variant was already marked as `extend` here"
				}
			},
		}
	}
}
