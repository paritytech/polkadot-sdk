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

//! # Substrate Primitives: IO
//!
//! This crate contains interfaces for the runtime to communicate with the outside world, ergo `io`.
//! In other context, such interfaces are referred to as "**host functions**".
//!
//! Each set of host functions are defined with an instance of the
//! [`sp_runtime_interface::runtime_interface`] macro.
//!
//! Most notably, this crate contains host functions for:
//!
//! - [`hashing`]
//! - [`crypto`]
//! - [`trie`]
//! - [`offchain`]
//! - [`storage`]
//! - [`allocator`]
//! - [`logging`]
//!
//! All of the default host functions provided by this crate, and by default contained in all
//! substrate-based clients are amalgamated in [`SubstrateHostFunctions`].
//!
//! ## Externalities
//!
//! Host functions go hand in hand with the concept of externalities. Externalities are an
//! environment in which host functions are provided, and thus can be accessed. Some host functions
//! are only accessible in an externality environment that provides it.
//!
//! A typical error for substrate developers is the following:
//!
//! ```should_panic
//! use sp_io::storage::get;
//! # fn main() {
//! let data = get(b"hello world");
//! # }
//! ```
//!
//! This code will panic with the following error:
//!
//! ```no_compile
//! thread 'main' panicked at '`get_version_1` called outside of an Externalities-provided environment.'
//! ```
//!
//! Such error messages should always be interpreted as "code accessing host functions accessed
//! outside of externalities".
//!
//! An externality is any type that implements [`sp_externalities::Externalities`]. A simple example
//! of which is [`TestExternalities`], which is commonly used in tests and is exported from this
//! crate.
//!
//! ```
//! use sp_io::{storage::get, TestExternalities};
//! # fn main() {
//! TestExternalities::default().execute_with(|| {
//! 	let data = get(b"hello world");
//! });
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(enable_alloc_error_handler, feature(alloc_error_handler))]

extern crate alloc;

#[cfg(not(substrate_runtime))]
use alloc::vec::Vec;

use strum::{EnumCount, FromRepr};

#[cfg(not(substrate_runtime))]
use tracing;

use sp_core::offchain::HttpError;

#[cfg(not(substrate_runtime))]
use sp_core::{crypto::KeyTypeId, ecdsa, ed25519, sr25519};

use codec::{Decode, Encode};

pub use sp_externalities::MultiRemovalResults;

#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, target_family = "wasm"))]
mod global_alloc_wasm;

#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, target_arch = "riscv64"))]
mod global_alloc_riscv;

#[cfg(not(substrate_runtime))]
const LOG_TARGET: &str = "runtime::io";

/// Error verifying ECDSA signature
#[derive(Encode, Decode)]
pub enum EcdsaVerifyError {
	/// Incorrect value of R or S
	BadRS,
	/// Incorrect value of V
	BadV,
	/// Invalid signature
	BadSignature,
}

// The FFI representation of EcdsaVerifyError.
#[derive(EnumCount, FromRepr)]
#[repr(i16)]
#[allow(missing_docs)]
pub enum RIEcdsaVerifyError {
	BadRS = -1_i16,
	BadV = -2_i16,
	BadSignature = -3_i16,
}

impl From<RIEcdsaVerifyError> for i64 {
	fn from(error: RIEcdsaVerifyError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIEcdsaVerifyError {
	type Error = ();
	fn try_from(value: i64) -> Result<Self, Self::Error> {
		let value: i16 = value.try_into().map_err(|_| ())?;
		RIEcdsaVerifyError::from_repr(value).ok_or(())
	}
}

impl From<EcdsaVerifyError> for RIEcdsaVerifyError {
	fn from(error: EcdsaVerifyError) -> Self {
		match error {
			EcdsaVerifyError::BadRS => RIEcdsaVerifyError::BadRS,
			EcdsaVerifyError::BadV => RIEcdsaVerifyError::BadV,
			EcdsaVerifyError::BadSignature => RIEcdsaVerifyError::BadSignature,
		}
	}
}

impl From<RIEcdsaVerifyError> for EcdsaVerifyError {
	fn from(error: RIEcdsaVerifyError) -> Self {
		match error {
			RIEcdsaVerifyError::BadRS => EcdsaVerifyError::BadRS,
			RIEcdsaVerifyError::BadV => EcdsaVerifyError::BadV,
			RIEcdsaVerifyError::BadSignature => EcdsaVerifyError::BadSignature,
		}
	}
}

// The FFI representation of HttpError.
#[derive(EnumCount, FromRepr)]
#[repr(i16)]
#[allow(missing_docs)]
pub enum RIHttpError {
	DeadlineReached = -1_i16,
	IoError = -2_i16,
	Invalid = -3_i16,
}

impl From<RIHttpError> for i64 {
	fn from(error: RIHttpError) -> Self {
		error as i64
	}
}

impl TryFrom<i64> for RIHttpError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		let value: i16 = value.try_into().map_err(|_| ())?;
		RIHttpError::from_repr(value).ok_or(())
	}
}

