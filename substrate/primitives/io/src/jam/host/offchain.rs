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

//! Offchain host calls (indices 350-367) and the offchain index host calls (indices 345-346);
//! `network_state` is not provided under JAM.
//!
//! The host calls follow the newest non-`register_only` version of each function in sdk3
//! `host_functions/offchain.rs`, with every `AllocateAndReturn*` converted to a caller-provided
//! buffer plus a returned length. The public functions keep the exact signatures of master's
//! `relay_chain.rs` `Offchain` trait.
//!
//! Besides the encodings described in [`crate::jam::host`], the host calls use:
//!
//! - `StorageKind` and `HttpRequestId`: a `u32`.
//! - `Option<Timestamp>`: the unix millis as `u64`, where `u64::MAX` is `None`.
//! - `Result<T, HttpError>`: an `i64`, where a non-negative value is `Ok` and `-1`, `-2` and `-3`
//!   are `HttpError::{DeadlineReached, IoError, Invalid}`.

use alloc::{vec, vec::Vec};

use sp_core::{
	offchain::{HttpError, HttpRequestId, HttpRequestStatus, StorageKind, Timestamp},
	OpaquePeerId,
};

#[cfg(not(substrate_runtime))]
use super::with_externalities;
#[cfg(substrate_runtime)]
use super::{option_u32_from_host, optional_slice, void_result_from_host};
#[cfg(not(substrate_runtime))]
use super::{option_u32_into_ffi, read_optional_slice, void_result_into_ffi};
#[cfg(not(substrate_runtime))]
use sp_core::offchain::{OffchainDbExt, OffchainWorkerExt, TransactionPoolExt};
#[cfg(not(substrate_runtime))]
use sp_externalities::ExternalitiesExt;
#[cfg(not(substrate_runtime))]
use sp_jam_runtime_interface::FunctionContext;

/// Encodes an `Option<Timestamp>` as `u64`.
#[cfg(substrate_runtime)]
fn deadline_into_ffi(deadline: Option<Timestamp>) -> u64 {
	deadline.map_or(u64::MAX, |deadline| deadline.unix_millis())
}

/// Decodes an `Option<Timestamp>` encoded as `u64`.
#[cfg(not(substrate_runtime))]
fn deadline_from_ffi(deadline: u64) -> Option<Timestamp> {
	(deadline != u64::MAX).then(|| Timestamp::from_unix_millis(deadline))
}

/// Encodes a `Result<T, HttpError>` as `i64`.
#[cfg(not(substrate_runtime))]
fn http_result_into_ffi<T: Into<i64>>(result: Result<T, HttpError>) -> i64 {
	match result {
		Ok(value) => value.into(),
		Err(HttpError::DeadlineReached) => -1,
		Err(HttpError::IoError) => -2,
		Err(HttpError::Invalid) => -3,
	}
}

/// Decodes a `Result<u32, HttpError>` returned by a host call.
#[cfg(substrate_runtime)]
fn http_result_from_host(result: i64) -> Result<u32, HttpError> {
	match result {
		-1 => Err(HttpError::DeadlineReached),
		-2 => Err(HttpError::IoError),
		-3 => Err(HttpError::Invalid),
		value => Ok(u32::try_from(value).expect("the host returns a valid result; qed")),
	}
}

/// Decodes a `StorageKind` argument of a host call.
#[cfg(not(substrate_runtime))]
fn storage_kind(kind: u32) -> Result<StorageKind, alloc::string::String> {
	StorageKind::try_from(kind).map_err(|_| alloc::format!("invalid storage kind: {kind}"))
}

/// Decodes an `HttpRequestId` argument of a host call.
#[cfg(not(substrate_runtime))]
fn request_id(id: u32) -> Result<HttpRequestId, alloc::string::String> {
	u16::try_from(id)
		.map(HttpRequestId)
		.map_err(|_| alloc::format!("invalid request id: {id}"))
}

/// Reads a UTF-8 string argument of a host call.
#[cfg(not(substrate_runtime))]
fn read_str(
	ctx: &mut dyn FunctionContext,
	ptr: *const u8,
	len: u64,
) -> Result<alloc::string::String, alloc::string::String> {
	alloc::string::String::from_utf8(ctx.read_memory(ptr, len)?)
		.map_err(|_| alloc::string::String::from("invalid UTF-8 string"))
}

/// Runs `f` with the [`OffchainWorkerExt`], panicking with `msg` if it is missing.
#[cfg(not(substrate_runtime))]
fn with_worker<R>(msg: &str, f: impl FnOnce(&mut OffchainWorkerExt) -> R) -> R {
	with_externalities(|mut ext| f(ext.extension::<OffchainWorkerExt>().expect(msg)))
}

/// Runs `f` with the [`OffchainDbExt`], panicking with `msg` if it is missing.
#[cfg(not(substrate_runtime))]
fn with_db<R>(msg: &str, f: impl FnOnce(&mut OffchainDbExt) -> R) -> R {
	with_externalities(|mut ext| f(ext.extension::<OffchainDbExt>().expect(msg)))
}

/// Node logic of `ext_offchain_is_validator`.
#[cfg(not(substrate_runtime))]
fn node_is_validator() -> bool {
	with_worker("is_validator can be called only in the offchain worker context", |ext| {
		ext.is_validator()
	})
}

