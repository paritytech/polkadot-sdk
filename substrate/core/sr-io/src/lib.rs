// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! This is part of the Substrate runtime.

#![warn(missing_docs)]

#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(not(feature = "std"), feature(lang_items))]
#![cfg_attr(not(feature = "std"), feature(alloc_error_handler))]
#![cfg_attr(not(feature = "std"), feature(core_intrinsics))]

#![cfg_attr(feature = "std", doc = "Substrate runtime standard library as compiled when linked with Rust's standard library.")]
#![cfg_attr(not(feature = "std"), doc = "Substrate's runtime standard library as compiled without Rust's standard library.")]

use hash_db::Hasher;
use rstd::vec::Vec;

#[doc(hidden)]
pub use codec;

pub use primitives::Blake2Hasher;
use primitives::offchain::{
	Timestamp,
	HttpRequestId, HttpRequestStatus, HttpError,
	CryptoKind, CryptoKeyId,
	StorageKind,
};

/// Error verifying ECDSA signature
pub enum EcdsaVerifyError {
	/// Incorrect value of R or S
	BadRS,
	/// Incorrect value of V
	BadV,
	/// Invalid signature
	BadSignature,
}

pub mod offchain;

/// Trait for things which can be printed.
pub trait Printable {
	/// Print the object.
	fn print(self);
}

