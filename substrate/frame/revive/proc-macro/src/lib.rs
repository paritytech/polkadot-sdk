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

//! Procedural macros used in `pallet-revive`.
//!
//! Most likely you should use the [`#[define_env]`][`macro@define_env`] attribute macro which hides
//! boilerplate of defining external environment for a polkavm module.

mod handle_define_versioned_type;

use proc_macro::TokenStream;
use proc_macro2::{Literal, Span, TokenStream as TokenStream2};
use quote::{quote, ToTokens};
use syn::{parse_quote, punctuated::Punctuated, spanned::Spanned, token::Comma, FnArg, Ident};

/// Defines a host functions set that can be imported by contract polkavm code.
///
/// **CAUTION**: Be advised that all functions defined by this macro
/// cause undefined behaviour inside the contract if the signature does not match.
///
/// WARNING: It is CRITICAL for contracts to make sure that the signatures match exactly.
/// Failure to do so may result in undefined behavior, traps or security vulnerabilities inside the
/// contract. The runtime itself is unharmed due to sandboxing.
/// For example, if a function is called with an incorrect signature, it could lead to memory
/// corruption or unexpected results within the contract.
#[proc_macro_attribute]
pub fn define_env(attr: TokenStream, item: TokenStream) -> TokenStream {
	if !attr.is_empty() {
		let msg = r#"Invalid `define_env` attribute macro: expected no attributes:
					 - `#[define_env]`"#;
		let span = TokenStream2::from(attr).span();
		return syn::Error::new(span, msg).to_compile_error().into();
	}

	let item = syn::parse_macro_input!(item as syn::ItemMod);

	match EnvDef::try_from(item) {
		Ok(mut def) => expand_env(&mut def).into(),
		Err(e) => e.to_compile_error().into(),
	}
}