/// Node logic of `ext_offchain_submit_transaction`.
#[cfg(not(substrate_runtime))]
fn node_submit_transaction(data: Vec<u8>) -> Result<(), ()> {
	with_externalities(|mut ext| {
		ext.extension::<TransactionPoolExt>()
			.expect(
				"submit_transaction can be called only in the offchain call context with
				TransactionPool capabilities enabled",
			)
			.submit_transaction(data)
	})
}

/// Node logic of `ext_offchain_network_peer_id`.
#[cfg(not(substrate_runtime))]
fn node_network_peer_id() -> Result<[u8; 38], ()> {
	let encoded_peer_id =
		with_worker("network_peer_id can be called only in the offchain worker context", |ext| {
			ext.network_state()
		})?
		.peer_id
		.0;

	let peer_id: Vec<u8> = codec::Decode::decode(&mut &encoded_peer_id[..]).map_err(|_| ())?;
	peer_id.try_into().map_err(|peer_id: Vec<u8>| {
		log::warn!(
			target: "offchain",
			"`network_peer_id`: peer id length ({}) does not match the output buffer length (38)",
			peer_id.len(),
		);
	})
}

/// Node logic of `ext_offchain_timestamp`.
#[cfg(not(substrate_runtime))]
fn node_timestamp() -> Timestamp {
	with_worker("timestamp can be called only in the offchain worker context", |ext| {
		ext.timestamp()
	})
}

/// Node logic of `ext_offchain_sleep_until`.
#[cfg(not(substrate_runtime))]
fn node_sleep_until(deadline: Timestamp) {
	with_worker("sleep_until can be called only in the offchain worker context", |ext| {
		ext.sleep_until(deadline)
	})
}

/// Node logic of `ext_offchain_random_seed`.
#[cfg(not(substrate_runtime))]
fn node_random_seed() -> [u8; 32] {
	with_worker("random_seed can be called only in the offchain worker context", |ext| {
		ext.random_seed()
	})
}

/// Node logic of `ext_offchain_local_storage_set`.
#[cfg(not(substrate_runtime))]
fn node_local_storage_set(kind: StorageKind, key: &[u8], value: &[u8]) {
	with_db(
		"local_storage_set can be called only in the offchain call context with
		OffchainDb extension",
		|ext| ext.local_storage_set(kind, key, value),
	)
}

/// Node logic of `ext_offchain_local_storage_clear`.
#[cfg(not(substrate_runtime))]
fn node_local_storage_clear(kind: StorageKind, key: &[u8]) {
	with_db(
		"local_storage_clear can be called only in the offchain call context with
		OffchainDb extension",
		|ext| ext.local_storage_clear(kind, key),
	)
}

/// Node logic of `ext_offchain_local_storage_compare_and_set`.
#[cfg(not(substrate_runtime))]
fn node_local_storage_compare_and_set(
	kind: StorageKind,
	key: &[u8],
	old_value: Option<&[u8]>,
	new_value: &[u8],
) -> bool {
	with_db(
		"local_storage_compare_and_set can be called only in the offchain call context
		with OffchainDb extension",
		|ext| ext.local_storage_compare_and_set(kind, key, old_value, new_value),
	)
}

/// Node logic of `ext_offchain_local_storage_read`.
#[cfg(not(substrate_runtime))]
fn node_local_storage_read(
	kind: StorageKind,
	key: &[u8],
	value_out: &mut [u8],
	offset: u32,
) -> Option<u32> {
	with_db(
		"local_storage_read can be called only in the offchain call context with
		OffchainDb extension",
		|ext| ext.local_storage_get(kind, key),
	)
	.map(|v| {
		let value_offset = offset as usize;
		let data = &v[value_offset.min(v.len())..];
		if data.len() <= value_out.len() {
			value_out[..data.len()].copy_from_slice(data);
		}
		data.len() as u32
	})
}

/// Node logic of `ext_offchain_http_request_start`.
#[cfg(not(substrate_runtime))]
fn node_http_request_start(method: &str, uri: &str, meta: &[u8]) -> Result<HttpRequestId, ()> {
	assert!(meta.is_empty(), "Meta must be empty in http_request_start");
	with_worker("http_request_start can be called only in the offchain worker context", |ext| {
		ext.http_request_start(method, uri, &[])
	})
}

/// Node logic of `ext_offchain_http_request_add_header`.
#[cfg(not(substrate_runtime))]
fn node_http_request_add_header(
	request_id: HttpRequestId,
	name: &str,
	value: &str,
) -> Result<(), ()> {
	with_worker(
		"http_request_add_header can be called only in the offchain worker context",
		|ext| ext.http_request_add_header(request_id, name, value),
	)
}

/// Node logic of `ext_offchain_http_request_write_body`.
#[cfg(not(substrate_runtime))]
fn node_http_request_write_body(
	request_id: HttpRequestId,
	chunk: &[u8],
	deadline: Option<Timestamp>,
) -> Result<(), HttpError> {
	with_worker(
		"http_request_write_body can be called only in the offchain worker context",
		|ext| ext.http_request_write_body(request_id, chunk, deadline),
	)
}

/// Node logic of `ext_offchain_http_response_wait`.
#[cfg(not(substrate_runtime))]
fn node_http_response_wait(
	ids: &[HttpRequestId],
	deadline: Option<Timestamp>,
) -> Vec<HttpRequestStatus> {
	with_worker("http_response_wait can be called only in the offchain worker context", |ext| {
		ext.http_response_wait(ids, deadline)
	})
}

