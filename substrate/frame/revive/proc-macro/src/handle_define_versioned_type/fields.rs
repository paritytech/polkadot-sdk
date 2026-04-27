use std::collections::BTreeMap;

use proc_macro2::Span;
use syn::{
	punctuated::Punctuated, spanned::Spanned, token::Comma, Field, Fields, FieldsNamed,
	FieldsUnnamed, Ident, Result, Visibility,
};

use super::attribute::FieldWithVersionedTypeAttribute;

/// Describes the syntax node that owns a field list.
///
/// Struct fields can carry visibility, while enum variant fields cannot. The owner decides which
/// visibility is assigned to fields copied from a previous version.
#[derive(Clone, Copy)]
pub(super) enum FieldOwner {
	/// The fields belong to a struct item.
	Struct,

	/// The fields belong to an enum variant.
	EnumVariant,
}

impl FieldOwner {
	/// Returns the visibility to use for fields copied from previous versions.
	#[must_use]
	fn copied_field_visibility(self) -> Visibility {
		match self {
			Self::Struct => Visibility::Public(Default::default()),
			Self::EnumVariant => Visibility::Inherited,
		}
	}
}

/// Extends a field list with fields from the immediately previous version.
///
/// This function preserves the current field shape whenever possible. Named current fields stay
/// named, tuple current fields stay tuple fields, and unit current fields become the previous
/// non-unit shape when there are fields to inherit.
pub(super) fn extend_fields(
	current: &mut Fields,
	previous: &Fields,
	owner: FieldOwner,
) -> Result<()> {
	let visibility = owner.copied_field_visibility();

	match (current, previous) {
		(Fields::Named(current), Fields::Named(previous)) => {
			extend_named_fields_with_named_previous(current, previous, visibility)
		},
		(Fields::Named(current), Fields::Unnamed(previous)) => {
			extend_named_fields_with_unnamed_previous(current, previous, visibility)
		},
		(Fields::Unnamed(current), Fields::Named(previous)) => {
			extend_unnamed_fields_with_named_previous(current, previous, visibility)
		},
		(Fields::Unnamed(current), Fields::Unnamed(previous)) => {
			extend_unnamed_fields_with_unnamed_previous(current, previous, visibility)
		},
		(Fields::Named(current), Fields::Unit) => strip_named_fields_after_unit_previous(current),
		(Fields::Unnamed(current), Fields::Unit) => {
			strip_unnamed_fields_after_unit_previous(current)
		},
		(current @ Fields::Unit, previous @ (Fields::Named(_) | Fields::Unnamed(_))) => {
			*current = apply_visibility(previous.clone(), visibility);
			Ok(())
		},
		(Fields::Unit, Fields::Unit) => Ok(()),
	}
}

/// Strips field-level helper attributes when no field extension is active.
///
/// Field-level override only has meaning inside a type or variant that is extending previous
/// fields. Without that context, keeping the helper attribute would leak an implementation detail
/// into the generated Rust item.
pub(super) fn strip_field_attributes(fields: &mut Fields) -> Result<()> {
	match fields {
		Fields::Named(fields) => strip_named_fields_without_extension(fields),
		Fields::Unnamed(fields) => strip_unnamed_fields_without_extension(fields),
		Fields::Unit => Ok(()),
	}
}

/// Extends named fields from previous named fields.
fn extend_named_fields_with_named_previous(
	current: &mut FieldsNamed,
	previous: &FieldsNamed,
	visibility: Visibility,
) -> Result<()> {
	let current_fields =
		FieldWithVersionedTypeAttribute::parse_all(core::mem::take(&mut current.named))?;
	let mut changes = classify_named_current_fields(current_fields, previous)?;
	let mut fields = Punctuated::<Field, Comma>::new();

	for mut previous_field in previous.named.iter().cloned() {
		let field_name = named_field_ident(&previous_field)?.to_string();
		if let Some(current_field) = changes.take_override(&field_name) {
			fields.push(current_field.field);
		} else {
			previous_field.vis = visibility.clone();
			fields.push(previous_field);
		}
	}

	if let Some(current_field) = changes.first_unmatched_override() {
		return Err(unmatched_named_field_override_error(current_field)?);
	}

	fields.extend(changes.new_fields);
	current.named = fields;
	Ok(())
}

