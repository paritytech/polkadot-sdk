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

use std::collections::{BTreeMap, BTreeSet};

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{format_ident, quote, ToTokens};
use syn::{
	parse::{Parse, ParseStream},
	punctuated::Punctuated,
	token::Comma,
	Attribute, Fields, GenericParam, Generics, Ident, ItemStruct, Path, Result, Token, Visibility,
};

pub fn handle_define_versioned_interface(
	input: DefineVersionedInterfaceInput,
) -> Result<TokenStream2> {
	let input_enum = generate_versioned_enum(&input, PayloadSide::Input)?;
	let output_enum = generate_versioned_enum(&input, PayloadSide::Output)?;
	let input_latest_alias = generate_latest_payload_alias(&input, PayloadSide::Input)?;
	let output_latest_alias = generate_latest_payload_alias(&input, PayloadSide::Output)?;
	let payload_structs = input.items.iter().map(VersionedInterfaceItem::item);

	Ok(quote! {
		#(#payload_structs)*
		#input_enum
		#output_enum
		#input_latest_alias
		#output_latest_alias
	})
}

fn generate_versioned_enum(
	input: &DefineVersionedInterfaceInput,
	side: PayloadSide,
) -> Result<TokenStream2> {
	let side_items = side_items(input, side);
	let Some(latest_item) = side_items.last().copied() else {
		return Err(syn::Error::new(
			Span::call_site(),
			format!(
				"internal error while generating latest {} payload version",
				side.diagnostic_name()
			),
		));
	};
	let latest_version = latest_item.payload_name().version().value();
	let generated_name = generated_interface_name(&input.name);
	let enum_ident =
		Ident::new(&format!("Versioned{}{}", generated_name, side.name_suffix()), input.name_span);
	let enum_generics = merged_generics(&side_items)?;
	let common_derives = common_derive_paths(&side_items)?;
	let derive_attribute = derive_attribute(common_derives);
	let variants = side_items.iter().map(|item| enum_variant(item)).collect::<Vec<_>>();
	let constructors = side_items.iter().map(|item| constructor_method(item)).collect::<Vec<_>>();
	let version_arms = side_items.iter().map(|item| version_match_arm(item)).collect::<Vec<_>>();
	let accessors = side_items.iter().map(|item| accessor_methods(item)).collect::<Vec<_>>();
	let from_impls = side_items
		.iter()
		.map(|item| from_impl(item, &enum_ident, &enum_generics))
		.collect::<Vec<_>>();
	let try_from_impls = side_items
		.iter()
		.map(|item| try_from_impl(item, &side_items, &enum_ident, &enum_generics))
		.collect::<Vec<_>>();
	let (impl_generics, type_generics, where_clause) = enum_generics.split_for_impl();

	Ok(quote! {
		#derive_attribute
		pub enum #enum_ident #enum_generics #where_clause {
			#(#variants,)*
		}

		impl #impl_generics #enum_ident #type_generics #where_clause {
			pub const LATEST_VERSION: u32 = #latest_version;

			#(#constructors)*

			pub fn version(&self) -> u32 {
				match self {
					#(#version_arms,)*
				}
			}

			#(#accessors)*
		}

		#(#from_impls)*

		#(#try_from_impls)*
	})
}

fn generate_latest_payload_alias(
	input: &DefineVersionedInterfaceInput,
	side: PayloadSide,
) -> Result<TokenStream2> {
	let Some(item) = side_items(input, side).last().copied() else {
		return Err(syn::Error::new(
			Span::call_site(),
			format!(
				"internal error while generating latest {} payload alias",
				side.diagnostic_name()
			),
		));
	};

	let generated_name = generated_interface_name(&input.name);
	let alias_ident =
		Ident::new(&format!("Latest{}{}", generated_name, side.name_suffix()), input.name_span);
	let payload_ident = &item.item().ident;
	let visibility = &item.item().vis;
	let alias_generics = type_alias_generics(&item.item().generics);
	let payload_generics = payload_type_generics(&alias_generics);
	let doc = format!("The latest version of `{}`{}.", generated_name, side.name_suffix());

	Ok(quote! {
		#[doc = #doc]
		#visibility type #alias_ident #alias_generics = #payload_ident #payload_generics;
	})
}

fn side_items(
	input: &DefineVersionedInterfaceInput,
	side: PayloadSide,
) -> Vec<&VersionedInterfaceItem> {
	side_payloads(&input.input_payloads, &input.output_payloads, side)
		.values()
		.map(|index| &input.items[*index])
		.collect::<Vec<_>>()
}

fn generated_interface_name(payload_family_name: &str) -> &str {
	payload_family_name.strip_suffix("Versioned").unwrap_or(payload_family_name)
}

fn side_payloads<'a>(
	input_payloads: &'a BTreeMap<Version, usize>,
	output_payloads: &'a BTreeMap<Version, usize>,
	side: PayloadSide,
) -> &'a BTreeMap<Version, usize> {
	match side {
		PayloadSide::Input => input_payloads,
		PayloadSide::Output => output_payloads,
	}
}

fn enum_variant(item: &VersionedInterfaceItem) -> TokenStream2 {
	let variant_ident =
		version_variant_ident(item.payload_name().version(), item.item().ident.span());
	let payload_ident = &item.item().ident;
	let payload_generics = payload_type_generics(&item.item().generics);

	quote! {
		#variant_ident(::alloc::boxed::Box<#payload_ident #payload_generics>)
	}
}

fn constructor_method(item: &VersionedInterfaceItem) -> TokenStream2 {
	let version = item.payload_name().version().value();
	let span = item.item().ident.span();
	let method_ident = format_ident!("new_v{}", version, span = span);
	let variant_ident = version_variant_ident(item.payload_name().version(), span);
	let payload_ident = &item.item().ident;
	let payload_generics = payload_type_generics(&item.item().generics);

	quote! {
		pub fn #method_ident(payload: #payload_ident #payload_generics) -> Self {
			Self::#variant_ident(::alloc::boxed::Box::new(payload))
		}
	}
}

