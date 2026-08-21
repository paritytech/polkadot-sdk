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

//! Native PolkaVM/JAM implementations of the `offchain` interface.

use crate::*;
use alloc::{vec, vec::Vec};
#[cfg(feature = "bandersnatch-experimental")]
use sp_core::bandersnatch;
#[cfg(feature = "bls-experimental")]
use sp_core::{bls381, ecdsa_bls381};
use sp_core::{
	offchain::{
		HttpError, HttpRequestId, HttpRequestStatus, OpaqueNetworkState, StorageKind, Timestamp,
	},
	OpaquePeerId,
};
/// Native PolkaVM/JAM implementation of `http_request_add_header`.
pub fn http_request_add_header(
	_request_id: HttpRequestId,
	_name: &str,
	_value: &str,
) -> Result<(), ()> {
	panic!("`offchain::http_request_add_header` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_request_start`.
pub fn http_request_start(_method: &str, _uri: &str, _meta: Vec<u8>) -> Result<HttpRequestId, ()> {
	panic!("`offchain::http_request_start` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_request_write_body`.
pub fn http_request_write_body(
	_request_id: HttpRequestId,
	_chunk: &[u8],
	_deadline: Option<Timestamp>,
) -> Result<(), HttpError> {
	panic!("`offchain::http_request_write_body` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_response_header_name`.
pub fn http_response_header_name(
	_request_id: HttpRequestId,
	_header_index: u32,
	_out: &mut [u8],
) -> Option<u32> {
	panic!("`offchain::http_response_header_name` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_response_header_value`.
pub fn http_response_header_value(
	_request_id: HttpRequestId,
	_header_index: u32,
	_out: &mut [u8],
) -> Option<u32> {
	panic!("`offchain::http_response_header_value` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_response_read_body`.
pub fn http_response_read_body(
	_request_id: HttpRequestId,
	_buffer_out: &mut [u8],
	_deadline: Option<Timestamp>,
) -> Result<u32, HttpError> {
	panic!("`offchain::http_response_read_body` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `http_response_wait__raw`.
pub fn http_response_wait__raw(
	_ids: &[HttpRequestId],
	_deadline: Option<Timestamp>,
	_out: &mut [u32],
) {
	panic!("`offchain::http_response_wait__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `is_validator`.
pub fn is_validator() -> bool {
	panic!("`offchain::is_validator` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `local_storage_clear`.
pub fn local_storage_clear(_kind: StorageKind, _key: &[u8]) {
	panic!(
		"`offchain::local_storage_clear` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `local_storage_compare_and_set`.
pub fn local_storage_compare_and_set(
	_kind: StorageKind,
	_key: &[u8],
	_old_value: Option<Vec<u8>>,
	_new_value: &[u8],
) -> bool {
	panic!("`offchain::local_storage_compare_and_set` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `local_storage_read`.
pub fn local_storage_read(
	_kind: StorageKind,
	_key: &[u8],
	_value_out: &mut [u8],
	_offset: u32,
) -> Option<u32> {
	panic!("`offchain::local_storage_read` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `local_storage_set`.
pub fn local_storage_set(_kind: StorageKind, _key: &[u8], _value: &[u8]) {
	panic!("`offchain::local_storage_set` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `network_peer_id`.
pub fn network_peer_id(_out: &mut NetworkPeerId) -> Result<(), ()> {
	panic!("`offchain::network_peer_id` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `random_seed__raw`.
pub fn random_seed__raw(_out: &mut [u8; 32]) {
	panic!("`offchain::random_seed__raw` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `set_authorized_nodes`.
pub fn set_authorized_nodes(_nodes: Vec<OpaquePeerId>, _authorized_only: bool) {
	panic!(
		"`offchain::set_authorized_nodes` needs node-side state and has no in-blob implementation"
	)
}

/// Native PolkaVM/JAM implementation of `sleep_until`.
pub fn sleep_until(_deadline: Timestamp) {
	panic!("`offchain::sleep_until` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `submit_transaction`.
pub fn submit_transaction(_data: Vec<u8>) -> Result<(), ()> {
	panic!("`offchain::submit_transaction` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `timestamp`.
pub fn timestamp() -> Timestamp {
	panic!("`offchain::timestamp` needs node-side state and has no in-blob implementation")
}

/// Native PolkaVM/JAM implementation of `random_seed`.
pub fn random_seed() -> [u8; 32] {
	let mut seed = [0u8; 32];
	random_seed__raw(&mut seed);
	seed
}

/// Native PolkaVM/JAM implementation of `local_storage_get`.
pub fn local_storage_get(kind: StorageKind, key: impl AsRef<[u8]>) -> Option<Vec<u8>> {
	let mut value_out = vec![0u8; 256];
	let len = local_storage_read(kind, key.as_ref(), &mut value_out[..], 0)?;
	if len as usize > value_out.len() {
		value_out.resize(len as usize, 0);
		local_storage_read(kind, key.as_ref(), &mut value_out[..], 0)?;
	}
	value_out.truncate(len as usize);
	Some(value_out)
}

/// Native PolkaVM/JAM implementation of `http_response_wait`.
pub fn http_response_wait(
	ids: &[HttpRequestId],
	deadline: Option<Timestamp>,
) -> Vec<HttpRequestStatus> {
	let mut statuses = vec![0u32; ids.len()];
	http_response_wait__raw(&ids, deadline.into(), &mut statuses[..]);
	statuses
		.into_iter()
		.map(|s| HttpRequestStatus::try_from(s).unwrap_or(HttpRequestStatus::Invalid))
		.collect::<Vec<_>>()
}

/// Native PolkaVM/JAM implementation of `http_response_headers`.
pub fn http_response_headers(request_id: HttpRequestId) -> Vec<(Vec<u8>, Vec<u8>)> {
	let mut name_buf = vec![0u8; 256];
	let mut value_buf = vec![0u8; 256];
	let mut head_idx = 0;
	let mut headers = Vec::new();

	while let Some(name_len) = http_response_header_name(request_id, head_idx, &mut name_buf[..]) {
		let name_len = name_len as usize;
		if name_len > name_buf.len() {
			name_buf.resize(name_len, 0);
			http_response_header_name(request_id, head_idx, &mut name_buf[..])
				.expect("It was checked that the header exists");
		}
		let value_len = http_response_header_value(request_id, head_idx, &mut value_buf[..])
			.expect("It was checked that the header exists") as usize;
		if value_len > value_buf.len() {
			value_buf.resize(value_len, 0);
			http_response_header_value(request_id, head_idx, &mut value_buf[..])
				.expect("It was checked that the header exists");
		}
		headers.push((name_buf[..name_len].to_vec(), value_buf[..value_len].to_vec()));
		head_idx += 1;
	}
	headers
}
