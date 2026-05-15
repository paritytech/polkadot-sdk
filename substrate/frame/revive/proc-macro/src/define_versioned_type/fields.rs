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
	spanned::Spanned, Attribute, Field, FieldMutability, Fields, FieldsNamed, FieldsUnnamed, Ident,
	Type, Visibility,
};

use super::*;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FieldsShape {
	NamedFields,
	TupleFields,
	Inherit,
}

#[derive(Clone)]
pub enum StructuredFields {
	Named(StructuredNamedFields),
	Unnamed(StructuredUnnamedFields),
	Unit,
}

#[derive(Clone)]
pub enum StructuredField {
	Named(StructuredNamedField),
	Unnamed(StructuredUnnamedField),
}

#[derive(Clone, Default)]
pub struct StructuredNamedFields {
	pub fields: IndexMap<Ident, StructuredNamedField>,
}

#[derive(Clone, Default)]
pub struct StructuredUnnamedFields {
	pub fields: Vec<StructuredUnnamedField>,
}

#[derive(Clone)]
pub struct StructuredNamedField {
	pub attributes: Vec<Attribute>,
	pub visibility: Visibility,
	pub ident: Ident,
	pub ty: Type,
}

#[derive(Clone)]
pub struct StructuredUnnamedField {
	pub attributes: Vec<Attribute>,
	pub visibility: Visibility,
	pub ty: Type,
}

// ========
// Helpers
// ========

impl StructuredFields {
	pub fn fields(self) -> Box<dyn Iterator<Item = StructuredField>> {
		match self {
			Self::Named(fields) => {
				Box::new(fields.fields.into_values().map(StructuredField::Named))
			},
			Self::Unnamed(fields) => {
				Box::new(fields.fields.into_iter().map(StructuredField::Unnamed))
			},
			Self::Unit => Box::new(core::iter::empty()),
		}
	}

	pub fn shape(&self) -> FieldsShape {
		match self {
			Self::Named(..) => FieldsShape::NamedFields,
			Self::Unnamed(..) => FieldsShape::TupleFields,
			Self::Unit => FieldsShape::Inherit,
		}
	}
}

impl StructuredField {
	pub fn attributes_ref_mut_vec(&mut self) -> &mut Vec<Attribute> {
		match self {
			StructuredField::Named(field) => &mut field.attributes,
			StructuredField::Unnamed(field) => &mut field.attributes,
		}
	}

	pub fn visibility_ref_mut(&mut self) -> &mut Visibility {
		match self {
			StructuredField::Named(field) => &mut field.visibility,
			StructuredField::Unnamed(field) => &mut field.visibility,
		}
	}
}

// ===========================
// Conversion Implementations
// ===========================

impl TryFrom<Fields> for StructuredFields {
	type Error = syn::Error;

	fn try_from(value: Fields) -> Result<Self, Self::Error> {
		match value {
			Fields::Named(fields) => Ok(Self::Named(fields.try_into()?)),
			Fields::Unnamed(fields) => Ok(Self::Unnamed(fields.try_into()?)),
			Fields::Unit => Ok(Self::Unit),
		}
	}
}

impl TryFrom<FieldsNamed> for StructuredNamedFields {
	type Error = syn::Error;

	fn try_from(value: FieldsNamed) -> Result<Self, Self::Error> {
		let fields = value.named.into_iter().map(StructuredNamedField::try_from).try_fold(
			IndexMap::new(),
			|mut fields, field| {
				let field = field?;
				if let Err(output) = fields.bounce_insert(field.ident.clone(), field) {
					return Err(syn_error! {
						output.attempted_insert_value.ident.span() =>
							"A field with the same identifier already exists",
						output.existing_value.ident.span() =>
							"Other field with the same identifier is defined here",
					});
				}
				Ok(fields)
			},
		)?;

		Ok(Self { fields })
	}
}

impl TryFrom<FieldsUnnamed> for StructuredUnnamedFields {
	type Error = syn::Error;

	fn try_from(value: FieldsUnnamed) -> Result<Self, Self::Error> {
		let fields = value
			.unnamed
			.into_iter()
			.map(StructuredUnnamedField::try_from)
			.collect::<Result<_, _>>()?;
		Ok(Self { fields })
	}
}

impl TryFrom<Field> for StructuredField {
	type Error = syn::Error;

	fn try_from(value: Field) -> Result<Self, Self::Error> {
		let Field { attrs, vis, ident, ty, .. } = value;
		Ok(match ident {
			Some(ident) => {
				Self::Named(StructuredNamedField { attributes: attrs, visibility: vis, ident, ty })
			},
			None => {
				Self::Unnamed(StructuredUnnamedField { attributes: attrs, visibility: vis, ty })
			},
		})
	}
}

impl TryFrom<Field> for StructuredNamedField {
	type Error = syn::Error;

	fn try_from(value: Field) -> Result<Self, Self::Error> {
		let Field { attrs, vis, ident, ty, .. } = value;
		let Some(ident) = ident else {
			return Err(syn::Error::new(ty.span(), "Expected a named field"));
		};
		Ok(Self { attributes: attrs, visibility: vis, ident, ty })
	}
}

impl TryFrom<Field> for StructuredUnnamedField {
	type Error = syn::Error;

	fn try_from(value: Field) -> Result<Self, Self::Error> {
		let Field { attrs, vis, ident, ty, .. } = value;
		if let Some(ident) = ident {
			return Err(syn::Error::new(ident.span(), "Expected an unnamed field"));
		}

		Ok(Self { attributes: attrs, visibility: vis, ty })
	}
}

impl From<StructuredFields> for Fields {
	fn from(value: StructuredFields) -> Self {
		match value {
			StructuredFields::Named(fields) => Self::Named(fields.into()),
			StructuredFields::Unnamed(fields) => Self::Unnamed(fields.into()),
			StructuredFields::Unit => Self::Unit,
		}
	}
}

impl From<StructuredNamedFields> for FieldsNamed {
	fn from(value: StructuredNamedFields) -> Self {
		Self {
			brace_token: Default::default(),
			named: value.fields.into_values().map(Field::from).collect(),
		}
	}
}

impl From<StructuredUnnamedFields> for FieldsUnnamed {
	fn from(value: StructuredUnnamedFields) -> Self {
		Self {
			paren_token: Default::default(),
			unnamed: value.fields.into_iter().map(Field::from).collect(),
		}
	}
}

impl From<StructuredField> for Field {
	fn from(value: StructuredField) -> Self {
		match value {
			StructuredField::Named(field) => field.into(),
			StructuredField::Unnamed(field) => field.into(),
		}
	}
}

impl From<StructuredNamedField> for Field {
	fn from(value: StructuredNamedField) -> Self {
		Self {
			attrs: value.attributes,
			vis: value.visibility,
			mutability: FieldMutability::None,
			ident: Some(value.ident),
			colon_token: Some(Default::default()),
			ty: value.ty,
		}
	}
}

impl From<StructuredUnnamedField> for Field {
	fn from(value: StructuredUnnamedField) -> Self {
		Self {
			attrs: value.attributes,
			vis: value.visibility,
			mutability: FieldMutability::None,
			ident: None,
			colon_token: None,
			ty: value.ty,
		}
	}
}
