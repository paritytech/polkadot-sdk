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
use syn::{spanned::Spanned, Ident, Result, Visibility};

use super::*;

pub trait Patchable<Patch> {
	fn apply_patch(&mut self, patch: impl Into<Patch>) -> Result<()>;
}

pub enum UnnamedFieldsPatch {
	AddField { new_field: StructuredField },
}

pub enum NamedFieldsPatch {
	AddField { new_field: StructuredField },
	OverrideField { new_field: StructuredNamedField },
}

pub enum FieldsPatch {
	AddField { new_field: StructuredField },
	OverrideField { new_field: StructuredNamedField },
	TransformFieldsShape { new_fields_shape: FieldsShape },
}

pub enum VariantsPatch {
	PatchFields { variant: Ident, patch: FieldsPatch },
	AddVariant { variant: StructuredEnumVariant },
	OverrideVariant { variant: StructuredEnumVariant },
}

impl From<UnnamedFieldsPatch> for FieldsPatch {
	fn from(value: UnnamedFieldsPatch) -> Self {
		match value {
			UnnamedFieldsPatch::AddField { new_field } => Self::AddField { new_field },
		}
	}
}

impl From<NamedFieldsPatch> for FieldsPatch {
	fn from(value: NamedFieldsPatch) -> Self {
		match value {
			NamedFieldsPatch::AddField { new_field } => Self::AddField { new_field },
			NamedFieldsPatch::OverrideField { new_field } => Self::OverrideField { new_field },
		}
	}
}

impl Patchable<UnnamedFieldsPatch> for StructuredUnnamedFields {
	fn apply_patch(&mut self, patch: impl Into<UnnamedFieldsPatch>) -> Result<()> {
		match patch.into() {
			UnnamedFieldsPatch::AddField { new_field } => {
				self.add_field(new_field);
				Ok(())
			},
		}
	}
}

impl StructuredUnnamedFields {
	fn add_field(&mut self, new_field: StructuredField) {
		let unnamed_field = match new_field {
			StructuredField::Named(new_field) => StructuredUnnamedField {
				attributes: new_field.attributes,
				visibility: new_field.visibility,
				ty: new_field.ty,
			},
			StructuredField::Unnamed(new_field) => new_field,
		};
		self.fields.push(unnamed_field)
	}
}

impl Patchable<NamedFieldsPatch> for StructuredNamedFields {
	fn apply_patch(&mut self, patch: impl Into<NamedFieldsPatch>) -> Result<()> {
		match patch.into() {
			NamedFieldsPatch::AddField { new_field } => self.add_field(new_field),
			NamedFieldsPatch::OverrideField { new_field } => self.override_field(new_field),
		}
	}
}

impl StructuredNamedFields {
	fn add_field(&mut self, new_field: StructuredField) -> Result<()> {
		let named_field = match new_field {
			StructuredField::Named(new_field) => new_field,
			StructuredField::Unnamed(new_field) => {
				let field_name = format!("unnamed_field_{}", self.fields.len());
				let field_ident = Ident::new(&field_name, new_field.ty.span());
				StructuredNamedField {
					attributes: new_field.attributes,
					visibility: new_field.visibility,
					ident: field_ident,
					ty: new_field.ty,
				}
			},
		};

		if let Err(BounceOutput {
			existing_value: existing_field,
			attempted_insert_value: new_field,
		}) = self.fields.bounce_insert(named_field.ident.clone(), named_field)
		{
			bail! {
				new_field.ident.span() => "A field with the same identifier is already defined",
				existing_field.ident.span() => "Existing field with the same identifier is defined here"
			}
		}
		Ok(())
	}

	fn override_field(&mut self, new_field: StructuredNamedField) -> Result<()> {
		if let Err(new_field) = self.fields.override_insert(new_field.ident.clone(), new_field) {
			bail! {
				new_field.ident.span() => "No field exists with this identifier in order to be \
					overridden"
			}
		}
		Ok(())
	}
}

