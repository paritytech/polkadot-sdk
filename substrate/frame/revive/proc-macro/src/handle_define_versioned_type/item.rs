use std::{collections::BTreeMap, fmt};

use quote::ToTokens;
use syn::{
	parse::{Parse, ParseStream},
	Attribute, Ident, ItemEnum, ItemStruct, Result, Token, Visibility,
};

/// The parsed input accepted by `define_versioned_type!`.
///
/// The macro accepts zero or more struct or enum items. Each item name must end
/// in `V` followed by a positive integer version. All items in one invocation
/// must share the same base name and their versions must be contiguous.
pub struct DefineVersionedTypeInput {
	/// The shared base name for all parsed definitions.
	pub(super) name: Option<String>,

	/// The parsed item definitions keyed by ascending version.
	pub(super) definitions: BTreeMap<Version, DefineVersionedTypeItem>,
}

impl Parse for DefineVersionedTypeInput {
	/// Parses every versioned type item and validates the version sequence.
	fn parse(input: ParseStream) -> Result<Self> {
		let mut name = None::<EstablishedName>;
		let mut definitions = BTreeMap::<Version, DefineVersionedTypeItem>::new();

		while !input.is_empty() {
			let item = input.parse::<DefineVersionedTypeItem>()?;
			let name_and_version = item.name_and_version()?;

			match &name {
				Some(existing_name) => existing_name.ensure_matches(&name_and_version, &item)?,
				None => name = Some(EstablishedName::from_item(&name_and_version, &item)),
			}

			reject_duplicate_version(&definitions, &name_and_version, &item)?;
			definitions.insert(name_and_version.version(), item);
		}

		ensure_contiguous_versions(&definitions)?;

		Ok(Self { name: name.map(EstablishedName::into_name), definitions })
	}
}

/// A struct or enum item accepted by `define_versioned_type!`.
pub enum DefineVersionedTypeItem {
	/// A versioned struct definition.
	Struct(ItemStruct),

	/// A versioned enum definition.
	Enum(ItemEnum),
}

impl DefineVersionedTypeItem {
	/// Removes outer attributes from the wrapped item.
	#[must_use]
	pub(super) fn take_attributes(&mut self) -> Vec<Attribute> {
		match self {
			Self::Struct(item_struct) => core::mem::take(&mut item_struct.attrs),
			Self::Enum(item_enum) => core::mem::take(&mut item_enum.attrs),
		}
	}

	/// Replaces outer attributes on the wrapped item.
	pub(super) fn set_attributes(&mut self, attributes: Vec<Attribute>) {
		match self {
			Self::Struct(item_struct) => item_struct.attrs = attributes,
			Self::Enum(item_enum) => item_enum.attrs = attributes,
		}
	}

	/// Returns the Rust identifier for the wrapped item.
	#[must_use]
	pub(super) fn ident(&self) -> &Ident {
		match self {
			Self::Struct(item_struct) => &item_struct.ident,
			Self::Enum(item_enum) => &item_enum.ident,
		}
	}

	/// Parses the base name and version from the wrapped item identifier.
	pub(super) fn name_and_version(&self) -> Result<NameAndVersion> {
		NameAndVersion::parse(self.ident())
	}
}

impl Parse for DefineVersionedTypeItem {
	/// Parses one struct or enum item after optional attributes and visibility.
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
	/// Writes the wrapped Rust item back into a token stream.
	fn to_tokens(&self, tokens: &mut proc_macro2::TokenStream) {
		match self {
			Self::Struct(item) => item.to_tokens(tokens),
			Self::Enum(item) => item.to_tokens(tokens),
		}
	}
}

/// A validated positive version number from a versioned type name.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(super) struct Version {
	/// The numeric value of the version suffix.
	value: usize,
}

impl Version {
	/// Parses and validates a version suffix from an item identifier.
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

		let value = version_suffix.parse::<usize>().map_err(|_| {
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

	/// Returns the numeric version value.
	#[must_use]
	pub(super) fn value(self) -> usize {
		self.value
	}

	/// Returns the next version number, reporting overflow as a syntax error.
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
	/// Formats the version as the suffix used in item names.
	fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
		write!(formatter, "V{}", self.value)
	}
}

/// The base name and version parsed from one item identifier.
#[derive(Debug)]
pub(super) struct NameAndVersion {
	/// The shared base name before the trailing `Vn` suffix.
	base_name: String,

	/// The validated numeric version suffix.
	version: Version,
}

impl NameAndVersion {
	/// Parses the base name and version suffix from an identifier.
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

	/// Returns the parsed base name.
	#[must_use]
	pub(super) fn base_name(&self) -> &str {
		&self.base_name
	}

	/// Returns the parsed version.
	#[must_use]
	pub(super) fn version(&self) -> Version {
		self.version
	}
}

/// The first parsed item name that future items must match.
struct EstablishedName {
	/// The base name established by the first parsed item.
	name: String,

	/// The identifier that established the base name.
	ident: Ident,
}

impl EstablishedName {
	/// Creates the established name record from a parsed item.
	fn from_item(name_and_version: &NameAndVersion, item: &DefineVersionedTypeItem) -> Self {
		Self { name: name_and_version.base_name().to_owned(), ident: item.ident().clone() }
	}

	/// Returns the owned base name stored in this record.
	fn into_name(self) -> String {
		self.name
	}

	/// Ensures a later item belongs to the same versioned type family.
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

/// A previous definition used while validating contiguous versions.
struct PreviousDefinition<'a> {
	/// The previous version number in sorted order.
	version: Version,

	/// The item that defined the previous version.
	item: &'a DefineVersionedTypeItem,
}

/// Rejects a version that has already appeared in the input.
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

/// Ensures sorted definitions do not skip any intermediate versions.
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

/// Formats the missing version or range between two parsed versions.
fn missing_versions_description(expected_version: Version, found_version: Version) -> String {
	let last_missing_version = found_version.value() - 1;

	if expected_version.value() == last_missing_version {
		format!("version {expected_version}")
	} else {
		format!("versions {expected_version}..V{last_missing_version}")
	}
}