impl From<HttpError> for RIHttpError {
	fn from(error: HttpError) -> Self {
		match error {
			HttpError::DeadlineReached => RIHttpError::DeadlineReached,
			HttpError::IoError => RIHttpError::IoError,
			HttpError::Invalid => RIHttpError::Invalid,
		}
	}
}

impl From<RIHttpError> for HttpError {
	fn from(error: RIHttpError) -> Self {
		match error {
			RIHttpError::DeadlineReached => HttpError::DeadlineReached,
			RIHttpError::IoError => HttpError::IoError,
			RIHttpError::Invalid => HttpError::Invalid,
		}
	}
}

/// The outcome of calling `storage_kill`. Returned value is the number of storage items
/// removed from the backend from making the `storage_kill` call.
#[derive(Encode, Decode)]
pub enum KillStorageResult {
	/// All keys to remove were removed, return number of iterations performed during the
	/// operation.
	AllRemoved(u32),
	/// Not all key to remove were removed, return number of iterations performed during the
	/// operation.
	SomeRemaining(u32),
}

impl From<MultiRemovalResults> for KillStorageResult {
	fn from(r: MultiRemovalResults) -> Self {
		// We use `loops` here rather than `backend` because that's the same as the original
		// functionality pre-#11490. This won't matter once we switch to the new host function
		// since we won't be using the `KillStorageResult` type in the runtime any more.
		match r.maybe_cursor {
			None => Self::AllRemoved(r.loops),
			Some(..) => Self::SomeRemaining(r.loops),
		}
	}
}

/// Storage iteration counters
#[repr(C)]
#[derive(Default)]
pub struct StorageIterations {
	/// The number of backend iterations.
	pub backend: u32,
	/// The number of unique iterations.
	pub unique: u32,
	/// The number of loops.
	pub loops: u32,
}

impl AsRef<[u8]> for StorageIterations {
	fn as_ref(&self) -> &[u8] {
		#[cfg(target_endian = "big")]
		compile_error!("StorageIterations only supports little-endian architectures");

		// SAFETY: The layout of this type is the same as for [u32; 3] and all the possible byte
		// sequences are valid for this type so casting it from and to a byte slice is safe.
		// However, the data may become corrupted when copied if host and runtime have different
		// endianness, so that is checked statically.
		unsafe {
			core::slice::from_raw_parts(
				(&raw const *self).cast::<u8>(),
				core::mem::size_of::<Self>(),
			)
		}
	}
}

impl AsMut<[u8]> for StorageIterations {
	fn as_mut(&mut self) -> &mut [u8] {
		#[cfg(target_endian = "big")]
		compile_error!("StorageIterations only supports little-endian architectures");

		// SAFETY: The layout of this type is the same as for [u32; 3] and all the possible byte
		// sequences are valid for this type so casting it from and to a byte slice is safe.
		// However, the data may become corrupted when copied if host and runtime have different
		// endianness, so that is checked statically.
		unsafe {
			core::slice::from_raw_parts_mut(
				self as *mut Self as *mut u8,
				core::mem::size_of::<Self>(),
			)
		}
	}
}

