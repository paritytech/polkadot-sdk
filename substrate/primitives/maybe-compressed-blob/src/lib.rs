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

//! Handling of blobs that may be compressed, based on an 8-byte magic identifier
//! at the head.

use std::{
	borrow::Cow,
	io::{Read, Write},
};

// An arbitrary prefix, that indicates a blob beginning with should be decompressed with
// Zstd compression.
//
// This differs from the WASM magic bytes, so real WASM blobs will not have this prefix.
const CBLOB_ZSTD_LEGACY: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 5];
const CBLOB_ZSTD_POV: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 6];
const CBLOB_ZSTD_WASM_CODE: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 7];
const CBLOB_ZSTD_PVM_CODE: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 8];

const CBLOB_PREFIX_LEN: usize = 8;

/// A recommendation for the bomb limit for code blobs.
///
/// This may be adjusted upwards in the future, but is set much higher than the
/// expected maximum code size. When adjusting upwards, nodes should be updated
/// before performing a runtime upgrade to a blob with larger compressed size.
pub const CODE_BLOB_BOMB_LIMIT: usize = 50 * 1024 * 1024;

/// A type of compressed blob.
#[derive(PartialEq, Clone, Copy)]
pub enum MaybeCompressedBlobType {
	Pov,
	Wasm,
	Pvm,
	Legacy,
}

impl MaybeCompressedBlobType {
	pub fn is_code(&self) -> bool {
		matches!(
			self,
			MaybeCompressedBlobType::Wasm |
				MaybeCompressedBlobType::Pvm |
				MaybeCompressedBlobType::Legacy
		)
	}

	pub fn is_pov(&self) -> bool {
		matches!(self, MaybeCompressedBlobType::Pov | MaybeCompressedBlobType::Legacy)
	}
}

/// A possible bomb was encountered.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum Error {
	/// Decoded size was too large, and the code payload may be a bomb.
	#[error("Possible compression bomb encountered")]
	PossibleBomb,
	/// The compressed value had an invalid format.
	#[error("Blob had invalid format")]
	Invalid,
}

fn read_from_decoder(
	decoder: impl Read,
	blob_len: usize,
	bomb_limit: usize,
) -> Result<Vec<u8>, Error> {
	let mut decoder = decoder.take((bomb_limit + 1) as u64);

	let mut buf = Vec::with_capacity(blob_len);
	decoder.read_to_end(&mut buf).map_err(|_| Error::Invalid)?;

	if buf.len() <= bomb_limit {
		Ok(buf)
	} else {
		Err(Error::PossibleBomb)
	}
}

fn is_compressed(blob: &[u8]) -> bool {
	blob.starts_with(&CBLOB_ZSTD_LEGACY) ||
		blob.starts_with(&CBLOB_ZSTD_POV) ||
		blob.starts_with(&CBLOB_ZSTD_WASM_CODE) ||
		blob.starts_with(&CBLOB_ZSTD_PVM_CODE)
}

fn decompress_zstd(blob: &[u8], bomb_limit: usize) -> Result<Vec<u8>, Error> {
	let decoder = zstd::Decoder::new(blob).map_err(|_| Error::Invalid)?;

	read_from_decoder(decoder, blob.len(), bomb_limit)
}

/// Decode a blob, if it indicates that it is compressed, checking its type. Provide a `bomb_limit`,
/// which is the limit of bytes which should be decompressed from the blob.
pub fn decompress_as(
	ty: MaybeCompressedBlobType,
	blob: &[u8],
	bomb_limit: usize,
) -> Result<Cow<[u8]>, Error> {
	use MaybeCompressedBlobType::*;
	let blob_type = blob_type(blob)?;
	match ty {
		Pov if blob_type != Pov && blob_type != Legacy => return Err(Error::Invalid),
		Wasm if blob_type != Wasm && blob_type != Legacy => return Err(Error::Invalid),
		Pvm if blob_type != Pvm => return Err(Error::Invalid),
		Legacy if blob_type != Legacy => return Err(Error::Invalid),
		_ => (),
	}

	if is_compressed(blob) {
		decompress_zstd(&blob[CBLOB_PREFIX_LEN..], bomb_limit).map(Into::into)
	} else {
		Ok(blob.into())
	}
}

/// Weakly compress a blob who's size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob,
/// as the decoder will not be able to be able to differentiate it from a compression bomb.
pub fn compress_weakly_as(ty: MaybeCompressedBlobType, blob: &[u8], bomb_limit: usize) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 3)
}