/// Defines a family of versioned struct or enum types where each successive version is expressed as
/// a delta from the previous version, while still emitting full, standalone Rust definitions for
/// every version.
///
/// # Motivation
///
/// On-chain types are often versioned (e.g. `CallLogV1`, `CallLogV2`, ...) and each successive
/// version usually adds, replaces, or rearranges a small number of fields or variants relative to
/// its predecessor. Hand-writing every version in full is verbose, error-prone, and obscures the
/// actual change between versions because the diff is buried inside a re-statement of the unchanged
/// fields. `define_versioned_type!` lets each version express only what it changes; the macro
/// reconstructs the full type for every version and emits each one as an ordinary Rust item.
///
/// The macro produces *standalone* type definitions: there is no inheritance, trait magic, or
/// runtime cost. Two versions of `CallLog` are two completely independent Rust types; only the
/// *source* representation is compact.
///
/// # Examples
///
/// The examples below cover every supported feature in a progression that is intended to be
/// readable on its own. Reading them straight through is enough to use the macro for almost any
/// task; the reference sections that follow document the exact rules and the diagnostics.
///
/// ## Adding fields to a struct across versions
///
/// The most common use: each new version appends one or more fields. Mark the new version with
/// `#[versioned_type(extend)]`; the macro inherits every field from the previous version and
/// appends the new ones.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct CallLogV1 {
///         pub caller: AccountId,
///     }
///
///     #[versioned_type(extend)]
///     pub struct CallLogV2 {
///         pub gas_used: Weight,
///     }
///
///     #[versioned_type(extend)]
///     pub struct CallLogV3 {
///         pub block: BlockNumber,
///     }
/// }
/// ```
///
/// expands to three independent structs equivalent to:
///
/// ```ignore
/// pub struct CallLogV1 {
///     pub caller: AccountId,
/// }
///
/// pub struct CallLogV2 {
///     pub caller: AccountId,
///     pub gas_used: Weight,
/// }
///
/// pub struct CallLogV3 {
///     pub caller: AccountId,
///     pub gas_used: Weight,
///     pub block: BlockNumber,
/// }
/// ```
///
/// ## Replacing a field's type (field-level override)
///
/// To change the type of a single field, redeclare it under `#[versioned_type(override)]`. The
/// replacement keeps its original position; only its type (and any attributes) is replaced.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct ConfigV1 {
///         pub timeout: u32,
///         pub retries: u8,
///     }
///
///     #[versioned_type(extend)]
///     pub struct ConfigV2 {
///         #[versioned_type(override)]
///         pub timeout: u64,
///         pub backoff: Duration,
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct ConfigV1 {
///     pub timeout: u32,
///     pub retries: u8,
/// }
///
/// pub struct ConfigV2 {
///     pub timeout: u64,
///     pub retries: u8,
///     pub backoff: Duration,
/// }
/// ```
///
/// `timeout` stays at field index 0 (its previous position); `retries` is inherited unchanged;
/// `backoff` is appended at the end.
///
/// ## Tuple-struct extension
///
/// Tuple structs work the same way; new fields are appended positionally.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct PointV1(pub u32, pub u32);
///
///     #[versioned_type(extend)]
///     pub struct PointV2(pub u32);
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct PointV1(pub u32, pub u32);
/// pub struct PointV2(pub u32, pub u32, pub u32);
/// ```
///
/// ## Inheriting an entire previous shape with a unit struct
///
/// A unit struct in the current version, marked `extend`, inherits the previous version's fields
/// verbatim. This is useful when a version exists only to bump the version number without altering
/// the schema.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct PayloadV1 {
///         pub data: Vec<u8>,
///         pub checksum: u32,
///     }
///
///     #[versioned_type(extend)]
///     pub struct PayloadV2;
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct PayloadV1 {
///     pub data: Vec<u8>,
///     pub checksum: u32,
/// }
///
/// pub struct PayloadV2 {
///     pub data: Vec<u8>,
///     pub checksum: u32,
/// }
/// ```
///
/// ## Changing shape between versions
///
/// A version may switch between named, tuple, and unit shapes. Field merging adapts: previous tuple
/// fields entering a named context are renamed to `field_0`, `field_1`, ...; previous named fields
/// entering a tuple context lose their names; and a unit current version inherits the previous
/// shape entirely.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct EnvelopeV1 {
///         pub kind: u8,
///         pub payload: Vec<u8>,
///     }
///
///     #[versioned_type(extend)]
///     pub struct EnvelopeV2(pub u32);
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct EnvelopeV1 {
///     pub kind: u8,
///     pub payload: Vec<u8>,
/// }
///
/// pub struct EnvelopeV2(pub u8, pub Vec<u8>, pub u32);
/// ```
///
/// ## Adding new variants to an enum
///
/// Enum extension copies all previous variants in order, then appends the new variants from the
/// current source.
///
/// ```ignore
/// define_versioned_type! {
///     pub enum EventV1 {
///         Started,
///         Stopped,
///     }
///
///     #[versioned_type(extend)]
///     pub enum EventV2 {
///         Paused,
///         Resumed,
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV1 {
///     Started,
///     Stopped,
/// }
///
/// pub enum EventV2 {
///     Started,
///     Stopped,
///     Paused,
///     Resumed,
/// }
/// ```
///
/// ## Replacing a variant in place (variant override)
///
/// `#[versioned_type(override)]` on a variant replaces the same-named variant from the previous
/// version, keeping its original position. The new variant's fields fully replace the previous
/// fields.
///
/// ```ignore
/// define_versioned_type! {
///     pub enum EventV1 {
///         Started,
///         Stopped { code: u8 },
///     }
///
///     #[versioned_type(extend)]
///     pub enum EventV2 {
///         #[versioned_type(override)]
///         Stopped { code: u32, reason: String },
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV1 {
///     Started,
///     Stopped { code: u8 },
/// }
///
/// pub enum EventV2 {
///     Started,
///     Stopped { code: u32, reason: String },
/// }
/// ```
///
/// ## Extending a variant's fields
///
/// `#[versioned_type(extend)]` on a variant inherits its previous fields and appends new ones,
/// mirroring the struct-level rule.
///
/// ```ignore
/// define_versioned_type! {
///     pub enum EventV1 {
///         Started { caller: AccountId },
///     }
///
///     #[versioned_type(extend)]
///     pub enum EventV2 {
///         #[versioned_type(extend)]
///         Started { gas_used: Weight },
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV1 {
///     Started { caller: AccountId },
/// }
///
/// pub enum EventV2 {
///     Started { caller: AccountId, gas_used: Weight },
/// }
/// ```
///
/// ## Replacing a variant *and* a single field inside it
///
/// Combine `override, extend` on the variant with `override` on a specific field to change one
/// field's type while keeping (and possibly extending) the rest.
///
/// ```ignore
/// define_versioned_type! {
///     pub enum EventV1 {
///         Stopped { code: u8, reason: String },
///     }
///
///     #[versioned_type(extend)]
///     pub enum EventV2 {
///         #[versioned_type(override, extend)]
///         Stopped {
///             #[versioned_type(override)]
///             code: u32,
///         },
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV1 {
///     Stopped { code: u8, reason: String },
/// }
///
/// pub enum EventV2 {
///     Stopped { code: u32, reason: String },
/// }
/// ```
///
/// ## Cherry-picking variants without inheriting all of them
///
/// If the current enum is *not* annotated with `#[versioned_type(extend)]`, only the variants
/// written in the current source appear in the output. The variant-level `extend` and `override`
/// attributes still let you reuse fields from the previous version selectively.
///
/// ```ignore
/// define_versioned_type! {
///     pub enum EventV1 {
///         Started,
///         Stopped { code: u8 },
///         Errored,
///     }
///
///     pub enum EventV2 {
///         #[versioned_type(extend)]
///         Stopped { reason: String },
///         Replayed,
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV1 {
///     Started,
///     Stopped { code: u8 },
///     Errored,
/// }
///
/// pub enum EventV2 {
///     Stopped { code: u8, reason: String },
///     Replayed,
/// }
/// ```
///
/// `Started` and `Errored` are dropped because the new enum only declares `Stopped` (extending the
/// previous one) and `Replayed`.
///
/// ## A variant whose previous version was a struct
///
/// Type kinds may switch across versions: an enum may follow a struct. A variant with
/// `#[versioned_type(extend)]` then inherits the struct's fields. (Type-level `extend` across kinds
/// is *not* allowed; this is a variant-level operation.)
///
/// ```ignore
/// define_versioned_type! {
///     pub struct CallLogV1 {
///         pub caller: AccountId,
///         pub gas_used: Weight,
///     }
///
///     pub enum CallLogV2 {
///         #[versioned_type(extend)]
///         Standard { block: BlockNumber },
///         Failed { reason: String },
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct CallLogV1 {
///     pub caller: AccountId,
///     pub gas_used: Weight,
/// }
///
/// pub enum CallLogV2 {
///     Standard { caller: AccountId, gas_used: Weight, block: BlockNumber },
///     Failed { reason: String },
/// }
/// ```
///
/// Inherited fields lose their `pub` modifiers because enum-variant fields take inherited
/// visibility (which is the only valid setting in that position).
///
/// ## Preserving derives, attributes, generics, and `where` clauses
///
/// Every attribute that is not `versioned_type` is preserved exactly as written, and so are
/// visibility, generic parameters, and `where` clauses. The macro only edits fields and variants
/// according to its own attributes.
///
/// ```ignore
/// define_versioned_type! {
///     /// A log of a contract call.
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct LogV1<T>
///     where
///         T: Encode,
///     {
///         #[serde(skip)]
///         pub data: T,
///     }
///
///     /// A log of a contract call.
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     #[versioned_type(extend)]
///     pub struct LogV2<T>
///     where
///         T: Encode,
///     {
///         pub timestamp: u64,
///     }
/// }
/// ```
///
/// The doc comment, the `#[derive]`, the `#[serde(skip)]` field attribute, the `<T>` generic
/// parameter, and the `where` clause are all preserved on every version. The
/// `#[versioned_type(extend)]` attribute is the only one stripped from the output.
///
/// ## Out-of-order definitions and non-1 starting versions
///
/// Items may be written in any source order, and the version sequence does not have to start at
/// `V1` — only contiguity matters. The output is always emitted in ascending version order
/// regardless of source order.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct StateV4 { /* ... */ }
///
///     #[versioned_type(extend)]
///     pub struct StateV5 { /* ... */ }
///
///     pub struct StateV3 { /* ... */ }
/// }
/// ```
///
/// is accepted (versions `V3`, `V4`, `V5` are contiguous) and emits `StateV3`, then `StateV4`, then
/// `StateV5`.
///
/// # Item shape and versioning rules
///
/// The macro accepts zero or more `struct` or `enum` items. Empty input is valid and produces no
/// output, which makes the macro safe to invoke from code generation that may or may not have
/// anything to declare.
///
/// Each item must be a `struct` or `enum` — no other Rust items are accepted — and each identifier
/// must end with `V` followed by a positive integer (the version number).
///
/// The version suffix must:
///
/// * be a non-empty sequence of ASCII digits (e.g. `V1`, `V42`);
/// * have no leading zeros (`V01` is rejected, `V10` is accepted);
/// * be greater than zero (`V0` is rejected — versions start at `V1`).
///
/// The base name extends as far back as possible: the parser uses the *last* `V` followed by
/// digits, so `VeryVerboseLogV2` correctly splits into base name `VeryVerboseLog` and version `V2`.
///
/// Within a single invocation:
///
/// * every item must share the same base name (mixing `CallLogV1` with `OtherLogV2` is rejected);
/// * versions must be contiguous — gaps are rejected with a diagnostic naming the missing version
///   or range;
/// * the same version cannot be defined twice;
/// * the starting version need not be `1` — `CallLogV3` and `CallLogV4` is a valid invocation;
/// * items may appear in any source order; they are emitted in ascending version order regardless
///   of the order they appear in source.
///
/// Each item independently chooses to be a `struct` or an `enum`. Switching kinds across versions
/// is permitted as long as the new version does not request a *type-level* `extend` from a
/// different kind (see "Type-level extension" below); variant-level operations against a previous
/// struct *are* allowed and have specific semantics.
///
/// # The `#[versioned_type(...)]` helper attribute
///
/// All of the macro's behavior is controlled by the `#[versioned_type(...)]` helper attribute. It
/// is recognized on:
///
/// * the item itself (struct or enum),
/// * an enum variant,
/// * a named struct field or named variant field.
///
/// The attribute is always written in *list form*: `#[versioned_type(option_a, option_b)]`. Other
/// attribute syntaxes are rejected:
///
/// * a bare `#[versioned_type]` (path form) is rejected with a hint to use
///   `#[versioned_type(extend)]`;
/// * `#[versioned_type = "..."]` (name-value form) is rejected;
/// * each option is a bare flag — `extend = true` and `extend(...)` are rejected because the
///   options take no arguments;
/// * `#[versioned_type()]` with an empty option list is accepted and is equivalent to the attribute
///   being absent;
/// * the same option cannot appear twice in the same attribute, and the same option cannot appear
///   across two `#[versioned_type(...)]` attributes on the same item; both cases are rejected with
///   diagnostics that point at both occurrences;
/// * unrecognized options are rejected with a diagnostic listing the supported options.
///
/// The supported options are `extend` and `override`. Where each one is allowed:
///
/// * **on a type (struct or enum)** — `extend` is supported, `override` is rejected;
/// * **on an enum variant** — both `extend` and `override` are supported, and they may be combined
///   as `#[versioned_type(extend, override)]` (or `override, extend` — order is irrelevant);
/// * **on a named field** — `override` is supported, `extend` is rejected;
/// * **on a tuple field** — neither `extend` nor `override` is supported, because tuple fields have
///   no stable names to anchor an override to.
///
/// All `#[versioned_type(...)]` attributes are stripped from the generated code. Every other
/// attribute (including `#[derive]`, `#[doc]`, `#[cfg]`, `#[serde(...)]`, ...) is preserved exactly
/// as written and travels with the item, variant, or field it adorns.
///
/// # Type-level extension: `#[versioned_type(extend)]`
///
/// Placing `#[versioned_type(extend)]` on a `struct` or `enum` declares that the item is derived
/// from its immediately previous version.
///
/// * It requires a previous version to exist in the same invocation. The first item in a family
///   cannot use `extend`; doing so produces a "no previous version" diagnostic.
/// * Cross-kind type-level extension is forbidden: a `struct` cannot extend a previous `enum`, and
///   an `enum` cannot extend a previous `struct`. Both cases produce diagnostics that point at the
///   current item, the previous item, and the offending `extend` attribute. This restriction
///   applies only to *type-level* `extend` — variant-level operations against a previous struct are
///   still possible (see "Variant-level operations" below).
///
/// For structs, type-level `extend` performs field merging (see "Field merging" below).
///
/// For enums, type-level `extend` first copies *every* variant from the previous enum, preserving
/// order, and then applies the current enum's variant declarations on top:
///
/// * variants without a `versioned_type` attribute are appended after the inherited variants (they
///   are *new* variants);
/// * variants with `#[versioned_type(override)]` replace the same-named inherited variant *in its
///   original position*, leaving variant order unchanged for callers;
/// * variants with `#[versioned_type(extend)]` merge with the same-named inherited variant (also in
///   its original position);
/// * variants with `#[versioned_type(override, extend)]` produce the same observable result as
///   `#[versioned_type(extend)]` here — both replace the inherited variant in place and merge
///   fields. The combined form is accepted as an explicit way of stating the intent.
///
/// A standalone variant in an extending enum that collides with the name of an inherited variant is
/// an error. The diagnostic suggests adding `override` to acknowledge the replacement.
///
/// # Variant-level operations
///
/// Enum variants accept four modes:
///
/// * **standalone** (no `versioned_type` attribute) — the variant is appended to the output enum;
/// * **`extend`** — the variant's fields are merged with the same-named variant in the previous
///   version (the previous version may be an enum or a struct — see below);
/// * **`override`** — the variant replaces the same-named variant from the previous enum *in its
///   original position*; no field merging happens;
/// * **`override, extend`** (or `extend, override` — order is irrelevant) — the variant replaces
///   the previous variant *and* its fields are merged with the previous variant's fields.
///
/// Variant operations work in two surrounding contexts, with different bookkeeping:
///
/// 1. **Inside an enum that itself uses `#[versioned_type(extend)]`** — the output starts with all
///    of the previous enum's variants in order, and current variants are merged in by name.
///    Standalone variants must not collide with inherited names.
///
/// 2. **Inside a standalone enum** (no type-level `extend`) — the output starts empty, and only the
///    variants declared in the current source appear. `extend` and `override` on individual
///    variants still work selectively against the previous version. This lets a new enum
///    cherry-pick which variants to inherit, override, or extend, and discard the rest.
///
/// Cross-kind variant inheritance is supported in one direction only: a variant's `extend` may
/// reference a previous *struct*. In that case the previous struct's fields are merged into the
/// variant. `override` (and therefore `override, extend`) on a variant cannot reference a previous
/// struct — a struct has no variants to identify by name, and both forms are rejected with the same
/// diagnostic that targets the offending option.
///
/// `extend` and `override` both require a target to exist in the previous version; targeting a
/// non-existent name produces a diagnostic that points at the current variant and the offending
/// attribute. Two variants in the same current enum cannot share an identifier; the macro rejects
/// duplicates regardless of their attributes.
///
/// # Field-level override
///
/// `#[versioned_type(override)]` may be placed on a *named* struct field or named variant field to
/// replace the same-named field from the previous version *in its original position*. The new
/// field's type, attributes, and visibility take effect; the previous field's type and attributes
/// are discarded.
///
/// Field-level override is only meaningful inside an extending context — the surrounding type
/// carries `#[versioned_type(extend)]`, or the surrounding variant carries
/// `#[versioned_type(extend)]` or `#[versioned_type(override, extend)]`. Using `override` on a
/// field outside such a context is rejected.
///
/// Constraints:
///
/// * the previous fields must be *named* — overriding when the previous version was a tuple struct
///   is rejected as ambiguous, because tuple positions have no stable names;
/// * the override target must exist in the previous version; an override of a missing field is
///   rejected with a diagnostic that points at the offending attribute;
/// * `override` is not allowed on tuple fields, because tuple fields lack stable names to identify
///   the override target;
/// * field-level `extend` does not exist — field merging is controlled by the enclosing item or
///   variant. Using `extend` on a field is rejected.
///
/// A current named field whose name matches an inherited field but lacks `override` is rejected
/// with a diagnostic that suggests adding `override`. Two current fields in the same field list
/// cannot share a name.
///
/// # Field merging
///
/// Field merging applies in two situations: a struct extending a previous struct, and an enum
/// variant extending a previous variant or struct. The merge produces a single field list using the
/// following rule:
///
/// 1. For every field in the *previous* version, in source order:
///    * if the current source carries an `override` for that name, emit the *current* field in this
///      position (the previous field's type and attributes are discarded);
///    * otherwise, emit the *previous* field with its visibility adjusted (see "Visibility of
///      inherited fields" below).
/// 2. Append every *new* current field — those that have no override and whose name did not exist
///    previously — in source order, after the inherited fields.
///
/// Overrides preserve the original field position from the previous version, while purely new
/// fields appear at the end.
///
/// The current and previous shapes (`Named`, `Unnamed` / tuple, `Unit`) can differ. The macro
/// handles every combination:
///
/// * **named ⇐ named** — fields are merged by name following the rule above.
/// * **named ⇐ tuple** — the previous tuple fields are renamed to `field_0`, `field_1`, ... and
///   placed first; current named fields are appended afterwards. If a current named field collides
///   with one of these synthetic names, the macro reports an error that points at both the current
///   field and the previous tuple field.
/// * **named ⇐ unit** — nothing is inherited; current named fields are kept as-is. `override` is
///   rejected with a "previous version has no fields" diagnostic.
/// * **tuple ⇐ named** — the previous named fields lose their names and become positional fields;
///   current tuple fields are appended afterwards.
/// * **tuple ⇐ tuple** — the previous tuple fields are placed first, current tuple fields
///   afterwards.
/// * **tuple ⇐ unit** — nothing is inherited; current tuple fields are kept as-is.
/// * **unit ⇐ named** — the current item becomes a *named* struct or variant containing the
///   previous fields verbatim (with adjusted visibility).
/// * **unit ⇐ tuple** — the current item becomes a *tuple* struct or variant containing the
///   previous fields verbatim (with adjusted visibility).
/// * **unit ⇐ unit** — the output stays unit.
///
/// # Visibility of inherited fields
///
/// Fields copied from a previous version have their visibility adjusted to fit their new location:
///
/// * struct fields become `pub`, regardless of their previous visibility;
/// * enum variant fields become `Inherited` (no visibility modifier), which is the only valid
///   setting on enum variant fields.
///
/// Fields written directly in the current source keep their stated visibility unchanged, including
/// `pub`, `pub(crate)`, `pub(super)`, or no modifier.
///
/// # Stripping and preservation
///
/// The macro never lets `versioned_type` helpers leak into the output. Every
/// `#[versioned_type(...)]` is removed from items, variants, and fields, even on items that do not
/// perform any extension (where the helper has no effect anyway).
///
/// Everything else is preserved verbatim:
///
/// * outer attributes on items (`#[derive]`, `#[doc]`, `#[cfg]`, `#[serde(...)]`, ...);
/// * doc comments on items, variants, and fields;
/// * visibility modifiers (`pub`, `pub(crate)`, `pub(super)`, ...);
/// * generic parameters and `where` clauses;
/// * field-, variant-, and item-level attributes other than `versioned_type`.
///
/// When a current variant or field replaces a previous one through `override`, the previous
/// attributes are dropped — the override is a full replacement, not an attribute merge.
///
/// # Output ordering
///
/// Items are emitted in ascending version order, regardless of the order they appear in source. So:
///
/// ```ignore
/// define_versioned_type! {
///     pub struct CallLogV4 { /* ... */ }
///     pub struct CallLogV3 { /* ... */ }
/// }
/// ```
///
/// emits `CallLogV3` before `CallLogV4` in the generated code.
///
/// # Diagnostics
///
/// The macro reports compile errors with spans that point at the offending source. Common
/// categories include:
///
/// * **Naming**: missing `V`, empty version suffix, non-numeric suffix, leading-zero version, `V0`,
///   missing base name.
/// * **Per-invocation**: mismatched base names, duplicate versions, non-contiguous versions.
/// * **Attribute syntax**: bare `#[versioned_type]`, name-value form, options with arguments,
///   duplicate options, unsupported options, `extend` on a field, `override` on a type.
/// * **Type-level extension**: `extend` without a previous version, struct extending an enum, enum
///   extending a struct.
/// * **Variant operations**: `extend` or `override` targeting a variant that does not exist in the
///   previous version, `override` against a previous struct, standalone variant colliding with an
///   inherited variant, duplicate current variants.
/// * **Field operations**: `override` on a tuple field, `override` against a previous tuple shape,
///   `override` outside an extending context, redefining an inherited named field without
///   `override`, override of a missing previous named field, current named field colliding with a
///   synthetic `field_N` name produced from previous tuple fields, duplicate current fields.
#[proc_macro]
pub fn define_versioned_type(input: TokenStream) -> TokenStream {
	let input =
		syn::parse_macro_input!(input as handle_define_versioned_type::DefineVersionedTypeInput);
	let items = match handle_define_versioned_type::handle_define_versioned_type(input) {
		Ok(items) => items,
		Err(error) => return error.to_compile_error().into(),
	};

	quote! { #( #items )* }.into()
}

/// Parsed environment definition.
struct EnvDef {
	host_funcs: Vec<HostFn>,
}

/// Parsed host function definition.
struct HostFn {
	item: syn::ItemFn,
	name: String,
	returns: HostFnReturn,
	cfg: Option<syn::Attribute>,
}

enum HostFnReturn {
	Unit,
	U32,
	U64,
	ReturnCode,
}

impl HostFnReturn {
	fn map_output(&self) -> TokenStream2 {
		match self {
			Self::Unit => quote! { |_| None },
			_ => quote! { |ret_val| Some(ret_val.into()) },
		}
	}

	fn success_type(&self) -> syn::ReturnType {
		match self {
			Self::Unit => syn::ReturnType::Default,
			Self::U32 => parse_quote! { -> u32 },
			Self::U64 => parse_quote! { -> u64 },
			Self::ReturnCode => parse_quote! { -> ReturnErrorCode },
		}
	}

	fn trace_return_value(&self) -> TokenStream2 {
		match self {
			Self::Unit => quote! { None },
			Self::U32 => quote! { result.as_ref().ok().map(|r| *r as u64) },
			Self::ReturnCode => quote! { result.as_ref().ok().copied().map(u64::from) },
			Self::U64 => quote! { result.as_ref().ok().copied() },
		}
	}
}

impl EnvDef {
	pub fn try_from(item: syn::ItemMod) -> syn::Result<Self> {
		let span = item.span();
		let err = |msg| syn::Error::new(span, msg);
		let items = &item
			.content
			.as_ref()
			.ok_or(err("Invalid environment definition, expected `mod` to be inlined."))?
			.1;

		let extract_fn = |i: &syn::Item| match i {
			syn::Item::Fn(i_fn) => Some(i_fn.clone()),
			_ => None,
		};

		let host_funcs = items
			.iter()
			.filter_map(extract_fn)
			.map(HostFn::try_from)
			.collect::<Result<Vec<_>, _>>()?;

		Ok(Self { host_funcs })
	}
}

impl HostFn {
	pub fn try_from(mut item: syn::ItemFn) -> syn::Result<Self> {
		let err = |span, msg| {
			let msg = format!("Invalid host function definition.\n{}", msg);
			syn::Error::new(span, msg)
		};

		// process attributes
		let msg = "Only #[cfg] and #[mutating] attributes are allowed.";
		let span = item.span();
		let mut attrs = item.attrs.clone();
		attrs.retain(|a| !a.path().is_ident("doc"));
		let mut mutating = false;
		let mut cfg = None;
		while let Some(attr) = attrs.pop() {
			let ident = attr.path().get_ident().ok_or(err(span, msg))?.to_string();
			match ident.as_str() {
				"mutating" => {
					if mutating {
						return Err(err(span, "#[mutating] can only be specified once"));
					}
					mutating = true;
				},
				"cfg" => {
					if cfg.is_some() {
						return Err(err(span, "#[cfg] can only be specified once"));
					}
					cfg = Some(attr);
				},
				id => return Err(err(span, &format!("Unsupported attribute \"{id}\". {msg}"))),
			}
		}

		if mutating {
			let stmt = syn::parse_quote! {
				if self.ext().is_read_only() {
					return Err(Error::<E::T>::StateChangeDenied.into());
				}
			};
			item.block.stmts.insert(0, stmt);
		}

		let name = item.sig.ident.to_string();

		let msg = "Every function must start with these two parameters: &mut self, memory: &mut M";
		let special_args = item
			.sig
			.inputs
			.iter()
			.take(2)
			.enumerate()
			.map(|(i, arg)| is_valid_special_arg(i, arg))
			.fold(0u32, |acc, valid| if valid { acc + 1 } else { acc });

		if special_args != 2 {
			return Err(err(span, msg));
		}

		// process return type
		let msg = r#"Should return one of the following:
				- Result<(), TrapReason>,
				- Result<ReturnErrorCode, TrapReason>,
				- Result<u32, TrapReason>,
				- Result<u64, TrapReason>"#;
		let ret_ty = match item.clone().sig.output {
			syn::ReturnType::Type(_, ty) => Ok(ty.clone()),
			_ => Err(err(span, &msg)),
		}?;
		match *ret_ty {
			syn::Type::Path(tp) => {
				let result = &tp.path.segments.last().ok_or(err(span, &msg))?;
				let (id, span) = (result.ident.to_string(), result.ident.span());
				id.eq(&"Result".to_string()).then_some(()).ok_or(err(span, &msg))?;

				match &result.arguments {
					syn::PathArguments::AngleBracketed(group) => {
						if group.args.len() != 2 {
							return Err(err(span, &msg));
						};

						let arg2 = group.args.last().ok_or(err(span, &msg))?;

						let err_ty = match arg2 {
							syn::GenericArgument::Type(ty) => Ok(ty.clone()),
							_ => Err(err(arg2.span(), &msg)),
						}?;

						match err_ty {
							syn::Type::Path(tp) => Ok(tp
								.path
								.segments
								.first()
								.ok_or(err(arg2.span(), &msg))?
								.ident
								.to_string()),
							_ => Err(err(tp.span(), &msg)),
						}?
						.eq("TrapReason")
						.then_some(())
						.ok_or(err(span, &msg))?;

						let arg1 = group.args.first().ok_or(err(span, &msg))?;
						let ok_ty = match arg1 {
							syn::GenericArgument::Type(ty) => Ok(ty.clone()),
							_ => Err(err(arg1.span(), &msg)),
						}?;
						let ok_ty_str = match ok_ty {
							syn::Type::Path(tp) => Ok(tp
								.path
								.segments
								.first()
								.ok_or(err(arg1.span(), &msg))?
								.ident
								.to_string()),
							syn::Type::Tuple(tt) => {
								if !tt.elems.is_empty() {
									return Err(err(arg1.span(), &msg));
								};
								Ok("()".to_string())
							},
							_ => Err(err(ok_ty.span(), &msg)),
						}?;
						let returns = match ok_ty_str.as_str() {
							"()" => Ok(HostFnReturn::Unit),
							"u32" => Ok(HostFnReturn::U32),
							"u64" => Ok(HostFnReturn::U64),
							"ReturnErrorCode" => Ok(HostFnReturn::ReturnCode),
							_ => Err(err(arg1.span(), &msg)),
						}?;

						Ok(Self { item, name, returns, cfg })
					},
					_ => Err(err(span, &msg)),
				}
			},
			_ => Err(err(span, &msg)),
		}
	}
}

fn is_valid_special_arg(idx: usize, arg: &FnArg) -> bool {
	match (idx, arg) {
		(0, FnArg::Receiver(rec)) => rec.reference.is_some() && rec.mutability.is_some(),
		(1, FnArg::Typed(pat)) => {
			let ident = if let syn::Pat::Ident(ref ident) = *pat.pat {
				&ident.ident
			} else {
				return false;
			};
			if !(ident == "memory" || ident == "_memory") {
				return false;
			}
			matches!(*pat.ty, syn::Type::Reference(_))
		},
		_ => false,
	}
}

fn arg_decoder<'a, P, I>(param_names: P, param_types: I) -> TokenStream2
where
	P: Iterator<Item = &'a std::boxed::Box<syn::Pat>> + Clone,
	I: Iterator<Item = &'a std::boxed::Box<syn::Type>> + Clone,
{
	const ALLOWED_REGISTERS: usize = 6;

	// too many arguments
	if param_names.clone().count() > ALLOWED_REGISTERS {
		panic!("Syscalls take a maximum of {ALLOWED_REGISTERS} arguments");
	}

	// all of them take one register but we truncate them before passing into the function
	// it is important to not allow any type which has illegal bit patterns like 'bool'
	if !param_types.clone().all(|ty| {
		let syn::Type::Path(path) = &**ty else {
			panic!("Type needs to be path");
		};
		let Some(ident) = path.path.get_ident() else {
			panic!("Type needs to be ident");
		};
		matches!(ident.to_string().as_ref(), "u8" | "u16" | "u32" | "u64")
	}) {
		panic!("Only primitive unsigned integers are allowed as arguments to syscalls");
	}

	// one argument per register
	let bindings = param_names.zip(param_types).enumerate().map(|(idx, (name, ty))| {
		let reg = quote::format_ident!("__a{}__", idx);
		quote! {
			let #name = #reg as #ty;
		}
	});
	quote! {
		#( #bindings )*
	}
}

/// Expands environment definition.
/// Should generate source code for:
///  - implementations of the host functions to be added to the polkavm runtime environment (see
///    `expand_impls()`).
fn expand_env(def: &EnvDef) -> TokenStream2 {
	let impls = expand_functions(def);
	let bench_impls = expand_bench_functions(def);
	let docs = expand_func_doc(def);
	let all_syscalls = expand_func_list(def);
	let lookup_syscall = expand_func_lookup(def);
	let all_trace_ops = expand_trace_op_list(def);
	let lookup_trace_op = expand_trace_op_lookup(def);

	quote! {
		/// Returns the list of all syscalls that contracts can import.
		pub fn list_syscalls() -> &'static [&'static [u8]] {
			#all_syscalls
		}

		/// Return the index of a syscall in the `list_syscalls()` list.
		pub fn lookup_syscall_index(name: &'static str) -> Option<u8> {
			#lookup_syscall
		}

		/// Returns the list of all trace operations (real syscalls + synthetic trace steps).
		pub fn list_trace_ops() -> &'static [&'static [u8]] {
			#all_trace_ops
		}

		/// Return the index of a trace operation in the `list_trace_ops()` list.
		pub fn lookup_trace_op_index(name: &'static str) -> Option<u8> {
			#lookup_trace_op
		}

		impl<'a, E: Ext, M: PolkaVmInstance<E::T>> Runtime<'a, E, M> {
			fn handle_ecall(
				&mut self,
				memory: &mut M,
				__syscall_symbol__: &[u8],
			) -> Result<Option<u64>, TrapReason>
			{
				#impls
			}
		}

		#[cfg(feature = "runtime-benchmarks")]
		impl<'a, E: Ext, M: ?Sized + Memory<E::T>> Runtime<'a, E, M> {
			#bench_impls
		}

		/// Documentation of the syscalls (host functions) available to contracts.
		///
		/// Each of the functions in this trait represent a function that is callable
		/// by the contract. Guests use the function name as the import symbol.
		///
		/// # Note
		///
		/// This module is not meant to be used by any code. Rather, it is meant to be
		/// consumed by humans through rustdoc.
		#[cfg(doc)]
		pub trait SyscallDoc {
			#docs
		}
	}
}

fn expand_functions(def: &EnvDef) -> TokenStream2 {
	let impls = def.host_funcs.iter().map(|f| {
		// skip the self and memory argument
		let params = f.item.sig.inputs.iter().skip(2);
		let param_names = params.clone().filter_map(|arg| {
			let FnArg::Typed(arg) = arg else {
				return None;
			};
			Some(&arg.pat)
		});
		let param_types = params.clone().filter_map(|arg| {
			let FnArg::Typed(arg) = arg else {
				return None;
			};
			Some(&arg.ty)
		});
		let arg_decoder = arg_decoder(param_names, param_types);
		let cfg = &f.cfg;
		let name = &f.name;
		let syscall_symbol = Literal::byte_string(name.as_bytes());
		let body = &f.item.block;
		let map_output = f.returns.map_output();
		let trace_return = f.returns.trace_return_value();
		let output = &f.item.sig.output;

		// wrapped host function body call with host function traces
		let wrapped_body_with_trace = {
			let trace_fmt_args = params.clone().filter_map(|arg| match arg {
				syn::FnArg::Receiver(_) => None,
				syn::FnArg::Typed(p) => match *p.pat.clone() {
					syn::Pat::Ident(ref pat_ident) => Some(pat_ident.ident.clone()),
					_ => None,
				},
			});

			let params_fmt_str = trace_fmt_args
				.clone()
				.map(|s| format!("{s}: {{:?}}"))
				.collect::<Vec<_>>()
				.join(", ");
			let trace_fmt_str = format!("{}({}) = {{:?}} weight_consumed: {{:?}}", name, params_fmt_str);
			let trace_args_for_tracer: Vec<_> = trace_fmt_args.clone().collect();

			quote! {
				crate::tracing::if_tracing(|tracer| {
					tracer.enter_ecall(#name, &[#( #trace_args_for_tracer as u64 ),*], self)
				});

				// wrap body in closure to make sure the tracing is always executed
				let result = (|| #body)();
				::log::trace!(target: "runtime::revive::strace", #trace_fmt_str, #( #trace_fmt_args, )* result, self.ext.frame_meter().weight_consumed());

				crate::tracing::if_tracing(|tracer| tracer.exit_step(self, #trace_return));
				result
			}
		};

		quote! {
			#cfg
			#syscall_symbol => {
				// closure is needed so that "?" can infere the correct type
				(|| #output {
					#arg_decoder
					#wrapped_body_with_trace
				})().map(#map_output)
			},
		}
	});

	quote! {
		crate::tracing::if_tracing(|tracer| {
			tracer.enter_ecall(crate::tracing::PVM_FUEL_NAME, &[], self)
		});

		let __sync_result__ = self.ext
			.frame_meter_mut()
			.sync_from_executor(memory.gas())
			.map_err(TrapReason::from);

		crate::tracing::if_tracing(|tracer| tracer.exit_step(self, None));

		__sync_result__?;

		// This is the overhead to call an empty syscall that always needs to be charged.
		self.charge_gas(crate::vm::RuntimeCosts::HostFn).map_err(TrapReason::from)?;

		// They will be mapped to variable names by the syscall specific code.
		let (__a0__, __a1__, __a2__, __a3__, __a4__, __a5__) = memory.read_input_regs();

		// Execute the syscall specific logic in a closure so that the gas metering code is always executed.
		let result = (|| match __syscall_symbol__ {
			#( #impls )*
			_ => Err(TrapReason::SupervisorError(Error::<E::T>::InvalidSyscall.into()))
		})();

		// Write gas from pallet-revive into polkavm after leaving the host function.
		let gas = self.ext.frame_meter_mut().sync_to_executor();
		memory.set_gas(gas.into());
		result
	}
}

fn expand_bench_functions(def: &EnvDef) -> TokenStream2 {
	let impls = def.host_funcs.iter().map(|f| {
		// skip the context and memory argument
		let params = f.item.sig.inputs.iter().skip(2);
		let cfg = &f.cfg;
		let name = &f.name;
		let body = &f.item.block;
		let output = &f.item.sig.output;

		let name = Ident::new(&format!("bench_{name}"), Span::call_site());
		quote! {
			#cfg
			pub fn #name(&mut self, memory: &mut M, #(#params),*) #output {
				#body
			}
		}
	});

	quote! {
		#( #impls )*
	}
}

fn expand_func_doc(def: &EnvDef) -> TokenStream2 {
	let docs = def.host_funcs.iter().map(|func| {
		// Remove auxiliary args: `ctx: _` and `memory: _`
		let func_decl = {
			let mut sig = func.item.sig.clone();
			sig.inputs = sig
				.inputs
				.iter()
				.skip(2)
				.map(|p| p.clone())
				.collect::<Punctuated<FnArg, Comma>>();
			sig.output = func.returns.success_type();
			sig.to_token_stream()
		};
		let func_doc = {
			let func_docs = {
				let docs = func.item.attrs.iter().filter(|a| a.path().is_ident("doc")).map(|d| {
					let docs = d.to_token_stream();
					quote! { #docs }
				});
				quote! { #( #docs )* }
			};
			quote! {
				#func_docs
			}
		};
		quote! {
			#func_doc
			#func_decl;
		}
	});

	quote! {
		#( #docs )*
	}
}

fn expand_func_list(def: &EnvDef) -> TokenStream2 {
	let docs = def.host_funcs.iter().map(|f| {
		let name = Literal::byte_string(f.name.as_bytes());
		quote! {
			#name.as_slice()
		}
	});
	let len = docs.clone().count();

	quote! {
		{
			static FUNCS: [&[u8]; #len] = [#(#docs),*];
			FUNCS.as_slice()
		}
	}
}

fn expand_func_lookup(def: &EnvDef) -> TokenStream2 {
	let arms = def.host_funcs.iter().enumerate().map(|(idx, f)| {
		let name_str = &f.name;
		quote! {
			#name_str => Some(#idx as u8)
		}
	});
	quote! {
		match name {
			#( #arms, )*
			_ => None,
		}
	}
}

fn expand_trace_op_list(def: &EnvDef) -> TokenStream2 {
	let syscalls = def.host_funcs.iter().map(|f| {
		let name = Literal::byte_string(f.name.as_bytes());
		quote! {
			#name.as_slice()
		}
	});
	let len = syscalls.clone().count() + 1;

	quote! {
		{
			static OPS: [&[u8]; #len] = [
				#(#syscalls,)*
				crate::tracing::PVM_FUEL_NAME.as_bytes(),
			];
			OPS.as_slice()
		}
	}
}

fn expand_trace_op_lookup(def: &EnvDef) -> TokenStream2 {
	let arms = def.host_funcs.iter().enumerate().map(|(idx, f)| {
		let name_str = &f.name;
		quote! {
			#name_str => Some(#idx as u8)
		}
	});
	let pvm_fuel_idx = def.host_funcs.len();

	quote! {
		match name {
			#( #arms, )*
			crate::tracing::PVM_FUEL_NAME => Some(#pvm_fuel_idx as u8),
			_ => None,
		}
	}
}