/// Node logic of `ext_offchain_http_response_header_name` (`value == false`) and
/// `ext_offchain_http_response_header_value` (`value == true`).
#[cfg(not(substrate_runtime))]
fn node_http_response_header(
	request_id: HttpRequestId,
	header_index: u32,
	value: bool,
	out: &mut [u8],
) -> Option<u32> {
	let headers = with_worker(
		"http_response_headers can be called only in the offchain worker context",
		|ext| ext.http_response_headers(request_id),
	);
	let header = headers.get(header_index as usize)?;
	let res = if value { &header.1 } else { &header.0 };
	if res.len() <= out.len() {
		out[..res.len()].copy_from_slice(res);
	}
	Some(res.len() as u32)
}

/// Node logic of `ext_offchain_http_response_read_body`.
#[cfg(not(substrate_runtime))]
fn node_http_response_read_body(
	request_id: HttpRequestId,
	buffer: &mut [u8],
	deadline: Option<Timestamp>,
) -> Result<u32, HttpError> {
	with_worker(
		"http_response_read_body can be called only in the offchain worker context",
		|ext| ext.http_response_read_body(request_id, buffer, deadline),
	)
	.map(|r| r as u32)
}

/// Node logic of `ext_offchain_set_authorized_nodes`.
#[cfg(not(substrate_runtime))]
fn node_set_authorized_nodes(nodes: Vec<OpaquePeerId>, authorized_only: bool) {
	with_worker("set_authorized_nodes can be called only in the offchain worker context", |ext| {
		ext.set_authorized_nodes(nodes, authorized_only)
	})
}

/// Node logic of `ext_offchain_index_set` and `ext_offchain_index_clear`.
#[cfg(not(substrate_runtime))]
fn node_index_set(key: &[u8], value: Option<&[u8]>) {
	with_externalities(|ext| ext.set_offchain_storage(key, value))
}