fn version_match_arm(item: &VersionedInterfaceItem) -> TokenStream2 {
	let version = item.payload_name().version().value();
	let variant_ident =
		version_variant_ident(item.payload_name().version(), item.item().ident.span());

	quote! {
		Self::#variant_ident(..) => #version
	}
}

fn accessor_methods(item: &VersionedInterfaceItem) -> TokenStream2 {
	let version = item.payload_name().version().value();
	let span = item.item().ident.span();
	let variant_ident = version_variant_ident(item.payload_name().version(), span);
	let as_ident = format_ident!("as_v{}", version, span = span);
	let into_ident = format_ident!("into_v{}", version, span = span);
	let unwrap_ident = format_ident!("unwrap_v{}", version, span = span);
	let payload_ident = &item.item().ident;
	let payload_generics = payload_type_generics(&item.item().generics);
	let panic_message =
		format!("Expected this to be a v{version} variant, but it is a v{{}} variant");

	quote! {
		pub fn #as_ident(&self) -> Option<&#payload_ident #payload_generics> {
			match self {
				Self::#variant_ident(value) => Some(value.as_ref()),
				_ => None,
			}
		}

		pub fn #into_ident(self) -> Option<#payload_ident #payload_generics> {
			match self {
				Self::#variant_ident(value) => Some(*value),
				_ => None,
			}
		}

		pub fn #unwrap_ident(self) -> #payload_ident #payload_generics {
			match self {
				Self::#variant_ident(value) => *value,
				other => panic!(#panic_message, other.version()),
			}
		}
	}
}

fn from_impl(
	item: &VersionedInterfaceItem,
	enum_ident: &Ident,
	enum_generics: &Generics,
) -> TokenStream2 {
	let variant_ident =
		version_variant_ident(item.payload_name().version(), item.item().ident.span());
	let payload_ident = &item.item().ident;
	let payload_generics = payload_type_generics(&item.item().generics);
	let (impl_generics, type_generics, where_clause) = enum_generics.split_for_impl();

	quote! {
		impl #impl_generics ::core::convert::From<#payload_ident #payload_generics>
			for #enum_ident #type_generics #where_clause
		{
			fn from(payload: #payload_ident #payload_generics) -> Self {
				Self::#variant_ident(::alloc::boxed::Box::new(payload))
			}
		}
	}
}

fn try_from_impl(
	item: &VersionedInterfaceItem,
	side_items: &[&VersionedInterfaceItem],
	enum_ident: &Ident,
	enum_generics: &Generics,
) -> TokenStream2 {
	let target_version = item.payload_name().version();
	let payload_ident = &item.item().ident;
	let payload_generics = payload_type_generics(&item.item().generics);
	let (impl_generics, type_generics, where_clause) = enum_generics.split_for_impl();
	let arms = side_items.iter().map(|other| {
		let other_version = other.payload_name().version();
		let other_variant = version_variant_ident(other_version, other.item().ident.span());
		if other_version == target_version {
			quote! {
				#enum_ident::#other_variant(value) => ::core::result::Result::Ok(*value)
			}
		} else {
			quote! {
				#enum_ident::#other_variant(..) => ::core::result::Result::Err(())
			}
		}
	});

	quote! {
		impl #impl_generics ::core::convert::TryFrom<#enum_ident #type_generics>
			for #payload_ident #payload_generics #where_clause
		{
			type Error = ();

			fn try_from(
				versioned: #enum_ident #type_generics,
			) -> ::core::result::Result<Self, Self::Error> {
				match versioned {
					#(#arms,)*
				}
			}
		}
	}
}

fn version_variant_ident(version: Version, span: Span) -> Ident {
	format_ident!("V{}", version.value(), span = span)
}

