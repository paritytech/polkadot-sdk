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

//! Compares the old and current `Revive` storage metadata by SCALE wire layout.
//!
//! The comparator deliberately ignores names inside type definitions, documentation, and type
//! paths. It checks the storage entry set and recursively compares only the structure that
//! contributes to encoded storage bytes. Product types are flattened so newtype wrappers and
//! single-field storage wrappers do not cause false incompatibilities.

use std::collections::{BTreeMap, BTreeSet};

use anyhow::{bail, Context as _, Result};
use codec::Decode;
use frame_metadata::{
	v16::{RuntimeMetadataV16, StorageEntryMetadata, StorageEntryType},
	RuntimeMetadata, RuntimeMetadataPrefixed,
};
use scale_info::{form::PortableForm, Field, PortableRegistry, TypeDef, TypeDefPrimitive, Variant};

const OLD_METADATA: &[u8] = include_bytes!(concat!(
	env!("CARGO_MANIFEST_DIR"),
	"/tests/assets/pre-storage-versioning-storage-metadata.scale"
));

#[test]
fn check_revive_storage_types_preserve_wire_layout() -> Result<()> {
	// Arrange
	let old_metadata = OLD_METADATA;

	// Act & Assert
	compare_current_metadata_against_old(old_metadata)
}

fn compare_current_metadata_against_old(old: &[u8]) -> Result<()> {
	let old = decode_metadata_as_v16(old)?;
	let new = current_metadata_v16()?;

	let old_storage_data = StorageData::new(&old)?;
	let new_storage_data = StorageData::new(&new)?;

	if old_storage_data.storage_prefix != new_storage_data.storage_prefix {
		bail!(
			"Old and new storage prefixes are not the same. Old = {}, New = {}",
			old_storage_data.storage_prefix,
			new_storage_data.storage_prefix
		)
	}
	println!("✅ Storage prefixes match");

	for old_key in old_storage_data.storage_entries.keys() {
		if !new_storage_data.storage_entries.contains_key(old_key) {
			bail!("Storage key present in the old metadata but not in the new one: {}", old_key)
		}
	}
	println!("✅ Every old storage entry exists in the new metadata");

	for new_key in new_storage_data.storage_entries.keys() {
		if !old_storage_data.storage_entries.contains_key(new_key) {
			bail!("Storage key present in the new metadata but not in the old one: {}", new_key)
		}
	}
	println!("✅ Every new storage entry exists in the old metadata");

	// Safe to zip them together. They're both in a BTreeMap so they're sorted by key and both have
	// the same set of keys at this point.
	let storage_entries = old_storage_data
		.storage_entries
		.iter()
		.zip(new_storage_data.storage_entries.iter())
		.map(|((old_key, old_entry), (_, new_entry))| (*old_key, (*old_entry, *new_entry)));

	for (key, (old_entry, new_entry)) in storage_entries {
		compare_storage_entries(key, old_entry, new_entry, &old_storage_data, &new_storage_data)?;
	}

	Ok(())
}

fn current_metadata_v16() -> Result<RuntimeMetadataV16> {
	let mut ext = sp_io::TestExternalities::new(Default::default());
	let encoded = ext
		.execute_with(|| {
			revive_dev_runtime::Runtime::metadata_at_version(16).map(|metadata| {
				let bytes: &[u8] = &metadata;
				bytes.to_vec()
			})
		})
		.context("current runtime does not expose metadata v16")?;
	decode_metadata_as_v16(&encoded)
}

fn decode_metadata_as_v16(input: &[u8]) -> Result<RuntimeMetadataV16> {
	let input = {
		let mut input = input;
		RuntimeMetadataPrefixed::decode(&mut input)?.1
	};
	let RuntimeMetadata::V16(input) = input else {
		bail!("Encountered metadata which isn't a v16")
	};
	Ok(input)
}