/// Defines a `#[repr(transparent)]` newtype over a fixed-size byte array with `Default`,
/// `AsRef<[u8]>`, and `AsMut<[u8]>` implementations.
macro_rules! define_byte_array_type {
	($(#[$meta:meta])* $vis:vis struct $name:ident(pub [u8; $size:expr])) => {
		$(#[$meta])*
		#[repr(transparent)]
		$vis struct $name(pub [u8; $size]);

		impl Default for $name {
			fn default() -> Self {
				Self([0; $size])
			}
		}

		impl AsRef<[u8]> for $name {
			fn as_ref(&self) -> &[u8] {
				&self.0
			}
		}

		impl AsMut<[u8]> for $name {
			fn as_mut(&mut self) -> &mut [u8] {
				&mut self.0
			}
		}
	};
}

define_byte_array_type! {
	/// Wrapper type for 512-bit hashes.
	pub struct Hash512(pub [u8; 64])
}

define_byte_array_type! {
	/// Wrapper type for 512-bit pubkeys.
	pub struct Pubkey512(pub [u8; 64])
}

define_byte_array_type! {
	/// A workaround wrapper type for 264-bit values (`[u8; 33]`) not implementing `Default`.
	pub struct Pubkey264(pub [u8; 33])
}

define_byte_array_type! {
	/// Represents an opaque network peer ID.
	pub struct NetworkPeerId(pub [u8; 38])
}

trait IntoI64: Into<i64> {
	const MAX: i64;
}

impl IntoI64 for u8 {
	const MAX: i64 = u8::MAX as i64;
}
impl IntoI64 for u16 {
	const MAX: i64 = u16::MAX as i64;
}
impl IntoI64 for u32 {
	const MAX: i64 = u32::MAX as i64;
}

/// A wrapper around `Option<T>` for the FFI marshalling.
///
/// Used to return less-than-64-bit passed as `i64` through the FFI boundary. `-1_i64` is used to
/// represent `None`.
#[derive(Copy, Clone)]
#[repr(transparent)]
pub struct RIIntOption<T>(Option<T>);

impl<T: IntoI64> From<RIIntOption<T>> for Option<T> {
	fn from(r: RIIntOption<T>) -> Self {
		r.0
	}
}

impl<T: IntoI64> From<Option<T>> for RIIntOption<T> {
	fn from(r: Option<T>) -> Self {
		Self(r)
	}
}

impl<T: IntoI64> From<RIIntOption<T>> for i64 {
	fn from(r: RIIntOption<T>) -> Self {
		match r.0 {
			Some(value) => value.into(),
			None => -1,
		}
	}
}

impl<T: TryFrom<i64> + IntoI64> TryFrom<i64> for RIIntOption<T> {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value == -1 {
			Ok(RIIntOption(None))
		} else if value >= 0 && value <= T::MAX.into() {
			Ok(RIIntOption(Some(value.try_into().map_err(|_| ())?)))
		} else {
			// Invalid FFI value (e.g., -2, or too large for T).
			// `ConvertAndReturnAs` will panic when `TryFrom` returns an `Err`, which is the correct
			// behavior here.
			Err(())
		}
	}
}

/// Used to return less-than-64-bit value passed as `i64` through the FFI boundary.
/// Negative values are used to represent error variants.
pub enum RIIntResult<R, E> {
	/// Successful result
	Ok(R),
	/// Error result
	Err(E),
}

impl<R, E, OR, OE> From<Result<OR, OE>> for RIIntResult<R, E>
where
	R: From<OR>,
	E: From<OE>,
{
	fn from(result: Result<OR, OE>) -> Self {
		match result {
			Ok(value) => Self::Ok(value.into()),
			Err(error) => Self::Err(error.into()),
		}
	}
}

impl<R, E, OR, OE> From<RIIntResult<R, E>> for Result<OR, OE>
where
	OR: From<R>,
	OE: From<E>,
{
	fn from(result: RIIntResult<R, E>) -> Self {
		match result {
			RIIntResult::Ok(value) => Ok(value.into()),
			RIIntResult::Err(error) => Err(error.into()),
		}
	}
}

/// Represents a void successful result (always 0 in FFI)
pub struct VoidResult;

impl IntoI64 for VoidResult {
	const MAX: i64 = 0;
}

impl From<VoidResult> for u32 {
	fn from(_: VoidResult) -> Self {
		0
	}
}

impl From<u32> for VoidResult {
	fn from(_: u32) -> Self {
		VoidResult
	}
}

impl From<()> for VoidResult {
	fn from(_: ()) -> Self {
		VoidResult
	}
}

impl From<VoidResult> for () {
	fn from(_: VoidResult) -> Self {
		()
	}
}

impl From<VoidResult> for i64 {
	fn from(_: VoidResult) -> Self {
		0
	}
}

impl TryFrom<i64> for VoidResult {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value == 0 {
			Ok(VoidResult)
		} else {
			Err(())
		}
	}
}