/// Converts a public trait definition into a private trait and set of public functions
/// that assume the trait is implemented for `()` for ease of calling.
macro_rules! export_api {
	(
		$( #[$trait_attr:meta] )*
		pub(crate) trait $trait_name:ident {
			$(
				$( #[$attr:meta] )*
				fn $name:ident
					$(< $( $g_name:ident $( : $g_ty:path )? ),+ >)?
					( $( $arg:ident : $arg_ty:ty ),* )
					$( -> $ret:ty )?
					$( where $( $w_name:path : $w_ty:path ),+ )?;
			)*
		}
	) => {
		$( #[$trait_attr] )*
		pub(crate) trait $trait_name {
			$(
				$( #[$attr] )*
				fn $name $(< $( $g_name $( : $g_ty )? ),+ >)? ( $($arg : $arg_ty ),* ) $( -> $ret )?
				$( where $( $w_name : $w_ty ),+ )?;
			)*
		}

		$(
			$( #[$attr] )*
			pub fn $name $(< $( $g_name $( : $g_ty )? ),+ >)? ( $($arg : $arg_ty ),* ) $( -> $ret )?
				$( where $( $w_name : $w_ty ),+ )?
			{
				#[allow(deprecated)]
				<()>:: $name $(::< $( $g_name ),+ > )?  ( $( $arg ),* )
			}
		)*
	}
}

export_api! {
	pub(crate) trait StorageApi {
		/// Get `key` from storage and return a `Vec`, empty if there's a problem.
		fn storage(key: &[u8]) -> Option<Vec<u8>>;

		/// Get `key` from child storage and return a `Vec`, empty if there's a problem.
		fn child_storage(storage_key: &[u8], key: &[u8]) -> Option<Vec<u8>>;

		/// Get `key` from storage, placing the value into `value_out` (as much of it as possible) and return
		/// the number of bytes that the entry in storage had beyond the offset or None if the storage entry
		/// doesn't exist at all. Note that if the buffer is smaller than the storage entry length, the returned
		/// number of bytes is not equal to the number of bytes written to the `value_out`.
		fn read_storage(key: &[u8], value_out: &mut [u8], value_offset: usize) -> Option<usize>;

		/// Get `key` from child storage, placing the value into `value_out` (as much of it as possible) and return
		/// the number of bytes that the entry in storage had beyond the offset or None if the storage entry
		/// doesn't exist at all. Note that if the buffer is smaller than the storage entry length, the returned
		/// number of bytes is not equal to the number of bytes written to the `value_out`.
		fn read_child_storage(storage_key: &[u8], key: &[u8], value_out: &mut [u8], value_offset: usize) -> Option<usize>;

		/// Set the storage of some particular key to Some value.
		fn set_storage(key: &[u8], value: &[u8]);

		/// Set the child storage of some particular key to Some value.
		fn set_child_storage(storage_key: &[u8], key: &[u8], value: &[u8]);

		/// Clear the storage of a key.
		fn clear_storage(key: &[u8]);

		/// Clear the storage of a key.
		fn clear_child_storage(storage_key: &[u8], key: &[u8]);

		/// Clear an entire child storage.
		fn kill_child_storage(storage_key: &[u8]);

		/// Check whether a given `key` exists in storage.
		fn exists_storage(key: &[u8]) -> bool;

		/// Check whether a given `key` exists in storage.
		fn exists_child_storage(storage_key: &[u8], key: &[u8]) -> bool;

		/// Clear the storage entries with a key that starts with the given prefix.
		fn clear_prefix(prefix: &[u8]);

		/// "Commit" all existing operations and compute the resultant storage root.
		fn storage_root() -> [u8; 32];

		/// "Commit" all existing operations and compute the resultant child storage root.
		fn child_storage_root(storage_key: &[u8]) -> Vec<u8>;

		/// "Commit" all existing operations and get the resultant storage change root.
		fn storage_changes_root(parent_hash: [u8; 32]) -> Option<[u8; 32]>;

		/// A trie root formed from the enumerated items.
		/// TODO [#2382] remove (just use `ordered_trie_root` (NOTE currently not implemented for without_std))
		fn enumerated_trie_root<H>(input: &[&[u8]]) -> H::Out
		where
			H: Hasher,
			H: self::imp::HasherBounds,
			H::Out: Ord
		;

		/// A trie root formed from the iterated items.
		fn trie_root<H, I, A, B>(input: I) -> H::Out
		where
			I: IntoIterator<Item = (A, B)>,
			A: AsRef<[u8]>,
			A: Ord,
			B: AsRef<[u8]>,
			H: Hasher,
			H: self::imp::HasherBounds,
			H::Out: Ord
		;

		/// A trie root formed from the enumerated items.
		fn ordered_trie_root<H, I, A>(input: I) -> H::Out
		where
			I: IntoIterator<Item = A>,
			A: AsRef<[u8]>,
			H: Hasher,
			H: self::imp::HasherBounds,
			H::Out: Ord
		;
	}
}

export_api! {
	pub(crate) trait OtherApi {
		/// The current relay chain identifier.
		fn chain_id() -> u64;

		/// Print a printable value.
		fn print<T>(value: T)
		where
			T: Printable,
			T: Sized
		;
	}
}

export_api! {
	pub(crate) trait CryptoApi {
		/// Verify a ed25519 signature.
		fn ed25519_verify<P: AsRef<[u8]>>(sig: &[u8; 64], msg: &[u8], pubkey: P) -> bool;

		/// Verify an sr25519 signature.
		fn sr25519_verify<P: AsRef<[u8]>>(sig: &[u8; 64], msg: &[u8], pubkey: P) -> bool;

		/// Verify and recover a SECP256k1 ECDSA signature.
		/// - `sig` is passed in RSV format. V should be either 0/1 or 27/28.
		/// - returns `Err` if the signature is bad, otherwise the 64-byte pubkey (doesn't include the 0x04 prefix).
		fn secp256k1_ecdsa_recover(sig: &[u8; 65], msg: &[u8; 32]) -> Result<[u8; 64], EcdsaVerifyError>;
	}
}

export_api! {
	pub(crate) trait HashingApi {
		/// Conduct a 256-bit Keccak hash.
		fn keccak_256(data: &[u8]) -> [u8; 32] ;

		/// Conduct a 128-bit Blake2 hash.
		fn blake2_128(data: &[u8]) -> [u8; 16];

		/// Conduct a 256-bit Blake2 hash.
		fn blake2_256(data: &[u8]) -> [u8; 32];

		/// Conduct four XX hashes to give a 256-bit result.
		fn twox_256(data: &[u8]) -> [u8; 32];

		/// Conduct two XX hashes to give a 128-bit result.
		fn twox_128(data: &[u8]) -> [u8; 16];

		/// Conduct two XX hashes to give a 64-bit result.
		fn twox_64(data: &[u8]) -> [u8; 8];
	}
}

export_api! {
	pub(crate) trait OffchainApi {
		/// Submit transaction to the pool.
		///
		/// The transaction will end up in the pool.
		fn submit_transaction<T: codec::Encode>(data: &T) -> Result<(), ()>;

		/// Create new key(pair) for signing/encryption/decryption.
		///
		/// Returns an error if given crypto kind is not supported.
		fn new_crypto_key(crypto: CryptoKind) -> Result<CryptoKeyId, ()>;

		/// Encrypt a piece of data using given crypto key.
		///
		/// If `key` is `None`, it will attempt to use current authority key.
		///
		/// Returns an error if `key` is not available or does not exist.
		fn encrypt(key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

		/// Decrypt a piece of data using given crypto key.
		///
		/// If `key` is `None`, it will attempt to use current authority key.
		///
		/// Returns an error if data cannot be decrypted or the `key` is not available or does not exist.
		fn decrypt(key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

		/// Sign a piece of data using given crypto key.
		///
		/// If `key` is `None`, it will attempt to use current authority key.
		///
		/// Returns an error if `key` is not available or does not exist.
		fn sign(key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

		/// Verifies that `signature` for `msg` matches given `key`.
		///
		/// Returns an `Ok` with `true` in case it does, `false` in case it doesn't.
		/// Returns an error in case the key is not available or does not exist or the parameters
		/// lengths are incorrect.
		fn verify(key: Option<CryptoKeyId>, msg: &[u8], signature: &[u8]) -> Result<bool, ()>;

		/// Returns current UNIX timestamp (in millis)
		fn timestamp() -> Timestamp;

		/// Pause the execution until `deadline` is reached.
		fn sleep_until(deadline: Timestamp);

		/// Returns a random seed.
		///
		/// This is a trully random non deterministic seed generated by host environment.
		/// Obviously fine in the off-chain worker context.
		fn random_seed() -> [u8; 32];

		/// Sets a value in the local storage.
		///
		/// Note this storage is not part of the consensus, it's only accessible by
		/// offchain worker tasks running on the same machine. It IS persisted between runs.
		fn local_storage_set(kind: StorageKind, key: &[u8], value: &[u8]);

		/// Sets a value in the local storage if it matches current value.
		///
		/// Since multiple offchain workers may be running concurrently, to prevent
		/// data races use CAS to coordinate between them.
		///
		/// Returns `true` if the value has been set, `false` otherwise.
		///
		/// Note this storage is not part of the consensus, it's only accessible by
		/// offchain worker tasks running on the same machine. It IS persisted between runs.
		fn local_storage_compare_and_set(kind: StorageKind, key: &[u8], old_value: &[u8], new_value: &[u8]) -> bool;

		/// Gets a value from the local storage.
		///
		/// If the value does not exist in the storage `None` will be returned.
		/// Note this storage is not part of the consensus, it's only accessible by
		/// offchain worker tasks running on the same machine. It IS persisted between runs.
		fn local_storage_get(kind: StorageKind, key: &[u8]) -> Option<Vec<u8>>;

		/// Initiaties a http request given HTTP verb and the URL.
		///
		/// Meta is a future-reserved field containing additional, parity-codec encoded parameters.
		/// Returns the id of newly started request.
		fn http_request_start(
			method: &str,
			uri: &str,
			meta: &[u8]
		) -> Result<HttpRequestId, ()>;

		/// Append header to the request.
		fn http_request_add_header(
			request_id: HttpRequestId,
			name: &str,
			value: &str
		) -> Result<(), ()>;

		/// Write a chunk of request body.
		///
		/// Writing an empty chunks finalises the request.
		/// Passing `None` as deadline blocks forever.
		///
		/// Returns an error in case deadline is reached or the chunk couldn't be written.
		fn http_request_write_body(
			request_id: HttpRequestId,
			chunk: &[u8],
			deadline: Option<Timestamp>
		) -> Result<(), HttpError>;

		/// Block and wait for the responses for given requests.
		///
		/// Returns a vector of request statuses (the len is the same as ids).
		/// Note that if deadline is not provided the method will block indefinitely,
		/// otherwise unready responses will produce `DeadlineReached` status.
		///
		/// Passing `None` as deadline blocks forever.
		fn http_response_wait(
			ids: &[HttpRequestId],
			deadline: Option<Timestamp>
		) -> Vec<HttpRequestStatus>;

		/// Read all response headers.
		///
		/// Returns a vector of pairs `(HeaderKey, HeaderValue)`.
		/// NOTE response headers have to be read before response body.
		fn http_response_headers(
			request_id: HttpRequestId
		) -> Vec<(Vec<u8>, Vec<u8>)>;

		/// Read a chunk of body response to given buffer.
		///
		/// Returns the number of bytes written or an error in case a deadline
		/// is reached or server closed the connection.
		/// If `0` is returned it means that the response has been fully consumed
		/// and the `request_id` is now invalid.
		/// NOTE this implies that response headers must be read before draining the body.
		/// Passing `None` as a deadline blocks forever.
		fn http_response_read_body(
			request_id: HttpRequestId,
			buffer: &mut [u8],
			deadline: Option<Timestamp>
		) -> Result<usize, HttpError>;
	}
}

/// API trait that should cover all other APIs.
///
/// Implement this to make sure you implement all APIs.
trait Api: StorageApi + OtherApi + CryptoApi + HashingApi + OffchainApi {}

mod imp {
	use super::*;

	#[cfg(feature = "std")]
	include!("../with_std.rs");

	#[cfg(not(feature = "std"))]
	include!("../without_std.rs");
}

#[cfg(feature = "std")]
pub use self::imp::{StorageOverlay, ChildrenStorageOverlay, with_storage, with_externalities};
#[cfg(not(feature = "std"))]
pub use self::imp::ext::*;

/// Type alias for Externalities implementation used in tests.
#[cfg(feature = "std")]
pub type TestExternalities<H> = self::imp::TestExternalities<H, u64>;