fn payload_type_generics(generics: &Generics) -> TokenStream2 {
	let arguments = generics.params.iter().map(generic_argument).collect::<Vec<_>>();

	if arguments.is_empty() {
		quote! {}
	} else {
		quote! { <#(#arguments),*> }
	}
}

fn generic_argument(param: &GenericParam) -> TokenStream2 {
	match param {
		GenericParam::Lifetime(param) => param.lifetime.to_token_stream(),
		GenericParam::Type(param) => param.ident.to_token_stream(),
		GenericParam::Const(param) => param.ident.to_token_stream(),
	}
}

fn type_alias_generics(generics: &Generics) -> Generics {
	let mut generics = generics.clone();
	generics.where_clause = None;

	for param in &mut generics.params {
		match param {
			GenericParam::Lifetime(param) => {
				param.colon_token = None;
				param.bounds.clear();
			},
			GenericParam::Type(param) => {
				param.colon_token = None;
				param.bounds.clear();
			},
			GenericParam::Const(_) => {},
		}
	}

	generics
}

fn merged_generics(items: &[&VersionedInterfaceItem]) -> Result<Generics> {
	let mut generics = Generics::default();
	let mut indexes = BTreeMap::<String, usize>::new();

	for item in items {
		for param in &item.item().generics.params {
			merge_generic_param(&mut generics, &mut indexes, param)?;
		}

		if let Some(where_clause) = &item.item().generics.where_clause {
			generics
				.make_where_clause()
				.predicates
				.extend(where_clause.predicates.iter().cloned());
		}
	}

	Ok(generics)
}

fn merge_generic_param(
	generics: &mut Generics,
	indexes: &mut BTreeMap<String, usize>,
	param: &GenericParam,
) -> Result<()> {
	let key = generic_param_key(param);
	if let Some(index) = indexes.get(&key).copied() {
		let Some(existing) = generics.params.iter().nth(index).cloned() else {
			return Err(syn::Error::new_spanned(
				param,
				"internal error while merging versioned interface generic parameters",
			));
		};
		ensure_compatible_generic_param(&existing, param)?;

		let Some(existing) = generics.params.iter_mut().nth(index) else {
			return Err(syn::Error::new_spanned(
				param,
				"internal error while merging versioned interface generic parameters",
			));
		};

		merge_generic_bounds(existing, param);
		return Ok(());
	}

	indexes.insert(key, generics.params.len());
	generics.params.push(param.clone());
	Ok(())
}

fn generic_param_key(param: &GenericParam) -> String {
	match param {
		GenericParam::Lifetime(param) => param.lifetime.to_token_stream().to_string(),
		GenericParam::Type(param) => param.ident.to_string(),
		GenericParam::Const(param) => param.ident.to_string(),
	}
}

fn ensure_compatible_generic_param(existing: &GenericParam, incoming: &GenericParam) -> Result<()> {
	match (existing, incoming) {
		(GenericParam::Lifetime(_), GenericParam::Lifetime(_)) |
		(GenericParam::Type(_), GenericParam::Type(_)) => Ok(()),
		(GenericParam::Const(existing), GenericParam::Const(incoming)) => {
			let existing_type = existing.ty.to_token_stream().to_string();
			let incoming_type = incoming.ty.to_token_stream().to_string();

			if existing_type == incoming_type {
				return Ok(());
			}

			let mut error = syn::Error::new_spanned(
				incoming,
				format!(
					"const generic parameter `{}` has type `{}` here, but it was already \
					defined with type `{}`",
					incoming.ident, incoming_type, existing_type
				),
			);
			error.combine(syn::Error::new_spanned(
				existing,
				format!("first const generic parameter `{}` is defined here", existing.ident),
			));
			Err(error)
		},
		(existing, incoming) => {
			let mut error = syn::Error::new_spanned(
				incoming,
				format!(
					"generic parameter `{}` is used as {} here, but it was already used as {}",
					generic_param_key(incoming),
					generic_param_kind(incoming),
					generic_param_kind(existing)
				),
			);
			error.combine(syn::Error::new_spanned(
				existing,
				format!(
					"first generic parameter `{}` is defined here",
					generic_param_key(existing)
				),
			));
			Err(error)
		},
	}
}

fn generic_param_kind(param: &GenericParam) -> &'static str {
	match param {
		GenericParam::Lifetime(_) => "a lifetime parameter",
		GenericParam::Type(_) => "a type parameter",
		GenericParam::Const(_) => "a const parameter",
	}
}

fn merge_generic_bounds(existing: &mut GenericParam, incoming: &GenericParam) {
	match (existing, incoming) {
		(GenericParam::Lifetime(existing), GenericParam::Lifetime(incoming)) => {
			if !incoming.bounds.is_empty() {
				existing.colon_token = existing.colon_token.or(incoming.colon_token);
				existing.bounds.extend(incoming.bounds.iter().cloned());
			}
		},
		(GenericParam::Type(existing), GenericParam::Type(incoming)) => {
			if !incoming.bounds.is_empty() {
				existing.colon_token = existing.colon_token.or(incoming.colon_token);
				existing.bounds.extend(incoming.bounds.iter().cloned());
			}
		},
		(GenericParam::Const(_), GenericParam::Const(_)) |
		(GenericParam::Lifetime(_), GenericParam::Type(_) | GenericParam::Const(_)) |
		(GenericParam::Type(_), GenericParam::Lifetime(_) | GenericParam::Const(_)) |
		(GenericParam::Const(_), GenericParam::Lifetime(_) | GenericParam::Type(_)) => {},
	}
}

fn derive_attribute(paths: Vec<Path>) -> TokenStream2 {
	if paths.is_empty() {
		quote! {}
	} else {
		quote! {
			#[derive(#(#paths),*)]
		}
	}
}

fn common_derive_paths(items: &[&VersionedInterfaceItem]) -> Result<Vec<Path>> {
	let Some(first_item) = items.first() else {
		return Ok(Vec::new());
	};
	let first_paths = derive_paths(first_item.item())?;
	let other_sets = items
		.iter()
		.skip(1)
		.map(|item| derive_path_keys(item.item()))
		.collect::<Result<Vec<_>>>()?;
	let mut seen = BTreeSet::<String>::new();
	let mut common_paths = Vec::<Path>::new();

	for path in first_paths {
		let key = path.to_token_stream().to_string();
		if !seen.insert(key.clone()) {
			continue;
		}

		if other_sets.iter().all(|set| set.contains(&key)) {
			common_paths.push(path);
		}
	}

	Ok(common_paths)
}

fn derive_path_keys(item: &ItemStruct) -> Result<BTreeSet<String>> {
	Ok(derive_paths(item)?
		.into_iter()
		.map(|path| path.to_token_stream().to_string())
		.collect::<BTreeSet<_>>())
}

fn derive_paths(item: &ItemStruct) -> Result<Vec<Path>> {
	let mut paths = Vec::<Path>::new();

	for attribute in &item.attrs {
		if !attribute.path().is_ident("derive") {
			continue;
		}

		let derive_paths = attribute
			.parse_args_with(Punctuated::<Path, Comma>::parse_terminated)
			.map_err(|error| {
			let mut diagnostic = error;
			diagnostic.combine(syn::Error::new_spanned(
				attribute,
				"failed to parse payload derive attribute while computing generated enum derives",
			));
			diagnostic
		})?;
		paths.extend(derive_paths);
	}

	Ok(paths)
}

pub struct DefineVersionedInterfaceInput {
	name: String,

	name_span: Span,

	items: Vec<VersionedInterfaceItem>,

	input_payloads: BTreeMap<Version, usize>,

	output_payloads: BTreeMap<Version, usize>,
}