fn compare_storage_entries(
	key: &str,
	old_entry: &StorageEntryMetadata<PortableForm>,
	new_entry: &StorageEntryMetadata<PortableForm>,
	old_storage_data: &StorageData<'_>,
	new_storage_data: &StorageData<'_>,
) -> Result<()> {
	println!("🔮 Working on the storage entry '{key}'");
	let mut comparator = TypeComparator::new(old_storage_data.types, new_storage_data.types);

	if old_entry.modifier != new_entry.modifier {
		bail!(
			"Storage entry '{key}' has different modifiers. Old = {:?}, New = {:?}",
			old_entry.modifier,
			new_entry.modifier,
		);
	}
	println!("✅ Storage entry '{key}' modifiers match");

	if old_entry.default != new_entry.default {
		bail!("Storage entry '{key}' has a different default value");
	}
	println!("✅ Storage entry '{key}' default values match");

	match (&old_entry.ty, &new_entry.ty) {
		(StorageEntryType::Plain(old_ty), StorageEntryType::Plain(new_ty)) => {
			println!("✅ Storage entry '{key}' is plain in both metadata versions");

			comparator.compare_types(&format!("{key}.value"), old_ty.id, new_ty.id)?;
			println!("✅ Storage entry '{key}' value encoding shapes match");

			Ok(())
		},
		(
			StorageEntryType::Map { hashers: old_hashers, key: old_key, value: old_value },
			StorageEntryType::Map { hashers: new_hashers, key: new_key, value: new_value },
		) => {
			println!("✅ Storage entry '{key}' is a map in both metadata versions");

			if old_hashers != new_hashers {
				bail!(
					"Storage entry '{key}' has different hashers. Old = {:?}, New = {:?}",
					old_hashers,
					new_hashers,
				);
			}
			println!("✅ Storage entry '{key}' map hashers match");

			comparator.compare_types(&format!("{key}.key"), old_key.id, new_key.id)?;
			println!("✅ Storage entry '{key}' key encoding shapes match");

			comparator.compare_types(&format!("{key}.value"), old_value.id, new_value.id)?;
			println!("✅ Storage entry '{key}' value encoding shapes match");

			Ok(())
		},
		(StorageEntryType::Plain(_), StorageEntryType::Map { .. }) => {
			bail!("Storage entry '{key}' changed from plain to map")
		},
		(StorageEntryType::Map { .. }, StorageEntryType::Plain(_)) => {
			bail!("Storage entry '{key}' changed from map to plain")
		},
	}
}

struct TypeComparator<'a> {
	old_registry: &'a PortableRegistry,
	new_registry: &'a PortableRegistry,
	in_progress: BTreeSet<TypePair>,
}

impl<'a> TypeComparator<'a> {
	fn new(old_registry: &'a PortableRegistry, new_registry: &'a PortableRegistry) -> Self {
		Self { old_registry, new_registry, in_progress: BTreeSet::new() }
	}

	fn compare_types(&mut self, path: &str, old_id: u32, new_id: u32) -> Result<()> {
		let pair = TypePair { old_id, new_id };
		if !self.in_progress.insert(pair) {
			return Ok(());
		}

		let result = self.compare_types_inner(path, old_id, new_id);
		self.in_progress.remove(&pair);
		result
	}

	fn compare_types_inner(&mut self, path: &str, old_id: u32, new_id: u32) -> Result<()> {
		let old_type_def = self.resolve(RegistrySide::Old, old_id, path)?.type_def.clone();
		let new_type_def = self.resolve(RegistrySide::New, new_id, path)?.type_def.clone();

		if is_product(&old_type_def) || is_product(&new_type_def) {
			let old_atoms =
				self.encoding_atoms(RegistrySide::Old, old_id, path, &mut Vec::new())?;
			let new_atoms =
				self.encoding_atoms(RegistrySide::New, new_id, path, &mut Vec::new())?;
			return self.compare_encoding_atoms(path, &old_atoms, &new_atoms);
		}

		match (&old_type_def, &new_type_def) {
			(TypeDef::Variant(old), TypeDef::Variant(new)) => {
				self.compare_variants(path, old_id, &old.variants, new_id, &new.variants)
			},
			(old, new) => {
				let old_atom = self.encoding_atom_for_non_product(old_id, old)?;
				let new_atom = self.encoding_atom_for_non_product(new_id, new)?;
				self.compare_encoding_atom(path, &old_atom, &new_atom)
			},
		}
	}

