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

use alloc::{vec, vec::Vec};

use strum::{EnumCount, FromRepr};

#[cfg(not(substrate_runtime))]
use tracing;

#[cfg(not(substrate_runtime))]
use sp_core::{
	crypto::Pair,
	hexdisplay::HexDisplay,
	offchain::{OffchainDbExt, OffchainWorkerExt, TransactionPoolExt},
	storage::ChildInfo,
};
#[cfg(not(substrate_runtime))]
use sp_keystore::KeystoreExt;

#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
use sp_core::{
	crypto::KeyTypeId,
	ecdsa, ed25519,
	offchain::{
		HttpError, HttpRequestId, HttpRequestStatus, OpaqueNetworkState, StorageKind, Timestamp,
	},
	sr25519,
	storage::StateVersion,
	LogLevelFilter, OpaquePeerId, RuntimeInterfaceLogLevel, H256,
};

#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};

#[cfg(not(substrate_runtime))]
use sp_trie::{LayoutV0, LayoutV1, TrieConfiguration};

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnByCodec, AllocateAndReturnFatPointer, AllocateAndReturnPointer,
		ConvertAndPassAs, ConvertAndReturnAs, PassAs, PassFatPointerAndDecode,
		PassFatPointerAndDecodeSlice, PassFatPointerAndRead, PassFatPointerAndReadWrite,
		PassFatPointerAndWriteInputData, PassMaybeFatPointerAndRead, PassPointerAndRead,
		PassPointerAndReadCopy, PassPointerAndWrite, PassPointerToPrimitiveAndWrite, ReturnAs,
	},
	runtime_interface, Pointer,
};

use codec::{Decode, Encode};

#[cfg(not(substrate_runtime))]
use secp256k1::{
	ecdsa::{RecoverableSignature, RecoveryId},
	Message,
};

#[cfg(not(substrate_runtime))]
use sp_externalities::{Externalities, ExternalitiesExt};

pub use sp_externalities::MultiRemovalResults;

#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, target_family = "wasm"))]
mod global_alloc_wasm;

#[cfg(all(
	not(feature = "disable_allocator"),
	substrate_runtime,
	any(target_arch = "riscv32", target_arch = "riscv64")
))]
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
/// A workaround for 512-bit values (`[u8; 64]`) not implementing `Default`.
pub struct Val512(pub [u8; 64]);

impl Default for Val512 {
	fn default() -> Self {
		Self([0; 64])
	}
}

impl AsRef<[u8]> for Val512 {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl AsMut<[u8]> for Val512 {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0
	}
}

/// Wrapper type for 512-bit hashes.
pub type Hash512 = Val512;
/// Wrapper type for 512-bit pubkeys.
pub type Pubkey512 = Val512;

/// A workaround wrapper type for 264-bit values (`[u8; 33]`) not implementing `Default`.
pub struct Pubkey264(pub [u8; 33]);

impl Default for Pubkey264 {
	fn default() -> Self {
		Self([0; 33])
	}
}

impl AsRef<[u8]> for Pubkey264 {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl AsMut<[u8]> for Pubkey264 {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0
	}
}

/// Represents an opaque network peer ID
pub struct NetworkPeerId(pub [u8; 38]);

impl Default for NetworkPeerId {
	fn default() -> Self {
		Self([0; 38])
	}
}

impl AsRef<[u8]> for NetworkPeerId {
	fn as_ref(&self) -> &[u8] {
		&self.0
	}
}

impl AsMut<[u8]> for NetworkPeerId {
	fn as_mut(&mut self) -> &mut [u8] {
		&mut self.0
	}
}

trait LessThan64BitPositiveInteger: Into<i64> {
	const MAX: i64;
}

impl LessThan64BitPositiveInteger for u8 {
	const MAX: i64 = u8::MAX as i64;
}
impl LessThan64BitPositiveInteger for u16 {
	const MAX: i64 = u16::MAX as i64;
}
impl LessThan64BitPositiveInteger for u32 {
	const MAX: i64 = u32::MAX as i64;
}

/// A wrapper around `Option<T>` for the FFI marshalling.
///
/// Used to return less-than-64-bit passed as `i64` through the FFI boundary. `-1_i64` is used to
/// represent `None`.
#[derive(Copy, Clone)]
pub struct RIIntOption<T>(Option<T>);

impl<T: LessThan64BitPositiveInteger> From<RIIntOption<T>> for Option<T> {
	fn from(r: RIIntOption<T>) -> Self {
		r.0
	}
}

impl<T: LessThan64BitPositiveInteger> From<Option<T>> for RIIntOption<T> {
	fn from(r: Option<T>) -> Self {
		Self(r)
	}
}

impl<T: LessThan64BitPositiveInteger> From<RIIntOption<T>> for i64 {
	fn from(r: RIIntOption<T>) -> Self {
		match r.0 {
			Some(value) => value.into(),
			None => -1,
		}
	}
}

impl<T: TryFrom<i64> + LessThan64BitPositiveInteger> TryFrom<i64> for RIIntOption<T> {
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

impl LessThan64BitPositiveInteger for VoidResult {
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

impl<R: Into<i64> + LessThan64BitPositiveInteger, E: Into<i64> + strum::EnumCount>
	From<RIIntResult<R, E>> for i64
{
	fn from(result: RIIntResult<R, E>) -> Self {
		match result {
			RIIntResult::Ok(value) => value.into(),
			RIIntResult::Err(e) => {
				let error_code: i64 = e.into();
				assert!(
					error_code > 0 && error_code <= E::COUNT as i64,
					"Error variant index out of bounds"
				);
				-error_code
			},
		}
	}
}

impl<R: TryFrom<i64> + LessThan64BitPositiveInteger, E: TryFrom<i64> + strum::EnumCount>
	TryFrom<i64> for RIIntResult<R, E>
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

/// Interface for accessing the storage from within the runtime.
#[runtime_interface]
pub trait Storage {
	/// Returns the data for `key` in the storage or `None` if the key can not be found.
	#[version(1, register_only)]
	fn get(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bytes::Bytes>> {
		self.storage(key).map(|s| bytes::Bytes::from(s.to_vec()))
	}

	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	fn read(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> AllocateAndReturnByCodec<Option<u32>> {
		self.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			data.len() as u32
		})
	}

	/// Get `key` from storage, placing the value into `value_out` and return the number of
	/// bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	#[version(2)]
	fn read(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		self.storage(key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			if value_out.len() >= data.len() {
				value_out[..data.len()].copy_from_slice(&data[..]);
			}
			data.len() as u32
		})
	}

	/// A convenience wrapper implementing the deprecated `get` host function
	/// functionality through the new interface.
	#[wrapper]
	fn get(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut value_out = vec![0u8; 256];
		let len = read(key.as_ref(), &mut value_out[..], 0)?;
		if len as usize > value_out.len() {
			value_out.resize(len as usize, 0);
			read(key.as_ref(), &mut value_out[..], 0)?;
		}
		value_out.truncate(len as usize);
		Some(value_out)
	}

	/// Set `key` to `value` in the storage.
	fn set(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<&[u8]>) {
		self.set_storage(key.to_vec(), value.to_vec());
	}

	/// Clear the storage of the given `key` and its value.
	fn clear(&mut self, key: PassFatPointerAndRead<&[u8]>) {
		self.clear_storage(key)
	}