/// Represents a void error (always -1 in FFI)
pub struct VoidError;

impl strum::EnumCount for VoidError {
	const COUNT: usize = 1;
}

impl From<VoidError> for i64 {
	fn from(_: VoidError) -> Self {
		-1
	}
}

impl From<VoidError> for () {
	fn from(_: VoidError) -> Self {
		()
	}
}

impl From<()> for VoidError {
	fn from(_: ()) -> Self {
		VoidError
	}
}

impl TryFrom<i64> for VoidError {
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value == -1 {
			Ok(VoidError)
		} else {
			Err(())
		}
	}
}

impl<R: Into<i64> + IntoI64, E: Into<i64> + strum::EnumCount> TryFrom<RIIntResult<R, E>> for i64 {
	type Error = ();

	fn try_from(result: RIIntResult<R, E>) -> Result<Self, ()> {
		match result {
			RIIntResult::Ok(value) => Ok(value.into()),
			RIIntResult::Err(e) => {
				let error_code: i64 = e.into();
				if error_code < 0 && error_code >= -(E::COUNT as i64) {
					Ok(error_code)
				} else {
					Err(())
				}
			},
		}
	}
}

impl<R: TryFrom<i64> + IntoI64, E: TryFrom<i64> + strum::EnumCount> TryFrom<i64>
	for RIIntResult<R, E>
{
	type Error = ();

	fn try_from(value: i64) -> Result<Self, Self::Error> {
		if value >= 0 && value <= R::MAX.into() {
			Ok(RIIntResult::Ok(value.try_into().map_err(|_| ())?))
		} else if value < 0 && value >= -(E::COUNT as i64) {
			Ok(RIIntResult::Err(value.try_into().map_err(|_| ())?))
		} else {
			Err(())
		}
	}
}

impl<R: TryFrom<i64> + IntoI64, E: TryFrom<i64> + strum::EnumCount> TryFrom<i32>
	for RIIntResult<R, E>
{
	type Error = ();

	fn try_from(value: i32) -> Result<Self, Self::Error> {
		(value as i64).try_into()
	}
}

impl<E: Into<i64> + strum::EnumCount> TryFrom<RIIntResult<VoidResult, E>> for i32 {
	type Error = ();

	fn try_from(value: RIIntResult<VoidResult, E>) -> Result<Self, ()> {
		match value {
			RIIntResult::Ok(_) => Ok(0),
			RIIntResult::Err(e) => {
				let error_code: i64 = e.into();
				if error_code < 0 && error_code >= -(E::COUNT as i64) {
					Ok(error_code as i32)
				} else {
					Err(())
				}
			},
		}
	}
}

#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	/// Extension to signal to [`crypt::ed25519_verify`] to use the dalek crate.
	///
	/// The switch from `ed25519-dalek` to `ed25519-zebra` was a breaking change.
	/// `ed25519-zebra` is more permissive when it comes to the verification of signatures.
	/// This means that some chains may fail to sync from genesis when using `ed25519-zebra`.
	/// So, this extension can be registered to the runtime execution environment to signal
	/// that `ed25519-dalek` should be used for verification. The extension can be registered
	/// in the following way:
	///
	/// ```nocompile
	/// client.execution_extensions().set_extensions_factory(
	/// 	// Let the `UseDalekExt` extension being registered for each runtime invocation
	/// 	// until the execution happens in the context of block `1000`.
	/// 	sc_client_api::execution_extensions::ExtensionBeforeBlock::<Block, UseDalekExt>::new(1000)
	/// );
	/// ```
	pub struct UseDalekExt;
}

#[cfg(not(substrate_runtime))]
impl Default for UseDalekExt {
	fn default() -> Self {
		Self
	}
}

