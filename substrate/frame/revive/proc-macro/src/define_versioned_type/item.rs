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

use std::{collections::BTreeMap, fmt};

use quote::ToTokens;
use syn::{
	parse::{Parse, ParseStream},
	Attribute, Generics, Ident, ItemEnum, ItemStruct, Result, Token, Visibility,
};

pub struct DefineVersionedTypeInput {
	pub(super) name: Option<String>,

	pub(super) highest_version: Option<Version>,

	pub(super) definitions: BTreeMap<Version, DefineVersionedTypeItem>,
}

impl Parse for DefineVersionedTypeInput {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut name = None::<EstablishedName>;
		let mut highest_version = None::<Version>;
		let mut definitions = BTreeMap::<Version, DefineVersionedTypeItem>::new();

		while !input.is_empty() {
			let item = input.parse::<DefineVersionedTypeItem>()?;
			let name_and_version = item.name_and_version()?;
			let version = name_and_version.version();

			match &name {
				Some(existing_name) => existing_name.ensure_matches(&name_and_version, &item)?,
				None => name = Some(EstablishedName::from_item(&name_and_version, &item)),
			}

			reject_duplicate_version(&definitions, &name_and_version, &item)?;
			highest_version = Some(highest_version.map_or(version, |highest| highest.max(version)));
			definitions.insert(version, item);
		}

		ensure_contiguous_versions(&definitions)?;

		Ok(Self { name: name.map(EstablishedName::into_name), highest_version, definitions })
	}
}

pub enum DefineVersionedTypeItem {
	Struct(ItemStruct),

	Enum(ItemEnum),
}

impl DefineVersionedTypeItem {
	#[must_use]
	pub(super) fn take_attributes(&mut self) -> Vec<Attribute> {
		match self {
			Self::Struct(item_struct) => core::mem::take(&mut item_struct.attrs),
			Self::Enum(item_enum) => core::mem::take(&mut item_enum.attrs),
		}
	}

	pub(super) fn set_attributes(&mut self, attributes: Vec<Attribute>) {
		match self {
			Self::Struct(item_struct) => item_struct.attrs = attributes,
			Self::Enum(item_enum) => item_enum.attrs = attributes,
		}
	}

	#[must_use]
	pub(super) fn ident(&self) -> &Ident {
		match self {
			Self::Struct(item_struct) => &item_struct.ident,
			Self::Enum(item_enum) => &item_enum.ident,
		}
	}

	#[must_use]
	pub(super) fn visibility(&self) -> &Visibility {
		match self {
			Self::Struct(item_struct) => &item_struct.vis,
			Self::Enum(item_enum) => &item_enum.vis,
		}
	}

	#[must_use]
	pub(super) fn generics(&self) -> &Generics {
		match self {
			Self::Struct(item_struct) => &item_struct.generics,
			Self::Enum(item_enum) => &item_enum.generics,
		}
	}

	pub(super) fn name_and_version(&self) -> Result<NameAndVersion> {
		NameAndVersion::parse(self.ident())
	}
}

impl Parse for DefineVersionedTypeItem {
	fn parse(input: ParseStream) -> Result<Self> {
		let attributes = Attribute::parse_outer(input)?;
		let visibility = input.parse::<Visibility>()?;
		let type_kind = input.lookahead1();

		if type_kind.peek(Token![struct]) {
			let mut item_struct = input.parse::<ItemStruct>()?;
			item_struct.attrs = attributes;
			item_struct.vis = visibility;
			Ok(Self::Struct(item_struct))
		} else if type_kind.peek(Token![enum]) {
			let mut item_enum = input.parse::<ItemEnum>()?;
			item_enum.attrs = attributes;
			item_enum.vis = visibility;
			Ok(Self::Enum(item_enum))
		} else {
			Err(input.error(
				"define_versioned_type! expects a struct or enum item after any outer \
                attributes and visibility",
			))
		}
	}
}

impl ToTokens for DefineVersionedTypeItem {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		match self {
			Self::Struct(item) => item.to_tokens(tokens),
			Self::Enum(item) => item.to_tokens(tokens),
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Version {
	value: u32,
}

impl Version {
	fn parse(ident: &Ident, version_suffix: &str) -> Result<Self> {
		if version_suffix.is_empty() {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned type names must include a positive integer after the `V` suffix",
			));
		}

