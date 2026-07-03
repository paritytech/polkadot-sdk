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
//! Utility impl for the RPC types.
use super::*;
use codec::{Decode, Encode};
use scale_info::TypeInfo;

/// Configuration specific to a dry-run execution.
///
/// Passed as an argument to the `eth_transact_with_config` runtime API method. Contains optional
/// overrides that control how the dry-run is executed, such as timestamp simulation and state
/// injection.
///
/// # Backwards Compatibility
///
/// This type is SCALE-encoded when passed across the runtime API boundary via `state_call`.
/// SCALE is a non-self-describing format: fields are encoded sequentially with no field names,
/// delimiters, or end-of-message markers. This has important implications when the struct evolves
/// over time.
///
/// ## Adding new trailing fields
///
/// New fields may be appended to the end of this struct without requiring a new runtime API
/// method, provided that:
///
/// 1. **New RPC, old runtime (trailing bytes are ignored):** The `sp_api` runtime API argument
///    decoding machinery uses `Decode::decode` (not `decode_all`) for parameterized calls. This
///    means bytes remaining after decoding all known fields are silently ignored. An old runtime
///    that does not know about a newly appended field will decode the fields it recognizes and
///    discard the rest. This is intentional behavior in `sp_api` — see the generated code in
///    `substrate/primitives/api/proc-macro/src/impl_runtime_apis.rs`.
///
/// 2. **Old RPC, new runtime (missing bytes are defaulted):** A new runtime expecting more fields
///    than an old RPC provides would hit EOF during decoding and fail. To guard against this, this
///    type uses a **custom `Decode` implementation** that falls back to `Default` for any trailing
///    fields that are absent from the input. This ensures that an old RPC sending a shorter
///    encoding is handled gracefully.
///
/// ## Constraints on fields
///
/// - New fields **must** be appended to the end. Inserting or reordering fields changes the byte
///   layout of all subsequent fields, breaking both directions.
/// - New fields **must** implement `Default` so that the custom `Decode` fallback can produce a
///   sensible value when the field is absent from the input. This is the only requirement on the
///   field's type — it does not need to be `Option`.
/// - This pattern relies on `sp_api` continuing to use `Decode::decode` rather than `decode_all`.
///   If that ever changes, a new runtime API method would be needed instead.
///
/// ## Constraints on runtime API placement
///
/// The trailing-bytes trick described in point 1 above only works because `sp_api` discards
/// unconsumed bytes **at the end of the entire argument buffer**. This means `DryRunConfig`
/// must be the **last argument** of any runtime API method that uses it (which is currently
/// the case for both `eth_transact_with_config` and `eth_estimate_gas`). If it were placed
/// before another argument, the extra bytes from newly appended fields would shift the
/// decoding offset and corrupt the subsequent argument.
#[derive(Debug, Encode, TypeInfo, Clone)]
pub struct DryRunConfig<Moment> {
	/// Optional timestamp override for dry-run in pending block.
	pub timestamp_override: Option<Moment>,
	/// Used to control if the dry run logic should perform the balance checks or not.
	pub perform_balance_checks: Option<bool>,
	/// Optional state overrides to apply before executing the call. Each entry maps an account
	/// address to a set of fields (balance, nonce, code, storage) that should be temporarily
	/// replaced for the duration of the dry-run.
	pub state_overrides: Option<StateOverrideSet>,
}

impl<Moment> Default for DryRunConfig<Moment> {
	fn default() -> Self {
		Self { timestamp_override: None, perform_balance_checks: Some(true), state_overrides: None }
	}
}

/// A custom implementation of [`Decode`] to ensure forward and backward compatibility of the
/// [`DryRunConfig`] type.
///
/// # Backwards Compatibility
///
/// Please review the documentation on the [`DryRunConfig`] for more information about how we
/// manage and handle compatibility for this type and instructions on what you should do when adding
/// a new field to this type.
impl<Moment: Decode> Decode for DryRunConfig<Moment> {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let timestamp_override = Option::<Moment>::decode(input)?;
		let perform_balance_checks = Option::<bool>::decode(input)?;
		let state_overrides = Option::<StateOverrideSet>::decode(input).unwrap_or_default();
		Ok(Self { timestamp_override, perform_balance_checks, state_overrides })
	}
}

impl<Moment> DryRunConfig<Moment> {
	/// Create a new `DryRunConfig` with default values.
	///
	/// Balance checks are enabled by default. Use the builder methods to customize.
	pub fn new() -> Self {
		Self::default()
	}

	/// A builder method which consumes the object and modifies the `timestamp_override` field.
	pub fn with_timestamp_override(
		mut self,
		timestamp_override: impl Into<Option<Moment>>,
	) -> Self {
		self.timestamp_override = timestamp_override.into();
		self
	}

	/// A builder method which consumes the object and modifies the `perform_balance_checks` field.
	pub fn with_perform_balance_checks(
		mut self,
		perform_balance_checks: impl Into<Option<bool>>,
	) -> Self {
		self.perform_balance_checks = perform_balance_checks.into();
		self
	}

	/// A builder method which consumes the object and sets the state overrides.
	pub fn with_state_overrides(
		mut self,
		state_overrides: impl Into<Option<StateOverrideSet>>,
	) -> Self {
		self.state_overrides = state_overrides.into();
		self
	}
}

/// Configuration specific to a tracing execution.
///
/// Passed as the last argument to the `trace_call_with_config` runtime API method. Contains
/// optional overrides that affect how the traced execution is performed.
///
/// # Backwards Compatibility
///
/// This type follows the same backwards compatibility strategy as [`DryRunConfig`]. SCALE is a
/// non-self-describing format: fields are encoded sequentially with no names or delimiters. This
/// type uses a custom [`Decode`] implementation that defaults missing trailing fields, and relies
/// on `sp_api`'s use of `Decode::decode` (not `decode_all`) to silently discard trailing bytes
/// that an old runtime does not recognize.
///
/// ## Constraints on fields
///
/// - New fields **must** be appended to the end. Inserting or reordering fields breaks the byte
///   layout in both directions.
/// - New fields **must** implement `Default` so the custom `Decode` fallback can produce a sensible
///   value when the field is absent from the input.
///
/// ## Constraints on runtime API placement
///
/// `TracingConfig` must be the **last argument** of any runtime API method that uses it. If it
/// were placed before another argument, extra bytes from newly appended fields would shift the
/// decoding offset and corrupt the subsequent argument.
#[derive(Debug, Default, Encode, TypeInfo, Clone)]
pub struct TracingConfig {
	/// Optional state overrides to apply before executing the traced call. Each entry maps an
	/// account address to a set of fields (balance, nonce, code, storage) that should be
	/// temporarily replaced for the duration of the trace.
	pub state_overrides: Option<StateOverrideSet>,
}

/// A custom implementation of [`Decode`] to ensure forward and backward compatibility of the
/// [`TracingConfig`] type.
///
/// # Backwards Compatibility
///
/// Please review the documentation on [`TracingConfig`] for more information about how we manage
/// and handle compatibility for this type and instructions on what you should do when adding a
/// new field.
impl Decode for TracingConfig {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let state_overrides = Option::<StateOverrideSet>::decode(input).unwrap_or_default();
		Ok(Self { state_overrides })
	}
}

impl TracingConfig {
	/// Create a new `TracingConfig` with default values.
	pub fn new() -> Self {
		Self::default()
	}

	/// A builder method which consumes the object and sets the state overrides.
	pub fn with_state_overrides(
		mut self,
		state_overrides: impl Into<Option<StateOverrideSet>>,
	) -> Self {
		self.state_overrides = state_overrides.into();
		self
	}
}