/// Per-algorithm snapshots of public keys retained by [`PublicKeysCacheExt`].
///
/// At most one snapshot is cached per algorithm — the one populated by the most
/// recent `*_public_keys` host call whose output buffer was too small to receive
/// the full result. The next call with a sufficiently large buffer drains the
/// snapshot, giving the two-call probe-then-fetch pattern of the convenience
/// wrappers a consistent view of the keystore.
#[cfg(not(substrate_runtime))]
#[derive(Default)]
pub struct PublicKeysCache {
	/// Cached `ed25519` public keys for one key type.
	pub ed25519: Option<(KeyTypeId, Vec<ed25519::Public>)>,
	/// Cached `sr25519` public keys for one key type.
	pub sr25519: Option<(KeyTypeId, Vec<sr25519::Public>)>,
	/// Cached `ecdsa` public keys for one key type.
	pub ecdsa: Option<(KeyTypeId, Vec<ecdsa::Public>)>,
}

#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	/// Externalities extension backing the [`PublicKeysCache`].
	pub struct PublicKeysCacheExt(PublicKeysCache);
}

#[cfg(not(substrate_runtime))]
impl Default for PublicKeysCacheExt {
	fn default() -> Self {
		Self(PublicKeysCache::default())
	}
}

#[cfg(not(substrate_runtime))]
macro_rules! ensure_public_keys_cache_ext_registered {
	($self:expr) => {
		match $self.register_extension(PublicKeysCacheExt::default()) {
			Ok(()) | Err(sp_externalities::Error::ExtensionAlreadyRegistered) => (),
			Err(e) => panic!("Failed to register `PublicKeysCacheExt`: {e:?}"),
		}
	};
}

#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	/// Deprecated verification context.
	///
	/// Stores the combined result of all verifications that are done in the same context.
	struct VerificationExtDeprecated(bool);
}

#[cfg(all(substrate_runtime, feature = "with-tracing"))]
mod tracing_setup {
	use super::wasm_tracing;
	use core::sync::atomic::{AtomicBool, Ordering};
	use tracing_core::{
		dispatcher::{set_global_default, Dispatch},
		span::{Attributes, Id, Record},
		Event, Metadata,
	};

	static TRACING_SET: AtomicBool = AtomicBool::new(false);

	/// The PassingTracingSubscriber implements `tracing_core::Subscriber`
	/// and pushes the information across the runtime interface to the host
	struct PassingTracingSubscriber;

	impl tracing_core::Subscriber for PassingTracingSubscriber {
		fn enabled(&self, metadata: &Metadata<'_>) -> bool {
			wasm_tracing::enabled(metadata.into())
		}
		fn new_span(&self, attrs: &Attributes<'_>) -> Id {
			Id::from_u64(wasm_tracing::enter_span(attrs.into()))
		}
		fn enter(&self, _: &Id) {
			// Do nothing, we already entered the span previously
		}
		/// Not implemented! We do not support recording values later
		/// Will panic when used.
		fn record(&self, _: &Id, _: &Record<'_>) {
			unimplemented! {} // this usage is not supported
		}
		/// Not implemented! We do not support recording values later
		/// Will panic when used.
		fn record_follows_from(&self, _: &Id, _: &Id) {
			unimplemented! {} // this usage is not supported
		}
		fn event(&self, event: &Event<'_>) {
			wasm_tracing::event(event.into())
		}
		fn exit(&self, span: &Id) {
			wasm_tracing::exit(span.into_u64())
		}
	}

	/// Initialize tracing of sp_tracing on wasm with `with-tracing` enabled.
	/// Can be called multiple times from within the same process and will only
	/// set the global bridging subscriber once.
	pub fn init_tracing() {
		if TRACING_SET.load(Ordering::Relaxed) == false {
			set_global_default(Dispatch::new(PassingTracingSubscriber {}))
				.expect("We only ever call this once");
			TRACING_SET.store(true, Ordering::Relaxed);
		}
	}
}

#[cfg(not(all(substrate_runtime, feature = "with-tracing")))]
mod tracing_setup {
	/// Initialize tracing of sp_tracing not necessary – noop. To enable build
	/// when not both `substrate_runtime` and `with-tracing`-feature.
	pub fn init_tracing() {}
}

pub use tracing_setup::init_tracing;

/// Crashes the execution of the program.
///
/// Equivalent to the WASM `unreachable` instruction, RISC-V `unimp` instruction,
/// or just the `unreachable!()` macro everywhere else.
pub fn unreachable() -> ! {
	#[cfg(target_family = "wasm")]
	{
		core::arch::wasm32::unreachable();
	}

	#[cfg(any(target_arch = "riscv32", target_arch = "riscv64"))]
	unsafe {
		core::arch::asm!("unimp", options(noreturn));
	}

	#[cfg(not(any(target_arch = "riscv32", target_arch = "riscv64", target_family = "wasm")))]
	unreachable!();
}