	fn compare_encoding_atoms(
		&mut self,
		path: &str,
		old: &[EncodingAtom],
		new: &[EncodingAtom],
	) -> Result<()> {
		if old.len() != new.len() {
			bail!("{path} encoding atom count differs. Old = {}, New = {}", old.len(), new.len());
		}

		for (index, (old_atom, new_atom)) in old.iter().zip(new).enumerate() {
			let atom_path =
				if index == 0 { path.to_string() } else { format!("{path}.atom[{index}]") };
			self.compare_encoding_atom(&atom_path, old_atom, new_atom)?;
		}

		Ok(())
	}

	fn compare_encoding_atom(
		&mut self,
		path: &str,
		old: &EncodingAtom,
		new: &EncodingAtom,
	) -> Result<()> {
		match (old, new) {
			(EncodingAtom::Primitive(old), EncodingAtom::Primitive(new)) => {
				if old == new {
					Ok(())
				} else {
					bail!("{path} primitive differs. Old = {:?}, New = {:?}", old, new)
				}
			},
			(EncodingAtom::Sequence(old), EncodingAtom::Sequence(new)) => {
				self.compare_type_refs(&format!("{path}.sequence_element"), old, new)
			},
			(EncodingAtom::Compact(old), EncodingAtom::Compact(new)) => {
				self.compare_type_refs(&format!("{path}.compact"), old, new)
			},
			(EncodingAtom::Variant(old), EncodingAtom::Variant(new)) => {
				self.compare_types(path, *old, *new)
			},
			(
				EncodingAtom::BitSequence { bit_store: old_bit_store, bit_order: old_bit_order },
				EncodingAtom::BitSequence { bit_store: new_bit_store, bit_order: new_bit_order },
			) => {
				self.compare_type_refs(&format!("{path}.bit_store"), old_bit_store, new_bit_store)?;
				self.compare_type_refs(&format!("{path}.bit_order"), old_bit_order, new_bit_order)
			},
			(old, new) => bail!(
				"{path} encoding atom kind differs. Old = {}, New = {}",
				old.kind(),
				new.kind()
			),
		}
	}

	fn compare_type_refs(
		&mut self,
		path: &str,
		old: &EncodingType,
		new: &EncodingType,
	) -> Result<()> {
		match (old, new) {
			(EncodingType::Id(old), EncodingType::Id(new)) => self.compare_types(path, *old, *new),
			(EncodingType::Primitive(old), EncodingType::Primitive(new)) => self
				.compare_encoding_atom(
					path,
					&EncodingAtom::Primitive(old.clone()),
					&EncodingAtom::Primitive(new.clone()),
				),
			(EncodingType::Id(old), EncodingType::Primitive(new)) => {
				let old_atoms =
					self.encoding_atoms(RegistrySide::Old, *old, path, &mut Vec::new())?;
				let new_atoms = vec![EncodingAtom::Primitive(new.clone())];
				self.compare_encoding_atoms(path, &old_atoms, &new_atoms)
			},
			(EncodingType::Primitive(old), EncodingType::Id(new)) => {
				let old_atoms = vec![EncodingAtom::Primitive(old.clone())];
				let new_atoms =
					self.encoding_atoms(RegistrySide::New, *new, path, &mut Vec::new())?;
				self.compare_encoding_atoms(path, &old_atoms, &new_atoms)
			},
		}
	}