impl Parse for DefineVersionedInterfaceInput {
	fn parse(input: ParseStream) -> Result<Self> {
		let mut name = None::<EstablishedName>;
		let mut items = Vec::<VersionedInterfaceItem>::new();
		let mut input_payloads = BTreeMap::<Version, usize>::new();
		let mut output_payloads = BTreeMap::<Version, usize>::new();

		while !input.is_empty() {
			let item = VersionedInterfaceItem::parse(input)?;
			let payload_name = item.payload_name();

			match &name {
				Some(existing_name) => existing_name.ensure_matches(payload_name, item.item())?,
				None => name = Some(EstablishedName::from_payload(payload_name, item.item())),
			}

			let item_index = items.len();
			reject_duplicate_payload(
				side_payloads_mut(&mut input_payloads, &mut output_payloads, payload_name.side()),
				payload_name,
				item.item(),
				&items,
			)?;
			side_payloads_mut(&mut input_payloads, &mut output_payloads, payload_name.side())
				.insert(payload_name.version(), item_index);
			items.push(item);
		}

		let Some(name) = name else {
			return Err(input.error(
				"define_versioned_interface! requires at least one input and output payload pair",
			));
		};

		ensure_matching_payload_pairs(&input_payloads, &output_payloads, &items)?;
		ensure_contiguous_versions(&input_payloads, &output_payloads, &items)?;

		let name_span = name.ident.span();
		Ok(Self { name: name.into_name(), name_span, items, input_payloads, output_payloads })
	}
}

struct VersionedInterfaceItem {
	item: ItemStruct,

	payload_name: PayloadName,
}

impl VersionedInterfaceItem {
	fn parse(input: ParseStream) -> Result<Self> {
		let attributes = Attribute::parse_outer(input)?;
		let visibility = input.parse::<Visibility>()?;
		let type_kind = input.lookahead1();

		if !type_kind.peek(Token![struct]) {
			return Err(input.error(match non_struct_item_kind(input) {
				Some(item_kind) => format!(
					"define_versioned_interface! expects named struct payload items only; found \
					{item_kind} item"
				),
				None => {
					"define_versioned_interface! expects named struct payload items only".to_owned()
				},
			}));
		}

		let mut item = input.parse::<ItemStruct>()?;
		item.attrs = attributes;
		item.vis = visibility;
		ensure_named_struct(&item)?;
		let payload_name = PayloadName::parse(&item.ident)?;

		Ok(Self { item, payload_name })
	}

	#[must_use]
	fn item(&self) -> &ItemStruct {
		&self.item
	}

	#[must_use]
	fn payload_name(&self) -> &PayloadName {
		&self.payload_name
	}
}

fn non_struct_item_kind(input: ParseStream) -> Option<&'static str> {
	if input.peek(Token![enum]) {
		Some("enum")
	} else if input.peek(Token![fn]) {
		Some("function")
	} else if input.peek(Token![mod]) {
		Some("module")
	} else if input.peek(Token![impl]) {
		Some("impl")
	} else if input.peek(Token![type]) {
		Some("type alias")
	} else if input.peek(Token![const]) {
		Some("const")
	} else if input.peek(Token![static]) {
		Some("static")
	} else if input.peek(Token![union]) {
		Some("union")
	} else {
		None
	}
}

struct PayloadName {
	name: String,

	side: PayloadSide,

	version: Version,
}

impl PayloadName {
	fn parse(ident: &Ident) -> Result<Self> {
		let ident_string = ident.to_string();
		let Some((prefix, version_suffix)) = ident_string.rsplit_once('V') else {
			return Err(payload_name_error(ident));
		};

		let (name, side) = if let Some(name) = prefix.strip_suffix(PayloadSide::Input.name_suffix())
		{
			(name, PayloadSide::Input)
		} else if let Some(name) = prefix.strip_suffix(PayloadSide::Output.name_suffix()) {
			(name, PayloadSide::Output)
		} else {
			return Err(payload_name_error(ident));
		};

		if name.is_empty() {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned interface payload names must include a non-empty family name",
			));
		}

		Ok(Self { name: name.to_owned(), side, version: Version::parse(ident, version_suffix)? })
	}

	#[must_use]
	fn name(&self) -> &str {
		&self.name
	}

	#[must_use]
	fn side(&self) -> PayloadSide {
		self.side
	}

	#[must_use]
	fn version(&self) -> Version {
		self.version
	}

	fn expected_ident(&self, side: PayloadSide) -> String {
		format!("{}{}{}", self.name, side.name_suffix(), self.version)
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PayloadSide {
	Input,

	Output,
}

impl PayloadSide {
	#[must_use]
	fn name_suffix(self) -> &'static str {
		match self {
			Self::Input => "InputPayload",
			Self::Output => "OutputPayload",
		}
	}

	#[must_use]
	fn diagnostic_name(self) -> &'static str {
		match self {
			Self::Input => "input",
			Self::Output => "output",
		}
	}
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Version {
	value: u32,
}

impl Version {
	fn parse(ident: &Ident, version_suffix: &str) -> Result<Self> {
		if version_suffix.is_empty() {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned interface payload names must include a positive integer after `V`",
			));
		}

		if version_suffix.len() > 1 && version_suffix.starts_with('0') {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned interface payload versions must not contain leading zeros",
			));
		}

		let value = version_suffix.parse::<u32>().map_err(|_| {
			syn::Error::new_spanned(
				ident,
				"versioned interface payload names must end with `V` followed by a positive \
				integer",
			)
		})?;

		if value == 0 {
			return Err(syn::Error::new_spanned(
				ident,
				"versioned interface payload versions must start at 1",
			));
		}

		Ok(Self { value })
	}

	#[must_use]
	fn value(self) -> u32 {
		self.value
	}

	fn next_after(self, previous_item: &ItemStruct) -> Result<Self> {
		self.value.checked_add(1).map(|value| Self { value }).ok_or_else(|| {
			syn::Error::new_spanned(
				&previous_item.ident,
				"version number is too large to compute the next contiguous version",
			)
		})
	}
}

impl core::fmt::Display for Version {
	fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
		write!(formatter, "V{}", self.value)
	}
}

struct EstablishedName {
	name: String,

	ident: Ident,
}

impl EstablishedName {
	fn from_payload(payload_name: &PayloadName, item: &ItemStruct) -> Self {
		Self { name: payload_name.name().to_owned(), ident: item.ident.clone() }
	}

