// Copyright 2019 Parity Technologies (UK) Ltd.
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

//! Offchain workers types

use rstd::prelude::{Vec, Box};
use rstd::convert::TryFrom;

/// A type of supported crypto.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
#[repr(C)]
pub enum StorageKind {
	/// Persistent storage is non-revertible and not fork-aware. It means that any value
	/// set by the offchain worker triggered at block `N(hash1)` is persisted even
	/// if that block is reverted as non-canonical and is available for the worker
	/// that is re-run at block `N(hash2)`.
	/// This storage can be used by offchain workers to handle forks
	/// and coordinate offchain workers running on different forks.
	PERSISTENT = 1,
	/// Local storage is revertible and fork-aware. It means that any value
	/// set by the offchain worker triggered at block `N(hash1)` is reverted
	/// if that block is reverted as non-canonical and is NOT available for the worker
	/// that is re-run at block `N(hash2)`.
	LOCAL = 2,
}

impl TryFrom<u32> for StorageKind {
	type Error = ();

	fn try_from(kind: u32) -> Result<Self, Self::Error> {
		match kind {
			e if e == u32::from(StorageKind::PERSISTENT as u8) => Ok(StorageKind::PERSISTENT),
			e if e == u32::from(StorageKind::LOCAL as u8) => Ok(StorageKind::LOCAL),
			_ => Err(()),
		}
	}
}

/// A type of supported crypto.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
#[repr(C)]
pub enum CryptoKind {
	/// SR25519 crypto (Schnorrkel)
	Sr25519 = 1,
	/// ED25519 crypto (Edwards)
	Ed25519 = 2,
}

impl TryFrom<u32> for CryptoKind {
	type Error = ();

	fn try_from(kind: u32) -> Result<Self, Self::Error> {
		match kind {
			e if e == u32::from(CryptoKind::Sr25519 as u8) => Ok(CryptoKind::Sr25519),
			e if e == u32::from(CryptoKind::Ed25519 as u8) => Ok(CryptoKind::Ed25519),
			_ => Err(()),
		}
	}
}

/// Opaque type for created crypto keys.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct CryptoKeyId(pub u16);

/// Opaque type for offchain http requests.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct HttpRequestId(pub u16);

/// An error enum returned by some http methods.
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
#[repr(C)]
pub enum HttpError {
	/// The requested action couldn't been completed within a deadline.
	DeadlineReached = 1,
	/// There was an IO Error while processing the request.
	IoError = 2,
}

impl TryFrom<u32> for HttpError {
	type Error = ();

	fn try_from(error: u32) -> Result<Self, Self::Error> {
		match error {
			e if e == HttpError::DeadlineReached as u8 as u32 => Ok(HttpError::DeadlineReached),
			e if e == HttpError::IoError as u8 as u32 => Ok(HttpError::IoError),
			_ => Err(())
		}
	}
}

/// Status of the HTTP request
#[derive(Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum HttpRequestStatus {
	/// Deadline was reached while we waited for this request to finish.
	///
	/// Note the deadline is controlled by the calling part, it not necessarily means
	/// that the request has timed out.
	DeadlineReached,
	/// Request timed out.
	///
	/// This means that the request couldn't be completed by the host environment
	/// within a reasonable time (according to the host), has now been terminated
	/// and is considered finished.
	/// To retry the request you need to construct it again.
	Timeout,
	/// Request status of this ID is not known.
	Unknown,
	/// The request has finished with given status code.
	Finished(u16),
}

impl From<HttpRequestStatus> for u32 {
	fn from(status: HttpRequestStatus) -> Self {
		match status {
			HttpRequestStatus::Unknown => 0,
			HttpRequestStatus::DeadlineReached => 10,
			HttpRequestStatus::Timeout => 20,
			HttpRequestStatus::Finished(code) => u32::from(code),
		}
	}
}

impl TryFrom<u32> for HttpRequestStatus {
	type Error = ();

	fn try_from(status: u32) -> Result<Self, Self::Error> {
		match status {
			0 => Ok(HttpRequestStatus::Unknown),
			10 => Ok(HttpRequestStatus::DeadlineReached),
			20 => Ok(HttpRequestStatus::Timeout),
			100..=999 => u16::try_from(status).map(HttpRequestStatus::Finished).map_err(|_| ()),
			_ => Err(()),
		}
	}
}