	fn compare_variants(
		&mut self,
		path: &str,
		old_id: u32,
		old: &[Variant<PortableForm>],
		new_id: u32,
		new: &[Variant<PortableForm>],
	) -> Result<()> {
		let old_variants = variants_by_index(old_id, old)?;
		let new_variants = variants_by_index(new_id, new)?;
		let old_indexes = old_variants.keys().copied().collect::<BTreeSet<_>>();
		let new_indexes = new_variants.keys().copied().collect::<BTreeSet<_>>();

		if old_indexes != new_indexes {
			let missing = old_indexes.difference(&new_indexes).copied().collect::<Vec<_>>();
			let extra = new_indexes.difference(&old_indexes).copied().collect::<Vec<_>>();
			bail!(
				"{path} variant indexes differ. Missing from new = {:?}, extra in new = {:?}",
				missing,
				extra
			);
		}

		for index in old_indexes {
			let variant_path = format!("{path}.variant[{index}]");
			let old_atoms = self.encoding_atoms_from_fields(
				RegistrySide::Old,
				&old_variants[&index].fields,
				&variant_path,
				&mut Vec::new(),
			)?;
			let new_atoms = self.encoding_atoms_from_fields(
				RegistrySide::New,
				&new_variants[&index].fields,
				&variant_path,
				&mut Vec::new(),
			)?;
			self.compare_encoding_atoms(&variant_path, &old_atoms, &new_atoms)?;
		}

		Ok(())
	}

	fn encoding_atoms(
		&self,
		side: RegistrySide,
		type_id: u32,
		path: &str,
		stack: &mut Vec<u32>,
	) -> Result<Vec<EncodingAtom>> {
		if stack.contains(&type_id) {
			bail!("{path}: type #{type_id} recursively expands without a wire boundary");
		}

		stack.push(type_id);
		let result = (|| {
			let ty = self.resolve(side, type_id, path)?;
			match &ty.type_def {
				TypeDef::Composite(composite) => {
					self.encoding_atoms_from_fields(side, &composite.fields, path, stack)
				},
				TypeDef::Tuple(tuple) => self.encoding_atoms_from_type_ids(
					side,
					tuple.fields.iter().map(|field| field.id),
					path,
					stack,
				),
				TypeDef::Array(array) => {
					let element_atoms =
						self.encoding_atoms(side, array.type_param.id, path, stack)?;
					let mut atoms = Vec::new();
					for _ in 0..array.len {
						atoms.extend(element_atoms.clone());
					}
					Ok(atoms)
				},
				type_def => Ok(vec![self.encoding_atom_for_non_product(type_id, type_def)?]),
			}
		})();
		stack.pop();
		result
	}

	fn encoding_atoms_from_fields(
		&self,
		side: RegistrySide,
		fields: &[Field<PortableForm>],
		path: &str,
		stack: &mut Vec<u32>,
	) -> Result<Vec<EncodingAtom>> {
		self.encoding_atoms_from_type_ids(side, fields.iter().map(|field| field.ty.id), path, stack)
	}

	fn encoding_atoms_from_type_ids(
		&self,
		side: RegistrySide,
		type_ids: impl IntoIterator<Item = u32>,
		path: &str,
		stack: &mut Vec<u32>,
	) -> Result<Vec<EncodingAtom>> {
		let mut atoms = Vec::new();

		for type_id in type_ids {
			atoms.extend(self.encoding_atoms(side, type_id, path, stack)?);
		}

		Ok(atoms)
	}

