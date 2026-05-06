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

extern crate alloc;

mod define_env;
mod define_versioned_interface;
mod define_versioned_type;

use proc_macro::TokenStream;
use quote::quote;

/// Defines a host functions set that can be imported by contract polkavm code.
///
/// **CAUTION**: Be advised that all functions defined by this macro
/// cause undefined behavior inside the contract if the signature does not match.
///
/// WARNING: It is CRITICAL for contracts to make sure that the signatures match exactly.
/// Failure to do so may result in undefined behavior, traps or security vulnerabilities inside the
/// contract. The runtime itself is unharmed due to sandboxing.
/// For example, if a function is called with an incorrect signature, it could lead to memory
/// corruption or unexpected results within the contract.
#[proc_macro_attribute]
pub fn define_env(attr: TokenStream, item: TokenStream) -> TokenStream {
	define_env::handle_define_env(attr, item)
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
/// ## Inserting a field before or after an inherited field
///
/// New named fields can be inserted next to a named field from the previous version with
/// `insert_before = "target"` or `insert_after = "target"`. This is useful when the encoded field
/// order is externally constrained and a new field cannot simply be appended.
///
/// ```ignore
/// define_versioned_type! {
///     pub struct CodeInfoV1 {
///         pub code_len: u32,
///         pub behaviour_version: u32,
///     }
///
///     #[versioned_type(extend)]
///     pub struct CodeInfoV2 {
///         #[versioned_type(insert_before = "behaviour_version")]
///         pub code_type: BytecodeType,
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub struct CodeInfoV1 {
///     pub code_len: u32,
///     pub behaviour_version: u32,
/// }
///
/// pub struct CodeInfoV2 {
///     pub code_len: u32,
///     pub code_type: BytecodeType,
///     pub behaviour_version: u32,
/// }
/// ```
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
/// ## Inserting new variants into an inherited enum
///
/// New variants in an extending enum can be inserted next to a variant from the previous version
/// with `insert_before = "Target"` or `insert_after = "Target"`.
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
///         #[versioned_type(insert_after = "Started")]
///         Paused,
///     }
/// }
/// ```
///
/// expands to:
///
/// ```ignore
/// pub enum EventV2 {
///     Started,
///     Paused,
///     Stopped,
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
/// * `extend` and `override` are bare flags — `extend = true` and `extend(...)` are rejected;
/// * `insert_before` and `insert_after` require a string literal target using name-value syntax,
///   for example `#[versioned_type(insert_before = "behaviour_version")]`;
/// * `#[versioned_type()]` with an empty option list is accepted and is equivalent to the attribute
///   being absent;
/// * the same option cannot appear twice in the same attribute, and the same option cannot appear
///   across two `#[versioned_type(...)]` attributes on the same item; both cases are rejected with
///   diagnostics that point at both occurrences;
/// * unrecognized options are rejected with a diagnostic listing the supported options.
///
/// The supported options are `extend`, `override`, `insert_before`, and `insert_after`. Where each
/// one is allowed:
///
/// * **on a type (struct or enum)** — `extend` is supported; `override`, `insert_before`, and
///   `insert_after` are rejected;
/// * **on an enum variant** — `extend` and `override` are supported, and they may be combined as
///   `#[versioned_type(extend, override)]` (or `override, extend` — order is irrelevant);
///   `insert_before` and `insert_after` are supported only on fresh variants and cannot be combined
///   with `extend` or `override`;
/// * **on a named field** — `override`, `insert_before`, and `insert_after` are supported;
///   `insert_before` and `insert_after` cannot be combined with `override`, and `extend` is
///   rejected;
/// * **on a tuple field** — no helper operations are supported, because tuple fields have no stable
///   names to anchor an operation to.
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
/// * variants with `insert_before = "Target"` or `insert_after = "Target"` are inserted next to the
///   named inherited target variant.
///
/// A standalone variant in an extending enum that collides with the name of an inherited variant is
/// an error. The diagnostic suggests adding `override` to acknowledge the replacement.
///
/// # Variant-level operations
///
/// Enum variants accept five modes:
///
/// * **standalone** (no `versioned_type` attribute) — the variant is appended to the output enum;
/// * **`extend`** — the variant's fields are merged with the same-named variant in the previous
///   version (the previous version may be an enum or a struct — see below);
/// * **`override`** — the variant replaces the same-named variant from the previous enum *in its
///   original position*; no field merging happens;
/// * **`override, extend`** (or `extend, override` — order is irrelevant) — the variant replaces
///   the previous variant *and* its fields are merged with the previous variant's fields.
/// * **`insert_before = "Target"` or `insert_after = "Target"`** — the fresh variant is inserted
///   next to `Target`, which must be a variant inherited from the previous enum.
///
/// Variant operations work in two surrounding contexts, with different bookkeeping:
///
/// 1. **Inside an enum that itself uses `#[versioned_type(extend)]`** — the output starts with all
///    of the previous enum's variants in order, and current variants are merged in by name.
///    Standalone variants must not collide with inherited names. Inserted variants are placed next
///    to their inherited target variants.
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
/// `extend`, `override`, `insert_before`, and `insert_after` all require a target to exist in the
/// previous version. Targeting a non-existent name produces a diagnostic that points at the current
/// variant and the offending attribute. Inserted variants also require enum-level `extend`, because
/// otherwise the inherited target variant is not present in the output. Two variants in the same
/// current enum cannot share an identifier; the macro rejects duplicates regardless of their
/// attributes.
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
/// # Field-level insertion
///
/// `#[versioned_type(insert_before = "target")]` and
/// `#[versioned_type(insert_after = "target")]` may be placed on a fresh named struct field or
/// named variant field to position it next to a field from the previous version. The target must
/// name a previous named field, and the inserted field's own name must not collide with any
/// inherited field.
///
/// Field insertion is only meaningful inside the same extending contexts as field override: the
/// surrounding type carries `#[versioned_type(extend)]`, or the surrounding variant carries
/// `#[versioned_type(extend)]` or `#[versioned_type(override, extend)]`. Using insertion outside
/// such a context is rejected.
///
/// Constraints:
///
/// * the previous fields must be *named* — inserting around tuple fields is rejected as ambiguous,
///   because tuple positions have no stable names;
/// * the insertion target must exist in the previous version;
/// * `insert_before` and `insert_after` are not allowed on tuple fields;
/// * insertion cannot be combined with `override`, because an override already preserves the
///   inherited field's position.
///
/// # Field merging
///
/// Field merging applies in two situations: a struct extending a previous struct, and an enum
/// variant extending a previous variant or struct. The merge produces a single field list using the
/// following rule:
///
/// 1. For every field in the *previous* version, in source order:
///    * first emit any current fields marked `insert_before` for this previous field, in source
///      order;
///    * if the current source carries an `override` for that name, emit the *current* field in this
///      position (the previous field's type and attributes are discarded);
///    * otherwise, emit the *previous* field with its visibility adjusted (see "Visibility of
///      inherited fields" below).
///    * then emit any current fields marked `insert_after` for this previous field, in source
///      order.
/// 2. Append every *new* current field — those that have no field operation and whose name did not
///    exist previously — in source order, after the inherited fields.
///
/// Overrides preserve the original field position from the previous version, inserted fields appear
/// next to their targets, and purely new fields appear at the end.
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
/// # Latest alias
///
/// Each invocation also emits a `Latest{Name}` type alias pointing at the highest version in that
/// invocation. For example, a family named `CallLogV1`, `CallLogV2` emits:
///
/// ```ignore
/// pub type LatestCallLog = CallLogV2;
/// ```
///
/// If the latest version is generic, the alias carries the same generic parameter names but omits
/// bounds and `where` clauses because Rust does not enforce bounds written on type aliases.
///
/// # Diagnostics
///
/// The macro reports compile errors with spans that point at the offending source. Common
/// categories include:
///
/// * **Naming**: missing `V`, empty version suffix, non-numeric suffix, leading-zero version, `V0`,
///   missing base name.
/// * **Per-invocation**: mismatched base names, duplicate versions, non-contiguous versions.
/// * **Attribute syntax**: bare `#[versioned_type]`, unsupported name-value form, options with the
///   wrong argument shape, duplicate options, unsupported options, `extend` on a field, `override`
///   on a type, insertion on a type, insertion combined with `extend` or `override`.
/// * **Type-level extension**: `extend` without a previous version, struct extending an enum, enum
///   extending a struct.
/// * **Variant operations**: `extend`, `override`, or insertion targeting a variant that does not
///   exist in the previous version, insertion without enum-level `extend`, `override` against a
///   previous struct, standalone variant colliding with an inherited variant, duplicate current
///   variants.
/// * **Field operations**: helper operations on tuple fields, `override` or insertion against a
///   previous tuple shape, `override` or insertion outside an extending context, redefining an
///   inherited named field without `override`, missing previous named field targets, current named
///   field colliding with a synthetic `field_N` name produced from previous tuple fields, duplicate
///   current fields.
#[proc_macro]
pub fn define_versioned_type(input: TokenStream) -> TokenStream {
	let input = syn::parse_macro_input!(input as define_versioned_type::DefineVersionedTypeInput);
	let output = match define_versioned_type::handle_define_versioned_type(input) {
		Ok(output) => output,
		Err(error) => return error.to_compile_error().into(),
	};

	quote! { #output }.into()
}

/// Defines a paired family of input and output payload structs for a versioned wire interface and
/// generates the enums, conversions, and accessors that route any known version through it.
///
/// # Motivation
///
/// Runtime APIs in `pallet-revive` are addressed by stable names like `eth_transact`, but the
/// argument and return shapes carried by those names evolve as the runtime is upgraded. A node
/// running a new version must still accept requests from clients that only know an older version,
/// and historical state queries must continue to round-trip through every version that ever
/// shipped. The accepted way to keep this honest is two layers of types:
///
/// 1. *Wire types* — one enum per side of the interface that lists every known version of the
///    payload as its own variant. Anything that crosses the runtime boundary uses these.
/// 2. *Execution types* — concrete `…V{n}` structs that the runtime works with internally once it
///    has chosen which version of the interface this call belongs to.
///
/// Hand-writing both layers is repetitive and error-prone: every new version adds two structs
/// (input and output), two enum variants, two `From` impls, two `TryFrom` impls, and a fresh batch
/// of accessor methods, and any drift between the input and output sides has to be caught by code
/// review. `define_versioned_interface!` collapses all of that boilerplate to the only thing that
/// genuinely changes between versions: the field shapes of the input and output payloads.
///
/// One invocation declares one interface family. Multiple unrelated interfaces (e.g. `eth_transact`
/// and `estimate_gas`) live in separate invocations.
///
/// `define_versioned_interface!` is the *interface* counterpart to `define_versioned_type!`: the
/// type macro expresses each version of one struct as a delta from the previous version, while this
/// macro pairs the input and output types of one runtime API and emits the wire-level enums and
/// conversions that connect them.
///
/// # At a glance
///
/// The simplest invocation declares both sides of one interface for two versions. The payload
/// structs are emitted verbatim; the macro adds one versioned enum per side, an inherent `impl`
/// block of helpers, and `From`/`TryFrom` impls for each variant.
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactInputPayloadV1 {
///         pub tx: GenericTransaction,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactOutputPayloadV1 {
///         pub result: EthTransactInfo,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactInputPayloadV2 {
///         pub tx: GenericTransaction,
///         pub config: DryRunConfig,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactOutputPayloadV2 {
///         pub result: EthTransactInfo,
///     }
/// }
/// ```
///
/// expands (conceptually) to the four payload structs verbatim, plus:
///
/// ```ignore
/// // One enum per side, with each variant boxing its payload to keep the enum a fixed size.
/// #[derive(Clone, Debug, PartialEq, Encode, Decode)]
/// pub enum VersionedEthTransactInputPayload {
///     V1(::alloc::boxed::Box<EthTransactInputPayloadV1>),
///     V2(::alloc::boxed::Box<EthTransactInputPayloadV2>),
/// }
/// // …and identically `VersionedEthTransactOutputPayload` with V1/V2 boxed variants.
///
/// pub type LatestEthTransactInputPayload = EthTransactInputPayloadV2;
/// pub type LatestEthTransactOutputPayload = EthTransactOutputPayloadV2;
///
/// // An inherent impl with constructors, version inspection, and per-version accessors.
/// impl VersionedEthTransactInputPayload {
///     pub const LATEST_VERSION: u32 = 2;
///
///     pub fn new_v1(payload: EthTransactInputPayloadV1) -> Self { /* … */ }
///     pub fn new_v2(payload: EthTransactInputPayloadV2) -> Self { /* … */ }
///
///     pub fn version(&self) -> usize { /* returns 1 for V1, 2 for V2 */ }
///
///     pub fn as_v1(&self) -> Option<&EthTransactInputPayloadV1> { /* … */ }
///     pub fn into_v1(self) -> Option<EthTransactInputPayloadV1> { /* … */ }
///     pub fn unwrap_v1(self) -> EthTransactInputPayloadV1 { /* panics on a different variant */ }
///     // …same trio for v2.
/// }
///
/// // `From` and `TryFrom` impls for every variant.
/// impl ::core::convert::From<EthTransactInputPayloadV1> for VersionedEthTransactInputPayload {
///     fn from(payload: EthTransactInputPayloadV1) -> Self { /* Self::V1(Box::new(payload)) */ }
/// }
/// impl ::core::convert::TryFrom<VersionedEthTransactInputPayload> for EthTransactInputPayloadV1 {
///     type Error = ();
///     fn try_from(versioned: VersionedEthTransactInputPayload) -> Result<Self, ()> {
///         /* Ok(*v) on V1, Err(()) on every other variant */
///     }
/// }
/// // …same `From` and `TryFrom` for V2, plus the same trio of helpers, From, and TryFrom for the
/// // output side.
/// ```
///
/// # Naming convention
///
/// Every payload identifier must match `{Name}{Side}PayloadV{n}` exactly:
///
/// * `{Name}` — the *family name*. A non-empty identifier prefix that is identical for every
///   payload in the invocation. The family name may itself contain the letter `V`; the parser
///   locates the version suffix by splitting at the *last* `V` in the identifier, so
///   `EveVInputPayloadV1` is accepted and parses as the family `EveV` at version 1. If the family
///   name ends with `Versioned`, that marker is stripped from generated enum and latest-alias
///   names. For example, `EthTransactVersionedInputPayloadV1` still keeps its concrete struct name,
///   but generates `VersionedEthTransactInputPayload`.
/// * `{Side}` — exactly the literal `InputPayload` or `OutputPayload`. No other tokens are
///   permitted in this position.
/// * `V{n}` — the literal `V` followed by a positive decimal integer with no leading zeros. `V0` is
///   rejected (versions start at 1) and `V01`, `V001`, `V010` are rejected (leading zero). The
///   integer must be parseable as a `usize`.
///
/// Identifiers that fail to split into all three components are rejected with diagnostics that
/// point at the offending identifier:
///
/// * `EthTransactPayloadV1` — missing `Input`/`Output`.
/// * `InputPayloadV1` — empty family name.
/// * `EthTransactInputPayloadVNext` — non-numeric version.
/// * `EthTransactInputPayloadV1Extra` — extra suffix after the version.
/// * `EthTransactInputPayloadV` — `V` with no digit suffix.
///
/// # Versioning rules
///
/// Within one invocation:
///
/// * Every payload must use the same family name. Mixing `EthTransactInputPayloadV1` with
///   `EstimateGasOutputPayloadV1` is rejected and the diagnostic points at both spans.
/// * Every version must define both an input payload *and* an output payload. The diagnostic names
///   the missing struct (e.g. ``Expected `EthTransactOutputPayloadV1` to pair with
///   `EthTransactInputPayloadV1` ``) and accumulates one error per missing pair so a single compile
///   pass surfaces them all.
/// * Versions must be contiguous. If you ship `V1` and `V3`, the macro reports the missing `V2` and
///   points at the `V3` definition.
/// * Versions need *not* start at `V1`. A family can begin at `V3`, `V42`, or any positive integer
///   — only contiguity from the chosen starting point matters. This is useful when an interface is
///   grafted onto an older numbering scheme or extracted from a previous codebase.
/// * The same `(side, version)` pair cannot appear twice; a duplicate is rejected with both spans
///   pointed out.
/// * Source order is irrelevant. You can write V4 before V3, or interleave input and output structs
///   however you find readable; the generated enum variants are always emitted in ascending version
///   order.
/// * Only named-field structs are accepted. Tuple structs (`struct V1(u32);`), unit structs
///   (`struct V1;`), enums, type aliases, `const`s, `static`s, modules, `impl` blocks, functions,
///   and unions are rejected with a diagnostic that names the offending kind.
/// * The invocation must contribute at least one input and one output payload — empty input is
///   rejected.
///
/// # Generated items
///
/// For every invocation, the macro emits, in this order:
///
/// 1. **Each payload struct, verbatim.** Doc comments, `#[derive(...)]`, `#[cfg(...)]`,
///    `#[serde(...)]`, visibility, generic parameters, where-clauses, field attributes, and field
///    visibility are all preserved exactly as written. The macro never edits the body of a payload
///    struct.
/// 2. **`Versioned{Name}InputPayload`** — a `pub` enum with one `V{n}` variant per input payload
///    version. Each variant wraps the payload in `Box<…>` so that every variant is the same size
///    and the enum's footprint stays bounded as new versions are added.
/// 3. **`Versioned{Name}OutputPayload`** — same shape as the input enum, listed independently.
/// 4. **`Latest{Name}InputPayload`** and **`Latest{Name}OutputPayload`** — aliases to the
///    highest-numbered input and output payload structs. The aliases use the latest payload
///    struct's visibility and generic parameters, but omit generic bounds and where-clauses because
///    Rust accepts those on type aliases without enforcing them.
/// 5. For each enum, an inherent `impl` block exposing:
///    - `pub fn new_v{n}(payload: PayloadVn) -> Self` — builds the corresponding variant.
///    - `pub fn version(&self) -> usize` — returns the integer version of the held variant (`1`,
///      `2`, `3`, …).
///    - `pub fn as_v{n}(&self) -> Option<&PayloadVn>` — borrowing accessor; `None` if the contained
///      variant is a different version.
///    - `pub fn into_v{n}(self) -> Option<PayloadVn>` — consuming accessor; `None` if the contained
///      variant is a different version.
///    - `pub fn unwrap_v{n}(self) -> PayloadVn` — consuming accessor that panics with a message
///      identifying the actual version (`Expected this to be a v3 variant, but it is a v2 variant`)
///      on a mismatched variant.
/// 6. For each variant, an `impl ::core::convert::From<PayloadVn> for Versioned…Payload` that boxes
///    the payload into the matching variant.
/// 7. For each variant, an `impl ::core::convert::TryFrom<Versioned…Payload> for PayloadVn` with
///    `type Error = ()` that returns `Ok(payload)` on the matching variant and `Err(())` on every
///    other variant. The `match` lists every variant explicitly — there is no wildcard arm — so
///    single-variant enums compile cleanly without unreachable-pattern warnings.
///
/// All of these identifiers are predictable from the family name and the version numbers, so
/// downstream code can refer to them directly without inspecting the expansion.
///
/// # Why are variants boxed?
///
/// Each enum variant carries a `Box<PayloadVn>` rather than the payload directly. Without the box
/// the enum's size grows with every new version, since a Rust enum is laid out at the size of its
/// largest variant. Boxing keeps the enum at a single pointer's width regardless of how the
/// payloads evolve, which matters when many of these are stored in collections, encoded for the
/// wire, or held in long-lived state.
///
/// The cost is a single heap allocation on construction, which is negligible compared to the
/// runtime API call the value is feeding into.
///
/// The generated code uses `::alloc::boxed::Box`. Consuming crates must make `alloc` reachable,
/// usually with `extern crate alloc;` in the crate root.
///
/// # Generics across versions
///
/// Each payload struct may declare its own generic parameters and where-clauses. The generated
/// versioned enum carries the *union* of the parameters and the *union* of the bounds on each side,
/// computed independently per side:
///
/// * Lifetime, type, and const parameters with the same name across versions are merged into a
///   single declaration on the enum.
/// * Inline bounds on a shared name are concatenated. If `V1` declares `T: Clone` and `V2` declares
///   `T: Default`, the enum and every conversion impl on that side carry `T: Clone + Default`.
/// * `where`-clause predicates from every payload on a side are concatenated. They are not
///   deduplicated — if two payloads both declare `where T: Clone`, the enum's `where` clause
///   contains both predicates. This compiles correctly and is rarely user-visible, but it is a
///   detail worth knowing if you ever read the expansion.
/// * Same-name parameters of *different kinds* (e.g. `T` is a type parameter in `V1` but a const
///   parameter in `V2`) are rejected with a diagnostic that points at both definitions.
/// * Same-name const parameters with *different types* (`const N: usize` vs. `const N: u32`) are
///   rejected with a diagnostic that points at both definitions.
///
/// The merge happens per side. The input enum sees only the input payloads' generics; the output
/// enum sees only the output payloads' generics. A type parameter that appears only on the input
/// side does not bleed into the output enum.
///
/// Because the enum carries the union, every conversion impl uses the *enum's* generic signature —
/// even when the payload alone needs fewer bounds. A `From<PayloadV1>` impl is callable only when
/// the *enum-level* bounds are satisfied, not just `V1`'s narrower bounds. In practice this is a
/// non-issue because constructing the enum already requires the union, but it explains why later
/// versions' bounds appear together with `V1`'s at the conversion site.
///
/// # Derive forwarding
///
/// `#[derive(...)]` attributes on payload structs propagate to the generated enum on a per-side,
/// *intersection* basis:
///
/// * The macro inspects every `#[derive(...)]` attribute on every payload on a side.
/// * It computes the set of derive paths that appear on *every* payload on that side, in the source
///   order they appear on the first payload.
/// * That set is emitted as a single `#[derive(...)]` on the corresponding enum.
///
/// If `EthTransactInputPayloadV1` derives `Clone, Debug` and `EthTransactInputPayloadV2` derives
/// only `Clone`, the input enum derives `Clone` (the intersection). The output side is computed
/// independently — it does not see the input side's derives. Non-derive attributes (`#[doc = "…"]`,
/// `#[cfg(...)]`, `#[serde(...)]`, `#[encode(...)]`, …) are *not* propagated; they remain on each
/// payload struct only.
///
/// Multiple `#[derive(...)]` attributes on a single payload are flattened together. Malformed
/// derive arguments inside a payload produce a diagnostic that names the offending payload.
///
/// # Examples
///
/// ## A two-version interface, no generics
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactInputPayloadV1 {
///         pub tx: GenericTransaction,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactOutputPayloadV1 {
///         pub result: EthTransactInfo,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactInputPayloadV2 {
///         pub tx: GenericTransaction,
///         pub config: DryRunConfig,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Encode, Decode)]
///     pub struct EthTransactOutputPayloadV2 {
///         pub result: EthTransactInfo,
///     }
/// }
///
/// // Build a versioned input from a v2 payload using `From`:
/// let payload = EthTransactInputPayloadV2 { tx: some_tx, config: some_config };
/// let versioned: VersionedEthTransactInputPayload = payload.into();
///
/// // Inspect its version, then borrow the inner data without consuming the enum:
/// assert_eq!(versioned.version(), 2);
/// let v2_ref: Option<&EthTransactInputPayloadV2> = versioned.as_v2();
///
/// // Convert back to a concrete version, returning Err(()) on any other variant:
/// let v1: Result<EthTransactInputPayloadV1, ()> = versioned.try_into();
/// assert_eq!(v1, Err(()));
/// ```
///
/// ## A single version, starting after V1
///
/// Versions need not start at 1 — a family that begins at V7 is valid. Such an invocation generates
/// exactly one `V7` variant and the matching helper trio, plus the `From`/`TryFrom` pair. The
/// `TryFrom` impl emits an exhaustive `match` with no wildcard arm, so it compiles without
/// unreachable-pattern warnings.
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct SingleInputPayloadV7 {
///         pub value: u8,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct SingleOutputPayloadV7 {
///         pub value: u8,
///     }
/// }
///
/// let v7 = SingleInputPayloadV7 { value: 17 };
/// let versioned = VersionedSingleInputPayload::from(v7.clone());
/// assert_eq!(SingleInputPayloadV7::try_from(versioned), Ok(v7));
/// ```
///
/// ## Out-of-order definitions
///
/// The macro does not require source order to follow version order. The generated enum variants are
/// always emitted in ascending version order regardless of how the structs are written, and input
/// and output payloads can be freely interleaved.
///
/// ```ignore
/// define_versioned_interface! {
///     // V4 first, V3 second — the generated enum has V3 then V4 anyway.
///     pub struct TransferInputPayloadV4 {
///         pub account: u64,
///         pub amount: u128,
///         pub memo: Option<&'static str>,
///     }
///
///     pub struct TransferOutputPayloadV4 {
///         pub accepted: bool,
///         pub receipt: Option<u64>,
///     }
///
///     pub struct TransferInputPayloadV3 {
///         pub account: u64,
///         pub amount: u128,
///     }
///
///     pub struct TransferOutputPayloadV3 {
///         pub accepted: bool,
///     }
/// }
/// ```
///
/// ## Generic payloads with merged inline bounds
///
/// Per-version inline bounds are unioned per side. Below, `T: Clone` from V1 and `T: Default` from
/// V2 merge into `T: Clone + Default` on the input enum; the output side is non-generic because
/// none of its payloads declare any generics.
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct EthTransactInputPayloadV1<T: Clone> {
///         pub tx: u8,
///         pub marker: T,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct EthTransactOutputPayloadV1 {
///         pub result: u8,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct EthTransactInputPayloadV2<T: Default>
///     where
///         T: Clone,
///     {
///         pub tx: u8,
///         pub marker: T,
///         pub timestamp: u64,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct EthTransactOutputPayloadV2 {
///         pub result: u16,
///     }
/// }
/// ```
///
/// expands the input enum to (approximately):
///
/// ```ignore
/// #[derive(Clone, Debug, PartialEq)]
/// pub enum VersionedEthTransactInputPayload<T: Clone + Default>
/// where
///     T: Clone,
/// {
///     V1(::alloc::boxed::Box<EthTransactInputPayloadV1<T>>),
///     V2(::alloc::boxed::Box<EthTransactInputPayloadV2<T>>),
/// }
/// ```
///
/// (The `where T: Clone` predicate is preserved verbatim from V2's payload — the macro unions
/// where-clauses without deduplication.)
///
/// ## Lifetime and const generics
///
/// Lifetime parameters and const parameters merge the same way as type parameters. The example
/// below mixes a borrowed key, a const generic for an array length, and an output side whose own
/// generics differ from the input side's.
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct QueryInputPayloadV1<'a, T: Clone>
///     where
///         T: PartialEq,
///     {
///         pub key: &'a T,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct QueryOutputPayloadV1<R>
///     where
///         R: Clone + PartialEq,
///     {
///         pub value: Option<R>,
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct QueryInputPayloadV2<'a, T: Default, const N: usize>
///     where
///         T: Clone + PartialEq,
///     {
///         pub key: T,
///         pub borrowed: Option<&'a T>,
///         pub bytes: [u8; N],
///     }
///
///     #[derive(Clone, Debug, PartialEq)]
///     pub struct QueryOutputPayloadV2<R, E>
///     where
///         R: Clone + PartialEq,
///         E: Clone + PartialEq,
///     {
///         pub value: Option<R>,
///         pub error: Option<E>,
///     }
/// }
/// ```
///
/// The input enum becomes `VersionedQueryInputPayload<'a, T: Clone + Default + PartialEq, const N:
/// usize>` and the output enum becomes `VersionedQueryOutputPayload<R, E>`, each carrying the
/// merged where-clauses from its own side.
///
/// ## Asymmetric derives between input and output
///
/// Input and output payloads do not need to derive the same set of traits. The macro intersects the
/// derives on each side independently — an enum receives only the derives shared by every payload
/// on its side.
///
/// ```ignore
/// define_versioned_interface! {
///     #[derive(Clone, Debug, PartialEq, Eq)]
///     pub struct AuditInputPayloadV1 {
///         pub id: u64,
///     }
///
///     // The output payloads do not derive `Clone`. The output enum will not derive `Clone`
///     // either, but the input enum still derives `Clone` because both inputs derive it.
///     #[derive(Debug, PartialEq, Eq)]
///     pub struct AuditOutputPayloadV1 {
///         pub ok: bool,
///     }
///
///     #[derive(Clone, Debug, PartialEq, Eq)]
///     pub struct AuditInputPayloadV2 {
///         pub id: u64,
///         pub tag: &'static str,
///     }
///
///     #[derive(Debug, PartialEq, Eq)]
///     pub struct AuditOutputPayloadV2 {
///         pub ok: bool,
///         pub code: u16,
///     }
/// }
///
/// // Input enum derives:  Clone, Debug, PartialEq, Eq
/// // Output enum derives: Debug, PartialEq, Eq
/// ```
///
/// ## Visibility, doc comments, and other attributes are preserved
///
/// Each payload struct keeps the visibility, doc comments, and any unrelated attributes it was
/// written with. The generated enum is always `pub`; the macro does not propagate `#[cfg(...)]`,
/// `#[doc(...)]`, or any other attribute from the payload structs to the enum. If you need to gate
/// the entire interface, wrap the invocation in a private module or apply the attribute at the use
/// site.
///
/// ```ignore
/// define_versioned_interface! {
///     /// The first version of the request shape. Carries the raw transaction.
///     #[derive(Clone, Debug, PartialEq)]
///     #[cfg(feature = "rpc")]
///     pub(crate) struct EthTransactInputPayloadV1 {
///         /// The encoded transaction submitted by the client.
///         pub tx: Vec<u8>,
///     }
///
///     /// The first version of the response shape.
///     #[derive(Clone, Debug, PartialEq)]
///     #[cfg(feature = "rpc")]
///     pub(crate) struct EthTransactOutputPayloadV1 {
///         pub result: u32,
///     }
/// }
/// ```
///
/// The doc comments, the `#[cfg(...)]`, and the `pub(crate)` are preserved on each emitted payload
/// struct; the generated `VersionedEthTransactInputPayload` and `VersionedEthTransactOutputPayload`
/// enums are emitted as plain `pub` items without those attributes.
///
/// ## Constructing, inspecting, and round-tripping a value
///
/// ```ignore
/// // Two equivalent ways to build a versioned value: an explicit constructor and `From`.
/// let payload = EthTransactInputPayloadV2 { tx: some_tx, config: some_config };
/// let v_via_new = VersionedEthTransactInputPayload::new_v2(payload.clone());
/// let v_via_from = VersionedEthTransactInputPayload::from(payload.clone());
/// assert_eq!(v_via_new, v_via_from);
///
/// // Inspect the version without consuming the value:
/// assert_eq!(v_via_new.version(), 2);
///
/// // Borrow the inner payload, then later consume the enum to recover ownership:
/// assert!(v_via_new.as_v1().is_none());
/// assert!(v_via_new.as_v2().is_some());
/// let inner: Option<EthTransactInputPayloadV2> = v_via_new.into_v2();
/// assert_eq!(inner, Some(payload));
///
/// // The `unwrap_*` family panics with the actual version on a mismatch:
/// let v2 = VersionedEthTransactInputPayload::new_v2(other_payload);
/// // v2.unwrap_v1() panics with: "Expected this to be a v1 variant, but it is a v2 variant".
/// ```
///
/// ## Round-tripping through `From` and `TryFrom`
///
/// ```ignore
/// // `From` always succeeds — the conversion target is a single variant.
/// let v2 = EthTransactInputPayloadV2 { tx: some_tx, config: some_config };
/// let versioned: VersionedEthTransactInputPayload = v2.clone().into();
///
/// // `TryFrom` succeeds on a matching variant…
/// let extracted: EthTransactInputPayloadV2 =
///     EthTransactInputPayloadV2::try_from(versioned).unwrap();
/// assert_eq!(extracted, v2);
///
/// // …and returns `Err(())` on every other variant.
/// let v1 = VersionedEthTransactInputPayload::new_v1(EthTransactInputPayloadV1 { tx: some_tx });
/// let attempted = EthTransactInputPayloadV2::try_from(v1);
/// assert_eq!(attempted, Err(()));
/// ```
///
/// # Diagnostics
///
/// Every error reported by the macro is a compile error with spans pointing at the offending
/// source. The diagnostics fall into a small number of categories:
///
/// * **Naming.** Missing or empty `V`-suffix; non-numeric version (`VNext`); leading-zero version
///   (`V01`); zero version (`V0`); missing `Input`/`Output` (`EthTransactPayloadV1`); empty family
///   name (`InputPayloadV1`); extra suffix after the version (`EthTransactInputPayloadV1Extra`).
/// * **Item shape.** Tuple struct, unit struct, enum, function, module, `impl`, type alias,
///   `const`, `static`, or `union` items. The diagnostic names the offending kind so the correction
///   is clear.
/// * **Family-level pairing.** Different family names mixed in one invocation; missing input or
///   missing output payload for a version (one diagnostic per missing pair, accumulated so a single
///   compile pass surfaces them all); duplicate `(side, version)` pair; non-contiguous versions;
///   empty input.
/// * **Generic merging.** A name used as a different kind across versions (lifetime vs. type vs.
///   const); a const parameter declared with two different concrete types (`const N: usize` vs.
///   `const N: u32`).
/// * **Derive parsing.** Malformed derive arguments inside a payload; the macro surfaces the
///   underlying parse error along with a note identifying the payload it failed on.
///
/// Each diagnostic identifies both the offending site and any earlier site it conflicts with, so
/// the error is actionable without having to scroll back through the source.
///
/// # Gotchas
///
/// * **Naming is non-negotiable.** Even small typos (`EthTransactInputV1` instead of
///   `EthTransactInputPayloadV1`) are rejected. The generated enum uses the family name verbatim,
///   so the family name is also user-visible — choose it as you would a public type.
/// * **Variants are heap-allocated.** The macro forces every variant through `Box`. This is
///   intentional but worth noting: avoid the macro for hot paths where the allocation cost matters
///   (it is fine for the runtime API boundary it was designed for).
/// * **`alloc` must be reachable in no-std consumers.** Disabling default features on
///   `pallet-revive-proc-macro` makes the expansion use `::alloc::boxed::Box`, so a no-std crate
///   without `extern crate alloc;` will fail to compile.
/// * **Bounds on the enum are the union.** A user calling `VersionedX::from(v1_payload)` has to
///   satisfy the *enum's* bounds, which include any later versions' bounds, not just the bounds on
///   V1. This is unavoidable because the value being constructed is the enum, but it can surprise
///   readers who only consult the V1 payload's bounds.
/// * **`From` and `TryFrom` are emitted unconditionally.** They are not gated on whether the
///   payload struct itself implements anything; the impls only require the enum's bounds.
/// * **Enum visibility is fixed at `pub`.** The macro does not let you scope the generated enum to
///   `pub(crate)` or `pub(super)`. If you need a non-public enum, place the entire invocation
///   inside a private module.
/// * **Trait impls always pick the merged generics.** The `From`/`TryFrom` impls use the enum's
///   full generic signature even when the payload alone has fewer parameters. Calls that cannot
///   infer the missing parameters (e.g. an output-side type parameter that only appears in V2)
///   require explicit turbofish at the call site.
/// * **Latest aliases intentionally omit bounds.** Bounds and where-clauses remain on the payload
///   structs that enforce them. Emitting them again on the alias would only trigger Rust's
///   `type_alias_bounds` warning because those bounds are not checked at the alias site.
/// * **Non-derive attributes are not propagated to the enum.** `#[doc(...)]`, `#[cfg(...)]`,
///   `#[serde(...)]`, and other attributes stay on the payload structs. If you need them on the
///   enum, write them on the wrapping module that contains the invocation.
#[proc_macro]
pub fn define_versioned_interface(input: TokenStream) -> TokenStream {
	let input =
		syn::parse_macro_input!(input as define_versioned_interface::DefineVersionedInterfaceInput);
	let output = match define_versioned_interface::handle_define_versioned_interface(input) {
		Ok(output) => output,
		Err(error) => return error.to_compile_error().into(),
	};

	output.into()
}