sp_jam_runtime_interface::host_calls! {
	/// Write a key value pair to the Offchain DB database in a buffered fashion.
	#[index = 345]
	fn ext_offchain_index_set(
		ctx: &mut dyn FunctionContext,
		key: *const u8,
		key_len: u64,
		value: *const u8,
		value_len: u64,
	) {
		let key = ctx.read_memory(key, key_len)?;
		let value = ctx.read_memory(value, value_len)?;
		node_index_set(&key, Some(&value));
		Ok(())
	}

	/// Remove a key and its associated value from the Offchain DB.
	#[index = 346]
	fn ext_offchain_index_clear(ctx: &mut dyn FunctionContext, key: *const u8, key_len: u64) {
		node_index_set(&ctx.read_memory(key, key_len)?, None);
		Ok(())
	}

	/// Returns if the local node is a potential validator.
	///
	/// Even if this function returns `true`, it does not mean that any keys are configured
	/// and that the validator is registered in the chain.
	#[index = 350]
	fn ext_offchain_is_validator(ctx: &mut dyn FunctionContext) -> u32 {
		Ok(node_is_validator() as u32)
	}

	/// Submit an encoded transaction to the pool.
	///
	/// The transaction will end up in the pool.
	#[index = 351]
	fn ext_offchain_submit_transaction(ctx: &mut dyn FunctionContext, data: *const u8, data_len: u64) -> i32 {
		Ok(void_result_into_ffi(node_submit_transaction(ctx.read_memory(data, data_len)?)))
	}

	/// Writes the 38-byte peer ID of the local node into `out`.
	#[index = 352]
	fn ext_offchain_network_peer_id(ctx: &mut dyn FunctionContext, out: *mut u8) -> i32 {
		let peer_id = node_network_peer_id();
		if let Ok(peer_id) = &peer_id {
			ctx.write_memory(out, peer_id)?;
		}
		Ok(void_result_into_ffi(peer_id.map(drop)))
	}

	/// Returns current UNIX timestamp (in millis).
	#[index = 353]
	fn ext_offchain_timestamp(ctx: &mut dyn FunctionContext) -> u64 {
		Ok(node_timestamp().unix_millis())
	}

	/// Pause the execution until `deadline` (UNIX timestamp in millis) is reached.
	#[index = 354]
	fn ext_offchain_sleep_until(ctx: &mut dyn FunctionContext, deadline: u64) {
		node_sleep_until(Timestamp::from_unix_millis(deadline));
		Ok(())
	}

	/// Writes a random seed to the 32-byte output buffer `out`.
	///
	/// This is a truly random, non-deterministic seed generated by host environment.
	/// Obviously fine in the off-chain worker context.
	#[index = 355]
	fn ext_offchain_random_seed(ctx: &mut dyn FunctionContext, out: *mut u8) {
		ctx.write_memory(out, &node_random_seed())
	}

	/// Sets a value in the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	#[index = 356]
	fn ext_offchain_local_storage_set(
		ctx: &mut dyn FunctionContext,
		kind: u32,
		key: *const u8,
		key_len: u64,
		value: *const u8,
		value_len: u64,
	) {
		let key = ctx.read_memory(key, key_len)?;
		let value = ctx.read_memory(value, value_len)?;
		node_local_storage_set(storage_kind(kind)?, &key, &value);
		Ok(())
	}

	/// Remove a value from the local storage.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	#[index = 357]
	fn ext_offchain_local_storage_clear(ctx: &mut dyn FunctionContext, kind: u32, key: *const u8, key_len: u64) {
		let key = ctx.read_memory(key, key_len)?;
		node_local_storage_clear(storage_kind(kind)?, &key);
		Ok(())
	}

	/// Sets a value in the local storage if it matches current value.
	///
	/// Since multiple offchain workers may be running concurrently, to prevent
	/// data races use CAS to coordinate between them.
	///
	/// Returns `1` if the value has been set, `0` otherwise.
	///
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	#[index = 358]
	fn ext_offchain_local_storage_compare_and_set(
		ctx: &mut dyn FunctionContext,
		kind: u32,
		key: *const u8,
		key_len: u64,
		old_value: *const u8,
		old_value_len: u64,
		new_value: *const u8,
		new_value_len: u64,
	) -> u32 {
		let key = ctx.read_memory(key, key_len)?;
		let old_value = read_optional_slice(ctx, old_value, old_value_len)?;
		let new_value = ctx.read_memory(new_value, new_value_len)?;
		Ok(node_local_storage_compare_and_set(
			storage_kind(kind)?,
			&key,
			old_value.as_deref(),
			&new_value,
		) as u32)
	}

	/// Reads a value from the local storage.
	///
	/// If the value does not exist in the storage `None` will be returned.
	/// Note this storage is not part of the consensus, it's only accessible by
	/// offchain worker tasks running on the same machine. It IS persisted between runs.
	#[index = 359]
	fn ext_offchain_local_storage_read(
		ctx: &mut dyn FunctionContext,
		kind: u32,
		key: *const u8,
		key_len: u64,
		value_out: *mut u8,
		value_out_len: u64,
		offset: u32,
	) -> i64 {
		let kind = storage_kind(kind)?;
		let key = ctx.read_memory(key, key_len)?;
		ctx.with_buffer(value_out, value_out_len, |value_out| {
			node_local_storage_read(kind, &key, value_out, offset)
		})
		.map(option_u32_into_ffi)
	}

	/// Initiates a http request given HTTP verb and the URL.
	///
	/// Meta is a future-reserved field and must be empty. Returns the id of newly started
	/// request, `-1` on error.
	#[index = 360]
	fn ext_offchain_http_request_start(
		ctx: &mut dyn FunctionContext,
		method: *const u8,
		method_len: u64,
		uri: *const u8,
		uri_len: u64,
		meta: *const u8,
		meta_len: u64,
	) -> i64 {
		let method = read_str(ctx, method, method_len)?;
		let uri = read_str(ctx, uri, uri_len)?;
		let meta = ctx.read_memory(meta, meta_len)?;
		Ok(node_http_request_start(&method, &uri, &meta).map_or(-1, |id| id.0.into()))
	}

	/// Append header to the request.
	#[index = 361]
	fn ext_offchain_http_request_add_header(
		ctx: &mut dyn FunctionContext,
		request_id: u32,
		name: *const u8,
		name_len: u64,
		value: *const u8,
		value_len: u64,
	) -> i32 {
		let name = read_str(ctx, name, name_len)?;
		let value = read_str(ctx, value, value_len)?;
		Ok(void_result_into_ffi(node_http_request_add_header(
			self::request_id(request_id)?,
			&name,
			&value,
		)))
	}

	/// Write a chunk of request body.
	///
	/// Writing an empty chunks finalizes the request.
	/// Passing `u64::MAX` as deadline blocks forever.
	///
	/// Returns an error in case deadline is reached or the chunk couldn't be written.
	#[index = 362]
	fn ext_offchain_http_request_write_body(
		ctx: &mut dyn FunctionContext,
		request_id: u32,
		chunk: *const u8,
		chunk_len: u64,
		deadline: u64,
	) -> i64 {
		let chunk = ctx.read_memory(chunk, chunk_len)?;
		Ok(http_result_into_ffi(
			node_http_request_write_body(
				self::request_id(request_id)?,
				&chunk,
				deadline_from_ffi(deadline),
			)
			.map(|()| 0u32),
		))
	}

	/// Block and wait for the responses for given requests.
	///
	/// `ids` points to `ids_len` bytes of little endian `u16` request ids. Fills the provided
	/// output buffer with the little endian `u32` request statuses. The output buffer must be
	/// able to hold a status for every id.
	///
	/// Note that if deadline is not provided the method will block indefinitely,
	/// otherwise unready responses will produce `DeadlineReached` status.
	///
	/// Passing `u64::MAX` as deadline blocks forever.
	#[index = 363]
	fn ext_offchain_http_response_wait(
		ctx: &mut dyn FunctionContext,
		ids: *const u8,
		ids_len: u64,
		deadline: u64,
		out: *mut u8,
		out_len: u64,
	) {
		let ids = ctx
			.read_memory(ids, ids_len)?
			.chunks_exact(2)
			.map(|id| HttpRequestId(u16::from_le_bytes([id[0], id[1]])))
			.collect::<Vec<_>>();
		ctx.with_buffer(out, out_len, |out| {
			assert!(
				out.len() >= ids.len() * 4,
				"Output buffer of {} bytes cannot hold {} response statuses (need {} bytes)",
				out.len(),
				ids.len(),
				ids.len() * 4
			);
			let statuses = node_http_response_wait(&ids, deadline_from_ffi(deadline));
			for (i, status) in statuses.into_iter().enumerate() {
				let start = i * 4;
				out[start..start + 4].copy_from_slice(&u32::from(status).to_le_bytes());
			}
		})
	}

	/// Read the name of the header at the given index into the provided output buffer.
	///
	/// Returns the full length of the header name. The value is actually stored only if the
	/// buffer is large enough. Otherwise, the buffer is not written into, and its contents are
	/// unchanged.
	///
	/// Returns `None` if the index is out of bounds.
	#[index = 364]
	fn ext_offchain_http_response_header_name(
		ctx: &mut dyn FunctionContext,
		request_id: u32,
		header_index: u32,
		out: *mut u8,
		out_len: u64,
	) -> i64 {
		let request_id = self::request_id(request_id)?;
		ctx.with_buffer(out, out_len, |out| {
			node_http_response_header(request_id, header_index, false, out)
		})
		.map(option_u32_into_ffi)
	}

	/// Read the value of the header at the given index into the provided output buffer.
	///
	/// Returns the full length of the header value. The value is actually stored only if the
	/// buffer is large enough. Otherwise, the buffer is not written into, and its contents are
	/// unchanged.
	///
	/// Returns `None` if the index is out of bounds.
	#[index = 365]
	fn ext_offchain_http_response_header_value(
		ctx: &mut dyn FunctionContext,
		request_id: u32,
		header_index: u32,
		out: *mut u8,
		out_len: u64,
	) -> i64 {
		let request_id = self::request_id(request_id)?;
		ctx.with_buffer(out, out_len, |out| {
			node_http_response_header(request_id, header_index, true, out)
		})
		.map(option_u32_into_ffi)
	}

	/// Read a chunk of body response to given buffer.
	///
	/// Returns the number of bytes written or an error in case a deadline
	/// is reached or server closed the connection.
	/// If `0` is returned it means that the response has been fully consumed
	/// and the `request_id` is now invalid.
	/// NOTE: this implies that response headers must be read before draining the body.
	/// Passing `u64::MAX` as a deadline blocks forever.
	#[index = 366]
	fn ext_offchain_http_response_read_body(
		ctx: &mut dyn FunctionContext,
		request_id: u32,
		buffer: *mut u8,
		buffer_len: u64,
		deadline: u64,
	) -> i64 {
		let request_id = self::request_id(request_id)?;
		ctx.with_buffer(buffer, buffer_len, |buffer| {
			node_http_response_read_body(request_id, buffer, deadline_from_ffi(deadline))
		})
		.map(http_result_into_ffi)
	}

	/// Set the authorized nodes (a SCALE encoded `Vec<OpaquePeerId>`) and the
	/// `authorized_only` flag.
	#[index = 367]
	fn ext_offchain_set_authorized_nodes(
		ctx: &mut dyn FunctionContext,
		nodes: *const u8,
		nodes_len: u64,
		authorized_only: u32,
	) {
		let nodes = ctx.read_memory(nodes, nodes_len)?;
		let nodes = <Vec<OpaquePeerId> as codec::Decode>::decode(&mut &nodes[..])
			.map_err(|_| alloc::string::String::from("invalid authorized nodes"))?;
		node_set_authorized_nodes(nodes, authorized_only != 0);
		Ok(())
	}
}