	/// Check whether the given `key` exists in storage.
	fn exists(&mut self, key: PassFatPointerAndRead<&[u8]>) -> bool {
		self.exists_storage(key)
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	fn clear_prefix(&mut self, prefix: PassFatPointerAndRead<&[u8]>) {
		let _ = Externalities::clear_prefix(*self, prefix, None, None);
	}

	/// Clear the storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// # Limit
	///
	/// Deletes all keys from the overlay and up to `limit` keys from the backend if
	/// it is set to `Some`. No limit is applied when `limit` is set to `None`.
	///
	/// The limit can be used to partially delete a prefix storage in case it is too large
	/// to delete in one go (block).
	///
	/// Returns [`KillStorageResult`] to inform about the result.
	///
	/// # Note
	///
	/// Please note that keys that are residing in the overlay for that prefix when
	/// issuing this call are all deleted without counting towards the `limit`. Only keys
	/// written during the current block are part of the overlay. Deleting with a `limit`
	/// mostly makes sense with an empty overlay for that prefix.
	///
	/// Calling this function multiple times per block for the same `prefix` does
	/// not make much sense because it is not cumulative when called inside the same block.
	/// The deletion would always start from `prefix` resulting in the same keys being deleted
	/// every time this function is called with the exact same arguments per block. This happens
	/// because the keys in the overlay are not taken into account when deleting keys in the
	/// backend.
	#[version(2)]
	fn clear_prefix(
		&mut self,
		prefix: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		Externalities::clear_prefix(*self, prefix, limit, None).into()
	}

	/// Partially clear the storage of each key-value pair where the key starts with the given
	/// prefix.
	///
	/// # Limit
	///
	/// A *limit* should always be provided through `maybe_limit`. This is one fewer than the
	/// maximum number of backend iterations which may be done by this operation and as such
	/// represents the maximum number of backend deletions which may happen. A *limit* of zero
	/// implies that no keys will be deleted, though there may be a single iteration done.
	///
	/// The limit can be used to partially delete a prefix storage in case it is too large or costly
	/// to delete in a single operation.
	///
	/// # Cursor
	///
	/// A *cursor* may be passed in to this operation with `maybe_cursor`. `None` should only be
	/// passed once (in the initial call) for any given `maybe_prefix` value. Subsequent calls
	/// operating on the same prefix should always pass `Some`, and this should be equal to the
	/// previous call result's `maybe_cursor` field.
	///
	/// Stores the output cursor and three counters (backend deletions, unique key deletions, number
	/// of iterations performed) into the provided output buffers. See
	/// [`MultiRemovalResults`](sp_io::MultiRemovalResults) for more details.
	///
	/// Returns the number of bytes in the output cursor. If the output buffer is not large enough,
	/// the cursor will be truncated to the length of the buffer, but the full length of the cursor
	/// is still returned.
	///
	/// NOTE: After the initial call for any given prefix, it is important that no further
	/// keys under the same prefix are inserted. If so, then they may or may not be deleted by
	/// subsequent calls.
	///
	/// NOTE: Please note that keys which are residing in the overlay for that prefix when
	/// issuing this call are deleted without counting towards the `limit`.
	#[version(3)]
	#[wrapped]
	fn clear_prefix(
		&mut self,
		maybe_prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassMaybeFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndReadWrite<&mut [u8]>,
		backend: PassPointerToPrimitiveAndWrite<&mut u32>,
		unique: PassPointerToPrimitiveAndWrite<&mut u32>,
		loops: PassPointerToPrimitiveAndWrite<&mut u32>,
	) -> u32 {
		let removal_results = Externalities::clear_prefix(
			*self,
			maybe_prefix,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		*backend = removal_results.backend;
		*unique = removal_results.unique;
		*loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `clear_prefix` host
	/// function.
	#[wrapper]
	fn clear_prefix(
		maybe_prefix: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor_in: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let cursor_len = clear_prefix__wrapped(
			maybe_prefix.as_ref(),
			maybe_limit,
			maybe_cursor_in,
			&mut maybe_cursor_out,
			&mut result.backend,
			&mut result.unique,
			&mut result.loops,
		) as usize;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				debug_assert!(cached_cursor_len.is_some());
				debug_assert_eq!(cached_cursor_len.unwrap_or(0) as usize, cursor_len);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}
		result
	}

	/// Append the encoded `value` to the storage item at `key`.
	///
	/// The storage item needs to implement [`EncodeAppend`](codec::EncodeAppend).
	///
	/// # Warning
	///
	/// If the storage item does not support [`EncodeAppend`](codec::EncodeAppend) or
	/// something else fails at appending, the storage item will be set to `[value]`.
	fn append(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<Vec<u8>>) {
		self.storage_append(key.to_vec(), value);
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	fn root(&mut self) -> AllocateAndReturnFatPointer<Vec<u8>> {
		self.storage_root(StateVersion::V0)
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	#[version(2)]
	fn root(&mut self, version: PassAs<StateVersion, u8>) -> AllocateAndReturnFatPointer<Vec<u8>> {
		self.storage_root(version)
	}

	/// "Commit" all existing operations and compute the resulting storage root.
	///
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash.
	#[version(3)]
	#[wrapped]
	fn root(&mut self, out: PassFatPointerAndReadWrite<&mut [u8]>) -> u32 {
		let root = self.storage_root(StateVersion::V0);
		if out.len() >= root.len() {
			out[..root.len()].copy_from_slice(&root[..]);
		}
		root.len() as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `root` host
	/// function.
	#[wrapper]
	fn root() -> Vec<u8> {
		let mut root_out = vec![0u8; 256];
		let len = root__wrapped(&mut root_out[..]);
		if len as usize > root_out.len() {
			root_out.resize(len as usize, 0);
			root__wrapped(&mut root_out[..]);
		}
		root_out.truncate(len as usize);
		root_out
	}

	/// Always returns `None`. This function exists for compatibility reasons.
	#[version(1, register_only)]
	fn changes_root(
		&mut self,
		_parent_hash: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		None
	}

	/// Get the next key in storage after the given one in lexicographic order.
	fn next_key(
		&mut self,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		self.next_storage_key(key)
	}

	/// Get the next key in storage after the given one in lexicographic order.
	#[wrapped]
	#[version(2)]
	fn next_key(
		&mut self,
		key_in: PassFatPointerAndRead<&[u8]>,
		key_out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> u32 {
		let next_key = self.next_storage_key(key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `next_key` host
	/// function.
	#[wrapper]
	fn next_key(key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut key_out = vec![0u8; 256];
		let len = next_key__wrapped(key.as_ref(), &mut key_out[..]);
		if len as usize > key_out.len() {
			key_out.resize(len as usize, 0);
			next_key__wrapped(key.as_ref(), &mut key_out[..]);
		}
		key_out.truncate(len as usize);
		if len > 0 {
			Some(key_out)
		} else {
			None
		}
	}

	/// Start a new nested transaction.
	///
	/// This allows to either commit or roll back all changes that are made after this call.
	/// For every transaction there must be a matching call to either `rollback_transaction`
	/// or `commit_transaction`. This is also effective for all values manipulated using the
	/// `DefaultChildStorage` API.
	///
	/// # Warning
	///
	/// This is a low level API that is potentially dangerous as it can easily result
	/// in unbalanced transactions. For example, FRAME users should use high level storage
	/// abstractions.
	fn start_transaction(&mut self) {
		self.storage_start_transaction();
	}

	/// Rollback the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are discarded.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	fn rollback_transaction(&mut self) {
		self.storage_rollback_transaction()
			.expect("No open transaction that can be rolled back.");
	}

	/// Commit the last transaction started by `start_transaction`.
	///
	/// Any changes made during that transaction are committed.
	///
	/// # Panics
	///
	/// Will panic if there is no open transaction.
	fn commit_transaction(&mut self) {
		self.storage_commit_transaction()
			.expect("No open transaction that can be committed.");
	}
}

/// Interface for accessing the child storage for default child trie,
/// from within the runtime.
#[runtime_interface]
pub trait DefaultChildStorage {
	/// Get a default child storage value for a given key.
	///
	/// Parameter `storage_key` is the unprefixed location of the root of the child trie in the
	/// parent trie. Result is `None` if the value for `key` in the child storage can not be found.
	#[version(1, register_only)]
	fn get(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key).map(|s| s.to_vec())
	}

	/// Allocation efficient variant of `get`.
	///
	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	fn read(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> AllocateAndReturnByCodec<Option<u32>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key).map(|value| {
			let value_offset = value_offset as usize;
			let data = &value[value_offset.min(value.len())..];
			let written = core::cmp::min(data.len(), value_out.len());
			value_out[..written].copy_from_slice(&data[..written]);
			data.len() as u32
		})
	}

	/// Allocation efficient variant of `get`.
	///
	/// Get `key` from child storage, placing the value into `value_out` and return the number
	/// of bytes that the entry in storage has beyond the offset or `None` if the storage entry
	/// doesn't exist at all.
	/// If `value_out` length is smaller than the returned length, only `value_out` length bytes
	/// are copied into `value_out`.
	#[version(2)]
	fn read(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		value_offset: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage(&child_info, key)
			.map(|value| {
				let value_offset = value_offset as usize;
				let data = &value[value_offset.min(value.len())..];
				if value_out.len() >= data.len() {
					value_out[..data.len()].copy_from_slice(&data[..]);
				}
				data.len() as u32
			})
			.into()
	}

	/// A convenience wrapper implementing the deprecated `get` host function
	/// functionality through the new interface.
	#[wrapper]
	fn get(storage_key: impl AsRef<[u8]>, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut value_out = vec![0u8; 256];
		let len = read(storage_key.as_ref(), key.as_ref(), &mut value_out[..], 0)?;
		if len as usize > value_out.len() {
			value_out.resize(len as usize, 0);
			read(storage_key.as_ref(), key.as_ref(), &mut value_out[..], 0)?;
		}
		value_out.truncate(len as usize);
		Some(value_out)
	}

	/// Set a child storage value.
	///
	/// Set `key` to `value` in the child storage denoted by `storage_key`.
	fn set(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		self.set_child_storage(&child_info, key.to_vec(), value.to_vec());
	}

	/// Clear a child storage key.
	///
	/// For the default child storage at `storage_key`, clear value at `key`.
	fn clear(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		self.clear_child_storage(&child_info, key);
	}

	/// Clear an entire child storage.
	///
	/// If it exists, the child storage for `storage_key`
	/// is removed.
	fn storage_kill(&mut self, storage_key: PassFatPointerAndRead<&[u8]>) {
		let child_info = ChildInfo::new_default(storage_key);
		let _ = self.kill_child_storage(&child_info, None, None);
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(2)]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> bool {
		let child_info = ChildInfo::new_default(storage_key);
		let r = self.kill_child_storage(&child_info, limit, None);
		r.maybe_cursor.is_none()
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(3)]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		let child_info = ChildInfo::new_default(storage_key);
		self.kill_child_storage(&child_info, limit, None).into()
	}

	/// Clear a child storage key.
	///
	/// See `Storage` module `clear_prefix` documentation.
	#[version(4)]
	#[wrapped]
	fn storage_kill(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassMaybeFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndReadWrite<&mut [u8]>,
		backend: PassPointerToPrimitiveAndWrite<&mut u32>,
		unique: PassPointerToPrimitiveAndWrite<&mut u32>,
		loops: PassPointerToPrimitiveAndWrite<&mut u32>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = self.kill_child_storage(
			&child_info,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		*backend = removal_results.backend;
		*unique = removal_results.unique;
		*loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `storage_kill` host
	/// function.
	#[wrapper]
	fn storage_kill(
		storage_key: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let cursor_len = storage_kill__wrapped(
			storage_key.as_ref(),
			maybe_limit,
			maybe_cursor,
			&mut maybe_cursor_out[..],
			&mut result.backend,
			&mut result.unique,
			&mut result.loops,
		) as usize;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				debug_assert!(cached_cursor_len.is_some());
				debug_assert_eq!(cached_cursor_len.unwrap_or(0) as usize, cursor_len);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}

		result
	}

	/// Check a child storage key.
	///
	/// Check whether the given `key` exists in default child defined at `storage_key`.
	fn exists(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		let child_info = ChildInfo::new_default(storage_key);
		self.exists_child_storage(&child_info, key)
	}

	/// Clear child default key by prefix.
	///
	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
	) {
		let child_info = ChildInfo::new_default(storage_key);
		let _ = self.clear_child_prefix(&child_info, prefix, None, None);
	}

	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// See `Storage` module `clear_prefix` documentation for `limit` usage.
	#[version(2)]
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
		limit: PassFatPointerAndDecode<Option<u32>>,
	) -> AllocateAndReturnByCodec<KillStorageResult> {
		let child_info = ChildInfo::new_default(storage_key);
		self.clear_child_prefix(&child_info, prefix, limit, None).into()
	}

	/// Clear the child storage of each key-value pair where the key starts with the given `prefix`.
	///
	/// See `Storage` module `clear_prefix` documentation.
	#[version(3)]
	#[wrapped]
	fn clear_prefix(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		prefix: PassFatPointerAndRead<&[u8]>,
		maybe_limit: ConvertAndPassAs<Option<u32>, RIIntOption<u32>, i64>,
		maybe_cursor_in: PassMaybeFatPointerAndRead<Option<&[u8]>>,
		maybe_cursor_out: PassFatPointerAndReadWrite<&mut [u8]>,
		backend: PassPointerToPrimitiveAndWrite<&mut u32>,
		unique: PassPointerToPrimitiveAndWrite<&mut u32>,
		loops: PassPointerToPrimitiveAndWrite<&mut u32>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let removal_results = self.clear_child_prefix(
			&child_info,
			prefix,
			maybe_limit,
			maybe_cursor_in.as_ref().map(|x| &x[..]),
		);
		let cursor_out_len = removal_results.maybe_cursor.as_ref().map(|c| c.len()).unwrap_or(0);
		if let Some(cursor_out) = removal_results.maybe_cursor {
			self.store_last_cursor(&cursor_out[..]);
			if maybe_cursor_out.len() >= cursor_out_len {
				maybe_cursor_out[..cursor_out_len].copy_from_slice(&cursor_out[..]);
			}
		}
		*backend = removal_results.backend;
		*unique = removal_results.unique;
		*loops = removal_results.loops;
		cursor_out_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `clear_prefix` host
	/// function.
	#[wrapper]
	fn clear_prefix(
		storage_key: impl AsRef<[u8]>,
		maybe_prefix: impl AsRef<[u8]>,
		maybe_limit: Option<u32>,
		maybe_cursor_in: Option<&[u8]>,
	) -> MultiRemovalResults {
		let mut result = MultiRemovalResults::default();
		let mut maybe_cursor_out = vec![0u8; 1024];
		let cursor_len = clear_prefix__wrapped(
			storage_key.as_ref(),
			maybe_prefix.as_ref(),
			maybe_limit,
			maybe_cursor_in,
			&mut maybe_cursor_out,
			&mut result.backend,
			&mut result.unique,
			&mut result.loops,
		) as usize;
		if cursor_len > 0 {
			if maybe_cursor_out.len() < cursor_len {
				maybe_cursor_out.resize(cursor_len, 0);
				let cached_cursor_len = misc::last_cursor(maybe_cursor_out.as_mut_slice());
				debug_assert!(cached_cursor_len.is_some());
				debug_assert_eq!(cached_cursor_len.unwrap_or(0) as usize, cursor_len);
			}
			maybe_cursor_out.truncate(cursor_len);
			result.maybe_cursor = Some(maybe_cursor_out);
		}
		result
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnFatPointer<Vec<u8>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage_root(&child_info, StateVersion::V0)
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Returns a `Vec<u8>` that holds the SCALE encoded hash.
	#[version(2)]
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnFatPointer<Vec<u8>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.child_storage_root(&child_info, version)
	}

	/// Default child root calculation.
	///
	/// "Commit" all existing operations and compute the resulting child storage root.
	/// The hashing algorithm is defined by the `Block`.
	///
	/// Fills provided output buffer with the SCALE encoded hash.
	#[version(3)]
	#[wrapped]
	fn root(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let root = self.child_storage_root(&child_info, StateVersion::V0);
		if out.len() >= root.len() {
			out[..root.len()].copy_from_slice(&root[..]);
		}
		root.len() as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `root` host
	/// function.
	#[wrapper]
	fn root(storage_key: impl AsRef<[u8]>) -> Vec<u8> {
		let mut root_out = vec![0u8; 256];
		let len = root__wrapped(storage_key.as_ref(), &mut root_out[..]);
		if len as usize > root_out.len() {
			root_out.resize(len as usize, 0);
			root__wrapped(storage_key.as_ref(), &mut root_out[..]);
		}
		root_out.truncate(len as usize);
		root_out
	}

	/// Child storage key iteration.
	///
	/// Get the next key in storage after the given one in lexicographic order in child storage.
	fn next_key(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		let child_info = ChildInfo::new_default(storage_key);
		self.next_child_storage_key(&child_info, key)
	}

	// TODO: Interface changed, reflect in RFC
	/// Child storage key iteration.
	///
	/// Get the next key in storage after the given one in lexicographic order in child storage.
	#[version(2)]
	#[wrapped]
	fn next_key(
		&mut self,
		storage_key: PassFatPointerAndRead<&[u8]>,
		key_in: PassFatPointerAndRead<&[u8]>,
		key_out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> u32 {
		let child_info = ChildInfo::new_default(storage_key);
		let next_key = self.next_child_storage_key(&child_info, key_in);
		let next_key_len = next_key.as_ref().map(|k| k.len()).unwrap_or(0);
		if let Some(next_key) = next_key {
			if key_out.len() >= next_key_len {
				key_out[..next_key_len].copy_from_slice(&next_key[..]);
			}
		}
		next_key_len as u32
	}

	/// A convenience wrapper providing a developer-friendly interface for the `next_key` host
	/// function.
	#[wrapper]
	fn next_key(storage_key: impl AsRef<[u8]>, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut key_out = vec![0u8; 256];
		let len = next_key__wrapped(storage_key.as_ref(), key.as_ref(), &mut key_out[..]);
		if len as usize > key_out.len() {
			key_out.resize(len as usize, 0);
			next_key__wrapped(storage_key.as_ref(), key.as_ref(), &mut key_out[..]);
		}
		key_out.truncate(len as usize);
		if len > 0 {
			Some(key_out)
		} else {
			None
		}
	}
}

/// Interface that provides trie related functionality.
#[runtime_interface]
pub trait Trie {
	/// A trie root formed from the iterated items.
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::Blake2Hasher>::trie_root(input)
	}

	/// A trie root formed from the iterated items.
	#[version(2)]
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::trie_root(input),
		}
	}

	/// A trie root formed from the iterated items.
	#[version(3)]
	#[wrapped]
	fn blake2_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `blake2_256_root`
	/// host function.
	#[wrapper]
	fn blake2_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		blake2_256_root__wrapped(data, state_version, &mut root);
		root
	}
	/// A trie root formed from the enumerated items.
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input)
	}