	fn into_name(self) -> String {
		self.name
	}

	fn ensure_matches(&self, payload_name: &PayloadName, item: &ItemStruct) -> Result<()> {
		if payload_name.name() == self.name {
			return Ok(());
		}

		let mut error = syn::Error::new_spanned(
			&item.ident,
			format!(
				"all payloads in define_versioned_interface! must use the same family name; \
				found `{}` but expected `{}`",
				payload_name.name(),
				self.name
			),
		);
		error.combine(syn::Error::new_spanned(
			&self.ident,
			format!("the expected interface family name `{}` was established here", self.name),
		));
		Err(error)
	}
}

fn side_payloads_mut<'a>(
	input_payloads: &'a mut BTreeMap<Version, usize>,
	output_payloads: &'a mut BTreeMap<Version, usize>,
	side: PayloadSide,
) -> &'a mut BTreeMap<Version, usize> {
	match side {
		PayloadSide::Input => input_payloads,
		PayloadSide::Output => output_payloads,
	}
}

fn ensure_named_struct(item: &ItemStruct) -> Result<()> {
	match &item.fields {
		Fields::Named(_) => Ok(()),
		Fields::Unnamed(fields) => Err(syn::Error::new_spanned(
			fields,
			"define_versioned_interface! only accepts named-field payload structs",
		)),
		Fields::Unit => {
			let span = item
				.semi_token
				.as_ref()
				.map(|semi_token| semi_token.spans[0])
				.unwrap_or_else(|| item.ident.span());
			Err(syn::Error::new(
				span,
				"define_versioned_interface! only accepts named-field payload structs",
			))
		},
	}
}

fn reject_duplicate_payload(
	payloads: &BTreeMap<Version, usize>,
	payload_name: &PayloadName,
	item: &ItemStruct,
	items: &[VersionedInterfaceItem],
) -> Result<()> {
	if let Some(existing_index) = payloads.get(&payload_name.version()) {
		let existing_item = items[*existing_index].item();
		let side = payload_name.side().diagnostic_name();
		let version = payload_name.version();
		let mut error = syn::Error::new_spanned(
			&item.ident,
			format!("duplicate {side} payload version {version} for `{}`", payload_name.name()),
		);
		error.combine(syn::Error::new_spanned(
			&existing_item.ident,
			format!("first {side} payload for version {version} is defined here"),
		));
		return Err(error);
	}

	Ok(())
}

fn ensure_matching_payload_pairs(
	input_payloads: &BTreeMap<Version, usize>,
	output_payloads: &BTreeMap<Version, usize>,
	items: &[VersionedInterfaceItem],
) -> Result<()> {
	let versions = input_payloads
		.keys()
		.chain(output_payloads.keys())
		.copied()
		.collect::<BTreeSet<_>>();
	let mut errors = None::<syn::Error>;

	for version in versions {
		if !input_payloads.contains_key(&version) {
			let output_item = interface_item_for_version(output_payloads, version, items)?;
			let output_name = output_item.payload_name();
			let expected_input = output_name.expected_ident(PayloadSide::Input);
			let defined_output = &output_item.item().ident;
			let error = syn::Error::new_spanned(
				defined_output,
				format!(
					"No input type defined for {version}. Expected `{expected_input}` to pair \
					with `{defined_output}`. Output type is defined here"
				),
			);
			combine_error(&mut errors, error);
		}

		if !output_payloads.contains_key(&version) {
			let input_item = interface_item_for_version(input_payloads, version, items)?;
			let input_name = input_item.payload_name();
			let expected_output = input_name.expected_ident(PayloadSide::Output);
			let defined_input = &input_item.item().ident;
			let error = syn::Error::new_spanned(
				defined_input,
				format!(
					"No output type defined for {version}. Expected `{expected_output}` to pair \
					with `{defined_input}`. Input type is defined here"
				),
			);
			combine_error(&mut errors, error);
		}
	}

	if let Some(error) = errors {
		return Err(error);
	}

	Ok(())
}

fn ensure_contiguous_versions(
	input_payloads: &BTreeMap<Version, usize>,
	output_payloads: &BTreeMap<Version, usize>,
	items: &[VersionedInterfaceItem],
) -> Result<()> {
	let mut previous_version = None::<Version>;

	for version in input_payloads.keys() {
		if let Some(previous) = previous_version {
			let previous_item = item_for_version(input_payloads, previous, items)?;
			let expected = previous.next_after(previous_item)?;
			if *version != expected {
				let mut error = syn::Error::new_spanned(
					&item_for_version(input_payloads, *version, items)?.ident,
					format!(
						"versioned interface payload versions must be contiguous; missing \
						{} before {version}",
						missing_versions_description(expected, *version)
					),
				);
				error.combine(syn::Error::new_spanned(
					&previous_item.ident,
					format!("previous defined version was {previous} here"),
				));
				return Err(error);
			}
		}

		previous_version = Some(*version);
	}

	if input_payloads.len() != output_payloads.len() {
		return Err(syn::Error::new(
			Span::call_site(),
			"input and output payload version counts diverged after pair validation",
		));
	}

	Ok(())
}

fn item_for_version<'a>(
	payloads: &BTreeMap<Version, usize>,
	version: Version,
	items: &'a [VersionedInterfaceItem],
) -> Result<&'a ItemStruct> {
	payloads
		.get(&version)
		.map(|index| items[*index].item())
		.ok_or_else(|| syn::Error::new(Span::call_site(), format!("missing payload {version}")))
}

fn interface_item_for_version<'a>(
	payloads: &BTreeMap<Version, usize>,
	version: Version,
	items: &'a [VersionedInterfaceItem],
) -> Result<&'a VersionedInterfaceItem> {
	payloads
		.get(&version)
		.map(|index| &items[*index])
		.ok_or_else(|| syn::Error::new(Span::call_site(), format!("missing payload {version}")))
}

