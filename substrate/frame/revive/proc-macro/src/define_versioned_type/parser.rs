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

	let item_fields = core::mem::replace(&mut item.fields, StructuredFields::Unit);
	let mut patches =
		vec![FieldsPatch::TransformFieldsShape { new_fields_shape: item_fields.shape() }];

	let fields_to_add = match (struct_attribute.extend, previous_item) {
		(None, _) => vec![item_fields],
		(Some(extend_span), None) => {
			bail! {
				extend_span => "Using `extend` requires a previous version to extend, but no \
					previous type definition exists"
			}
		},
		(Some(_), Some(StructuredTypeDefinition::Struct(previous_item))) => {
			vec![previous_item.fields.clone(), item_fields]
		},
		(Some(extend_span), Some(StructuredTypeDefinition::Enum(previous_item))) => {
			bail! {
				extend_span => "A struct can't extend an enum; `extend` requires the \
					previous version to also be a struct",
				previous_item.ident.span() => "The previous version is this enum"
			}
		},
	};

	for mut new_field in fields_to_add.into_iter().flat_map(|fields| fields.fields()) {
		let field_attribute = FieldVersioningAttribute::take(new_field.attributes_ref_mut_vec())?;

		match (field_attribute.r#override, new_field) {
			(None, new_field) => patches.push(FieldsPatch::AddField { new_field }),
			(Some(_), StructuredField::Named(field)) => {
				patches.push(FieldsPatch::OverrideField { new_field: field });
			},
			(Some(override_span), StructuredField::Unnamed(field)) => {
				bail! {
					override_span => "`override` can't be used on tuple fields because tuple \
						fields do not have stable field identifiers",
					field.ty.span() => "This tuple field cannot be overridden by name"
				}
			},
		}
	}

	for patch in patches {
		item.apply_patch(patch)?;
	}

	Ok(item)
}

fn handle_enum(
	mut item: StructuredItemEnum,
	previous_item: Option<&StructuredTypeDefinition>,
) -> Result<StructuredItemEnum> {
	let enum_attributes = EnumVersioningAttribute::take(&mut item.attributes)?;

	let mut patches = Vec::<VariantsPatch>::new();

	let enum_variants = match (enum_attributes.extend, previous_item) {
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

	for mut new_variant in core::mem::take(&mut item.variants.variants).into_values() {
		let variant_attribute = VariantVersioningAttribute::take(&mut new_variant.attributes)?;

		let new_variant_no_fields = {
			let mut variant = new_variant.clone();
			variant.fields.drain_fields();
			variant
		};

		match (variant_attribute, previous_item) {
			(VariantVersioningAttribute::None, ..) => {
				patches.push(VariantsPatch::AddVariant { variant: new_variant_no_fields })
			},
			(
				VariantVersioningAttribute::Extend(..),
				Some(StructuredTypeDefinition::Struct(previous_item)),
			) => {
				patches.push(VariantsPatch::AddVariant { variant: new_variant_no_fields });
				for previous_field in previous_item.fields.clone().fields() {
					patches.push(VariantsPatch::PatchFields {
						variant: new_variant.ident.clone(),
						patch: FieldsPatch::AddField { new_field: previous_field },
					})
				}
			},
			(
				VariantVersioningAttribute::Extend(extend_span),
				Some(StructuredTypeDefinition::Enum(previous_item)),
			) => {
				let previous_variant =
					previous_item.variants.variants.get(&new_variant.ident).ok_or_else(|| {
						syn_error! {
							extend_span => "Using `extend` on a variant requires a variant with \
								the same name in the previous enum version",
							new_variant.ident.span() => "No previous variant with this name exists \
								to extend",
							previous_item.ident.span() => "The previous enum version is here"
						}
					})?;
				if enum_variants.variants.contains_key(&new_variant.ident) {
					patches.push(VariantsPatch::OverrideVariant { variant: new_variant_no_fields });
				} else {
					patches.push(VariantsPatch::AddVariant { variant: new_variant_no_fields });
				}

				for previous_field in previous_variant.fields.clone().fields() {
					patches.push(VariantsPatch::PatchFields {
						variant: new_variant.ident.clone(),
						patch: FieldsPatch::AddField { new_field: previous_field },
					})
				}
			},
			(
				VariantVersioningAttribute::Override(..),
				Some(StructuredTypeDefinition::Enum(..)),
			) => patches.push(VariantsPatch::OverrideVariant { variant: new_variant_no_fields }),
			// Error cases
			(VariantVersioningAttribute::Extend(span), None) => {
				bail! {
					span => "Using `extend` on a variant requires a previous version to extend, \
						but no previous type definition exists",
					new_variant.ident.span() => "This variant is marked as `extend`"
				}
			},
			(VariantVersioningAttribute::Override(span), None) => {
				bail! {
					span => "Using `override` on a variant requires a previous enum version to \
						override, but no previous type definition exists",
					new_variant.ident.span() => "This variant is marked as `override`"
				}
			},
			(
				VariantVersioningAttribute::Override(span),
				Some(StructuredTypeDefinition::Struct(previous_item)),
			) => {
				bail! {
					span => "A variant can't override a previous struct; `override` requires \
						the previous version to be an enum with a matching variant",
					new_variant.ident.span() => "This variant is marked as `override`",
					previous_item.ident.span() => "The previous version is this struct"
				}
			},
		}

		for mut field in new_variant.fields.drain_fields() {
			let field_attribute =
				FieldVersioningAttribute::take(&mut field.attributes_ref_mut_vec())?;

			match (field_attribute.r#override, field) {
				(None, field) => {
					patches.push(VariantsPatch::PatchFields {
						variant: new_variant.ident.clone(),
						patch: FieldsPatch::AddField { new_field: field },
					});
				},
				(Some(_), StructuredField::Named(field)) => {
					patches.push(VariantsPatch::PatchFields {
						variant: new_variant.ident.clone(),
						patch: FieldsPatch::OverrideField { new_field: field },
					});
				},
				(Some(override_span), StructuredField::Unnamed(field)) => {
					bail! {
						override_span => "`override` can't be used on tuple variant fields \
							because tuple fields do not have stable field identifiers",
						field.ty.span() => "This tuple field cannot be overridden by name"
					}
				},
			}
		}
	}

	item.variants = enum_variants;
	for patch in patches {
		item.apply_patch(patch)?;
	}

	Ok(item)
}