	/// A trie root formed from the enumerated items.
	#[version(2)]
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::ordered_trie_root(input),
		}
	}

	/// A trie root formed from the enumerated items.
	#[version(3)]
	#[wrapped]
	fn blake2_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::Blake2Hasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::Blake2Hasher>::ordered_trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_256_ordered_root` host function.
	#[wrapper]
	fn blake2_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		blake2_256_ordered_root__wrapped(data, state_version, &mut root);
		root
	}

	/// A trie root formed from the iterated items.
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::KeccakHasher>::trie_root(input)
	}

	/// A trie root formed from the iterated items.
	#[version(2)]
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::trie_root(input),
		}
	}

	/// A trie root formed from the iterated items.
	#[version(3)]
	#[wrapped]
	fn keccak_256_root(
		input: PassFatPointerAndDecode<Vec<(Vec<u8>, Vec<u8>)>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `keccak_256_root`
	/// host function.
	#[wrapper]
	fn keccak_256_root(data: Vec<(Vec<u8>, Vec<u8>)>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		keccak_256_root__wrapped(data, state_version, &mut root);
		root
	}

	/// A trie root formed from the enumerated items.
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
	) -> AllocateAndReturnPointer<H256, 32> {
		LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input)
	}

	/// A trie root formed from the enumerated items.
	#[version(2)]
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
	) -> AllocateAndReturnPointer<H256, 32> {
		match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::ordered_trie_root(input),
		}
	}

	/// A trie root formed from the enumerated items.
	#[version(3)]
	#[wrapped]
	fn keccak_256_ordered_root(
		input: PassFatPointerAndDecode<Vec<Vec<u8>>>,
		version: PassAs<StateVersion, u8>,
		out: PassPointerAndWrite<&mut H256, 32>,
	) {
		let root = match version {
			StateVersion::V0 => LayoutV0::<sp_core::KeccakHasher>::ordered_trie_root(input),
			StateVersion::V1 => LayoutV1::<sp_core::KeccakHasher>::ordered_trie_root(input),
		};
		out.0.copy_from_slice(&root.0);
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_256_ordered_root` host function.
	#[wrapper]
	fn keccak_256_ordered_root(data: Vec<Vec<u8>>, state_version: StateVersion) -> H256 {
		let mut root = H256::default();
		keccak_256_ordered_root__wrapped(data, state_version, &mut root);
		root
	}

	/// Verify trie proof
	fn blake2_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		sp_trie::verify_trie_proof::<LayoutV0<sp_core::Blake2Hasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok()
	}

	/// Verify trie proof
	#[version(2)]
	fn blake2_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
		version: PassAs<StateVersion, u8>,
	) -> bool {
		match version {
			StateVersion::V0 => sp_trie::verify_trie_proof::<
				LayoutV0<sp_core::Blake2Hasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
			StateVersion::V1 => sp_trie::verify_trie_proof::<
				LayoutV1<sp_core::Blake2Hasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
		}
	}

	/// Verify trie proof
	fn keccak_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		sp_trie::verify_trie_proof::<LayoutV0<sp_core::KeccakHasher>, _, _, _>(
			&root,
			proof,
			&[(key, Some(value))],
		)
		.is_ok()
	}

	/// Verify trie proof
	#[version(2)]
	fn keccak_256_verify_proof(
		root: PassPointerAndReadCopy<H256, 32>,
		proof: PassFatPointerAndDecodeSlice<&[Vec<u8>]>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
		version: PassAs<StateVersion, u8>,
	) -> bool {
		match version {
			StateVersion::V0 => sp_trie::verify_trie_proof::<
				LayoutV0<sp_core::KeccakHasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
			StateVersion::V1 => sp_trie::verify_trie_proof::<
				LayoutV1<sp_core::KeccakHasher>,
				_,
				_,
				_,
			>(&root, proof, &[(key, Some(value))])
			.is_ok(),
		}
	}
}