/// Returns if the local node is a potential validator.
///
/// Even if this function returns `true`, it does not mean that any keys are configured
/// and that the validator is registered in the chain.
pub fn is_validator() -> bool {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no arguments.
	return unsafe { ext_offchain_is_validator() } != 0;
	#[cfg(not(substrate_runtime))]
	node_is_validator()
}

/// Submit an encoded transaction to the pool.
///
/// The transaction will end up in the pool.
pub fn submit_transaction(data: Vec<u8>) -> Result<(), ()> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live vector.
	return void_result_from_host(unsafe {
		ext_offchain_submit_transaction(data.as_ptr(), data.len() as u64)
	});
	#[cfg(not(substrate_runtime))]
	node_submit_transaction(data)
}

/// Returns current UNIX timestamp (in millis).
pub fn timestamp() -> Timestamp {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no arguments.
	return Timestamp::from_unix_millis(unsafe { ext_offchain_timestamp() });
	#[cfg(not(substrate_runtime))]
	node_timestamp()
}

/// Pause the execution until `deadline` is reached.
pub fn sleep_until(deadline: Timestamp) {
	#[cfg(substrate_runtime)]
	// SAFETY: The host call has no pointer arguments.
	unsafe {
		ext_offchain_sleep_until(deadline.unix_millis())
	};
	#[cfg(not(substrate_runtime))]
	node_sleep_until(deadline)
}

/// Returns a random seed.
///
/// This is a truly random, non-deterministic seed generated by host environment.
/// Obviously fine in the off-chain worker context.
pub fn random_seed() -> [u8; 32] {
	#[cfg(substrate_runtime)]
	{
		let mut seed = [0u8; 32];
		// SAFETY: The pointer comes from a live 32-byte array.
		unsafe { ext_offchain_random_seed(seed.as_mut_ptr()) };
		seed
	}
	#[cfg(not(substrate_runtime))]
	node_random_seed()
}

/// Sets a value in the local storage.
///
/// Note this storage is not part of the consensus, it's only accessible by
/// offchain worker tasks running on the same machine. It IS persisted between runs.
pub fn local_storage_set(kind: StorageKind, key: &[u8], value: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	unsafe {
		ext_offchain_local_storage_set(
			kind.into(),
			key.as_ptr(),
			key.len() as u64,
			value.as_ptr(),
			value.len() as u64,
		)
	};
	#[cfg(not(substrate_runtime))]
	node_local_storage_set(kind, key, value)
}