fn combine_error(errors: &mut Option<syn::Error>, error: syn::Error) {
	match errors {
		Some(errors) => errors.combine(error),
		None => *errors = Some(error),
	}
}

fn missing_versions_description(expected_version: Version, found_version: Version) -> String {
	let last_missing_version = found_version.value() - 1;

	if expected_version.value() == last_missing_version {
		format!("version {expected_version}")
	} else {
		format!("versions {expected_version}..V{last_missing_version}")
	}
}

fn payload_name_error(ident: &Ident) -> syn::Error {
	syn::Error::new_spanned(
		ident,
		"versioned interface payload names must match `{Name}InputPayloadVn` or \
		`{Name}OutputPayloadVn`",
	)
}

#[cfg(test)]
mod tests {
	use syn::{parse2, Item};

	use super::*;

	fn parse_input(tokens: TokenStream2) -> DefineVersionedInterfaceInput {
		parse2::<DefineVersionedInterfaceInput>(tokens).unwrap()
	}

	fn expand(tokens: TokenStream2) -> TokenStream2 {
		handle_define_versioned_interface(parse_input(tokens)).unwrap()
	}

	fn expand_error(tokens: TokenStream2) -> syn::Error {
		handle_define_versioned_interface(parse_input(tokens)).unwrap_err()
	}

	fn parse_error(tokens: TokenStream2) -> syn::Error {
		match parse2::<DefineVersionedInterfaceInput>(tokens) {
			Ok(_) => panic!("expected interface input parsing to fail"),
			Err(error) => error,
		}
	}

	#[test]
	fn rejects_empty_macro_input() {
		let tokens = quote! {};

		let error = parse_error(tokens);

		assert!(error
			.to_string()
			.contains("requires at least one input and output payload pair"));
	}

	#[test]
	fn generates_payload_structs_and_boxed_versioned_enums_for_valid_pairs() {
		let tokens = quote! {
			#[derive(Clone, Debug)]
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			#[derive(Clone)]
			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
				pub config: DryRunConfig,
			}

			#[derive(Clone)]
			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			#[derive(Clone)]
			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens);

		let file = parse2::<syn::File>(output.clone()).unwrap();
		let item_names = file
			.items
			.iter()
			.filter_map(|item| match item {
				Item::Struct(item) => Some(item.ident.to_string()),
				Item::Enum(item) => Some(item.ident.to_string()),
				Item::Impl(_) => None,
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(
			item_names,
			vec![
				"EthTransactInputPayloadV1",
				"EthTransactInputPayloadV2",
				"EthTransactOutputPayloadV1",
				"EthTransactOutputPayloadV2",
				"VersionedEthTransactInputPayload",
				"VersionedEthTransactOutputPayload",
			]
		);
		let type_aliases = file
			.items
			.iter()
			.filter_map(|item| match item {
				Item::Type(item) => {
					Some((item.ident.to_string(), item.ty.to_token_stream().to_string()))
				},
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(
			type_aliases,
			vec![
				(
					"LatestEthTransactInputPayload".to_owned(),
					"EthTransactInputPayloadV2".to_owned()
				),
				(
					"LatestEthTransactOutputPayload".to_owned(),
					"EthTransactOutputPayloadV2".to_owned(),
				),
			]
		);
		assert!(output.to_string().contains(":: alloc :: boxed :: Box"));
	}

	#[test]
	fn generated_names_strip_trailing_versioned_marker_from_payload_family() {
		let tokens = quote! {
			pub struct EthTransactVersionedInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactVersionedOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens);

		let file = parse2::<syn::File>(output.clone()).unwrap();
		let item_names = file
			.items
			.iter()
			.filter_map(|item| match item {
				Item::Struct(item) => Some(item.ident.to_string()),
				Item::Enum(item) => Some(item.ident.to_string()),
				Item::Type(item) => Some(item.ident.to_string()),
				Item::Impl(_) => None,
				_ => None,
			})
			.collect::<Vec<_>>();
		assert!(item_names.contains(&"EthTransactVersionedInputPayloadV1".to_owned()));
		assert!(item_names.contains(&"EthTransactVersionedOutputPayloadV1".to_owned()));
		assert!(item_names.contains(&"VersionedEthTransactInputPayload".to_owned()));
		assert!(item_names.contains(&"VersionedEthTransactOutputPayload".to_owned()));
		assert!(item_names.contains(&"LatestEthTransactInputPayload".to_owned()));
		assert!(item_names.contains(&"LatestEthTransactOutputPayload".to_owned()));
		assert!(!item_names.contains(&"VersionedEthTransactVersionedInputPayload".to_owned()));
		assert!(!item_names.contains(&"VersionedEthTransactVersionedOutputPayload".to_owned()));
	}

	#[test]
	fn generated_latest_aliases_omit_unenforced_generic_bounds() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<T: Clone>
			where
				T: Default,
			{
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV1<R: Clone>
			where
				R: Default,
			{
				pub result: R,
			}
		};

		let output = expand(tokens);

		let file = parse2::<syn::File>(output).unwrap();
		let aliases = file
			.items
			.iter()
			.filter_map(|item| match item {
				Item::Type(item) => Some(item),
				_ => None,
			})
			.collect::<Vec<_>>();
		assert_eq!(aliases.len(), 2);

		let input_alias = aliases
			.iter()
			.find(|item| item.ident == "LatestEthTransactInputPayload")
			.unwrap();
		assert_eq!(input_alias.generics.to_token_stream().to_string(), "< T >");
		assert!(input_alias.generics.where_clause.is_none());
		assert_eq!(input_alias.ty.to_token_stream().to_string(), "EthTransactInputPayloadV1 < T >");

		let output_alias = aliases
			.iter()
			.find(|item| item.ident == "LatestEthTransactOutputPayload")
			.unwrap();
		assert_eq!(output_alias.generics.to_token_stream().to_string(), "< R >");
		assert!(output_alias.generics.where_clause.is_none());
		assert_eq!(
			output_alias.ty.to_token_stream().to_string(),
			"EthTransactOutputPayloadV1 < R >"
		);
	}

	#[test]
	fn accepts_contiguous_payload_family_starting_after_v1() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV3 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV3 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV4 {
				pub tx: GenericTransaction,
				pub config: DryRunConfig,
			}

			pub struct EthTransactOutputPayloadV4 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens);

		let output_string = output.to_string();
		let file = parse2::<syn::File>(output).unwrap();
		let type_aliases = file
			.items
			.iter()
			.filter_map(|item| match item {
				Item::Type(item) => {
					Some((item.ident.to_string(), item.ty.to_token_stream().to_string()))
				},
				_ => None,
			})
			.collect::<Vec<_>>();
		assert!(output_string.contains("V3"));
		assert!(output_string.contains("V4"));
		assert!(!output_string.contains("V1"));
		assert_eq!(
			type_aliases,
			vec![
				(
					"LatestEthTransactInputPayload".to_owned(),
					"EthTransactInputPayloadV4".to_owned()
				),
				(
					"LatestEthTransactOutputPayload".to_owned(),
					"EthTransactOutputPayloadV4".to_owned(),
				),
			]
		);
	}