/// Interface that provides miscellaneous functions for communicating between the runtime and the
/// node.
#[runtime_interface]
pub trait Misc {
	// NOTE: We use the target 'runtime' for messages produced by general printing functions,
	// instead of LOG_TARGET.

	/// Print a number.
	fn print_num(val: u64) {
		log::debug!(target: "runtime", "{}", val);
	}

	/// Print any valid `utf8` buffer.
	fn print_utf8(utf8: PassFatPointerAndRead<&[u8]>) {
		if let Ok(data) = core::str::from_utf8(utf8) {
			log::debug!(target: "runtime", "{}", data)
		}
	}

	/// Print any `u8` slice as hex.
	fn print_hex(data: PassFatPointerAndRead<&[u8]>) {
		log::debug!(target: "runtime", "{}", HexDisplay::from(&data));
	}

	/// Extract the runtime version of the given wasm blob by calling `Core_version`.
	///
	/// Returns `None` if calling the function failed for any reason or `Some(Vec<u8>)` where
	/// the `Vec<u8>` holds the SCALE encoded runtime version.
	///
	/// # Performance
	///
	/// This function may be very expensive to call depending on the wasm binary. It may be
	/// relatively cheap if the wasm binary contains version information. In that case,
	/// uncompression of the wasm blob is the dominating factor.
	///
	/// If the wasm binary does not have the version information attached, then a legacy mechanism
	/// may be involved. This means that a runtime call will be performed to query the version.
	///
	/// Calling into the runtime may be incredible expensive and should be approached with care.
	fn runtime_version(
		&mut self,
		wasm: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		use sp_core::traits::ReadRuntimeVersionExt;

		let mut ext = sp_state_machine::BasicExternalities::default();

		match self
			.extension::<ReadRuntimeVersionExt>()
			.expect("No `ReadRuntimeVersionExt` associated for the current context!")
			.read_runtime_version(wasm, &mut ext)
		{
			Ok(v) => Some(v),
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"cannot read version from the given runtime: {}",
					err,
				);
				None
			},
		}
	}

	/// Extract the runtime version of the given wasm blob by calling `Core_version`.
	///
	/// Returns `None` if calling the function failed for any reason. Otherwise, write the
	/// SCALE-encoded version information to the provided output buffer and returns the full length
	/// of the encoded version information. If the output buffer is not large enough, the version
	/// information is truncated.
	///
	/// # Performance
	///
	/// This function may be very expensive to call depending on the wasm binary. It may be
	/// relatively cheap if the wasm binary contains version information. In that case,
	/// uncompression of the wasm blob is the dominating factor.
	///
	/// If the wasm binary does not have the version information attached, then a legacy mechanism
	/// may be involved. This means that a runtime call will be performed to query the version.
	///
	/// Calling into the runtime may be incredible expensive and should be approached with care.
	#[version(2)]
	#[wrapped]
	fn runtime_version(
		&mut self,
		wasm: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		use sp_core::traits::ReadRuntimeVersionExt;

		let mut ext = sp_state_machine::BasicExternalities::default();

		match self
			.extension::<ReadRuntimeVersionExt>()
			.expect("No `ReadRuntimeVersionExt` associated for the current context!")
			.read_runtime_version(wasm, &mut ext)
		{
			Ok(v) => {
				if out.len() >= v.len() {
					out.copy_from_slice(&v[..]);
				}
				Some(v.len() as u32)
			},
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"cannot read version from the given runtime: {}",
					err,
				);
				None
			},
		}
	}

	/// A convenience wrapper providing a developer-friendly interface for the `runtime_version`
	/// host function.
	#[wrapper]
	fn runtime_version(code: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut version = vec![0u8; 1024];
		let maybe_len = runtime_version__wrapped(code.as_ref(), &mut version);
		maybe_len.map(|len| {
			version.truncate(len as usize);
			version
		})
	}

	/// Get the last storage cursor stored by `storage::clear_prefix`,
	/// `default_child_storage::clear_prefix` and `default_child_storage::kill_prefix`.
	///
	/// Returns the length of the cursor or `None` if no cursor is stored.
	fn last_cursor(
		&mut self,
		out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let cursor = self.take_last_cursor()?;

		if out.len() >= cursor.len() {
			out.copy_from_slice(&cursor[..]);
		} else {
			self.store_last_cursor(&cursor[..]);
		}

		Some(cursor.len() as u32)
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

/// Interfaces for working with crypto related types from within the runtime.
#[runtime_interface]
pub trait Crypto {
	/// Returns all `ed25519` public keys for the given key id from the keystore.
	fn ed25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<ed25519::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_public_keys(id)
	}

	/// Returns the number of `ed25519` public keys for the given key type in the keystore.
	fn ed25519_num_public_keys(&mut self, id: PassPointerAndReadCopy<KeyTypeId, 4>) -> u32 {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_public_keys(id)
			.len() as u32
	}

	/// Returns the `ed25519` public key for the given key type and index in the keystore.
	/// Panics if the key index is out of bounds.
	fn ed25519_public_key(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		index: u32,
		out: PassPointerAndWrite<&mut ed25519::Public, 32>,
	) {
		out.0.copy_from_slice(
			self.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ed25519_public_keys(id)
				.get(index as usize)
				.expect("Key index out of bounds!"),
		);
	}

	/// Generate an `ed22519` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn ed25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ed25519::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_generate_new(id, seed)
			.expect("`ed25519_generate` failed")
	}

	/// Generate an `ed22519` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	#[version(2)]
	#[wrapped]
	fn ed25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut ed25519::Public, 32>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ed25519_generate_new(id, seed)
				.expect("`ed25519_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ed25519_generate`
	/// host function.
	#[wrapper]
	fn ed25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ed25519::Public {
		let mut public = ed25519::Public::default();
		ed25519_generate__wrapped(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn ed25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<ed25519::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `ed25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[wrapped]
	fn ed25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut ed25519::Signature, 64>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ed25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ed25519_sign` host
	/// function.
	#[wrapper]
	fn ed25519_sign(
		id: KeyTypeId,
		pub_key: &ed25519::Public,
		message: &[u8],
	) -> Option<ed25519::Signature> {
		let mut signature = ed25519::Signature::default();
		ed25519_sign__wrapped(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Verify `ed25519` signature.
	///
	/// Returns `true` when the verification was successful.
	fn ed25519_verify(
		sig: PassPointerAndRead<&ed25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
	) -> bool {
		// We don't want to force everyone needing to call the function in an externalities context.
		// So, we assume that we should not use dalek when we are not in externalities context.
		// Otherwise, we check if the extension is present.
		if sp_externalities::with_externalities(|mut e| e.extension::<UseDalekExt>().is_some())
			.unwrap_or_default()
		{
			use ed25519_dalek::Verifier;

			let Ok(public_key) = ed25519_dalek::VerifyingKey::from_bytes(&pub_key.0) else {
				return false
			};

			let sig = ed25519_dalek::Signature::from_bytes(&sig.0);

			public_key.verify(msg, &sig).is_ok()
		} else {
			ed25519::Pair::verify(sig, msg, pub_key)
		}
	}

	/// Register a `ed25519` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn ed25519_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&ed25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ed25519::Public, 32>,
	) -> bool {
		let res = ed25519_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Verify `sr25519` signature.
	///
	/// Returns `true` when the verification was successful.
	#[version(2)]
	fn sr25519_verify(
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		sr25519::Pair::verify(sig, msg, pub_key)
	}

	/// Register a `sr25519` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn sr25519_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		let res = sr25519_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Start verification extension.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn start_batch_verify(&mut self) {
		self.register_extension(VerificationExtDeprecated(true))
			.expect("Failed to register required extension: `VerificationExt`");
	}

	/// Finish batch-verification of signatures.
	///
	/// Verify or wait for verification to finish for all signatures which were previously
	/// deferred by `sr25519_verify`/`ed25519_verify`.
	///
	/// Will panic if no `VerificationExt` is registered (`start_batch_verify` was not called).
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn finish_batch_verify(&mut self) -> bool {
		let result = self
			.extension::<VerificationExtDeprecated>()
			.expect("`finish_batch_verify` should only be called after `start_batch_verify`")
			.0;

		self.deregister_extension::<VerificationExtDeprecated>()
			.expect("No verification extension in current context!");

		result
	}

	/// Returns all `sr25519` public keys for the given key id from the keystore.
	fn sr25519_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<sr25519::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_public_keys(id)
	}

	/// Returns the number of `sr25519` public keys for the given key type in the keystore.
	fn sr25519_num_public_keys(&mut self, id: PassPointerAndReadCopy<KeyTypeId, 4>) -> u32 {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_public_keys(id)
			.len() as u32
	}

	/// Returns the `sr25519` public key for the given key type and index in the keystore.
	/// Panics if the key index is out of bounds.
	fn sr25519_public_key(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		index: u32,
		out: PassPointerAndWrite<&mut sr25519::Public, 32>,
	) {
		out.0.copy_from_slice(
			self.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.sr25519_public_keys(id)
				.get(index as usize)
				.expect("Key index out of bounds!"),
		);
	}

	/// Generate an `sr22519` key for the given key type using an optional seed and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn sr25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<sr25519::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_generate_new(id, seed)
			.expect("`sr25519_generate` failed")
	}

	/// Generate an `sr22519` key for the given key type using an optional seed and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	#[version(2)]
	#[wrapped]
	fn sr25519_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut sr25519::Public, 32>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.sr25519_generate_new(id, seed)
				.expect("`sr25519_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `sr25519_generate`
	/// host function.
	#[wrapper]
	fn sr25519_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> sr25519::Public {
		let mut public = sr25519::Public::default();
		sr25519_generate__wrapped(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn sr25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<sr25519::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `sr25519` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[wrapped]
	fn sr25519_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&sr25519::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut sr25519::Signature, 64>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.sr25519_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `sr25519_sign` host
	/// function.
	#[wrapper]
	fn sr25519_sign(
		id: KeyTypeId,
		pub_key: &sr25519::Public,
		message: &[u8],
	) -> Option<sr25519::Signature> {
		let mut signature = sr25519::Signature::default();
		sr25519_sign__wrapped(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Verify an `sr25519` signature.
	///
	/// Returns `true` when the verification in successful regardless of
	/// signature version.
	fn sr25519_verify(
		sig: PassPointerAndRead<&sr25519::Signature, 64>,
		msg: PassFatPointerAndRead<&[u8]>,
		pubkey: PassPointerAndRead<&sr25519::Public, 32>,
	) -> bool {
		sr25519::Pair::verify_deprecated(sig, msg, pubkey)
	}

	/// Returns all `ecdsa` public keys for the given key id from the keystore.
	fn ecdsa_public_keys(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
	) -> AllocateAndReturnByCodec<Vec<ecdsa::Public>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_public_keys(id)
	}

	/// Returns the number of `ecdsa` public keys for the given key type in the keystore.
	fn ecdsa_num_public_keys(&mut self, id: PassPointerAndReadCopy<KeyTypeId, 4>) -> u32 {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_public_keys(id)
			.len() as u32
	}

	/// Returns the `ecdsa` public key for the given key type and index in the keystore.
	/// Panics if the key index is out of bounds.
	fn ecdsa_public_key(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		index: u32,
		out: PassPointerAndWrite<&mut ecdsa::Public, 33>,
	) {
		out.0.copy_from_slice(
			self.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ecdsa_public_keys(id)
				.get(index as usize)
				.expect("Key index out of bounds!"),
		);
	}

	/// Generate an `ecdsa` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	fn ecdsa_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ecdsa::Public, 33> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_generate_new(id, seed)
			.expect("`ecdsa_generate` failed")
	}

	/// Generate an `ecdsa` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Stores the public key in the provided output buffer.
	#[version(2)]
	#[wrapped]
	fn ecdsa_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
		out: PassPointerAndWrite<&mut ecdsa::Public, 33>,
	) {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		out.0.copy_from_slice(
			&self
				.extension::<KeystoreExt>()
				.expect("No `keystore` associated for the current context!")
				.ecdsa_generate_new(id, seed)
				.expect("`ecdsa_generate` failed"),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ecdsa_generate` host
	/// function.
	#[wrapper]
	fn ecdsa_generate(id: KeyTypeId, seed: Option<Vec<u8>>) -> ecdsa::Public {
		let mut public = ecdsa::Public::default();
		ecdsa_generate__wrapped(id, seed, &mut public);
		public
	}

	/// Sign the given `msg` with the `ecdsa` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	fn ecdsa_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<ecdsa::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given `msg` with the `ecdsa` key that corresponds to the given public key and
	/// key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	#[wrapped]
	fn ecdsa_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassFatPointerAndRead<&[u8]>,
		out: PassPointerAndWrite<&mut ecdsa::Signature, 65>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the `ecdsa_sign` host
	/// function.
	#[wrapper]
	fn ecdsa_sign(
		id: KeyTypeId,
		pub_key: &ecdsa::Public,
		message: &[u8],
	) -> Option<ecdsa::Signature> {
		let mut signature = ecdsa::Signature::default();
		ecdsa_sign__wrapped(id, pub_key, message, &mut signature).ok()?;
		Some(signature)
	}

	/// Sign the given a pre-hashed `msg` with the `ecdsa` key that corresponds to the given public
	/// key and key type in the keystore.
	///
	/// Returns the signature.
	fn ecdsa_sign_prehashed(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Option<ecdsa::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign_prehashed(id, pub_key, msg)
			.ok()
			.flatten()
	}

	/// Sign the given a pre-hashed `msg` with the `ecdsa` key that corresponds to the given public
	/// key and key type in the keystore.
	///
	/// Returns the signature.
	#[version(2)]
	fn ecdsa_sign_prehashed(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut ecdsa::Signature, 65>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_sign_prehashed(id, pub_key, msg)
			.ok()
			.flatten()
			.map(|sig| {
				out.0.copy_from_slice(&sig);
			})
			.ok_or(())
	}

	/// Verify `ecdsa` signature.
	///
	/// Returns `true` when the verification was successful.
	/// This version is able to handle, non-standard, overflowing signatures.
	fn ecdsa_verify(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		#[allow(deprecated)]
		ecdsa::Pair::verify_deprecated(sig, msg, pub_key)
	}

	/// Verify `ecdsa` signature.
	///
	/// Returns `true` when the verification was successful.
	#[version(2)]
	fn ecdsa_verify(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		ecdsa::Pair::verify(sig, msg, pub_key)
	}

	/// Verify `ecdsa` signature with pre-hashed `msg`.
	///
	/// Returns `true` when the verification was successful.
	fn ecdsa_verify_prehashed(
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		ecdsa::Pair::verify_prehashed(sig, msg, pub_key)
	}

	/// Register a `ecdsa` signature for batch verification.
	///
	/// Batch verification must be enabled by calling [`start_batch_verify`].
	/// If batch verification is not enabled, the signature will be verified immediately.
	/// To get the result of the batch verification, [`finish_batch_verify`]
	/// needs to be called.
	///
	/// Returns `true` when the verification is either successful or batched.
	///
	/// NOTE: Is tagged with `register_only` to keep the functions around for backwards
	/// compatibility with old runtimes, but it should not be used anymore by new runtimes.
	/// The implementation emulates the old behavior, but isn't doing any batch verification
	/// anymore.
	#[version(1, register_only)]
	fn ecdsa_batch_verify(
		&mut self,
		sig: PassPointerAndRead<&ecdsa::Signature, 65>,
		msg: PassFatPointerAndRead<&[u8]>,
		pub_key: PassPointerAndRead<&ecdsa::Public, 33>,
	) -> bool {
		let res = ecdsa_verify(sig, msg, pub_key);

		if let Some(ext) = self.extension::<VerificationExtDeprecated>() {
			ext.0 &= res;
		}

		res
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	/// This version is able to handle, non-standard, overflowing signatures.
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 64], EcdsaVerifyError>> {
		let rid = libsecp256k1::RecoveryId::parse(
			if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8,
		)
		.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = libsecp256k1::Message::parse(msg);
		let pubkey =
			libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
		let mut res = [0u8; 64];
		res.copy_from_slice(&pubkey.serialize()[1..65]);
		Ok(res)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	#[version(2)]
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 64], EcdsaVerifyError>> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		let mut res = [0u8; 64];
		res.copy_from_slice(&pubkey.serialize_uncompressed()[1..65]);
		Ok(res)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 64-byte pubkey
	/// (doesn't include the 0x04 prefix).
	#[version(3)]
	#[wrapped]
	fn secp256k1_ecdsa_recover(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut Pubkey512, 64>,
	) -> ConvertAndReturnAs<
		Result<(), EcdsaVerifyError>,
		RIIntResult<VoidResult, RIEcdsaVerifyError>,
		i64,
	> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		out.0.copy_from_slice(&pubkey.serialize_uncompressed()[1..]);
		Ok(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `secp256k1_ecdsa_recover` host function.
	#[wrapper]
	fn secp256k1_ecdsa_recover(
		signature: &[u8; 65],
		message: &[u8; 32],
	) -> Result<[u8; 64], EcdsaVerifyError> {
		let mut public = Val512([0u8; 64]);
		secp256k1_ecdsa_recover__wrapped(signature, message, &mut public)?;
		Ok(public.0)
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 33], EcdsaVerifyError>> {
		let rid = libsecp256k1::RecoveryId::parse(
			if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as u8,
		)
		.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = libsecp256k1::Signature::parse_overflowing_slice(&sig[0..64])
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = libsecp256k1::Message::parse(msg);
		let pubkey =
			libsecp256k1::recover(&msg, &sig, &rid).map_err(|_| EcdsaVerifyError::BadSignature)?;
		Ok(pubkey.serialize_compressed())
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	#[version(2)]
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
	) -> AllocateAndReturnByCodec<Result<[u8; 33], EcdsaVerifyError>> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		Ok(pubkey.serialize())
	}

	/// Verify and recover a SECP256k1 ECDSA signature.
	///
	/// - `sig` is passed in RSV format. V should be either `0/1` or `27/28`.
	/// - `msg` is the blake2-256 hash of the message.
	///
	/// Returns `Err` if the signature is bad, otherwise the 33-byte compressed pubkey.
	#[version(3)]
	#[wrapped]
	fn secp256k1_ecdsa_recover_compressed(
		sig: PassPointerAndRead<&[u8; 65], 65>,
		msg: PassPointerAndRead<&[u8; 32], 32>,
		out: PassPointerAndWrite<&mut Pubkey264, 33>,
	) -> ConvertAndReturnAs<
		Result<(), EcdsaVerifyError>,
		RIIntResult<VoidResult, RIEcdsaVerifyError>,
		i64,
	> {
		let rid = RecoveryId::from_i32(if sig[64] > 26 { sig[64] - 27 } else { sig[64] } as i32)
			.map_err(|_| EcdsaVerifyError::BadV)?;
		let sig = RecoverableSignature::from_compact(&sig[..64], rid)
			.map_err(|_| EcdsaVerifyError::BadRS)?;
		let msg = Message::from_digest_slice(msg).expect("Message is 32 bytes; qed");
		#[cfg(feature = "std")]
		let ctx = secp256k1::SECP256K1;
		#[cfg(not(feature = "std"))]
		let ctx = secp256k1::Secp256k1::<secp256k1::VerifyOnly>::gen_new();
		let pubkey = ctx.recover_ecdsa(&msg, &sig).map_err(|_| EcdsaVerifyError::BadSignature)?;
		out.0.copy_from_slice(&pubkey.serialize());
		Ok(())
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `secp256k1_ecdsa_recover_compressed` host function.
	#[wrapper]
	fn secp256k1_ecdsa_recover_compressed(
		signature: &[u8; 65],
		message: &[u8; 32],
	) -> Result<[u8; 33], EcdsaVerifyError> {
		let mut public = Pubkey264([0u8; 33]);
		secp256k1_ecdsa_recover_compressed__wrapped(signature, message, &mut public)?;
		Ok(public.0)
	}

	/// Generate an `bls12-381` key for the given key type using an optional `seed` and
	/// store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bls-experimental")]
	fn bls381_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<bls381::Public, 144> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bls381_generate_new(id, seed)
			.expect("`bls381_generate` failed")
	}

	/// Generate a 'bls12-381' Proof Of Possession for the corresponding public key.
	///
	/// Returns the Proof Of Possession as an option of the ['bls381::Signature'] type
	/// or 'None' if an error occurs.
	#[cfg(feature = "bls-experimental")]
	fn bls381_generate_proof_of_possession(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&bls381::Public, 144>,
		owner: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bls381::ProofOfPossession>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bls381_generate_proof_of_possession(id, pub_key, owner)
			.ok()
			.flatten()
	}

	/// Generate combination `ecdsa & bls12-381` key for the given key type using an optional `seed`
	/// and store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bls-experimental")]
	fn ecdsa_bls381_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<ecdsa_bls381::Public, { 144 + 33 }> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.ecdsa_bls381_generate_new(id, seed)
			.expect("`ecdsa_bls381_generate` failed")
	}

	/// Generate a `bandersnatch` key pair for the given key type using an optional
	/// `seed` and store it in the keystore.
	///
	/// The `seed` needs to be a valid utf8.
	///
	/// Returns the public key.
	#[cfg(feature = "bandersnatch-experimental")]
	fn bandersnatch_generate(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		seed: PassFatPointerAndDecode<Option<Vec<u8>>>,
	) -> AllocateAndReturnPointer<bandersnatch::Public, 32> {
		let seed = seed.as_ref().map(|s| core::str::from_utf8(s).expect("Seed is valid utf8!"));
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bandersnatch_generate_new(id, seed)
			.expect("`bandernatch_generate` failed")
	}

	/// Sign the given `msg` with the `bandersnatch` key that corresponds to the given public key
	/// and key type in the keystore.
	///
	/// Returns the signature or `None` if an error occurred.
	#[cfg(feature = "bandersnatch-experimental")]
	fn bandersnatch_sign(
		&mut self,
		id: PassPointerAndReadCopy<KeyTypeId, 4>,
		pub_key: PassPointerAndRead<&bandersnatch::Public, 32>,
		msg: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<bandersnatch::Signature>> {
		self.extension::<KeystoreExt>()
			.expect("No `keystore` associated for the current context!")
			.bandersnatch_sign(id, pub_key, msg)
			.ok()
			.flatten()
	}
}

/// Interface that provides functions for hashing with different algorithms.
#[runtime_interface]
pub trait Hashing {
	/// Conduct a 256-bit Keccak hash.
	fn keccak_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::keccak_256(data)
	}

	/// Conduct a 256-bit Keccak hash.
	#[version(2)]
	#[wrapped]
	fn keccak_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::keccak_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_256` host function.
	#[wrapper]
	fn keccak_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		keccak_256__wrapped(data, &mut out);
		out
	}

	/// Conduct a 512-bit Keccak hash.
	fn keccak_512(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 64], 64> {
		sp_crypto_hashing::keccak_512(data)
	}

	/// Conduct a 512-bit Keccak hash.
	#[version(2)]
	#[wrapped]
	fn keccak_512(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut Val512, 64>) {
		out.0.copy_from_slice(&sp_crypto_hashing::keccak_512(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `keccak_512` host function.
	#[wrapper]
	fn keccak_512(data: &[u8]) -> [u8; 64] {
		let mut out = Val512::default();
		keccak_512__wrapped(data, &mut out);
		out.0
	}

	/// Conduct a 256-bit Sha2 hash.
	fn sha2_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::sha2_256(data)
	}

	/// Conduct a 256-bit Sha2 hash.
	#[version(2)]
	#[wrapped]
	fn sha2_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::sha2_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `sha2_256` host function.
	#[wrapper]
	fn sha2_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		sha2_256__wrapped(data, &mut out);
		out
	}

	/// Conduct a 128-bit Blake2 hash.
	fn blake2_128(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 16], 16> {
		sp_crypto_hashing::blake2_128(data)
	}

	/// Conduct a 128-bit Blake2 hash.
	#[version(2)]
	#[wrapped]
	fn blake2_128(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 16], 16>) {
		out.copy_from_slice(&sp_crypto_hashing::blake2_128(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_128` host function.
	#[wrapper]
	fn blake2_128(data: &[u8]) -> [u8; 16] {
		let mut out = [0u8; 16];
		blake2_128__wrapped(data, &mut out);
		out
	}

	/// Conduct a 256-bit Blake2 hash.
	fn blake2_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::blake2_256(data)
	}

	/// Conduct a 256-bit Blake2 hash.
	#[version(2)]
	#[wrapped]
	fn blake2_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::blake2_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `blake2_256` host function.
	#[wrapper]
	fn blake2_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		blake2_256__wrapped(data, &mut out);
		out
	}

	/// Conduct four XX hashes to give a 256-bit result.
	fn twox_256(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 32], 32> {
		sp_crypto_hashing::twox_256(data)
	}

	/// Conduct four XX hashes to give a 256-bit result.
	#[version(2)]
	#[wrapped]
	fn twox_256(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_256(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_256` host function.
	#[wrapper]
	fn twox_256(data: &[u8]) -> [u8; 32] {
		let mut out = [0u8; 32];
		twox_256__wrapped(data, &mut out);
		out
	}

	/// Conduct two XX hashes to give a 128-bit result.
	fn twox_128(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 16], 16> {
		sp_crypto_hashing::twox_128(data)
	}

	/// Conduct two XX hashes to give a 128-bit result.
	#[version(2)]
	#[wrapped]
	fn twox_128(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 16], 16>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_128(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_128` host function.
	#[wrapper]
	fn twox_128(data: &[u8]) -> [u8; 16] {
		let mut out = [0u8; 16];
		twox_128__wrapped(data, &mut out);
		out
	}

	/// Conduct two XX hashes to give a 64-bit result.
	fn twox_64(data: PassFatPointerAndRead<&[u8]>) -> AllocateAndReturnPointer<[u8; 8], 8> {
		sp_crypto_hashing::twox_64(data)
	}

	/// Conduct two XX hashes to give a 64-bit result.
	#[version(2)]
	#[wrapped]
	fn twox_64(data: PassFatPointerAndRead<&[u8]>, out: PassPointerAndWrite<&mut [u8; 8], 8>) {
		out.copy_from_slice(&sp_crypto_hashing::twox_64(data));
	}

	/// A convenience wrapper providing a developer-friendly interface for the
	/// `twox_64` host function.
	#[wrapper]
	fn twox_64(data: &[u8]) -> [u8; 8] {
		let mut out = [0u8; 8];
		twox_64__wrapped(data, &mut out);
		out
	}
}

/// Interface that provides transaction indexing API.
#[runtime_interface]
pub trait TransactionIndex {
	/// Add transaction index. Returns indexed content hash.
	fn index(
		&mut self,
		extrinsic: u32,
		size: u32,
		context_hash: PassPointerAndReadCopy<[u8; 32], 32>,
	) {
		self.storage_index_transaction(extrinsic, &context_hash, size);
	}

	/// Conduct a 512-bit Keccak hash.
	fn renew(&mut self, extrinsic: u32, context_hash: PassPointerAndReadCopy<[u8; 32], 32>) {
		self.storage_renew_transaction_index(extrinsic, &context_hash);
	}
}

/// Interface that provides functions to access the Offchain DB.
#[runtime_interface]
pub trait OffchainIndex {
	/// Write a key value pair to the Offchain DB database in a buffered fashion.
	fn set(&mut self, key: PassFatPointerAndRead<&[u8]>, value: PassFatPointerAndRead<&[u8]>) {
		self.set_offchain_storage(key, Some(value));
	}

	/// Remove a key and its associated value from the Offchain DB.
	fn clear(&mut self, key: PassFatPointerAndRead<&[u8]>) {
		self.set_offchain_storage(key, None);
	}
}

#[cfg(not(substrate_runtime))]
sp_externalities::decl_extension! {
	/// Deprecated verification context.
	///
	/// Stores the combined result of all verifications that are done in the same context.
	struct VerificationExtDeprecated(bool);
}

/// Interface that provides functions to access the offchain functionality.
///
/// These functions are being made available to the runtime and are called by the runtime.
#[runtime_interface]
pub trait Offchain {
	/// Returns if the local node is a potential validator.
	///
	/// Even if this function returns `true`, it does not mean that any keys are configured
	/// and that the validator is registered in the chain.
	fn is_validator(&mut self) -> bool {
		self.extension::<OffchainWorkerExt>()
			.expect("is_validator can be called only in the offchain worker context")
			.is_validator()
	}

	/// Submit an encoded transaction to the pool.
	///
	/// The transaction will end up in the pool.
	fn submit_transaction(
		&mut self,
		data: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<(), ()>> {
		self.extension::<TransactionPoolExt>()
			.expect(
				"submit_transaction can be called only in the offchain call context with
				TransactionPool capabilities enabled",
			)
			.submit_transaction(data)
	}

	/// Submit an encoded transaction to the pool.
	///
	/// The transaction will end up in the pool.
	#[version(2)]
	fn submit_transaction(
		&mut self,
		data: PassFatPointerAndRead<Vec<u8>>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<TransactionPoolExt>()
			.expect(
				"submit_transaction can be called only in the offchain call context with
				TransactionPool capabilities enabled",
			)
			.submit_transaction(data)
	}

	/// Returns information about the local node's network state.
	#[version(1, register_only)]
	fn network_state(&mut self) -> AllocateAndReturnByCodec<Result<OpaqueNetworkState, ()>> {
		self.extension::<OffchainWorkerExt>()
			.expect("network_state can be called only in the offchain worker context")
			.network_state()
	}

	/// Returns the peer ID of the local node.
	fn network_peer_id(
		&mut self,
		out: PassPointerAndWrite<&mut NetworkPeerId, 38>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		let peer_id = self
			.extension::<OffchainWorkerExt>()
			.expect("network_state can be called only in the offchain worker context")
			.network_state()?
			.peer_id
			.0;

		out.0.copy_from_slice(&peer_id);
		Ok(())
	}

	/// Returns current UNIX timestamp (in millis)
	fn timestamp(&mut self) -> ReturnAs<Timestamp, u64> {
		self.extension::<OffchainWorkerExt>()
			.expect("timestamp can be called only in the offchain worker context")
			.timestamp()
	}

	/// Pause the execution until `deadline` is reached.
	fn sleep_until(&mut self, deadline: PassAs<Timestamp, u64>) {
		self.extension::<OffchainWorkerExt>()
			.expect("sleep_until can be called only in the offchain worker context")
			.sleep_until(deadline)
	}

	/// Returns a random seed.
	///
	/// This is a truly random, non-deterministic seed generated by host environment.
	/// Obviously fine in the off-chain worker context.
	fn random_seed(&mut self) -> AllocateAndReturnPointer<[u8; 32], 32> {
		self.extension::<OffchainWorkerExt>()
			.expect("random_seed can be called only in the offchain worker context")
			.random_seed()
	}

	/// Writes a random seed to the provided output buffer.
	///
	/// This is a truly random, non-deterministic seed generated by host environment.
	/// Obviously fine in the off-chain worker context.
	#[version(2)]
	#[wrapped]
	fn random_seed(&mut self, out: PassPointerAndWrite<&mut [u8; 32], 32>) {
		out.copy_from_slice(
			&self
				.extension::<OffchainWorkerExt>()
				.expect("random_seed can be called only in the offchain worker context")
				.random_seed(),
		);
	}

	/// A convenience wrapper providing a developer-friendly interface for the `random_seed` host
	/// function.
	#[wrapper]
	fn random_seed() -> [u8; 32] {
		let mut seed = [0u8; 32];
		random_seed__wrapped(&mut seed);
		seed
	}

	/// Sets a value in the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_set(
		&mut self,
		kind: PassAs<StorageKind, u32>,
		key: PassFatPointerAndRead<&[u8]>,
		value: PassFatPointerAndRead<&[u8]>,
	) {
		self.extension::<OffchainDbExt>()
			.expect(
				"local_storage_set can be called only in the offchain call context with
				OffchainDb extension",
			)
			.local_storage_set(kind, key, value)
	}

	/// Remove a value from the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_clear(
		&mut self,
		kind: PassAs<StorageKind, u32>,
		key: PassFatPointerAndRead<&[u8]>,
	) {
		self.extension::<OffchainDbExt>()
			.expect(
				"local_storage_clear can be called only in the offchain call context with
				OffchainDb extension",
			)
			.local_storage_clear(kind, key)
	}

	/// Sets a value in the local storage if it matches current value.
	///
	/// Since multiple offchain workers may be running concurrently, to prevent
	/// data races use CAS to coordinate between them.
	///
	/// Returns `true` if the value has been set, `false` otherwise.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_compare_and_set(
		&mut self,
		kind: PassAs<StorageKind, u32>,
		key: PassFatPointerAndRead<&[u8]>,
		old_value: PassFatPointerAndDecode<Option<Vec<u8>>>,
		new_value: PassFatPointerAndRead<&[u8]>,
	) -> bool {
		self.extension::<OffchainDbExt>()
			.expect(
				"local_storage_compare_and_set can be called only in the offchain call context
				with OffchainDb extension",
			)
			.local_storage_compare_and_set(kind, key, old_value.as_deref(), new_value)
	}

	/// Gets a value from the local storage.
	///
	/// If the value does not exist in the storage `None` will be returned.
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_get(
		&mut self,
		kind: PassAs<StorageKind, u32>,
		key: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		self.extension::<OffchainDbExt>()
			.expect(
				"local_storage_get can be called only in the offchain call context with
				OffchainDb extension",
			)
			.local_storage_get(kind, key)
	}

	/// Reads a value from the local storage.
	///
	/// If the value does not exist in the storage `None` will be returned.
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_read(
		&mut self,
		kind: PassAs<StorageKind, u32>,
		key: PassFatPointerAndRead<&[u8]>,
		value_out: PassFatPointerAndReadWrite<&mut [u8]>,
		offset: u32,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		self.extension::<OffchainDbExt>()
			.expect(
				"local_storage_get can be called only in the offchain call context with
				OffchainDb extension",
			)
			.local_storage_get(kind, key)
			.map(|v| {
				let value_offset = offset as usize;
				let data = &v[value_offset.min(v.len())..];
				let written = core::cmp::min(data.len(), value_out.len());
				value_out[..written].copy_from_slice(&data[..written]);
				data.len() as u32
			})
	}

	/// Initiates a http request given HTTP verb and the URL.
	///
	/// Meta is a future-reserved field containing additional, parity-scale-codec encoded
	/// parameters. Returns the id of newly started request.
	fn http_request_start(
		&mut self,
		method: PassFatPointerAndRead<&str>,
		uri: PassFatPointerAndRead<&str>,
		meta: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Result<HttpRequestId, ()>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_start can be called only in the offchain worker context")
			.http_request_start(method, uri, meta)
	}

	/// Initiates a http request given HTTP verb and the URL.
	///
	/// Meta is a future-reserved field containing additional, parity-scale-codec encoded
	/// parameters. Returns the id of newly started request.
	#[version(2)]
	fn http_request_start(
		&mut self,
		method: PassFatPointerAndRead<&str>,
		uri: PassFatPointerAndRead<&str>,
		meta: PassFatPointerAndRead<&[u8]>,
	) -> ConvertAndReturnAs<Result<HttpRequestId, ()>, RIIntResult<u16, VoidError>, i64> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_start can be called only in the offchain worker context")
			.http_request_start(method, uri, meta)
			.into()
	}

	/// Append header to the request.
	fn http_request_add_header(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		name: PassFatPointerAndRead<&str>,
		value: PassFatPointerAndRead<&str>,
	) -> AllocateAndReturnByCodec<Result<(), ()>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_add_header can be called only in the offchain worker context")
			.http_request_add_header(request_id, name, value)
	}

	/// Append header to the request.
	#[version(2)]
	fn http_request_add_header(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		name: PassFatPointerAndRead<&str>,
		value: PassFatPointerAndRead<&str>,
	) -> ConvertAndReturnAs<Result<(), ()>, RIIntResult<VoidResult, VoidError>, i64> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_add_header can be called only in the offchain worker context")
			.http_request_add_header(request_id, name, value)
	}

	/// Write a chunk of request body.
	///
	/// Writing an empty chunks finalizes the request.
	/// Passing `None` as deadline blocks forever.
	///
	/// Returns an error in case deadline is reached or the chunk couldn't be written.
	fn http_request_write_body(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		chunk: PassFatPointerAndRead<&[u8]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
	) -> AllocateAndReturnByCodec<Result<(), HttpError>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_write_body can be called only in the offchain worker context")
			.http_request_write_body(request_id, chunk, deadline)
	}

	/// Write a chunk of request body.
	///
	/// Writing an empty chunks finalizes the request.
	/// Passing `None` as deadline blocks forever.
	///
	/// Returns an error in case deadline is reached or the chunk couldn't be written.
	#[version(2)]
	fn http_request_write_body(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		chunk: PassFatPointerAndRead<&[u8]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
	) -> ConvertAndReturnAs<Result<(), HttpError>, RIIntResult<VoidResult, RIHttpError>, i64> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_request_write_body can be called only in the offchain worker context")
			.http_request_write_body(request_id, chunk, deadline)
	}

	/// Block and wait for the responses for given requests.
	///
	/// Returns a vector of request statuses (the len is the same as ids).
	/// Note that if deadline is not provided the method will block indefinitely,
	/// otherwise unready responses will produce `DeadlineReached` status.
	///
	/// Passing `None` as deadline blocks forever.
	fn http_response_wait(
		&mut self,
		ids: PassFatPointerAndDecodeSlice<&[HttpRequestId]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
	) -> AllocateAndReturnByCodec<Vec<HttpRequestStatus>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_response_wait can be called only in the offchain worker context")
			.http_response_wait(ids, deadline)
	}

	/// TODO: Original error codes are used as they do not contradict anything. That should be
	/// either reflected in RFC-145 or changed here.
	///
	/// Block and wait for the responses for given requests.
	///
	/// Fills the provided output buffer with request statuses. The length of the provided buffer
	/// should be no less than the length of the input ids.
	///
	/// Note that if deadline is not provided the method will block indefinitely,
	/// otherwise unready responses will produce `DeadlineReached` status.
	///
	/// Passing `None` as deadline blocks forever.
	#[version(2)]
	#[wrapped]
	fn http_response_wait(
		&mut self,
		ids: PassFatPointerAndDecodeSlice<&[HttpRequestId]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
		out: PassFatPointerAndReadWrite<&mut [u32]>,
	) {
		assert_eq!(out.len(), ids.len());
		let statuses = self
			.extension::<OffchainWorkerExt>()
			.expect("http_response_wait can be called only in the offchain worker context")
			.http_response_wait(ids, deadline);
		statuses.into_iter().zip(out).for_each(|(status, out)| {
			*out = status.into();
		});
	}

	/// A convenience wrapper providing a developer-friendly interface for the `http_response_wait`
	/// host function.
	#[wrapper]
	fn http_response_wait(
		ids: &[HttpRequestId],
		deadline: Option<Timestamp>,
	) -> Vec<HttpRequestStatus> {
		let mut statuses = vec![0u32; ids.len()];
		http_response_wait__wrapped(&ids, deadline.into(), &mut statuses[..]);
		statuses
			.into_iter()
			.map(|s| HttpRequestStatus::try_from(s).unwrap_or(HttpRequestStatus::Invalid))
			.collect::<Vec<_>>()
	}

	/// Read all response headers.
	///
	/// Returns a vector of pairs `(HeaderKey, HeaderValue)`.
	/// NOTE: response headers have to be read before response body.
	fn http_response_headers(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
	) -> AllocateAndReturnByCodec<Vec<(Vec<u8>, Vec<u8>)>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_response_headers can be called only in the offchain worker context")
			.http_response_headers(request_id)
	}

	/// Read the name of the header at the given index into the provided output buffer.
	///
	/// Returns the full length of the header name. If the output buffer is not large enough,
	/// the header name is truncated.
	///
	/// Returns `None` if the index is out of bounds.
	fn http_response_header_name(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		header_index: u32,
		out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let headers = self
			.extension::<OffchainWorkerExt>()
			.expect("http_response_header_name can be called only in the offchain worker context")
			.http_response_headers(request_id);
		let res = &headers.get(header_index as usize)?.0;
		if out.len() >= res.len() {
			out.copy_from_slice(&res[..]);
		}
		Some(res.len() as u32)
	}

	/// Read the value of the header at the given index into the provided output buffer.
	///
	/// Returns the full length of the header value. If the output buffer is not large enough,
	/// the header value is truncated.
	///
	/// Returns `None` if the index is out of bounds.
	fn http_response_header_value(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		header_index: u32,
		out: PassFatPointerAndReadWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let headers = self
			.extension::<OffchainWorkerExt>()
			.expect("http_response_header_value can be called only in the offchain worker context")
			.http_response_headers(request_id);
		let res = &headers.get(header_index as usize)?.1;
		if out.len() >= res.len() {
			out.copy_from_slice(&res[..]);
		}
		Some(res.len() as u32)
	}

	/// Read a chunk of body response to given buffer.
	///
	/// Returns the number of bytes written or an error in case a deadline
	/// is reached or server closed the connection.
	/// If `0` is returned it means that the response has been fully consumed
	/// and the `request_id` is now invalid.
	/// NOTE: this implies that response headers must be read before draining the body.
	/// Passing `None` as a deadline blocks forever.
	fn http_response_read_body(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		buffer: PassFatPointerAndReadWrite<&mut [u8]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
	) -> AllocateAndReturnByCodec<Result<u32, HttpError>> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_response_read_body can be called only in the offchain worker context")
			.http_response_read_body(request_id, buffer, deadline)
			.map(|r| r as u32)
	}

	/// Read a chunk of body response to given buffer.
	///
	/// Returns the number of bytes written or an error in case a deadline
	/// is reached or server closed the connection.
	/// If `0` is returned it means that the response has been fully consumed
	/// and the `request_id` is now invalid.
	/// NOTE: this implies that response headers must be read before draining the body.
	/// Passing `None` as a deadline blocks forever.
	#[version(2)]
	fn http_response_read_body(
		&mut self,
		request_id: PassAs<HttpRequestId, u16>,
		buffer: PassFatPointerAndReadWrite<&mut [u8]>,
		deadline: PassFatPointerAndDecode<Option<Timestamp>>,
	) -> ConvertAndReturnAs<Result<u32, HttpError>, RIIntResult<u32, RIHttpError>, i64> {
		self.extension::<OffchainWorkerExt>()
			.expect("http_response_read_body can be called only in the offchain worker context")
			.http_response_read_body(request_id, buffer, deadline)
			.map(|r| r as u32)
	}

	/// Set the authorized nodes and authorized_only flag.
	fn set_authorized_nodes(
		&mut self,
		nodes: PassFatPointerAndDecode<Vec<OpaquePeerId>>,
		authorized_only: bool,
	) {
		self.extension::<OffchainWorkerExt>()
			.expect("set_authorized_nodes can be called only in the offchain worker context")
			.set_authorized_nodes(nodes, authorized_only)
	}
}

/// Wasm only interface that provides functions for calling into the allocator.
#[runtime_interface(wasm_only)]
pub trait Allocator {
	/// Malloc the given number of bytes and return the pointer to the allocated memory location.
	fn malloc(&mut self, size: u32) -> Pointer<u8> {
		self.allocate_memory(size).expect("Failed to allocate memory")
	}

	/// Free the given pointer.
	fn free(&mut self, ptr: Pointer<u8>) {
		self.deallocate_memory(ptr).expect("Failed to deallocate memory")
	}
}

/// WASM-only interface which allows for aborting the execution in case
/// of an unrecoverable error.
#[runtime_interface(wasm_only)]
pub trait PanicHandler {
	/// Aborts the current execution with the given error message.
	#[trap_on_return]
	fn abort_on_panic(&mut self, message: PassFatPointerAndRead<&str>) {
		self.register_panic_error_message(message);
	}
}

/// Interface that provides functions for logging from within the runtime.
#[runtime_interface]
pub trait Logging {
	/// Request to print a log message on the host.
	///
	/// Note that this will be only displayed if the host is enabled to display log messages with
	/// given level and target.
	///
	/// Instead of using directly, prefer setting up `RuntimeLogger` and using `log` macros.
	fn log(
		level: PassAs<RuntimeInterfaceLogLevel, u8>,
		target: PassFatPointerAndRead<&str>,
		message: PassFatPointerAndRead<&[u8]>,
	) {
		if let Ok(message) = core::str::from_utf8(message) {
			log::log!(target: target, log::Level::from(level), "{}", message)
		}
	}

	/// Returns the max log level used by the host.
	fn max_level() -> ReturnAs<LogLevelFilter, u8> {
		log::max_level().into()
	}
}

/// Interface to provide tracing facilities for wasm. Modelled after tokios `tracing`-crate
/// interfaces. See `sp-tracing` for more information.
#[runtime_interface(wasm_only, no_tracing)]
pub trait WasmTracing {
	/// Whether the span described in `WasmMetadata` should be traced wasm-side
	/// On the host converts into a static Metadata and checks against the global `tracing`
	/// dispatcher.
	///
	/// When returning false the calling code should skip any tracing-related execution. In general
	/// within the same block execution this is not expected to change and it doesn't have to be
	/// checked more than once per metadata. This exists for optimisation purposes but is still not
	/// cheap as it will jump the wasm-native-barrier every time it is called. So an implementation
	/// might chose to cache the result for the execution of the entire block.
	fn enabled(&mut self, metadata: PassFatPointerAndDecode<sp_tracing::WasmMetadata>) -> bool {
		let metadata: &tracing_core::metadata::Metadata<'static> = (&metadata).into();
		tracing::dispatcher::get_default(|d| d.enabled(metadata))
	}

	/// Open a new span with the given attributes. Return the u64 Id of the span.
	///
	/// On the native side this goes through the default `tracing` dispatcher to register the span
	/// and then calls `clone_span` with the ID to signal that we are keeping it around on the wasm-
	/// side even after the local span is dropped. The resulting ID is then handed over to the wasm-
	/// side.
	fn enter_span(
		&mut self,
		span: PassFatPointerAndDecode<sp_tracing::WasmEntryAttributes>,
	) -> u64 {
		let span: tracing::Span = span.into();
		match span.id() {
			Some(id) => tracing::dispatcher::get_default(|d| {
				// inform dispatch that we'll keep the ID around
				// then enter it immediately
				let final_id = d.clone_span(&id);
				d.enter(&final_id);
				final_id.into_u64()
			}),
			_ => 0,
		}
	}

	/// Emit the given event to the global tracer on the native side
	fn event(&mut self, event: PassFatPointerAndDecode<sp_tracing::WasmEntryAttributes>) {
		event.emit();
	}

	/// Signal that a given span-id has been exited. On native, this directly
	/// proxies the span to the global dispatcher.
	fn exit(&mut self, span: u64) {
		tracing::dispatcher::get_default(|d| {
			let id = tracing_core::span::Id::from_u64(span);
			d.exit(&id);
		});
	}
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
		logging::log(RuntimeInterfaceLogLevel::Error, "runtime", message.as_bytes());
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
			RuntimeInterfaceLogLevel::Error,
			"runtime",
			b"Runtime memory exhausted. Aborting",
		);
		unreachable();
	}
}

/// Input data handling functions
#[runtime_interface]
pub trait Input {
	/// Read input data into the provided buffer.
	fn read(_buffer: PassFatPointerAndWriteInputData<&mut [u8]>) {
		// The body has been deliberately left empty. The logic is handled by a specific marshalling
		// strategy (see [`PassFatPointerAndWriteInputData`]).
	}
}

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
			let mut v = [0u8; 4];
			assert_eq!(storage::read(b":test", &mut v[..], 0).unwrap(), value.len() as u32);
			assert_eq!(v, [11u8, 0, 0, 0]);
			let mut w = [0u8; 11];
			assert_eq!(storage::read(b":test", &mut w[..], 4).unwrap(), value.len() as u32 - 4);
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

	fn zero_ed_pub() -> ed25519::Public {
		[0u8; 32].unchecked_into()
	}

	fn zero_ed_sig() -> ed25519::Signature {
		ed25519::Signature::from_raw([0u8; 64])
	}

	#[test]
	fn use_dalek_ext_works() {
		let mut ext = BasicExternalities::default();
		ext.register_extension(UseDalekExt::default());

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
		ext.register_extension(UseDalekExt::default());

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