	fn encoding_atom_for_non_product(
		&self,
		type_id: u32,
		type_def: &TypeDef<PortableForm>,
	) -> Result<EncodingAtom> {
		match type_def {
			TypeDef::Primitive(TypeDefPrimitive::Str) => {
				Ok(EncodingAtom::Sequence(EncodingType::Primitive(TypeDefPrimitive::U8)))
			},
			TypeDef::Primitive(primitive) => Ok(EncodingAtom::Primitive(primitive.clone())),
			TypeDef::Sequence(sequence) => {
				Ok(EncodingAtom::Sequence(EncodingType::Id(sequence.type_param.id)))
			},
			TypeDef::Compact(compact) => {
				Ok(EncodingAtom::Compact(EncodingType::Id(compact.type_param.id)))
			},
			TypeDef::Variant(_) => Ok(EncodingAtom::Variant(type_id)),
			TypeDef::BitSequence(bit_sequence) => Ok(EncodingAtom::BitSequence {
				bit_store: EncodingType::Id(bit_sequence.bit_store_type.id),
				bit_order: EncodingType::Id(bit_sequence.bit_order_type.id),
			}),
			TypeDef::Composite(_) | TypeDef::Tuple(_) | TypeDef::Array(_) => {
				bail!("product type #{type_id} cannot be represented as one encoding atom")
			},
		}
	}

	fn resolve(
		&self,
		side: RegistrySide,
		type_id: u32,
		path: &str,
	) -> Result<&'a scale_info::Type<PortableForm>> {
		self.registry(side)
			.resolve(type_id)
			.with_context(|| format!("{path}: failed to find metadata type #{type_id}"))
	}

	fn registry(&self, side: RegistrySide) -> &'a PortableRegistry {
		match side {
			RegistrySide::Old => self.old_registry,
			RegistrySide::New => self.new_registry,
		}
	}
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct TypePair {
	old_id: u32,
	new_id: u32,
}

#[derive(Clone, Copy, Debug)]
enum RegistrySide {
	Old,
	New,
}

#[derive(Clone, Debug)]
enum EncodingType {
	Id(u32),
	Primitive(TypeDefPrimitive),
}

#[derive(Clone, Debug)]
enum EncodingAtom {
	Primitive(TypeDefPrimitive),
	Sequence(EncodingType),
	Compact(EncodingType),
	Variant(u32),
	BitSequence { bit_store: EncodingType, bit_order: EncodingType },
}

impl EncodingAtom {
	fn kind(&self) -> &'static str {
		match self {
			EncodingAtom::Primitive(_) => "primitive",
			EncodingAtom::Sequence(_) => "sequence",
			EncodingAtom::Compact(_) => "compact",
			EncodingAtom::Variant(_) => "variant",
			EncodingAtom::BitSequence { .. } => "bit sequence",
		}
	}
}

fn is_product(type_def: &TypeDef<PortableForm>) -> bool {
	matches!(type_def, TypeDef::Composite(_) | TypeDef::Tuple(_) | TypeDef::Array(_))
}

fn variants_by_index(
	type_id: u32,
	variants: &[Variant<PortableForm>],
) -> Result<BTreeMap<u8, &Variant<PortableForm>>> {
	let mut by_index = BTreeMap::new();

	for variant in variants {
		if by_index.insert(variant.index, variant).is_some() {
			bail!("Type #{type_id} has duplicate variant index {}", variant.index);
		}
	}

	Ok(by_index)
}

pub struct StorageData<'a> {
	/// All of the types used.
	pub types: &'a PortableRegistry,
	/// The prefix of the storage.
	pub storage_prefix: &'a str,
	/// A map of the storage entries where the key is the name of the entry and the value is that
	/// same entry.
	pub storage_entries: BTreeMap<&'a str, &'a StorageEntryMetadata<PortableForm>>,
}

impl<'a> StorageData<'a> {
	pub fn new(metadata: &'a RuntimeMetadataV16) -> Result<Self> {
		let storage = metadata
			.pallets
			.iter()
			.find(|pallet| pallet.name == "Revive")
			.context("Failed to find pallet-revive in metadata")?
			.storage
			.as_ref()
			.context("No storage for pallet-revive")?;

		let storage_prefix = storage.prefix.as_str();
		let storage_entries = storage
			.entries
			.iter()
			.map(|storage_entry| (storage_entry.name.as_str(), storage_entry))
			.collect();

		Ok(Self { types: &metadata.types, storage_prefix, storage_entries })
	}
}