/// Opaque timestamp type
#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Default)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Timestamp(u64);

/// Duration type
#[derive(Clone, Copy, PartialEq, Eq, Ord, PartialOrd, Default)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Duration(u64);

impl Duration {
	/// Create new duration representing given number of milliseconds.
	pub fn from_millis(millis: u64) -> Self {
		Duration(millis)
	}

	/// Returns number of milliseconds this Duration represents.
	pub fn millis(&self) -> u64 {
		self.0
	}
}

impl Timestamp {
	/// Creates new `Timestamp` given unix timestamp in miliseconds.
	pub fn from_unix_millis(millis: u64) -> Self {
		Timestamp(millis)
	}

	/// Increase the timestamp by given `Duration`.
	pub fn add(&self, duration: Duration) -> Timestamp {
		Timestamp(self.0.saturating_add(duration.0))
	}

	/// Decrease the timestamp by given `Duration`
	pub fn sub(&self, duration: Duration) -> Timestamp {
		Timestamp(self.0.saturating_sub(duration.0))
	}

	/// Returns a saturated difference (Duration) between two Timestamps.
	pub fn diff(&self, other: &Self) -> Duration {
		Duration(self.0.saturating_sub(other.0))
	}

	/// Return number of milliseconds since UNIX epoch.
	pub fn unix_millis(&self) -> u64 {
		self.0
	}
}

/// An extended externalities for offchain workers.
pub trait Externalities {
	/// Submit transaction.
	///
	/// The transaction will end up in the pool and be propagated to others.
	fn submit_transaction(&mut self, extrinsic: Vec<u8>) -> Result<(), ()>;

	/// Create new key(pair) for signing/encryption/decryption.
	///
	/// Returns an error if given crypto kind is not supported.
	fn new_crypto_key(&mut self, crypto: CryptoKind) -> Result<CryptoKeyId, ()>;