	#[test]
	fn rejects_missing_output_payload_for_version() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("No output type defined for V1"));
		assert!(message.contains("Expected `EthTransactOutputPayloadV1`"));
		assert!(message.contains("Input type is defined here"));
	}

	#[test]
	fn rejects_missing_input_payload_for_version() {
		let tokens = quote! {
			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("No input type defined for V1"));
		assert!(message.contains("Expected `EthTransactInputPayloadV1`"));
		assert!(message.contains("Output type is defined here"));
	}

	#[test]
	fn rejects_all_missing_payload_pairs_in_one_diagnostic() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_compile_error().to_string();
		assert!(message.contains("No output type defined for V1"));
		assert!(message.contains("No input type defined for V2"));
	}

	#[test]
	fn rejects_skipped_payload_versions() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV3 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV3 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("must be contiguous"));
		assert!(message.contains("missing version V2 before V3"));
	}

	#[test]
	fn rejects_duplicate_payload_versions_on_same_side() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("duplicate input payload version V1"));
		assert!(message.contains("EthTransact"));
	}

	#[test]
	fn rejects_duplicate_output_payload_versions_on_same_side() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("duplicate output payload version V1"));
		assert!(message.contains("EthTransact"));
	}

	#[test]
	fn rejects_payloads_with_different_family_names() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EstimateGasOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("must use the same family name"));
		assert!(message.contains("EstimateGas"));
		assert!(message.contains("EthTransact"));
	}

	#[test]
	fn rejects_payload_version_with_leading_zero() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV01 {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("must not contain leading zeros"));
	}

	#[test]
	fn rejects_payload_version_zero() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV0 {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("must start at 1"));
	}

	#[test]
	fn rejects_payload_name_with_empty_family_name() {
		let tokens = quote! {
			pub struct InputPayloadV1 {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("non-empty family name"));
	}

	#[test]
	fn rejects_payload_name_with_malformed_suffix() {
		let tokens = quote! {
			pub struct EthTransactPayloadV1 {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("must match"));
	}

	#[test]
	fn rejects_payload_name_with_missing_version_number() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("positive integer after `V`"));
	}

	#[test]
	fn rejects_payload_name_with_non_numeric_version_suffix() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadVNext {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("followed by a positive integer"));
	}

	#[test]
	fn rejects_payload_name_with_extra_suffix_after_version() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1Extra {
				pub tx: GenericTransaction,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("followed by a positive integer"));
	}

	#[test]
	fn rejects_multiple_complete_payload_families_in_one_invocation() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EstimateGasInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			pub struct EstimateGasOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let error = parse_error(tokens);

		let message = error.to_string();
		assert!(message.contains("must use the same family name"));
		assert!(message.contains("EstimateGas"));
		assert!(message.contains("EthTransact"));
	}

	#[test]
	fn rejects_tuple_struct_payloads() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1(pub GenericTransaction);
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("named-field payload structs"));
	}

	#[test]
	fn rejects_unit_struct_payloads() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1;
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("named-field payload structs"));
	}

	#[test]
	fn rejects_enum_items() {
		let tokens = quote! {
			pub enum EthTransactInputPayloadV1 {
				Variant,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_function_items() {
		let tokens = quote! {
			pub fn eth_transact() {}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_module_items() {
		let tokens = quote! {
			pub mod eth_transact {}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_impl_items() {
		let tokens = quote! {
			impl EthTransact {
				pub fn transact() {}
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_type_alias_items() {
		let tokens = quote! {
			pub type EthTransactInputPayloadV1 = GenericTransaction;
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_const_items() {
		let tokens = quote! {
			pub const EthTransactInputPayloadV1: usize = 1;
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_static_items() {
		let tokens = quote! {
			pub static EthTransactInputPayloadV1: usize = 1;
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn rejects_union_items() {
		let tokens = quote! {
			pub union EthTransactInputPayloadV1 {
				pub value: u32,
			}
		};

		let error = parse_error(tokens);

		assert!(error.to_string().contains("expects named struct payload items only"));
	}

	#[test]
	fn preserves_payload_struct_attributes_visibility_fields_and_generics() {
		let tokens = quote! {
			#[doc = "input docs"]
			pub(crate) struct EthTransactInputPayloadV1<T: Clone>
			where
				T: Default,
			{
				tx: GenericTransaction,
				pub marker: T,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		assert!(output.contains("# [doc = \"input docs\"]"));
		assert!(output.contains("pub (crate) struct EthTransactInputPayloadV1"));
		assert!(output.contains("T : Clone"));
		assert!(output.contains("T : Default"));
		assert!(output.contains("tx : GenericTransaction"));
		assert!(output.contains("pub marker : T"));
	}

	#[test]
	fn generated_enums_copy_only_common_derives_per_side() {
		let tokens = quote! {
			#[derive(Clone, Debug)]
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			#[derive(Clone)]
			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			#[derive(Clone, Debug)]
			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			#[derive(Clone, Debug)]
			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens);

		let output = output.to_string();
		let derives = output.matches("# [derive").collect::<Vec<_>>();
		assert_eq!(derives.len(), 6);
		assert!(output.to_string().contains("pub enum VersionedEthTransactInputPayload"));
		assert!(output.to_string().contains("pub enum VersionedEthTransactOutputPayload"));
	}

	#[test]
	fn generated_helpers_are_present_for_every_version() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		for method in [
			"LATEST_VERSION",
			"new_v1",
			"new_v2",
			"version",
			"as_v1",
			"as_v2",
			"into_v1",
			"into_v2",
			"unwrap_v1",
			"unwrap_v2",
		] {
			assert!(output.contains(method));
		}
	}

	#[test]
	fn generated_enum_generics_merge_inline_bounds_by_side() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<T: Clone> {
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2<T: Default> {
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		assert!(
			output.contains("pub enum VersionedEthTransactInputPayload < T : Clone + Default >")
		);
		assert!(output.contains("pub enum VersionedEthTransactOutputPayload"));
	}

	#[test]
	fn generated_enum_generics_union_where_clauses_by_side() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<T>
			where
				T: Clone,
			{
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2<T>
			where
				T: Default,
			{
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		assert!(output.contains("where T : Clone , T : Default"));
	}

	#[test]
	fn rejects_same_name_generic_parameters_with_different_kinds() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<T> {
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2<const T: usize> {
				pub tx: [u8; T],
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let error = expand_error(tokens);

		let message = error.to_string();
		assert!(message.contains("generic parameter `T` is used as a const parameter here"));
		assert!(message.contains("already used as a type parameter"));
	}

	#[test]
	fn rejects_same_name_const_generics_with_different_types() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<const N: usize> {
				pub tx: [u8; N],
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2<const N: u32> {
				pub tx: [u8; N],
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let error = expand_error(tokens);

		let message = error.to_string();
		assert!(message.contains("const generic parameter `N` has type `u32` here"));
		assert!(message.contains("already defined with type `usize`"));
	}

	#[test]
	fn preserves_precise_error_context_for_malformed_payload_derive() {
		let tokens = quote! {
			#[derive(Clone, Debug())]
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let error = expand_error(tokens);

		let message = error.to_compile_error().to_string();
		assert!(message.contains("failed to parse payload derive attribute"));
		assert!(!message.contains("for generated enum"));
	}

	#[test]
	fn generates_from_impls_for_every_payload_version_on_both_sides() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		for expected in [
			":: core :: convert :: From < EthTransactInputPayloadV1 > for \
			 VersionedEthTransactInputPayload",
			":: core :: convert :: From < EthTransactInputPayloadV2 > for \
			 VersionedEthTransactInputPayload",
			":: core :: convert :: From < EthTransactOutputPayloadV1 > for \
			 VersionedEthTransactOutputPayload",
			":: core :: convert :: From < EthTransactOutputPayloadV2 > for \
			 VersionedEthTransactOutputPayload",
		] {
			assert!(output.contains(expected), "missing impl: {expected}\nin output: {output}");
		}
	}

	#[test]
	fn generates_try_from_impls_with_unit_error_for_every_payload_version() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		for expected in [
			":: core :: convert :: TryFrom < VersionedEthTransactInputPayload > for \
			 EthTransactInputPayloadV1",
			":: core :: convert :: TryFrom < VersionedEthTransactInputPayload > for \
			 EthTransactInputPayloadV2",
			":: core :: convert :: TryFrom < VersionedEthTransactOutputPayload > for \
			 EthTransactOutputPayloadV1",
			":: core :: convert :: TryFrom < VersionedEthTransactOutputPayload > for \
			 EthTransactOutputPayloadV2",
		] {
			assert!(output.contains(expected), "missing impl: {expected}\nin output: {output}");
		}
		assert_eq!(output.matches("type Error = () ;").count(), 4);
	}

	#[test]
	fn generated_try_from_match_arms_handle_every_variant_explicitly() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		for arm in [
			"VersionedEthTransactInputPayload :: V1 (value) => :: core :: result :: Result :: Ok \
			 (* value)",
			"VersionedEthTransactInputPayload :: V2 (..) => :: core :: result :: Result :: Err (())",
			"VersionedEthTransactInputPayload :: V1 (..) => :: core :: result :: Result :: Err (())",
			"VersionedEthTransactInputPayload :: V2 (value) => :: core :: result :: Result :: Ok \
			 (* value)",
		] {
			assert!(output.contains(arm), "missing arm: {arm}\nin output: {output}");
		}
	}

	#[test]
	fn generated_try_from_for_single_variant_enum_omits_wildcard_arm() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1 {
				pub tx: GenericTransaction,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		assert!(output.contains(
			"VersionedEthTransactInputPayload :: V1 (value) => :: core :: result :: Result :: Ok \
			 (* value)"
		));
		assert!(!output.contains(":: core :: result :: Result :: Err (())"));
	}

	#[test]
	fn generated_from_and_try_from_impls_inherit_merged_generics() {
		let tokens = quote! {
			pub struct EthTransactInputPayloadV1<T: Clone> {
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV1 {
				pub result: EthTransactInfo,
			}

			pub struct EthTransactInputPayloadV2<T: Default> {
				pub tx: T,
			}

			pub struct EthTransactOutputPayloadV2 {
				pub result: EthTransactInfo,
			}
		};

		let output = expand(tokens).to_string();

		assert!(output.contains(
			"impl < T : Clone + Default > :: core :: convert :: From < \
			 EthTransactInputPayloadV1 < T > > for VersionedEthTransactInputPayload < T >"
		));
		assert!(output.contains(
			"impl < T : Clone + Default > :: core :: convert :: TryFrom < \
			 VersionedEthTransactInputPayload < T > > for EthTransactInputPayloadV1 < T >"
		));
	}
}