/// Extends named fields from previous tuple fields.
fn extend_named_fields_with_unnamed_previous(
	current: &mut FieldsNamed,
	previous: &FieldsUnnamed,
	visibility: Visibility,
) -> Result<()> {
	let generated_names = generated_tuple_field_names(previous);
	let mut fields = Punctuated::<Field, Comma>::new();

	for (field_index, mut previous_field) in previous.unnamed.iter().cloned().enumerate() {
		let field_name = generated_tuple_field_name(field_index);
		previous_field.vis = visibility.clone();
		previous_field.ident = Some(Ident::new(&field_name, previous_field.span()));
		previous_field.colon_token = Some(Default::default());
		fields.push(previous_field);
	}

	let current_fields =
		FieldWithVersionedTypeAttribute::parse_all(core::mem::take(&mut current.named))?;
	reject_duplicate_named_current_fields(&current_fields)?;

	for current_field in current_fields {
		if let Some(override_span) = current_field.attribute.override_span() {
			return Err(override_against_tuple_previous_error(override_span));
		}

		let field_name = named_field_ident(&current_field.field)?.to_string();
		if let Some(previous_span) = generated_names.get(&field_name) {
			return Err(generated_tuple_field_collision_error(
				&current_field.field,
				&field_name,
				*previous_span,
			)?);
		}

		fields.push(current_field.field);
	}

	current.named = fields;
	Ok(())
}

/// Extends tuple fields from previous named fields.
fn extend_unnamed_fields_with_named_previous(
	current: &mut FieldsUnnamed,
	previous: &FieldsNamed,
	visibility: Visibility,
) -> Result<()> {
	let mut fields = Punctuated::<Field, Comma>::new();

	for mut previous_field in previous.named.iter().cloned() {
		previous_field.vis = visibility.clone();
		previous_field.ident = None;
		previous_field.colon_token = None;
		fields.push(previous_field);
	}

	append_current_unnamed_fields(&mut fields, core::mem::take(&mut current.unnamed))?;
	current.unnamed = fields;
	Ok(())
}

/// Extends tuple fields from previous tuple fields.
fn extend_unnamed_fields_with_unnamed_previous(
	current: &mut FieldsUnnamed,
	previous: &FieldsUnnamed,
	visibility: Visibility,
) -> Result<()> {
	let mut fields = Punctuated::<Field, Comma>::new();

	for mut previous_field in previous.unnamed.iter().cloned() {
		previous_field.vis = visibility.clone();
		fields.push(previous_field);
	}

	append_current_unnamed_fields(&mut fields, core::mem::take(&mut current.unnamed))?;
	current.unnamed = fields;
	Ok(())
}

/// Strips current named fields when the previous version had no fields.
fn strip_named_fields_after_unit_previous(current: &mut FieldsNamed) -> Result<()> {
	let current_fields =
		FieldWithVersionedTypeAttribute::parse_all(core::mem::take(&mut current.named))?;
	reject_duplicate_named_current_fields(&current_fields)?;

	let mut fields = Punctuated::<Field, Comma>::new();
	for current_field in current_fields {
		if let Some(override_span) = current_field.attribute.override_span() {
			return Err(syn::Error::new(
				override_span,
				"field is marked as an override but the previous version has no fields",
			));
		}

		fields.push(current_field.field);
	}

	current.named = fields;
	Ok(())
}

/// Strips current tuple fields when the previous version had no fields.
fn strip_unnamed_fields_after_unit_previous(current: &mut FieldsUnnamed) -> Result<()> {
	let mut fields = Punctuated::<Field, Comma>::new();
	append_current_unnamed_fields(&mut fields, core::mem::take(&mut current.unnamed))?;
	current.unnamed = fields;
	Ok(())
}