/// Strongly compress a blob who's size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
pub fn compress_strongly_as(ty: MaybeCompressedBlobType, blob: &[u8], bomb_limit: usize) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 22)
}

/// Compress a blob who's size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
#[deprecated(
	note = "Will be removed after June 2026. Use compress_strongly, compress_weakly or compress_with_level instead"
)]
pub fn compress_as(ty: MaybeCompressedBlobType, blob: &[u8], bomb_limit: usize) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 3)
}

/// Compress a blob who's size is limited by `bomb_limit` with adjustable compression level.
///
/// The levels are passed through to `zstd` and can be in range [1, 22] (weakest to strongest).
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
fn compress_with_level_as(ty: MaybeCompressedBlobType, blob: &[u8], bomb_limit: usize, level: i32) -> Option<Vec<u8>> {
	if blob.len() > bomb_limit {
		return None
	}

	let mut buf = match ty {
		MaybeCompressedBlobType::Pov => CBLOB_ZSTD_POV,
		MaybeCompressedBlobType::Wasm => CBLOB_ZSTD_WASM_CODE,
		MaybeCompressedBlobType::Pvm => CBLOB_ZSTD_PVM_CODE,
		MaybeCompressedBlobType::Legacy => CBLOB_ZSTD_LEGACY,
	}
	.to_vec();

	{
		let mut v = zstd::Encoder::new(&mut buf, level).ok()?.auto_finish();
		v.write_all(blob).ok()?;
	}

	Some(buf)
}

/// Determine the type of a compressed blob.
pub fn blob_type(blob: &[u8]) -> Result<MaybeCompressedBlobType, Error> {
	if blob.starts_with(&CBLOB_ZSTD_PVM_CODE) || blob.starts_with(b"PVM\x00") {
		Ok(MaybeCompressedBlobType::Pvm)
	} else if blob.starts_with(&CBLOB_ZSTD_WASM_CODE) || blob.starts_with(b"\x00asm") {
		Ok(MaybeCompressedBlobType::Wasm)
	} else if blob.starts_with(&CBLOB_ZSTD_POV) {
		Ok(MaybeCompressedBlobType::Pov)
	} else if blob.starts_with(&CBLOB_ZSTD_LEGACY) {
		Ok(MaybeCompressedBlobType::Legacy)
	} else {
		Err(Error::Invalid)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	const BOMB_LIMIT: usize = 10;

	#[test]
	fn refuse_to_encode_over_limit() {
		let mut v = vec![0; BOMB_LIMIT + 1];
		assert!(compress_weakly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).is_none());
		assert!(compress_strongly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).is_none());

		let _ = v.pop();
		assert!(compress_weakly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).is_some());
		assert!(compress_strongly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).is_some());
	}

	#[test]
	fn compress_and_decompress() {
		let v = vec![0; BOMB_LIMIT];

		let compressed_weakly = compress_weakly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).unwrap();
		let compressed_strongly = compress_strongly_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).unwrap();

		assert!(compressed_weakly.starts_with(&CBLOB_ZSTD_LEGACY));
		assert!(compressed_strongly.starts_with(&CBLOB_ZSTD_LEGACY));

		assert_eq!(&decompress_as(MaybeCompressedBlobType::Legacy, &compressed_weakly, BOMB_LIMIT).unwrap()[..], &v[..]);
		assert_eq!(&decompress_as(MaybeCompressedBlobType::Legacy, &compressed_strongly, BOMB_LIMIT).unwrap()[..], &v[..]);
	}

	#[test]
	fn decompresses_only_when_magic() {
		let v = vec![0; BOMB_LIMIT + 1];

		assert_eq!(
			&decompress_as(MaybeCompressedBlobType::Legacy, &v, BOMB_LIMIT).unwrap()[..],
			&v[..]
		);
	}

	#[test]
	fn possible_bomb_fails() {
		let encoded_bigger_than_bomb = vec![0; BOMB_LIMIT + 1];
		let mut buf = CBLOB_ZSTD_LEGACY.to_vec();

		{
			let mut v = zstd::Encoder::new(&mut buf, 3).unwrap().auto_finish();
			v.write_all(&encoded_bigger_than_bomb[..]).unwrap();
		}

		assert_eq!(
			decompress_as(MaybeCompressedBlobType::Legacy, &buf[..], BOMB_LIMIT).err(),
			Some(Error::PossibleBomb)
		);
	}
}