		if version_suffix.len() > 1 && version_suffix.starts_with('0') {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned type versions must not contain leading zeros",
			));
		}

		let value = version_suffix.parse::<u32>().map_err(|_| {
			syn::Error::new_spanned(
				ident,
				"versioned type names must end with `V` followed by a positive integer",
			)
		})?;

		if value == 0 {
			return Err(syn::Error::new_spanned(ident, "versioned type versions must start at 1"));
		}

		Ok(Self { value })
	}

	#[must_use]
	pub(super) fn value(self) -> u32 {
		self.value
	}

	fn next_after(self, previous_ident: &Ident) -> Result<Self> {
		self.value.checked_add(1).map(|value| Self { value }).ok_or_else(|| {
			syn::Error::new_spanned(
				previous_ident,
				"version number is too large to compute the next contiguous version",
			)
		})
	}
}

impl fmt::Display for Version {
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "V{}", self.value)
	}
}

#[derive(Debug)]
pub(super) struct NameAndVersion {
	base_name: String,

	version: Version,
}

impl NameAndVersion {
	fn parse(ident: &Ident) -> Result<Self> {
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

	#[must_use]
	pub(super) fn base_name(&self) -> &str {
		&self.base_name
	}

	#[must_use]
	pub(super) fn version(&self) -> Version {
		self.version
	}
}

struct EstablishedName {
	name: String,

	ident: Ident,
}

impl EstablishedName {
	fn from_item(name_and_version: &NameAndVersion, item: &DefineVersionedTypeItem) -> Self {
		Self { name: name_and_version.base_name().to_owned(), ident: item.ident().clone() }
	}

	fn into_name(self) -> String {
		self.name
	}

	fn ensure_matches(
		&self,
		name_and_version: &NameAndVersion,
		item: &DefineVersionedTypeItem,
	) -> Result<()> {
		if name_and_version.base_name() == self.name {
			return Ok(());
		}

		let mut error = syn::Error::new_spanned(
			item.ident(),
			format!(
				"all items in define_versioned_type! must define versions of the same type; \
                found `{}` but expected `{}`",
				name_and_version.base_name(),
				self.name
			),
		);
		error.combine(syn::Error::new_spanned(
			&self.ident,
			format!("the expected versioned type name `{}` was established here", self.name),
		));
		Err(error)
	}
}

struct PreviousDefinition<'a> {
	version: Version,

	item: &'a DefineVersionedTypeItem,
}

fn reject_duplicate_version(
	definitions: &BTreeMap<Version, DefineVersionedTypeItem>,
	name_and_version: &NameAndVersion,
	item: &DefineVersionedTypeItem,
) -> Result<()> {
	if let Some(existing_item) = definitions.get(&name_and_version.version()) {
		let version = name_and_version.version();
		let mut error = syn::Error::new_spanned(
			item.ident(),
			format!(
				"duplicate version {version} for versioned type `{}`; version {version} was \
                already defined by `{}`",
				name_and_version.base_name(),
				existing_item.ident()
			),
		);
		error.combine(syn::Error::new_spanned(
			existing_item.ident(),
			format!("first definition of version {version} is here"),
		));
		return Err(error);
	}

	Ok(())
}

fn ensure_contiguous_versions(
	definitions: &BTreeMap<Version, DefineVersionedTypeItem>,
) -> Result<()> {
	let mut previous_definition = None::<PreviousDefinition<'_>>;

	for (version, item) in definitions {
		if let Some(previous) = previous_definition {
			let expected_version = previous.version.next_after(previous.item.ident())?;
			if *version != expected_version {
				let missing_versions = missing_versions_description(expected_version, *version);
				let mut error = syn::Error::new_spanned(
					item.ident(),
					format!(
						"versioned type definitions must be contiguous; missing \
                        {missing_versions} before {version}"
					),
				);
				error.combine(syn::Error::new_spanned(
					previous.item.ident(),
					format!("previous defined version was {} here", previous.version),
				));
				return Err(error);
			}
		}

		previous_definition = Some(PreviousDefinition { version: *version, item });
	}

	Ok(())
}

fn missing_versions_description(expected_version: Version, found_version: Version) -> String {
	let last_missing_version = found_version.value() - 1;

	if expected_version.value() == last_missing_version {
		format!("version {expected_version}")
	} else {
		format!("versions {expected_version}..V{last_missing_version}")
	}
}