/// Strips current named fields when no field extension is active.
fn strip_named_fields_without_extension(current: &mut FieldsNamed) -> Result<()> {
	let current_fields =
		FieldWithVersionedTypeAttribute::parse_all(core::mem::take(&mut current.named))?;
	reject_duplicate_named_current_fields(&current_fields)?;

	let mut fields = Punctuated::<Field, Comma>::new();
	for current_field in current_fields {
		if let Some(override_span) = current_field.attribute.override_span() {
			return Err(syn::Error::new(
				override_span,
				"`#[versioned_type(override)]` can only be used inside a type or variant \
                that is extending a previous version",
			));
		}

		fields.push(current_field.field);
	}

	current.named = fields;
	Ok(())
}

/// Strips current tuple fields when no field extension is active.
fn strip_unnamed_fields_without_extension(current: &mut FieldsUnnamed) -> Result<()> {
	let mut fields = Punctuated::<Field, Comma>::new();
	append_current_unnamed_fields(&mut fields, core::mem::take(&mut current.unnamed))?;
	current.unnamed = fields;
	Ok(())
}

/// Appends current tuple fields after stripping helper attributes.
fn append_current_unnamed_fields(
	fields: &mut Punctuated<Field, Comma>,
	current_fields: Punctuated<Field, Comma>,
) -> Result<()> {
	for current_field in FieldWithVersionedTypeAttribute::parse_all(current_fields)? {
		reject_tuple_field_override(&current_field)?;
		fields.push(current_field.field);
	}

	Ok(())
}

/// Classifies current named fields as overrides or fresh additions.
fn classify_named_current_fields(
	current_fields: Vec<FieldWithVersionedTypeAttribute>,
	previous: &FieldsNamed,
) -> Result<NamedFieldChanges> {
	reject_duplicate_named_current_fields(&current_fields)?;

	let mut changes = NamedFieldChanges::default();
	for current_field in current_fields {
		let field_name = named_field_ident(&current_field.field)?.to_string();
		if current_field.attribute.override_span().is_some() {
			changes.insert_override(field_name, current_field);
		} else if let Some(previous_field) = find_named_field(previous, &field_name)? {
			return Err(redefined_named_field_error(
				&current_field.field,
				previous_field,
				&field_name,
			)?);
		} else {
			changes.new_fields.push(current_field.field);
		}
	}

	Ok(changes)
}

/// The field changes requested by a named current field list.
#[derive(Default)]
struct NamedFieldChanges {
	/// Named fields that should replace fields from the previous version.
	overrides: BTreeMap<String, FieldWithVersionedTypeAttribute>,

	/// Override names in source order so diagnostics report the first miss.
	override_order: Vec<String>,

	/// New named fields that should be appended after inherited fields.
	new_fields: Vec<Field>,
}

impl NamedFieldChanges {
	/// Inserts a field override that was already checked for duplicates.
	fn insert_override(&mut self, field_name: String, field: FieldWithVersionedTypeAttribute) {
		self.override_order.push(field_name.clone());
		self.overrides.insert(field_name, field);
	}

	/// Removes and returns the override for a previous field name.
	fn take_override(&mut self, field_name: &str) -> Option<FieldWithVersionedTypeAttribute> {
		self.overrides.remove(field_name)
	}

	/// Returns the first override that did not match a previous field.
	fn first_unmatched_override(&self) -> Option<&FieldWithVersionedTypeAttribute> {
		self.override_order.iter().find_map(|field_name| self.overrides.get(field_name))
	}
}

/// Rejects duplicate current named fields before merge logic runs.
fn reject_duplicate_named_current_fields(fields: &[FieldWithVersionedTypeAttribute]) -> Result<()> {
	let mut seen_fields = BTreeMap::<String, Ident>::new();

	for field in fields {
		let field_ident = named_field_ident(&field.field)?;
		let field_name = field_ident.to_string();
		if let Some(existing_ident) = seen_fields.get(&field_name) {
			let mut error = syn::Error::new_spanned(
				field_ident,
				format!("field `{field_name}` is defined more than once"),
			);
			error.combine(syn::Error::new_spanned(
				existing_ident,
				format!("first definition of field `{field_name}` is here"),
			));
			return Err(error);
		}

		seen_fields.insert(field_name, field_ident.clone());
	}

	Ok(())
}