/// Remove a value from the local storage.
///
/// Note this storage is not part of the consensus, it's only accessible by
/// offchain worker tasks running on the same machine. It IS persisted between runs.
pub fn local_storage_clear(kind: StorageKind, key: &[u8]) {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	unsafe {
		ext_offchain_local_storage_clear(kind.into(), key.as_ptr(), key.len() as u64)
	};
	#[cfg(not(substrate_runtime))]
	node_local_storage_clear(kind, key)
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
pub fn local_storage_compare_and_set(
	kind: StorageKind,
	key: &[u8],
	old_value: Option<Vec<u8>>,
	new_value: &[u8],
) -> bool {
	#[cfg(substrate_runtime)]
	{
		let (old_value, old_value_len) = optional_slice(old_value.as_deref());
		// SAFETY: The pointers and lengths come from live slices.
		unsafe {
			ext_offchain_local_storage_compare_and_set(
				kind.into(),
				key.as_ptr(),
				key.len() as u64,
				old_value,
				old_value_len,
				new_value.as_ptr(),
				new_value.len() as u64,
			) != 0
		}
	}
	#[cfg(not(substrate_runtime))]
	node_local_storage_compare_and_set(kind, key, old_value.as_deref(), new_value)
}

/// The implementation of [`local_storage_get`] on top of the `ext_offchain_local_storage_read`
/// host call (runtime) or the node logic directly (std).
fn local_storage_read_impl(kind: StorageKind, key: &[u8], value_out: &mut [u8]) -> Option<u32> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return option_u32_from_host(unsafe {
		ext_offchain_local_storage_read(
			kind.into(),
			key.as_ptr(),
			key.len() as u64,
			value_out.as_mut_ptr(),
			value_out.len() as u64,
			0,
		)
	});
	#[cfg(not(substrate_runtime))]
	node_local_storage_read(kind, key, value_out, 0)
}

/// Gets a value from the local storage.
///
/// If the value does not exist in the storage `None` will be returned.
/// Note this storage is not part of the consensus, it's only accessible by
/// offchain worker tasks running on the same machine. It IS persisted between runs.
pub fn local_storage_get(kind: StorageKind, key: &[u8]) -> Option<Vec<u8>> {
	let mut value_out = vec![0u8; 256];
	let len = local_storage_read_impl(kind, key, &mut value_out[..])?;
	if len as usize > value_out.len() {
		value_out.resize(len as usize, 0);
		local_storage_read_impl(kind, key, &mut value_out[..])?;
	}
	value_out.truncate(len as usize);
	Some(value_out)
}

/// Initiates a http request given HTTP verb and the URL.
///
/// Meta is a future-reserved field containing additional, parity-scale-codec encoded
/// parameters. Returns the id of newly started request.
pub fn http_request_start(method: &str, uri: &str, meta: &[u8]) -> Result<HttpRequestId, ()> {
	#[cfg(substrate_runtime)]
	{
		// SAFETY: The pointers and lengths come from live slices.
		let id = unsafe {
			ext_offchain_http_request_start(
				method.as_ptr(),
				method.len() as u64,
				uri.as_ptr(),
				uri.len() as u64,
				meta.as_ptr(),
				meta.len() as u64,
			)
		};
		match id {
			-1 => Err(()),
			id => Ok(HttpRequestId(
				u16::try_from(id).expect("the host returns a valid request id; qed"),
			)),
		}
	}
	#[cfg(not(substrate_runtime))]
	node_http_request_start(method, uri, meta)
}

/// Append header to the request.
pub fn http_request_add_header(
	request_id: HttpRequestId,
	name: &str,
	value: &str,
) -> Result<(), ()> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointers and lengths come from live slices.
	return void_result_from_host(unsafe {
		ext_offchain_http_request_add_header(
			request_id.0.into(),
			name.as_ptr(),
			name.len() as u64,
			value.as_ptr(),
			value.len() as u64,
		)
	});
	#[cfg(not(substrate_runtime))]
	node_http_request_add_header(request_id, name, value)
}

/// Write a chunk of request body.
///
/// Writing an empty chunks finalizes the request.
/// Passing `None` as deadline blocks forever.
///
/// Returns an error in case deadline is reached or the chunk couldn't be written.
pub fn http_request_write_body(
	request_id: HttpRequestId,
	chunk: &[u8],
	deadline: Option<Timestamp>,
) -> Result<(), HttpError> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return http_result_from_host(unsafe {
		ext_offchain_http_request_write_body(
			request_id.0.into(),
			chunk.as_ptr(),
			chunk.len() as u64,
			deadline_into_ffi(deadline),
		)
	})
	.map(drop);
	#[cfg(not(substrate_runtime))]
	node_http_request_write_body(request_id, chunk, deadline)
}

