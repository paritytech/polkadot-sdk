// Copyright 2017-2020 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Parsing of decl_storage input.

use frame_support_procedural_tools::{ToTokens, Parse, syn_ext as ext};
use syn::{Ident, Token, spanned::Spanned};

mod keyword {
	syn::custom_keyword!(hiddencrate);
	syn::custom_keyword!(add_extra_genesis);
	syn::custom_keyword!(extra_genesis_skip_phantom_data_field);
	syn::custom_keyword!(config);
	syn::custom_keyword!(build);
	syn::custom_keyword!(get);
	syn::custom_keyword!(map);
	syn::custom_keyword!(linked_map);
	syn::custom_keyword!(double_map);
	syn::custom_keyword!(blake2_256);
	syn::custom_keyword!(blake2_128);
	syn::custom_keyword!(blake2_128_concat);
	syn::custom_keyword!(twox_256);
	syn::custom_keyword!(twox_128);
	syn::custom_keyword!(twox_64_concat);
	syn::custom_keyword!(hasher);
}

/// Specific `Opt` to implement structure with optional parsing
#[derive(Debug, Clone)]
pub struct Opt<P> {
	pub inner: Option<P>,
}
impl<P: syn::export::ToTokens> syn::export::ToTokens for Opt<P> {
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		if let Some(ref p) = self.inner {
			p.to_tokens(tokens);
		}
	}
}

macro_rules! impl_parse_for_opt {
	($struct:ident => $token:path) => {
		impl syn::parse::Parse for Opt<$struct> {
			fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
				if input.peek($token) {
					input.parse().map(|p| Opt { inner: Some(p) })
				} else {
					Ok(Opt { inner: None })
				}
			}
		}
	};
}

/// Parsing usage only
#[derive(Parse, ToTokens, Debug)]
struct StorageDefinition {
	pub hidden_crate: Opt<SpecificHiddenCrate>,
	pub visibility: syn::Visibility,
	pub trait_token: Token![trait],
	pub ident: Ident,
	pub for_token: Token![for],
	pub module_ident: Ident,
	pub mod_lt_token: Token![<],
	pub mod_param_generic: syn::Ident,
	pub mod_param_bound_token: Option<Token![:]>,
	pub mod_param_bound: syn::Path,
	pub mod_instance_param_token: Option<Token![,]>,
	pub mod_instance: Option<syn::Ident>,
	pub mod_instantiable_token: Option<Token![:]>,
	pub mod_instantiable: Option<syn::Ident>,
	pub mod_default_instance_token: Option<Token![=]>,
	pub mod_default_instance: Option<syn::Ident>,
	pub mod_gt_token: Token![>],
	pub as_token: Token![as],
	pub crate_ident: Ident,
	pub where_clause: Option<syn::WhereClause>,
	pub content: ext::Braces<ext::Punctuated<DeclStorageLine, Token![;]>>,
	pub extra_genesis: Opt<AddExtraGenesis>,
}

#[derive(Parse, ToTokens, Debug)]
struct SpecificHiddenCrate {
	pub keyword: keyword::hiddencrate,
	pub ident: ext::Parens<Ident>,
}
impl_parse_for_opt!(SpecificHiddenCrate => keyword::hiddencrate);

#[derive(Parse, ToTokens, Debug)]
struct AddExtraGenesis {
	pub extragenesis_keyword: keyword::add_extra_genesis,
	pub content: ext::Braces<AddExtraGenesisContent>,
}

impl_parse_for_opt!(AddExtraGenesis => keyword::add_extra_genesis);

#[derive(Parse, ToTokens, Debug)]
struct AddExtraGenesisContent {
	pub lines: ext::Punctuated<AddExtraGenesisLineEnum, Token![;]>,
}

#[derive(ToTokens, Debug)]
enum AddExtraGenesisLineEnum {
	AddExtraGenesisLine(AddExtraGenesisLine),
	AddExtraGenesisBuild(DeclStorageBuild),
}