/// Rejects field override on tuple fields because tuple fields are positional.
fn reject_tuple_field_override(field: &FieldWithVersionedTypeAttribute) -> Result<()> {
	if let Some(override_span) = field.attribute.override_span() {
		return Err(syn::Error::new(
			override_span,
			"`#[versioned_type(override)]` is not supported on tuple fields because tuple \
            fields do not have stable names to override",
		));
	}

	Ok(())
}

/// Returns the identifier for a named field or reports a structural error.
fn named_field_ident(field: &Field) -> Result<&Ident> {
	field
		.ident
		.as_ref()
		.ok_or_else(|| syn::Error::new(field.span(), "expected a named field"))
}

/// Finds a named field by identifier.
fn find_named_field<'a>(fields: &'a FieldsNamed, field_name: &str) -> Result<Option<&'a Field>> {
	for field in &fields.named {
		if named_field_ident(field)? == field_name {
			return Ok(Some(field));
		}
	}

	Ok(None)
}

/// Returns the synthetic name assigned to an inherited tuple field.
fn generated_tuple_field_name(field_index: usize) -> String {
	format!("field_{field_index}")
}

/// Returns every synthetic name that inherited tuple fields will create.
fn generated_tuple_field_names(fields: &FieldsUnnamed) -> BTreeMap<String, Span> {
	fields
		.unnamed
		.iter()
		.enumerate()
		.map(|(field_index, field)| (generated_tuple_field_name(field_index), field.span()))
		.collect::<BTreeMap<_, _>>()
}

/// Applies the chosen visibility to every field in a cloned field list.
fn apply_visibility(fields: Fields, visibility: Visibility) -> Fields {
	match fields {
		Fields::Named(mut fields) => {
			for field in &mut fields.named {
				field.vis = visibility.clone();
			}
			Fields::Named(fields)
		},
		Fields::Unnamed(mut fields) => {
			for field in &mut fields.unnamed {
				field.vis = visibility.clone();
			}
			Fields::Unnamed(fields)
		},
		Fields::Unit => Fields::Unit,
	}
}

/// Builds a diagnostic for redefining an inherited named field.
fn redefined_named_field_error(
	current_field: &Field,
	previous_field: &Field,
	field_name: &str,
) -> Result<syn::Error> {
	let mut error = syn::Error::new_spanned(
		named_field_ident(current_field)?,
		format!(
			"field `{field_name}` is already defined in the previous version; add \
            `#[versioned_type(override)]` to replace it"
		),
	);
	error.combine(syn::Error::new_spanned(
		named_field_ident(previous_field)?,
		format!("original field `{field_name}` was defined here"),
	));
	Ok(error)
}

/// Builds a diagnostic for overriding a missing previous named field.
fn unmatched_named_field_override_error(
	current_field: &FieldWithVersionedTypeAttribute,
) -> Result<syn::Error> {
	let field_name = named_field_ident(&current_field.field)?.to_string();
	let Some(override_span) = current_field.attribute.override_span() else {
		return Ok(syn::Error::new_spanned(
			named_field_ident(&current_field.field)?,
			format!("field `{field_name}` was expected to be marked as an override"),
		));
	};

	let mut error = syn::Error::new(
		override_span,
		format!(
			"field `{field_name}` is marked as an override but no field with that name exists \
            in the previous version"
		),
	);
	error.combine(syn::Error::new_spanned(
		named_field_ident(&current_field.field)?,
		format!("override field `{field_name}` is defined here"),
	));
	Ok(error)
}

/// Builds a diagnostic for overriding fields inherited from tuple fields.
fn override_against_tuple_previous_error(override_span: Span) -> syn::Error {
	syn::Error::new(
		override_span,
		"`#[versioned_type(override)]` requires the previous version to have named fields; \
        overriding fields from tuple structs is ambiguous",
	)
}

/// Builds a diagnostic for a current field colliding with a generated name.
fn generated_tuple_field_collision_error(
	current_field: &Field,
	field_name: &str,
	previous_span: Span,
) -> Result<syn::Error> {
	let mut error = syn::Error::new_spanned(
		named_field_ident(current_field)?,
		format!(
			"field `{field_name}` conflicts with a field name generated from the previous \
            tuple fields"
		),
	);
	error.combine(syn::Error::new(
		previous_span,
		format!("tuple field generates inherited field `{field_name}` here"),
	));
	Ok(error)
}
