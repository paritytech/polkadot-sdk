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

//! Wire types and versioned runtime API payloads shared by `pallet-revive` and its off-chain
//! clients.
//!
//! # Overview
//!
//! This crate hosts the stable client-facing types and the versioned input/output payload pairs
//! that flow across `pallet-revive`'s runtime API boundary. Off-chain clients depend on this crate
//! alone to build payloads, decode responses, and reason about which versions are supported by a
//! given runtime; the pallet itself depends on it to declare the wire format and to convert each
//! version into the internal execution types that drive its logic.
//!
//! The wire types defined here are the *only* shape that ever crosses the runtime API boundary as
//! SCALE-encoded bytes. Every type in this crate is therefore subject to a hard
//! release-immutability rule: once a wire type has shipped as part of a published runtime, its
//! on-wire encoding is frozen forever. The complementary execution types live inside
//! `pallet-revive` proper, are deliberately not SCALE-encodable, and are freely refactorable as the
//! runtime's internal needs evolve.
//!
//! # Compatibility Guarantees
//!
//! The wire types in this crate carry a hard contract. Once a payload version has been released,
//! its SCALE encoding is fixed for the lifetime of the codebase. From that moment on:
//!
//! * *Backwards compatibility* — a client written against version `N` of any runtime API function
//!   continues to work when the runtime is upgraded to support newer versions, without any changes
//!   to the client. The client keeps sending its `N`-shaped payload and the runtime keeps
//!   recognising it.
//! * *Forward compatibility* — a client written against version `N` can talk to an older runtime
//!   that only understands earlier versions. The client queries the runtime's discovery surface,
//!   picks the highest mutually supported version (which may be older than `N`), and proceeds
//!   against that version.
//! * *Typed mismatch errors* — a request that names a payload version the runtime does not
//!   understand fails with a structured, named error variant rather than producing silent decode
//!   corruption or an opaque decode failure. There is no scenario in which the wire boundary
//!   returns garbled data.
//! * *Metadata introspection* — the runtime metadata exposes enough information for a client to
//!   discover, at runtime, every function and every payload version that the current runtime
//!   supports, without probing for the presence of specific function names or relying on
//!   out-of-band knowledge.
//!
//! These guarantees apply equally to *signature evolution* (a function gaining or losing an
//! argument) and to *payload evolution* (a struct or enum that crosses the boundary growing,
//! shrinking, or retyping a field). Both are expressed by adding a new payload version, never by
//! mutating an existing one.
//!
//! # Data Model: Versioned Payloads
//!
//! Each runtime API function in `pallet-revive` accepts exactly one input payload value and returns
//! exactly one output payload value. Both payloads are versioned enums whose variants are concrete,
//! version-specific structs — one variant per published version. A client speaks to the runtime by:
//!
//! 1. Constructing the input payload at the highest version it understands and that the runtime
//!    advertises support for.
//! 2. Calling the corresponding versioned runtime API function (e.g. `eth_transact_versioned`).
//! 3. Receiving an output payload guaranteed to be at the same version as the input it sent.
//!
//! This input/output version pairing is contractual: a `V1` input always returns `V1` output, a
//! `V2` input always returns `V2` output, and so on. There is no scenario in which a client needs
//! to inspect the version of the response it received — the version it gets back is always the
//! version it asked for.
//!
//! The arguments that each version carries are *not* individually versioned. Versioning lives one
//! level up, on the input or output payload as a whole. Adding a new field, removing a field,
//! retyping a field, or adding a new argument to a function is therefore handled as a new payload
//! variant — not as a recursive cascade of new wire types and not as a new sibling runtime API
//! function. The outer `(input, output)` signature of the runtime API function is frozen the day it
//! ships; everything that evolves lives inside the enums.
//!
//! # Wire Types vs. Execution Types
//!
//! The contents of this crate are *wire types* only. They cross the runtime API boundary as
//! SCALE-encoded bytes and exist purely to define the stable contract clients depend on. They never
//! appear inside any internal execution function.
//!
//! The complementary *execution types* live inside `pallet-revive` proper. They are deliberately
//! not SCALE-encodable so that they cannot accidentally cross the boundary. They exist to give the
//! runtime a refactor-friendly representation that can evolve freely as internal needs change.
//!
//! Each runtime API function dispatches by:
//!
//! 1. Decoding a versioned wire input payload from the request bytes.
//! 2. Converting the wire payload into the appropriate execution type for the chosen version.
//! 3. Calling the shared execution function on the resulting execution input.
//! 4. Converting the execution output back into the matching versioned wire output payload variant.
//!
//! This separation is what makes both layers stable in their own way. The wire layer is frozen once
//! shipped, ensuring clients are never broken by an upgrade. The execution layer is freely
//! refactorable, ensuring the runtime is never trapped in a shape chosen long ago. Field renames,
//! internal restructuring, performance improvements, and correctness fixes that touch only the
//! execution side are ordinary refactors with no wire-format consequence.
//!
//! It is this separation that justifies the existence of this crate as its own package: wire types
//! live here precisely so clients can depend on them without pulling in `pallet-revive`'s execution
//! internals, and so the boundary between "stable, frozen forever" and "freely refactorable" is
//! enforced by the compilation graph rather than by convention.
//!
//! # Provided Macros
//!
//! The wire types in this crate are declared with two procedural macros from
//! `pallet-revive-proc-macro`:
//!
//! * [`define_versioned_type!`] declares a family of versioned wire structs or enums (e.g.
//!   `CallLogV1`, `CallLogV2`, ...) where each successor expresses only its delta from the previous
//!   version. The macro emits independent, standalone Rust types for every version and a
//!   `Latest{Name}` alias pointing at the highest version. Refer to the macro's own documentation
//!   for the exact syntax.
//! * [`define_versioned_interface!`] declares a paired family of input and output payload structs
//!   for one runtime API function, plus the `Versioned{Name}InputPayload` and
//!   `Versioned{Name}OutputPayload` enums that route every published version through it, and the
//!   helper accessors, `From`, and `TryFrom` impls that connect each version to its enclosing enum.
//!   Refer to the macro's own documentation for the exact syntax.
//!
//! These two macros are the only mechanism this crate uses to declare wire types. New types should
//! reuse them rather than hand-write the version variants and conversion impls — the macros exist
//! to keep the wire-format invariants enforceable in one place.
//!
//! [`define_versioned_type!`]: pallet_revive_proc_macro::define_versioned_type
//! [`define_versioned_interface!`]: pallet_revive_proc_macro::define_versioned_interface
//!
//! # Versioning Policy
//!
//! This section is the policy a contributor must follow when changing anything in this crate. Every
//! rule here exists to preserve the guarantees in the *Compatibility Guarantees* section above.
//! Read every rule before editing any wire type or interface; the rules look small individually,
//! but each one was paid for in past breakage and ignoring any one of them risks the whole chain.
//!
//! ## Core Rule
//!
//! Each logical runtime API capability has exactly one maintained versioned runtime API function.
//! The outer signature of that function — `Versioned{Name}InputPayload ->
//! Result<Versioned{Name}OutputPayload, _>` — is stable after release. Ordinary evolution happens
//! by adding payload versions, *not* by changing that outer signature, *not* by adding `_v2` or
//! `_with_config` sibling functions, and *not* by using `#[changed_in(N)]`.
//!
//! The version chosen by the client is the version of the contract being requested. If a client
//! sends `V1` input, the runtime returns `V1` output. If a client wants fields or behavior
//! introduced in `V2`, it sends `V2` input and receives `V2` output. The input and output versions
//! advance as a pair, even when only one side of the shape actually changed.
//!
//! ## Classify the Change First
//!
//! Before implementing any change, decide what kind of change it is. The category drives whether a
//! new payload version is required.
//!
//! * *Internal refactor* — the change does not alter any runtime API argument, return value, wire
//!   type, or intended observable behavior. No new payload version is needed. The simplest test is
//!   whether the types being changed implement `Encode`, `Decode`, or `TypeInfo`. If any of them
//!   do, the change is observable and must be treated as a wire change.
//! * *Bug fix or security fix* — the change restores the intended behavior of an existing version
//!   rather than introducing new behavior. No new payload version is needed; the fix applies to
//!   every released version equally.
//! * *New observable behavior* — the change introduces new intended behavior, new data, a new
//!   option, or a new interpretation that clients can rely on. Add a payload version.
//! * *Brand new capability* — the change adds a new logical capability rather than evolving an
//!   existing capability. Add a new versioned runtime API function starting at `V1`. Do not add a
//!   new unversioned runtime API function as the maintained API surface.
//!
//! ## When Inputs Change
//!
//! If a function needs a new argument, no longer needs an argument, or needs to reinterpret an
//! argument, add the next input payload version for that function. Keep all existing input payload
//! variants unchanged.
//!
//! Add the matching output payload version at the same number. If the returned data did not change,
//! the new output version may have the same fields as the previous output version, but it still
//! exists as a separate variant so that the request and response contract remains version-aligned.
//!
//! ## When Outputs Change
//!
//! If a function needs to return additional data, remove returned data, retype returned data, or
//! change the shape of a returned struct or enum, add the next output payload version for that
//! function. Keep all existing output payload variants unchanged.
//!
//! Add the matching input payload version at the same number. If the caller does not need to pass
//! anything new, the new input version may have the same fields as the previous input version. It
//! still matters because it is how the client asks for the newer response contract.
//!
//! ## When Behavior Changes
//!
//! If the same input payload would now produce meaningfully different behavior that a client could
//! observe and depend on, add the next payload version. The old version keeps the old semantics;
//! the new version gets the new semantics.
//!
//! Do not add a new version for implementation refactors, performance improvements, or correctness
//! fixes that preserve the intended contract. A fix that changes behavior is allowed without a new
//! version only when the old behavior was wrong, unsafe, or impossible to treat as a supported
//! contract.
//!
//! ## When Wire Types Change
//!
//! Wire payload types are the stable client-facing contract. Once a payload version is released, do
//! not add fields to it, remove fields from it, reorder fields, change field types, reuse a field
//! for a different meaning, or change enum variant meanings. This applies recursively: any type
//! that participates in a released payload, however deeply nested, is itself release-immutable in
//! that payload's variant.
//!
//! Released versions are immutable; unreleased ones are not. Any change to a wire format that has
//! not yet been included in a published runtime is fair game and should be made directly rather
//! than by stacking another version on top of an unreleased one.
//!
//! To evolve a released wire type, define a new payload version and supply a conversion path from
//! it into the current execution types. The execution types may change freely, but every supported
//! wire version must keep a conversion path that preserves that version's contract.
//!
//! ## Version Discovery
//!
//! `PalletReviveRuntimeApiPayloadVersions` is the runtime's source of truth for the versioned API
//! surface. If a function is missing from the discovery map, the client treats the function as
//! unavailable. If the map says a function supports version `N`, the client may call any version
//! from `V1` through `VN`.
//!
//! Clients should pick the newest version that both sides understand, unless they intentionally
//! choose an older version because it already gives them the fields and behavior they need. A
//! client must not infer support from function names, failed calls, or the trait API version once
//! the versioned API surface exists.
//!
//! ## Runtime Dispatch
//!
//! The runtime dispatches on the input payload variant. Each branch converts that wire payload into
//! the execution types, calls the shared execution logic, and converts the result back into the
//! output payload variant carrying the same version number that the input carried.
//!
//! Do not duplicate business logic per payload version unless the behavior really differs by
//! contract. Prefer one execution implementation with explicit conversion layers around it.
//!
//! ## Adding New Versions to Discovery
//!
//! When a new payload version is added, update `PalletReviveRuntimeApiPayloadVersions` for that
//! function to the new latest version. The discovery value is only the latest version, so supported
//! versions must remain contiguous: a runtime advertising `V3` for a function is asserting that
//! `V1`, `V2`, and `V3` are all supported.
//!
//! ## Use of Runtime API Versions
//!
//! Trait-level runtime API versioning is used only to introduce the versioned API surface itself.
//! After that point, ordinary `pallet-revive` API evolution is expressed through payload versions.
//! Do not bump `api_version` for ordinary signature or payload evolution, and do not use
//! `#[changed_in]` to evolve `pallet-revive` runtime API signatures.
//!
//! ## Implementation Checklist
//!
//! For every new payload version, the following steps must be completed in the same change:
//!
//! 1. Define the new input and output wire variants in this crate, using
//!    `define_versioned_interface!` for top-level payloads and `define_versioned_type!` for any new
//!    wire types they reference.
//! 2. Add or update the conversion code between the new wire variant and the execution types in
//!    `pallet-revive`.
//! 3. Update the runtime dispatch for the function so it handles the new variant.
//! 4. Update `PalletReviveRuntimeApiPayloadVersions` for the function to advertise the new latest
//!    version.
//! 5. Add client-side unwrapping or construction helpers if they exist for earlier versions.
//! 6. Add tests covering both old-client-to-new-runtime and new-client-to-old-runtime behaviour.
//!    The tests must prove that an old payload still works on the new runtime, that the new payload
//!    is discoverable, that a client can choose the highest mutually supported version, and that
//!    the runtime returns the output version matching the input version.
//!
//! # Worked Examples
//!
//! ## Adding a Field to a Trace Output
//!
//! A new field needs to appear on the output of `trace_tx_versioned`. Do not edit the released `V1`
//! trace output type. Add `V2` input and output payloads for `trace_tx_versioned`. The `V2` input
//! may have the same fields as `V1` if the caller does not need to pass anything new. The `V2`
//! output contains the new trace shape with the new field.
//!
//! A `V1` caller continues to send `V1` input and receives the old `V1` output shape unchanged. A
//! client that wants the new field checks discovery, sees that `trace_tx_versioned` supports at
//! least `V2`, sends `V2` input, and receives `V2` output.
//!
//! ## Adding an Input Option
//!
//! A new dry-run option needs to be accepted by `eth_transact_versioned`. Do not add
//! `eth_transact_with_config`, do not add `eth_transact_v2`, and do not change the fields of the
//! released `V1` input. Add `V2` input and output payloads. The new option goes in the `V2` input.
//! The `V2` output may match the `V1` output shape if the returned data is unchanged.
//!
//! A client that does not need the new option can keep sending `V1` input. A client that needs the
//! new option checks discovery, sends `V2` input, and receives a `V2` response.
//!
//! # Note
//!
//! The owner and core consumer of this crate is the runtime API. This is very important for us to
//! define in order to make it clear when this crate evolves and when it doesn't. If other consumers
//! of this crate (e.g., the eth-rpc) end up needing to make changes or update this crate in any way
//! which doesn't directly involve the runtime API then the changes will be rejected. Models have a
//! serde implementation purely for convince, not because we're building this crate to be have a
//! canonical serde implementation.

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate alloc;

pub mod interfaces;
pub mod types;

pub use interfaces::*;
pub use types::*;
