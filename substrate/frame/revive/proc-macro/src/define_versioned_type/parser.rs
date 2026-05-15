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

use syn::{spanned::Spanned, Result};

use super::*;

pub fn parse(
	type_definitions: impl IntoIterator<Item = StructuredTypeDefinition>,
) -> Result<Vec<StructuredTypeDefinition>> {
	let mut updated_type_definitions = Vec::new();
	for type_definition in type_definitions.into_iter() {
		let new_type_definition = match type_definition {
			StructuredTypeDefinition::Struct(item_struct) => {
				handle_struct(item_struct, updated_type_definitions.last())
					.map(StructuredTypeDefinition::Struct)
			},
			StructuredTypeDefinition::Enum(item_enum) => {
				handle_enum(item_enum, updated_type_definitions.last())
					.map(StructuredTypeDefinition::Enum)
			},
		}?;
		updated_type_definitions.push(new_type_definition);
	}
	Ok(updated_type_definitions)
}

fn handle_struct(
	mut item: StructuredItemStruct,
	previous_item: Option<&StructuredTypeDefinition>,
) -> Result<StructuredItemStruct> {
	let struct_attribute = StructVersioningAttribute::take(&mut item.attributes)?;

	let mut patches = Vec::<FieldsPatch>::new();
	let starting_fields = match (struct_attribute.extend, previous_item) {
		(None, _) => StructuredFields::new(item.fields.shape()),
		(Some(_), Some(StructuredTypeDefinition::Struct(previous_item))) => {
			patches
				.push(FieldsPatch::TransformFieldsShape { new_fields_shape: item.fields.shape() });
			previous_item.fields.clone()
		},
		(Some(extend_span), Some(StructuredTypeDefinition::Enum(previous_item))) => {
			bail! {
				extend_span => "A struct can't extend an enum; `extend` requires the \
					previous version to also be a struct",
				item.ident.span() => "This struct is marked as `extend`",
				previous_item.ident.span() => "The previous version is this enum"
			}
		},
		(Some(extend_span), None) => {
			bail! {
				extend_span => "Using `extend` requires a previous version to extend, but no \
					previous type definition exists"
			}
		},
	};

	handle_field_patches(item.fields.drain_fields(), |patch| patches.push(patch))?;

	item.fields = starting_fields;
	for patch in patches {
		item.apply_patch(patch)?;
	}

	Ok(item)
}

fn handle_enum(
	mut item: StructuredItemEnum,
	previous_item: Option<&StructuredTypeDefinition>,
) -> Result<StructuredItemEnum> {
	let enum_attribute = EnumVersioningAttribute::take(&mut item.attributes)?;

	let mut patches = Vec::<VariantsPatch>::new();

	let starting_variants = match (enum_attribute.extend, previous_item) {
		(None, _) => StructuredEnumVariants::default(),
		(Some(_), Some(StructuredTypeDefinition::Enum(previous_item))) => {
			previous_item.variants.clone()
		},
		(Some(extend_span), None) => bail! {
			extend_span => "Using `extend` requires a previous version to extend, but no \
				previous type definition exists"
		},
		(Some(extend_span), Some(StructuredTypeDefinition::Struct(previous_item))) => {
			bail! {
				extend_span => "An enum can't extend a struct; `extend` requires the \
					previous version to also be an enum",
				previous_item.ident.span() => "The previous version is this struct"
			}
		},
	};

	for mut current_variant in item.variants.variants.into_values() {
		let variant_attribute = VariantVersioningAttribute::take(&mut current_variant.attributes)?;
		let variant_ident = current_variant.ident.clone();

		let fields = current_variant.fields.drain_fields();

		match (variant_attribute, previous_item) {
			(VariantVersioningAttribute::None, _) => {
				patches.push(VariantsPatch::AddVariant { variant: current_variant });
			},
			(
				VariantVersioningAttribute::Extend(..),
				Some(StructuredTypeDefinition::Struct(previous_item)),
			) => {
				patches.push(VariantsPatch::AddVariant { variant: current_variant });
				for previous_field in previous_item.fields.clone().fields() {
					patches.push(VariantsPatch::PatchFields {
						variant: variant_ident.clone(),
						patch: FieldsPatch::AddField { new_field: previous_field },
					})
				}
			},
			(
				VariantVersioningAttribute::Extend(extend_span),
				Some(StructuredTypeDefinition::Enum(previous_item)),
			) => {
				let previous_variant =
					previous_item.variants.variants.get(&variant_ident).ok_or_else(|| {
						syn_error! {
							extend_span => "Using `extend` on a variant requires a variant with \
								the same name in the previous enum version",
							variant_ident.span() => "No previous variant with this name exists \
								to extend",
							previous_item.ident.span() => "The previous enum version is here"
						}
					})?;
				patches.push(VariantsPatch::OverrideVariant { variant: current_variant });
				for previous_field in previous_variant.fields.clone().fields() {
					patches.push(VariantsPatch::PatchFields {
						variant: variant_ident.clone(),
						patch: FieldsPatch::AddField { new_field: previous_field },
					})
				}
			},
			(
				VariantVersioningAttribute::Override(..),
				Some(StructuredTypeDefinition::Enum(..)),
			) => {
				patches.push(VariantsPatch::OverrideVariant { variant: current_variant });
			},
			// Error cases.
			(
				VariantVersioningAttribute::Override(span),
				Some(StructuredTypeDefinition::Struct(previous_item)),
			) => {
				bail! {
					span => "A variant can't override a previous struct; `override` requires \
						the previous version to be an enum with a matching variant",
					variant_ident.span() => "This variant is marked as `override`",
					previous_item.ident.span() => "The previous version is this struct"
				}
			},
			(VariantVersioningAttribute::Override(span), None) => {
				bail! {
					span => "Using `override` on a variant requires a previous enum version to \
						override, but no previous type definition exists",
					variant_ident.span() => "This variant is marked as `override`"
				}
			},
			(VariantVersioningAttribute::Extend(span), None) => {
				bail! {
					span => "Using `extend` on a variant requires a previous version to extend, \
						but no previous type definition exists",
					variant_ident.span() => "This variant is marked as `extend`"
				}
			},
		}

		handle_field_patches(fields, |patch| {
			patches.push(VariantsPatch::PatchFields { variant: variant_ident.clone(), patch })
		})?
	}

	item.variants = starting_variants;
	for patch in patches {
		item.apply_patch(patch)?;
	}

	Ok(item)
}

fn handle_field_patches(
	fields: Vec<StructuredField>,
	mut callback: impl FnMut(FieldsPatch),
) -> Result<()> {
	for mut current_field in fields.into_iter() {
		let field_attribute =
			FieldVersioningAttribute::take(current_field.attributes_ref_mut_vec())?;

		match (field_attribute.r#override, current_field) {
			(None, current_field) => callback(FieldsPatch::AddField { new_field: current_field }),
			(Some(_), StructuredField::Named(current_field)) => {
				callback(FieldsPatch::OverrideField { new_field: current_field })
			},
			(Some(override_span), StructuredField::Unnamed(current_field)) => bail! {
				override_span => "`override` can't be used on tuple fields because tuple fields do \
					not have stable field identifiers",
				current_field.ty.span() => "This tuple field cannot be overridden by name"
			},
		}
	}
	Ok(())
}