	/// Encrypt a piece of data using given crypto key.
	///
	/// If `key` is `None`, it will attempt to use current authority key.
	///
	/// Returns an error if `key` is not available or does not exist.
	fn encrypt(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

	/// Decrypt a piece of data using given crypto key.
	///
	/// If `key` is `None`, it will attempt to use current authority key.
	///
	/// Returns an error if data cannot be decrypted or the `key` is not available or does not exist.
	fn decrypt(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

	/// Sign a piece of data using given crypto key.
	///
	/// If `key` is `None`, it will attempt to use current authority key.
	///
	/// Returns an error if `key` is not available or does not exist.
	fn sign(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()>;

	/// Verifies that `signature` for `msg` matches given `key`.
	///
	/// Returns an `Ok` with `true` in case it does, `false` in case it doesn't.
	/// Returns an error in case the key is not available or does not exist or the parameters
	/// lengths are incorrect.
	fn verify(&mut self, key: Option<CryptoKeyId>, msg: &[u8], signature: &[u8]) -> Result<bool, ()>;

	/// Returns current UNIX timestamp (in millis)
	fn timestamp(&mut self) -> Timestamp;

	/// Pause the execution until `deadline` is reached.
	fn sleep_until(&mut self, deadline: Timestamp);

	/// Returns a random seed.
	///
	/// This is a trully random non deterministic seed generated by host environment.
	/// Obviously fine in the off-chain worker context.
	fn random_seed(&mut self) -> [u8; 32];

	/// Sets a value in the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_set(&mut self, kind: StorageKind, key: &[u8], value: &[u8]);

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
		kind: StorageKind,
		key: &[u8],
		old_value: &[u8],
		new_value: &[u8],
	) -> bool;

	/// Gets a value from the local storage.
	///
	/// If the value does not exist in the storage `None` will be returned.
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	fn local_storage_get(&mut self, kind: StorageKind, key: &[u8]) -> Option<Vec<u8>>;

	/// Initiaties a http request given HTTP verb and the URL.
	///
	/// Meta is a future-reserved field containing additional, parity-codec encoded parameters.
	/// Returns the id of newly started request.
	fn http_request_start(
		&mut self,
		method: &str,
		uri: &str,
		meta: &[u8]
	) -> Result<HttpRequestId, ()>;

	/// Append header to the request.
	fn http_request_add_header(
		&mut self,
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
		&mut self,
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
		&mut self,
		ids: &[HttpRequestId],
		deadline: Option<Timestamp>
	) -> Vec<HttpRequestStatus>;

	/// Read all response headers.
	///
	/// Returns a vector of pairs `(HeaderKey, HeaderValue)`.
	fn http_response_headers(
		&mut self,
		request_id: HttpRequestId
	) -> Vec<(Vec<u8>, Vec<u8>)>;

	/// Read a chunk of body response to given buffer.
	///
	/// Returns the number of bytes written or an error in case a deadline
	/// is reached or server closed the connection.
	/// Passing `None` as a deadline blocks forever.
	fn http_response_read_body(
		&mut self,
		request_id: HttpRequestId,
		buffer: &mut [u8],
		deadline: Option<Timestamp>
	) -> Result<usize, HttpError>;

}
impl<T: Externalities + ?Sized> Externalities for Box<T> {
	fn submit_transaction(&mut self, ex: Vec<u8>) -> Result<(), ()> {
		(&mut **self).submit_transaction(ex)
	}

	fn new_crypto_key(&mut self, crypto: CryptoKind) -> Result<CryptoKeyId, ()> {
		(&mut **self).new_crypto_key(crypto)
	}

	fn encrypt(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()> {
		(&mut **self).encrypt(key, data)
	}

	fn decrypt(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()> {
		(&mut **self).decrypt(key, data)
	}

	fn sign(&mut self, key: Option<CryptoKeyId>, data: &[u8]) -> Result<Vec<u8>, ()> {
		(&mut **self).sign(key, data)
	}

	fn verify(&mut self, key: Option<CryptoKeyId>, msg: &[u8], signature: &[u8]) -> Result<bool, ()> {
		(&mut **self).verify(key, msg, signature)
	}

	fn timestamp(&mut self) -> Timestamp {
		(&mut **self).timestamp()
	}

	fn sleep_until(&mut self, deadline: Timestamp) {
		(&mut **self).sleep_until(deadline)
	}

	fn random_seed(&mut self) -> [u8; 32] {
		(&mut **self).random_seed()
	}

	fn local_storage_set(&mut self, kind: StorageKind, key: &[u8], value: &[u8]) {
		(&mut **self).local_storage_set(kind, key, value)
	}

	fn local_storage_compare_and_set(
		&mut self,
		kind: StorageKind,
		key: &[u8],
		old_value: &[u8],
		new_value: &[u8],
	) -> bool {
		(&mut **self).local_storage_compare_and_set(kind, key, old_value, new_value)
	}

	fn local_storage_get(&mut self, kind: StorageKind, key: &[u8]) -> Option<Vec<u8>> {
		(&mut **self).local_storage_get(kind, key)
	}

	fn http_request_start(&mut self, method: &str, uri: &str, meta: &[u8]) -> Result<HttpRequestId, ()> {
		(&mut **self).http_request_start(method, uri, meta)
	}

	fn http_request_add_header(&mut self, request_id: HttpRequestId, name: &str, value: &str) -> Result<(), ()> {
		(&mut **self).http_request_add_header(request_id, name, value)
	}

	fn http_request_write_body(
		&mut self,
		request_id: HttpRequestId,
		chunk: &[u8],
		deadline: Option<Timestamp>
	) -> Result<(), HttpError> {
		(&mut **self).http_request_write_body(request_id, chunk, deadline)
	}

	fn http_response_wait(&mut self, ids: &[HttpRequestId], deadline: Option<Timestamp>) -> Vec<HttpRequestStatus> {
		(&mut **self).http_response_wait(ids, deadline)
	}

	fn http_response_headers(&mut self, request_id: HttpRequestId) -> Vec<(Vec<u8>, Vec<u8>)> {
		(&mut **self).http_response_headers(request_id)
	}

	fn http_response_read_body(
		&mut self,
		request_id: HttpRequestId,
		buffer: &mut [u8],
		deadline: Option<Timestamp>
	) -> Result<usize, HttpError> {
		(&mut **self).http_response_read_body(request_id, buffer, deadline)
	}
}


#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn timestamp_ops() {
		let t = Timestamp(5);
		assert_eq!(t.add(Duration::from_millis(10)), Timestamp(15));
		assert_eq!(t.sub(Duration::from_millis(10)), Timestamp(0));
		assert_eq!(t.diff(&Timestamp(3)), Duration(2));
	}
}