/// A default panic handler for the runtime environment.
#[cfg(all(not(feature = "disable_panic_handler"), substrate_runtime))]
#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
	let message = alloc::format!("{}", info);
	#[cfg(feature = "improved_panic_error_reporting")]
	{
		panic_handler::abort_on_panic(&message);
	}
	#[cfg(not(feature = "improved_panic_error_reporting"))]
	{
		logging::log(sp_core::RuntimeInterfaceLogLevel::Error, "runtime", message.as_bytes());
		unreachable();
	}
}

/// A default OOM handler for the runtime environment.
#[cfg(all(not(feature = "disable_oom"), enable_alloc_error_handler))]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
	#[cfg(feature = "improved_panic_error_reporting")]
	{
		panic_handler::abort_on_panic("Runtime memory exhausted.");
	}
	#[cfg(not(feature = "improved_panic_error_reporting"))]
	{
		logging::log(
			sp_core::RuntimeInterfaceLogLevel::Error,
			"runtime",
			b"Runtime memory exhausted. Aborting",
		);
		unreachable();
	}
}

mod host_functions;

#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
mod native;

pub use host_functions::{
	input::input,
	storage::{default_child_storage, storage},
};

#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
pub use host_functions::{
	allocator::allocator, crypto::crypto, hashing::hashing, logging::logging, misc::misc,
	offchain::offchain, offchain_index::offchain_index, panic_handler::panic_handler,
	transaction_index::transaction_index, trie::trie, wasm_tracing::wasm_tracing,
};

#[cfg(all(substrate_runtime, any(target_arch = "riscv32", target_arch = "riscv64")))]
pub use native::{
	allocator, crypto, hashing, logging, misc, offchain, offchain_index, panic_handler,
	transaction_index, trie, wasm_tracing,
};

/// Type alias for Externalities implementation used in tests.
#[cfg(feature = "std")] // NOTE: Deliberately isn't `not(substrate_runtime)`.
pub type TestExternalities = sp_state_machine::TestExternalities<sp_core::Blake2Hasher>;

