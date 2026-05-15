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

use indexmap::IndexMap;
use syn::{
	punctuated::Punctuated, Attribute, Expr, Generics, Ident, ItemEnum, ItemStruct, Token, Variant,
	Visibility,
};

use super::*;

#[derive(Clone)]
pub enum StructuredTypeDefinition {
	Struct(StructuredItemStruct),
	Enum(StructuredItemEnum),
}

#[derive(Clone)]
pub struct StructuredItemStruct {
	pub attributes: Vec<Attribute>,
	pub visibility: Visibility,
	pub ident: Ident,
	pub generics: Generics,
	pub fields: StructuredFields,
}

#[derive(Clone)]
pub struct StructuredItemEnum {
	pub attributes: Vec<Attribute>,
	pub visibility: Visibility,
	pub ident: Ident,
	pub generics: Generics,
	pub variants: StructuredEnumVariants,
}

#[derive(Clone, Default)]
pub struct StructuredEnumVariants {
	pub variants: IndexMap<Ident, StructuredEnumVariant>,
}

#[derive(Clone)]
pub struct StructuredEnumVariant {
	pub attributes: Vec<Attribute>,
	pub ident: Ident,
	pub fields: StructuredFields,
	pub discriminant: Option<Expr>,
}

// ========
// Helpers
// ========

impl StructuredTypeDefinition {
	pub fn ident_ref(&self) -> &Ident {
		match self {
			StructuredTypeDefinition::Struct(item) => &item.ident,
			StructuredTypeDefinition::Enum(item) => &item.ident,
		}
	}

	pub fn generics_ref(&self) -> &Generics {
		match self {
			StructuredTypeDefinition::Struct(item) => &item.generics,
			StructuredTypeDefinition::Enum(item) => &item.generics,
		}
	}
}

// ===========================
// Conversion Implementations
// ===========================

impl TryFrom<ItemStruct> for StructuredItemStruct {
	type Error = syn::Error;

	fn try_from(value: ItemStruct) -> Result<Self, Self::Error> {
		Ok(Self {
			attributes: value.attrs,
			visibility: value.vis,
			ident: value.ident,
			generics: value.generics,
			fields: value.fields.try_into()?,
		})
	}
}

impl TryFrom<ItemEnum> for StructuredItemEnum {
	type Error = syn::Error;

	fn try_from(value: ItemEnum) -> Result<Self, Self::Error> {
		let variants = value.variants.into_iter().map(StructuredEnumVariant::try_from).try_fold(
			IndexMap::new(),
			|mut variants, variant| {
				let variant = variant?;
				if let Err(output) = variants.bounce_insert(variant.ident.clone(), variant) {
					return Err(syn_error! {
						output.attempted_insert_value.ident.span() =>
							"A variant with the same identifier is already defined",
						output.existing_value.ident.span() =>
							"Existing variant with the same identifier is defined here",
					});
				}
				Ok(variants)
			},
		)?;

		Ok(Self {
			attributes: value.attrs,
			visibility: value.vis,
			ident: value.ident,
			generics: value.generics,
			variants: StructuredEnumVariants { variants },
		})
	}
}

impl TryFrom<Variant> for StructuredEnumVariant {
	type Error = syn::Error;

	fn try_from(value: Variant) -> Result<Self, Self::Error> {
		Ok(Self {
			attributes: value.attrs,
			ident: value.ident,
			fields: value.fields.try_into()?,
			discriminant: value.discriminant.map(|v| v.1),
		})
	}
}

impl From<StructuredItemStruct> for ItemStruct {
	fn from(value: StructuredItemStruct) -> Self {
		Self {
			attrs: value.attributes,
			vis: value.visibility,
			struct_token: Default::default(),
			ident: value.ident,
			generics: value.generics,
			fields: value.fields.into(),
			semi_token: None,
		}
	}
}

impl From<StructuredItemEnum> for ItemEnum {
	fn from(value: StructuredItemEnum) -> Self {
		Self {
			attrs: value.attributes,
			vis: value.visibility,
			enum_token: Default::default(),
			ident: value.ident,
			generics: value.generics,
			brace_token: Default::default(),
			variants: value.variants.into(),
		}
	}
}

impl From<StructuredEnumVariants> for Punctuated<Variant, Token![,]> {
	fn from(value: StructuredEnumVariants) -> Self {
		value.variants.into_values().map(Variant::from).collect()
	}
}

impl From<StructuredEnumVariant> for Variant {
	fn from(value: StructuredEnumVariant) -> Self {
		Self {
			attrs: value.attributes,
			ident: value.ident,
			fields: value.fields.into(),
			discriminant: value.discriminant.map(|v| (Default::default(), v)),
		}
	}
}