impl syn::parse::Parse for AddExtraGenesisLineEnum {
	fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
		let input_fork = input.fork();
		// OuterAttributes are forbidden for build variant,
		// However to have better documentation we match against the keyword after those attributes.
		let _: ext::OuterAttributes = input_fork.parse()?;
		let lookahead = input_fork.lookahead1();
		if lookahead.peek(keyword::build) {
			Ok(Self::AddExtraGenesisBuild(input.parse()?))
		} else if lookahead.peek(keyword::config) {
			Ok(Self::AddExtraGenesisLine(input.parse()?))
		} else {
			Err(lookahead.error())
		}
	}
}

#[derive(Parse, ToTokens, Debug)]
struct AddExtraGenesisLine {
	pub attrs: ext::OuterAttributes,
	pub config_keyword: keyword::config,
	pub extra_field: ext::Parens<Ident>,
	pub coldot_token: Token![:],
	pub extra_type: syn::Type,
	pub default_value: Opt<DeclStorageDefault>,
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageLine {
	// attrs (main use case is doc)
	pub attrs: ext::OuterAttributes,
	// visibility (no need to make optional
	pub visibility: syn::Visibility,
	// name
	pub name: Ident,
	pub getter: Opt<DeclStorageGetter>,
	pub config: Opt<DeclStorageConfig>,
	pub build: Opt<DeclStorageBuild>,
	pub coldot_token: Token![:],
	pub storage_type: DeclStorageType,
	pub default_value: Opt<DeclStorageDefault>,
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageGetterBody {
	fn_keyword: Option<Token![fn]>,
	ident: Ident,
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageGetter {
	pub getter_keyword: keyword::get,
	pub getfn: ext::Parens<DeclStorageGetterBody>,
}

impl_parse_for_opt!(DeclStorageGetter => keyword::get);

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageConfig {
	pub config_keyword: keyword::config,
	pub expr: ext::Parens<Option<syn::Ident>>,
}

impl_parse_for_opt!(DeclStorageConfig => keyword::config);

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageBuild {
	pub build_keyword: keyword::build,
	pub expr: ext::Parens<syn::Expr>,
}

impl_parse_for_opt!(DeclStorageBuild => keyword::build);

#[derive(ToTokens, Debug)]
enum DeclStorageType {
	Map(DeclStorageMap),
	LinkedMap(DeclStorageLinkedMap),
	DoubleMap(DeclStorageDoubleMap),
	Simple(syn::Type),
}

impl syn::parse::Parse for DeclStorageType {
	fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
		if input.peek(keyword::map) {
			Ok(Self::Map(input.parse()?))
		} else if input.peek(keyword::linked_map) {
			Ok(Self::LinkedMap(input.parse()?))
		} else if input.peek(keyword::double_map) {
			Ok(Self::DoubleMap(input.parse()?))
		} else {
			Ok(Self::Simple(input.parse()?))
		}
	}
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageMap {
	pub map_keyword: keyword::map,
	pub hasher: Opt<SetHasher>,
	pub key: syn::Type,
	pub ass_keyword: Token![=>],
	pub value: syn::Type,
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageLinkedMap {
	pub map_keyword: keyword::linked_map,
	pub hasher: Opt<SetHasher>,
	pub key: syn::Type,
	pub ass_keyword: Token![=>],
	pub value: syn::Type,
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageDoubleMap {
	pub map_keyword: keyword::double_map,
	pub hasher1: Opt<SetHasher>,
	pub key1: syn::Type,
	pub comma_keyword: Token![,],
	pub hasher2: Opt<SetHasher>,
	pub key2: syn::Type,
	pub ass_keyword: Token![=>],
	pub value: syn::Type,
}

#[derive(ToTokens, Debug)]
enum Hasher {
	Blake2_256(keyword::blake2_256),
	Blake2_128(keyword::blake2_128),
	Blake2_128Concat(keyword::blake2_128_concat),
	Twox256(keyword::twox_256),
	Twox128(keyword::twox_128),
	Twox64Concat(keyword::twox_64_concat),
}

impl syn::parse::Parse for Hasher {
	fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
		let lookahead = input.lookahead1();
		if lookahead.peek(keyword::blake2_256) {
			Ok(Self::Blake2_256(input.parse()?))
		} else if lookahead.peek(keyword::blake2_128) {
			Ok(Self::Blake2_128(input.parse()?))
		} else if lookahead.peek(keyword::blake2_128_concat) {
			Ok(Self::Blake2_128Concat(input.parse()?))
		} else if lookahead.peek(keyword::twox_256) {
			Ok(Self::Twox256(input.parse()?))
		} else if lookahead.peek(keyword::twox_128) {
			Ok(Self::Twox128(input.parse()?))
		} else if lookahead.peek(keyword::twox_64_concat) {
			Ok(Self::Twox64Concat(input.parse()?))
		} else {
			Err(lookahead.error())
		}
	}
}

#[derive(Parse, ToTokens, Debug)]
struct DeclStorageDefault {
	pub equal_token: Token![=],
	pub expr: syn::Expr,
}

impl syn::parse::Parse for Opt<DeclStorageDefault> {
	fn parse(input: syn::parse::ParseStream) -> syn::parse::Result<Self> {
		if input.peek(Token![=]) {
			input.parse().map(|p| Opt { inner: Some(p) })
		} else {
			Ok(Opt { inner: None })
		}
	}
}

#[derive(Parse, ToTokens, Debug)]
struct SetHasher {
	pub hasher_keyword: keyword::hasher,
	pub inner: ext::Parens<Hasher>,
}

impl_parse_for_opt!(SetHasher => keyword::hasher);

impl From<SetHasher> for super::HasherKind {
	fn from(set_hasher: SetHasher) -> Self {
		set_hasher.inner.content.into()
	}
}

impl From<Hasher> for super::HasherKind {
	fn from(hasher: Hasher) -> Self {
		match hasher {
			Hasher::Blake2_256(_) => super::HasherKind::Blake2_256,
			Hasher::Blake2_128(_) => super::HasherKind::Blake2_128,
			Hasher::Blake2_128Concat(_) => super::HasherKind::Blake2_128Concat,
			Hasher::Twox256(_) => super::HasherKind::Twox256,
			Hasher::Twox128(_) => super::HasherKind::Twox128,
			Hasher::Twox64Concat(_) => super::HasherKind::Twox64Concat,
		}
	}
}

fn get_module_instance(
	instance: Option<syn::Ident>,
	instantiable: Option<syn::Ident>,
	default_instance: Option<syn::Ident>,
) -> syn::Result<Option<super::ModuleInstanceDef>> {
	let right_syntax = "Should be $Instance: $Instantiable = $DefaultInstance";

	match (instance, instantiable, default_instance) {
		(Some(instance), Some(instantiable), default_instance) => {
			Ok(Some(super::ModuleInstanceDef {
				instance_generic: instance,
				instance_trait: instantiable,
				instance_default: default_instance,
			}))
		},
		(None, None, None) => Ok(None),
		(Some(instance), None, _) => Err(
			syn::Error::new(
				instance.span(),
				format!(
					"Expect instantiable trait bound for instance: {}. {}",
					instance,
					right_syntax,
				)
			)
		),
		(None, Some(instantiable), _) => Err(
			syn::Error::new(
				instantiable.span(),
				format!(
					"Expect instance generic for bound instantiable: {}. {}",
					instantiable,
					right_syntax,
				)
			)
		),
		(None, _, Some(default_instance)) => Err(
			syn::Error::new(
				default_instance.span(),
				format!(
					"Expect instance generic for default instance: {}. {}",
					default_instance,
					right_syntax,
				)
			)
		),
	}
}

pub fn parse(input: syn::parse::ParseStream) -> syn::Result<super::DeclStorageDef> {
	use syn::parse::Parse;

	let def = StorageDefinition::parse(input)?;

	let module_instance = get_module_instance(
		def.mod_instance,
		def.mod_instantiable,
		def.mod_default_instance,
	)?;

	let mut extra_genesis_config_lines = vec![];
	let mut extra_genesis_build = None;

	for line in def.extra_genesis.inner.into_iter()
		.flat_map(|o| o.content.content.lines.inner.into_iter())
	{
		match line {
			AddExtraGenesisLineEnum::AddExtraGenesisLine(def) => {
				extra_genesis_config_lines.push(super::ExtraGenesisLineDef{
					attrs: def.attrs.inner,
					name: def.extra_field.content,
					typ: def.extra_type,
					default: def.default_value.inner.map(|o| o.expr),
				});
			}
			AddExtraGenesisLineEnum::AddExtraGenesisBuild(def) => {
				if extra_genesis_build.is_some() {
					return Err(syn::Error::new(
						def.span(),
						"Only one build expression allowed for extra genesis"
					))
				}

				extra_genesis_build = Some(def.expr.content);
			}
		}
	}

	let storage_lines = parse_storage_line_defs(def.content.content.inner.into_iter())?;

	Ok(super::DeclStorageDef {
		hidden_crate: def.hidden_crate.inner.map(|i| i.ident.content),
		visibility: def.visibility,
		module_name: def.module_ident,
		store_trait: def.ident,
		module_runtime_generic: def.mod_param_generic,
		module_runtime_trait: def.mod_param_bound,
		where_clause: def.where_clause,
		crate_name: def.crate_ident,
		module_instance,
		extra_genesis_build,
		extra_genesis_config_lines,
		storage_lines,
	})
}

/// Parse the `DeclStorageLine` into `StorageLineDef`.
fn parse_storage_line_defs(
	defs: impl Iterator<Item = DeclStorageLine>,
) -> syn::Result<Vec<super::StorageLineDef>> {
	let mut storage_lines = Vec::<super::StorageLineDef>::new();

	for line in defs {
		let getter = line.getter.inner.map(|o| o.getfn.content.ident);
		let config = if let Some(config) = line.config.inner {
			if let Some(ident) = config.expr.content {
				Some(ident)
			} else if let Some(ref ident) = getter {
				Some(ident.clone())
			} else {
				return Err(syn::Error::new(
					config.span(),
					"Invalid storage definition, couldn't find config identifier: storage must \
					either have a get identifier `get(fn ident)` or a defined config identifier \
					`config(ident)`",
				))
			}
		} else {
			None
		};

		if let Some(ref config) = config {
			storage_lines.iter().filter_map(|sl| sl.config.as_ref()).try_for_each(|other_config| {
				if other_config == config {
					Err(syn::Error::new(
						config.span(),
						"`config()`/`get()` with the same name already defined.",
					))
				} else {
					Ok(())
				}
			})?;
		}

		let span = line.storage_type.span();
		let no_hasher_error = || syn::Error::new(
			span,
			"Default hasher has been removed, use explicit hasher(blake2_256) instead."
		);

		let storage_type = match line.storage_type {
			DeclStorageType::Map(map) => super::StorageLineTypeDef::Map(
				super::MapDef {
					hasher: map.hasher.inner.ok_or_else(no_hasher_error)?.into(),
					key: map.key,
					value: map.value,
				}
			),
			DeclStorageType::LinkedMap(map) => super::StorageLineTypeDef::LinkedMap(
				super::MapDef {
					hasher: map.hasher.inner.ok_or_else(no_hasher_error)?.into(),
					key: map.key,
					value: map.value,
				}
			),
			DeclStorageType::DoubleMap(map) => super::StorageLineTypeDef::DoubleMap(
				super::DoubleMapDef {
					hasher1: map.hasher1.inner.ok_or_else(no_hasher_error)?.into(),
					hasher2: map.hasher2.inner.ok_or_else(no_hasher_error)?.into(),
					key1: map.key1,
					key2: map.key2,
					value: map.value,
				}
			),
			DeclStorageType::Simple(expr) => super::StorageLineTypeDef::Simple(expr),
		};

		storage_lines.push(super::StorageLineDef {
			attrs: line.attrs.inner,
			visibility: line.visibility,
			name: line.name,
			getter,
			config,
			build: line.build.inner.map(|o| o.expr.content),
			default_value: line.default_value.inner.map(|o| o.expr),
			storage_type,
		})
	}

	Ok(storage_lines)
}