/// Block and wait for the responses for given requests.
///
/// Returns a vector of request statuses (the len is the same as ids).
/// Note that if deadline is not provided the method will block indefinitely,
/// otherwise unready responses will produce `DeadlineReached` status.
///
/// Passing `None` as deadline blocks forever.
pub fn http_response_wait(
	ids: &[HttpRequestId],
	deadline: Option<Timestamp>,
) -> Vec<HttpRequestStatus> {
	#[cfg(substrate_runtime)]
	{
		let mut buf = vec![0u8; ids.len() * 4];
		let ids = ids.iter().flat_map(|id| id.0.to_le_bytes()).collect::<Vec<_>>();
		// SAFETY: The pointers and lengths come from live vectors.
		unsafe {
			ext_offchain_http_response_wait(
				ids.as_ptr(),
				ids.len() as u64,
				deadline_into_ffi(deadline),
				buf.as_mut_ptr(),
				buf.len() as u64,
			)
		};
		buf.chunks_exact(4)
			.map(|chunk| {
				let s = u32::from_le_bytes(chunk.try_into().expect("4-byte chunk; qed"));
				HttpRequestStatus::try_from(s).unwrap_or(HttpRequestStatus::Invalid)
			})
			.collect()
	}
	#[cfg(not(substrate_runtime))]
	node_http_response_wait(ids, deadline)
}

/// The implementation of [`http_response_headers`] on top of the
/// `ext_offchain_http_response_header_{name,value}` host calls (runtime) or the node logic
/// directly (std).
fn http_response_header_impl(
	request_id: HttpRequestId,
	header_index: u32,
	value: bool,
	out: &mut [u8],
) -> Option<u32> {
	#[cfg(substrate_runtime)]
	{
		let header = if value {
			ext_offchain_http_response_header_value
		} else {
			ext_offchain_http_response_header_name
		};
		// SAFETY: The pointer and length come from a live slice.
		option_u32_from_host(unsafe {
			header(request_id.0.into(), header_index, out.as_mut_ptr(), out.len() as u64)
		})
	}
	#[cfg(not(substrate_runtime))]
	node_http_response_header(request_id, header_index, value, out)
}

/// Read all response headers.
///
/// Returns a vector of pairs `(HeaderKey, HeaderValue)`.
/// NOTE: response headers have to be read before response body.
pub fn http_response_headers(request_id: HttpRequestId) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut name_buf = vec![0u8; 256];
	let mut value_buf = vec![0u8; 256];
	let mut head_idx = 0;
	let mut headers = Vec::new();

	while let Some(name_len) =
		http_response_header_impl(request_id, head_idx, false, &mut name_buf[..])
	{
		let name_len = name_len as usize;
		if name_len > name_buf.len() {
			name_buf.resize(name_len, 0);
			http_response_header_impl(request_id, head_idx, false, &mut name_buf[..])
				.expect("It was checked that the header exists");
		}
		let value_len = http_response_header_impl(request_id, head_idx, true, &mut value_buf[..])
			.expect("It was checked that the header exists") as usize;
		if value_len > value_buf.len() {
			value_buf.resize(value_len, 0);
			http_response_header_impl(request_id, head_idx, true, &mut value_buf[..])
				.expect("It was checked that the header exists");
		}
		headers.push((name_buf[..name_len].to_vec(), value_buf[..value_len].to_vec()));
		head_idx += 1;
	}
	headers
}

/// Read a chunk of body response to given buffer.
///
/// Returns the number of bytes written or an error in case a deadline
/// is reached or server closed the connection.
/// If `0` is returned it means that the response has been fully consumed
/// and the `request_id` is now invalid.
/// NOTE: this implies that response headers must be read before draining the body.
/// Passing `None` as a deadline blocks forever.
pub fn http_response_read_body(
	request_id: HttpRequestId,
	buffer: &mut [u8],
	deadline: Option<Timestamp>,
) -> Result<u32, HttpError> {
	#[cfg(substrate_runtime)]
	// SAFETY: The pointer and length come from a live slice.
	return http_result_from_host(unsafe {
		ext_offchain_http_response_read_body(
			request_id.0.into(),
			buffer.as_mut_ptr(),
			buffer.len() as u64,
			deadline_into_ffi(deadline),
		)
	});
	#[cfg(not(substrate_runtime))]
	node_http_response_read_body(request_id, buffer, deadline)
}

/// Set the authorized nodes and authorized_only flag.
pub fn set_authorized_nodes(nodes: Vec<OpaquePeerId>, authorized_only: bool) {
	#[cfg(substrate_runtime)]
	{
		let nodes = codec::Encode::encode(&nodes);
		// SAFETY: The pointer and length come from a live vector.
		unsafe {
			ext_offchain_set_authorized_nodes(
				nodes.as_ptr(),
				nodes.len() as u64,
				authorized_only.into(),
			)
		};
	}
	#[cfg(not(substrate_runtime))]
	node_set_authorized_nodes(nodes, authorized_only)
}

/// Writing to the Offchain DB from the runtime.
pub mod offchain_index {
	#[cfg(not(substrate_runtime))]
	use super::node_index_set;
	#[cfg(substrate_runtime)]
	use super::{ext_offchain_index_clear, ext_offchain_index_set};

	/// Write a key value pair to the Offchain DB database in a buffered fashion.
	pub fn set(key: &[u8], value: &[u8]) {
		#[cfg(substrate_runtime)]
		// SAFETY: The pointers and lengths come from live slices.
		unsafe {
			ext_offchain_index_set(
				key.as_ptr(),
				key.len() as u64,
				value.as_ptr(),
				value.len() as u64,
			)
		};
		#[cfg(not(substrate_runtime))]
		node_index_set(key, Some(value))
	}

	/// Remove a key and its associated value from the Offchain DB.
	pub fn clear(key: &[u8]) {
		#[cfg(substrate_runtime)]
		// SAFETY: The pointer and length come from a live slice.
		unsafe {
			ext_offchain_index_clear(key.as_ptr(), key.len() as u64)
		};
		#[cfg(not(substrate_runtime))]
		node_index_set(key, None)
	}

