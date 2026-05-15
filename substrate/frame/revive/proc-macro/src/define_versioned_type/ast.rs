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

use std::collections::BTreeMap;

use proc_macro2::Span;
use quote::ToTokens;
use syn::{
	parse::{Parse, ParseStream},
	Attribute, Ident, ItemEnum, ItemStruct, ItemType, Result, Token, Visibility,
};

use super::*;

pub struct DefineVersionedTypeInput {
	pub name: Option<String>,
	pub definitions: BTreeMap<Version, StructuredTypeDefinition>,
}

impl Parse for DefineVersionedTypeInput {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut family = None::<(String, Span)>;
		let mut definitions = BTreeMap::<Version, StructuredTypeDefinition>::new();

		while !input.is_empty() {
			let type_definition =
				input.parse::<TypeDefinition>().and_then(StructuredTypeDefinition::try_from)?;
			let name = NameAndVersion::parse(type_definition.ident_ref())?;

			match &family {
				Some((family_name, established_at_ident)) => {
					if name.base_name.as_str() != family_name.as_str() {
						bail! {
							type_definition.ident_ref().span() => format!(
								"expected base name `{family_name}` but found `{}`; all types in a \
								single `define_versioned_type!` invocation must share the same \
								base name",
								name.base_name,
							),
							*established_at_ident => format!(
								"the `{family_name}` family was established here"
							),
						}
					}
				},
				None => family = Some((name.base_name.clone(), type_definition.ident_ref().span())),
			}

			let new_definition_span = type_definition.ident_ref().span();
			if let Some(existing_type_definition) =
				definitions.insert(name.version, type_definition)
			{
				bail! {
					new_definition_span => format!(
						"version `V{}` is defined more than once in this define_versioned_type!` \
						invocation",
						name.version.value,
					),
					existing_type_definition.ident_ref().span() => format!(
						"the earlier definition of `V{}` is here",
						name.version.value,
					),
				}
			}
		}

		let this = Self { name: family.map(|(v, ..)| v), definitions };
		this.validate()?;
		Ok(this)
	}
}

impl DefineVersionedTypeInput {
	pub fn validate(&self) -> Result<()> {
		self.validate_all_versions_are_contiguous()?;
		Ok(())
	}

	pub fn validate_all_versions_are_contiguous(&self) -> Result<()> {
		let mut last_version = None::<(Version, Span)>;
		for (version, type_definition) in self.definitions.iter() {
			match last_version {
				Some((last_version, last_version_ident_span)) => {
					if version.value != last_version.value + 1 {
						bail! {
							type_definition.ident_ref().span() => format!(
								"version numbers must be contiguous; expected `V{}` after `V{}` \
								but found `V{}`",
								last_version.value + 1,
								last_version.value,
								version.value,
							),
							last_version_ident_span => format!(
								"the previous version `V{}` was defined here",
								last_version.value,
							),
						}
					}
				},
				None => {},
			}
			last_version = Some((*version, type_definition.ident_ref().span()))
		}
		Ok(())
	}
}

pub struct DefineVersionedTypeOutput {
	pub type_definitions: Vec<TypeDefinition>,
	pub latest_type_alias: Option<ItemType>,
}

impl ToTokens for DefineVersionedTypeOutput {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		for type_def in self.type_definitions.iter() {
			match type_def {
				TypeDefinition::Struct(item) => item.to_tokens(tokens),
				TypeDefinition::Enum(item) => item.to_tokens(tokens),
			}
		}
		if let Some(ref type_alias) = self.latest_type_alias {
			type_alias.to_tokens(tokens);
		}
	}
}

pub enum TypeDefinition {
	Struct(ItemStruct),
	Enum(ItemEnum),
}

impl Parse for TypeDefinition {
	fn parse(input: ParseStream) -> Result<Self> {
		let attributes = Attribute::parse_outer(input)?;
		let visibility = input.parse::<Visibility>()?;
		let lookahead = input.lookahead1();

		if lookahead.peek(Token![struct]) {
			let mut item = input.parse::<ItemStruct>()?;
			item.attrs = attributes;
			item.vis = visibility;
			Ok(Self::Struct(item))
		} else if lookahead.peek(Token![enum]) {
			let mut item = input.parse::<ItemEnum>()?;
			item.attrs = attributes;
			item.vis = visibility;
			Ok(Self::Enum(item))
		} else {
			Err(input.error(
				"define_versioned_type! expects a struct or enum item after any outer \
				attributes and visibility",
			))
		}
	}
}

impl ToTokens for TypeDefinition {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		match self {
			Self::Struct(item) => item.to_tokens(tokens),
			Self::Enum(item) => item.to_tokens(tokens),
		}
	}
}

impl TryFrom<TypeDefinition> for StructuredTypeDefinition {
	type Error = syn::Error;

	fn try_from(value: TypeDefinition) -> core::result::Result<Self, Self::Error> {
		match value {
			TypeDefinition::Struct(item) => Ok(Self::Struct(item.try_into()?)),
			TypeDefinition::Enum(item) => Ok(Self::Enum(item.try_into()?)),
		}
	}
}

impl From<StructuredTypeDefinition> for TypeDefinition {
	fn from(value: StructuredTypeDefinition) -> Self {
		match value {
			StructuredTypeDefinition::Struct(item) => Self::Struct(item.into()),
			StructuredTypeDefinition::Enum(item) => Self::Enum(item.into()),
		}
	}
}

pub struct NameAndVersion {
	pub base_name: String,
	pub version: Version,
}

impl NameAndVersion {
	pub fn parse(ident: &Ident) -> Result<Self> {
		let ident_string = ident.to_string();
		let Some((base_name, version_suffix)) = ident_string.rsplit_once('V') else {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned type names must end with `V` followed by a positive integer, \
				for example `CallLogV1`",
			));
		};

		if base_name.is_empty() {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned type names must include a base name before the version suffix",
			));
		}

		Ok(Self {
			base_name: base_name.to_owned(),
			version: Version::parse(ident, version_suffix)?,
		})
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
	pub value: u32,
}

impl Version {
	pub fn parse(ident: &Ident, version_suffix: &str) -> Result<Self> {
		if version_suffix.is_empty() {
			bail! {
				ident.span() => "versioned type names must include a positive integer after \
					the `V` suffix"
			}
		}

		if version_suffix.len() > 1 && version_suffix.starts_with('0') {
			bail! {
				ident.span() => "versioned type versions must not contain leading zeros"
			}
		}

		let value = version_suffix.parse::<u32>().map_err(|_| {
			syn_error! {
				ident.span() => "versioned type names must end with `V` followed by a positive \
					integer"
			}
		})?;

		if value == 0 {
			bail! {
				ident.span() => "versioned type versions must start at 1"
			}
		}

		Ok(Self { value })
	}
}