impl Patchable<FieldsPatch> for StructuredFields {
	fn apply_patch(&mut self, patch: impl Into<FieldsPatch>) -> Result<()> {
		match (self, patch.into()) {
			(StructuredFields::Named(fields), FieldsPatch::AddField { new_field }) => {
				Patchable::<NamedFieldsPatch>::apply_patch(
					fields,
					NamedFieldsPatch::AddField { new_field },
				)
			},
			(StructuredFields::Named(fields), FieldsPatch::OverrideField { new_field }) => {
				Patchable::<NamedFieldsPatch>::apply_patch(
					fields,
					NamedFieldsPatch::OverrideField { new_field },
				)
			},
			(StructuredFields::Unnamed(fields), FieldsPatch::AddField { new_field }) => {
				Patchable::<UnnamedFieldsPatch>::apply_patch(
					fields,
					UnnamedFieldsPatch::AddField { new_field },
				)
			},
			(this @ StructuredFields::Unit, FieldsPatch::AddField { new_field }) => match new_field
			{
				StructuredField::Named(new_field) => {
					let mut map = IndexMap::new();
					map.insert(new_field.ident.clone(), new_field);
					*this = Self::Named(StructuredNamedFields { fields: map });
					Ok(())
				},
				StructuredField::Unnamed(new_field) => {
					*this =
						Self::Unnamed(StructuredUnnamedFields { fields: vec![new_field].into() });
					Ok(())
				},
			},
			(this @ _, FieldsPatch::TransformFieldsShape { new_fields_shape }) => {
				let fields = this.drain_fields();

				*this = match new_fields_shape {
					FieldsShape::NamedFields => StructuredFields::Named(Default::default()),
					FieldsShape::TupleFields => StructuredFields::Unnamed(Default::default()),
					FieldsShape::Inherit => StructuredFields::Unit,
				};

				for field in fields {
					this.apply_patch(FieldsPatch::AddField { new_field: field })?
				}

				Ok(())
			},
			(
				StructuredFields::Unnamed(..) | StructuredFields::Unit,
				FieldsPatch::OverrideField { new_field },
			) => {
				bail! {
					new_field.ident.span() => "Tuple fields can't be overridden by name because \
						they do not have stable field identifiers"
				}
			},
		}
	}
}

impl StructuredFields {
	pub fn drain_fields(&mut self) -> Vec<StructuredField> {
		match self {
			StructuredFields::Named(fields) => {
				fields.fields.drain(..).map(|(_, v)| v).map(StructuredField::Named).collect()
			},
			StructuredFields::Unnamed(fields) => {
				fields.fields.drain(..).map(StructuredField::Unnamed).collect()
			},
			StructuredFields::Unit => vec![],
		}
	}
}

impl Patchable<FieldsPatch> for StructuredItemStruct {
	fn apply_patch(&mut self, patch: impl Into<FieldsPatch>) -> Result<()> {
		self.fields.apply_patch(patch)
	}
}

impl Patchable<FieldsPatch> for StructuredEnumVariant {
	fn apply_patch(&mut self, patch: impl Into<FieldsPatch>) -> Result<()> {
		self.fields.apply_patch(patch)
	}
}

impl Patchable<VariantsPatch> for StructuredEnumVariants {
	fn apply_patch(&mut self, patch: impl Into<VariantsPatch>) -> Result<()> {
		match patch.into() {
			VariantsPatch::PatchFields { variant, mut patch } => {
				match patch {
					FieldsPatch::AddField { ref mut new_field } => {
						*new_field.visibility_ref_mut() = Visibility::Inherited
					},
					FieldsPatch::OverrideField { ref mut new_field } => {
						new_field.visibility = Visibility::Inherited
					},
					FieldsPatch::TransformFieldsShape { .. } => {},
				}

				let variant = self.get_variant_mut(&variant)?;
				Patchable::<FieldsPatch>::apply_patch(variant, patch)
			},
			VariantsPatch::AddVariant { variant } => self.add_variant(variant),
			VariantsPatch::OverrideVariant { variant } => self.override_variant(variant),
		}
	}
}

impl StructuredEnumVariants {
	fn add_variant(&mut self, variant: StructuredEnumVariant) -> Result<()> {
		if let Err(BounceOutput {
			existing_value: existing_variant,
			attempted_insert_value: variant,
		}) = self.variants.bounce_insert(variant.ident.clone(), variant)
		{
			bail! {
				variant.ident.span() => "A variant with the same identifier is already defined",
				existing_variant.ident.span() =>
					"Existing variant with the same identifier is defined here",
			}
		}

		Ok(())
	}

	fn override_variant(&mut self, variant: StructuredEnumVariant) -> Result<()> {
		if let Err(variant) = self.variants.override_insert(variant.ident.clone(), variant) {
			bail! {
				variant.ident.span() => "No variant exists with this identifier in order to be \
					overridden"
			}
		}

		Ok(())
	}

	fn get_variant_mut(&mut self, variant: &Ident) -> Result<&mut StructuredEnumVariant> {
		self.variants.get_mut(variant).ok_or_else(|| {
			syn_error! {
				variant.span() => format!(
					"Attempted to get variant `{}` but it was not found",
					variant,
				),
			}
		})
	}
}

impl Patchable<VariantsPatch> for StructuredItemEnum {
	fn apply_patch(&mut self, patch: impl Into<VariantsPatch>) -> Result<()> {
		self.variants.apply_patch(patch)
	}
}