	#[cfg(test)]
	mod tests {
		use super::*;
		use crate::jam::TestExternalities;

		#[test]
		fn set_is_recorded_in_the_offchain_overlay() {
			let mut ext = TestExternalities::default();
			ext.execute_with(|| {
				set(b"key", b"value");
			});

			ext.persist_offchain_overlay();
			assert_eq!(ext.offchain_db().get(b"key"), Some(b"value".to_vec()));
		}

		#[test]
		fn clear_removes_the_key() {
			let mut ext = TestExternalities::default();
			ext.execute_with(|| {
				set(b"key", b"value");
				clear(b"key");
			});

			ext.persist_offchain_overlay();
			assert_eq!(ext.offchain_db().get(b"key"), None);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::jam::TestExternalities;
	use sp_core::offchain::{testing, OffchainDbExt, OffchainWorkerExt, TransactionPoolExt};

	#[test]
	fn is_validator_returns_the_extension_value() {
		let (offchain, _state) = testing::TestOffchainExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(OffchainWorkerExt::new(offchain));

		ext.execute_with(|| {
			assert!(is_validator());
		});
	}

	#[test]
	fn submit_transaction_reaches_the_pool() {
		let (_offchain, _state) = testing::TestOffchainExt::new();
		let (pool, pool_state) = testing::TestTransactionPoolExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(TransactionPoolExt::new(pool));

		ext.execute_with(|| {
			assert_eq!(submit_transaction(vec![1, 2, 3]), Ok(()));
		});
		assert_eq!(pool_state.read().transactions, vec![vec![1, 2, 3]]);
	}

	#[test]
	fn timestamp_and_sleep_until_round_trip() {
		let (offchain, state) = testing::TestOffchainExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(OffchainWorkerExt::new(offchain));

		state.write().timestamp = Timestamp::from_unix_millis(1234);
		ext.execute_with(|| {
			assert_eq!(timestamp(), Timestamp::from_unix_millis(1234));
			sleep_until(Timestamp::from_unix_millis(2000));
		});
		assert_eq!(state.read().timestamp, Timestamp::from_unix_millis(2000));
	}

	#[test]
	fn random_seed_comes_from_the_offchain_state() {
		let (offchain, state) = testing::TestOffchainExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(OffchainWorkerExt::new(offchain));

		state.write().seed = [42u8; 32];
		ext.execute_with(|| {
			assert_eq!(random_seed(), [42u8; 32]);
		});
	}

	#[test]
	fn local_storage_set_get_compare_and_set_clear() {
		let (offchain, _state) = testing::TestOffchainExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(OffchainDbExt::new(offchain));

		ext.execute_with(|| {
			assert_eq!(local_storage_get(StorageKind::PERSISTENT, b"key"), None);

			local_storage_set(StorageKind::PERSISTENT, b"key", b"value");
			assert_eq!(local_storage_get(StorageKind::PERSISTENT, b"key"), Some(b"value".to_vec()),);

			assert!(local_storage_compare_and_set(
				StorageKind::PERSISTENT,
				b"key",
				Some(b"value".to_vec()),
				b"new",
			));
			assert_eq!(local_storage_get(StorageKind::PERSISTENT, b"key"), Some(b"new".to_vec()),);

			// A wrong old value must not set and must return false.
			assert!(!local_storage_compare_and_set(
				StorageKind::PERSISTENT,
				b"key",
				Some(b"wrong".to_vec()),
				b"nope",
			));
			assert_eq!(local_storage_get(StorageKind::PERSISTENT, b"key"), Some(b"new".to_vec()),);

			// CAS on a missing key with `None` as the old value sets it.
			assert!(local_storage_compare_and_set(StorageKind::PERSISTENT, b"other", None, b"v2",));

			local_storage_clear(StorageKind::PERSISTENT, b"key");
			assert_eq!(local_storage_get(StorageKind::PERSISTENT, b"key"), None);
		});
	}

	#[test]
	fn http_request_round_trip() {
		let (offchain, state) = testing::TestOffchainExt::new();
		let mut ext = TestExternalities::default();
		ext.register_extension(OffchainWorkerExt::new(offchain));

		state.write().expect_request(testing::PendingRequest {
			method: "GET".into(),
			uri: "http://localhost:1234/test".into(),
			headers: vec![("Header".into(), "Value".into())],
			sent: true,
			response: Some(b"response body".to_vec()),
			response_headers: vec![("X-Test".into(), "header value".into())],
			..Default::default()
		});

		ext.execute_with(|| {
			let id =
				http_request_start("GET", "http://localhost:1234/test", &[]).expect("starts; qed");
			http_request_add_header(id, "Header", "Value").expect("header added; qed");
			http_request_write_body(id, &[], None).expect("body written; qed");

			let statuses = http_response_wait(&[id], None);
			assert_eq!(statuses, vec![HttpRequestStatus::Finished(200)]);

			let headers = http_response_headers(id);
			assert_eq!(headers, vec![(b"X-Test".to_vec(), b"header value".to_vec())]);

			let mut buf = [0u8; 64];
			let read = http_response_read_body(id, &mut buf, None).expect("body read; qed");
			assert_eq!(&buf[..read as usize], b"response body");
			// The body is fully consumed; a further read returns 0.
			assert_eq!(http_response_read_body(id, &mut buf, None).expect("read; qed"), 0);
		});
	}
}