/// The host functions Substrate provides for the Wasm runtime environment.
///
/// All these host functions will be callable from inside the Wasm environment.
#[docify::export]
#[cfg(not(substrate_runtime))]
pub type SubstrateHostFunctions = (
	storage::HostFunctions,
	default_child_storage::HostFunctions,
	misc::HostFunctions,
	wasm_tracing::HostFunctions,
	offchain::HostFunctions,
	crypto::HostFunctions,
	hashing::HostFunctions,
	allocator::HostFunctions,
	panic_handler::HostFunctions,
	logging::HostFunctions,
	crate::trie::HostFunctions,
	offchain_index::HostFunctions,
	transaction_index::HostFunctions,
	input::HostFunctions,
	sp_additional_data::additional_data::HostFunctions,
);

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::{crypto::UncheckedInto, map, storage::Storage};
	use sp_state_machine::BasicExternalities;

	#[test]
	fn storage_works() {
		let mut t = BasicExternalities::default();
		t.execute_with(|| {
			assert_eq!(storage::get(b"hello"), None);
			storage::set(b"hello", b"world");
			assert_eq!(storage::get(b"hello"), Some(b"world".to_vec().into()));
			assert_eq!(storage::get(b"foo"), None);
			storage::set(b"foo", &[1, 2, 3][..]);
		});

		t = BasicExternalities::new(Storage {
			top: map![b"foo".to_vec() => b"bar".to_vec()],
			children_default: map![],
		});

		t.execute_with(|| {
			assert_eq!(storage::get(b"hello"), None);
			assert_eq!(storage::get(b"foo"), Some(b"bar".to_vec().into()));
		});

		let value = vec![7u8; 35];
		let storage =
			Storage { top: map![b"foo00".to_vec() => value.clone()], children_default: map![] };
		t = BasicExternalities::new(storage);

		t.execute_with(|| {
			assert_eq!(storage::get(b"hello"), None);
			assert_eq!(storage::get(b"foo00"), Some(value.clone().into()));
		});
	}

	#[test]
	fn read_storage_works() {
		let value = b"\x0b\0\0\0Hello world".to_vec();
		let mut t = BasicExternalities::new(Storage {
			top: map![b":test".to_vec() => value.clone()],
			children_default: map![],
		});

		t.execute_with(|| {
			// `read_exact` with a buffer that is too small does NOT write data into the buffer
			// (RFC-145).
			let mut v = [0u8; 4];
			assert_eq!(storage::read_exact(b":test", &mut v[..], 0).unwrap(), value.len() as u32);
			assert_eq!(v, [0u8, 0, 0, 0]);

			// `read_partial` with a buffer that is too small DOES write partial data.
			let mut v = [0u8; 4];
			assert_eq!(storage::read_partial(b":test", &mut v[..], 0).unwrap(), value.len() as u32);
			assert_eq!(v, [11u8, 0, 0, 0]);

			// `read_exact` with an exact-sized buffer works.
			let mut w = [0u8; 11];
			assert_eq!(
				storage::read_exact(b":test", &mut w[..], 4).unwrap(),
				value.len() as u32 - 4
			);
			assert_eq!(&w, b"Hello world");
		});
	}

	#[test]
	fn clear_prefix_works() {
		let mut t = BasicExternalities::new(Storage {
			top: map![
				b":a".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abcd".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abc".to_vec() => b"\x0b\0\0\0Hello world".to_vec(),
				b":abdd".to_vec() => b"\x0b\0\0\0Hello world".to_vec()
			],
			children_default: map![],
		});

		t.execute_with(|| {
			let res = storage::clear_prefix(b":abc", None, None);
			assert_eq!(res.backend, 2);
			assert_eq!(res.unique, 2);
			assert_eq!(res.loops, 2);

			assert!(storage::get(b":a").is_some());
			assert!(storage::get(b":abdd").is_some());
			assert!(storage::get(b":abcd").is_none());
			assert!(storage::get(b":abc").is_none());

			let res = storage::clear_prefix(b":abc", None, None);
			assert_eq!(res.backend, 0);
			assert_eq!(res.unique, 0);
			assert_eq!(res.loops, 0);
		});
	}

	#[test]
	fn network_peer_id_writes_raw_peer_id_bytes() {
		use sp_core::offchain::{testing::TestOffchainExt, OffchainWorkerExt};

		let (offchain, _state) = TestOffchainExt::new();
		let mut ext = BasicExternalities::default();
		ext.register_extension(OffchainWorkerExt::new(offchain));

		ext.execute_with(|| {
			// `TestOffchainExt` reports the raw peer id `0..38`, SCALE-encoded (length-prefixed)
			// in the network state, exactly as the real `sc-offchain` externalities do.
			// `network_peer_id` must strip the length prefix and write the raw bytes into the
			// fixed-size buffer. Regression test for the panic where the length-prefixed vec (39
			// bytes) was copied straight into a `[u8; 38]`.
			let mut peer_id = NetworkPeerId::default();
			assert_eq!(offchain::network_peer_id(&mut peer_id), Ok(()));
			assert_eq!(peer_id.0.to_vec(), (0u8..38).collect::<Vec<u8>>());
		});
	}

	fn zero_ed_pub() -> ed25519::Public {
		[0u8; 32].unchecked_into()
	}

	fn zero_ed_sig() -> ed25519::Signature {
		ed25519::Signature::from_raw([0u8; 64])
	}

	#[test]
	fn use_dalek_ext_works() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(UseDalekExt);

		// With dalek the zero signature should fail to verify.
		ext.execute_with(|| {
			assert!(!crypto::ed25519_verify(&zero_ed_sig(), &Vec::new(), &zero_ed_pub()));
		});

		// But with zebra it should work.
		BasicExternalities::default().execute_with(|| {
			assert!(crypto::ed25519_verify(&zero_ed_sig(), &Vec::new(), &zero_ed_pub()));
		})
	}

	#[test]
	fn dalek_should_not_panic_on_invalid_signature() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(UseDalekExt);

		ext.execute_with(|| {
			let mut bytes = [0u8; 64];
			// Make it invalid
			bytes[63] = 0b1110_0000;

			assert!(!crypto::ed25519_verify(
				&ed25519::Signature::from_raw(bytes),
				&Vec::new(),
				&zero_ed_pub()
			));
		});
	}
}
