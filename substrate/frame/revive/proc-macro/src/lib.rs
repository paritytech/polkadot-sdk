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
/// # Item shape and versioning rules
///
/// Each item must be a `struct` or `enum` — no other Rust items are accepted — and each identifier
/// must end with `V` followed by a positive integer (the version number).
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
/// * The same `(side, version)` pair cannot appear twice; a duplicate is rejected with both spans
///   pointed out.
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
