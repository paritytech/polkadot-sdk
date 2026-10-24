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
//!
//! The magic prefixes not only indicate that the blob is compressed, but also carry
//! the type of the compressed blob, as specified in [RFC-135].
//!
//! [RFC-135]: https://polkadot-fellows.github.io/RFCs/approved/0135-compressed-blob-prefixes.html

use std::{
	borrow::Cow,
	io::{Read, Write},
};

// An open list of prefixes, indicating that a blob beginning with one of them is a compressed
// blob of a specific type, compressed with a specific compression method, as specified in
// RFC-135.
//
// These differ from the WASM magic bytes, so real WASM blobs will not have these prefixes.

// A Zstd-compressed blob of a non-specified type (legacy). Only Wasm code and PoVs were in
// use when this prefix was introduced, so a blob prefixed with it may only contain one of those.
const CBLOB_ZSTD_LEGACY: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 5];
// A Zstd-compressed proof-of-validity blob.
const CBLOB_ZSTD_POV: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 6];
// A Zstd-compressed Wasm code blob.
const CBLOB_ZSTD_WASM_CODE: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 7];
// A Zstd-compressed PolkaVM code blob.
const CBLOB_ZSTD_PVM_CODE: [u8; 8] = [82, 188, 83, 118, 70, 219, 142, 8];

const CBLOB_PREFIX_LEN: usize = 8;

// Magic bytes of an uncompressed Wasm code blob.
const WASM_MAGIC: [u8; 4] = [0, b'a', b's', b'm'];
// Magic bytes of an uncompressed PolkaVM code blob.
const PVM_MAGIC: [u8; 4] = [b'P', b'V', b'M', 0];

/// A recommendation for the bomb limit for code blobs.
///
/// This may be adjusted upwards in the future, but is set much higher than the
/// expected maximum code size. When adjusting upwards, nodes should be updated
/// before performing a runtime upgrade to a blob with larger compressed size.
pub const CODE_BLOB_BOMB_LIMIT: usize = 50 * 1024 * 1024;

/// A type of a maybe compressed blob.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum MaybeCompressedBlobType {
	/// A proof-of-validity blob.
	Pov,
	/// A Wasm code blob.
	Wasm,
	/// A PolkaVM code blob.
	Pvm,
	/// A blob compressed with the legacy prefix, not carrying any type information.
	/// May contain either Wasm code or a proof-of-validity.
	Legacy,
}

impl MaybeCompressedBlobType {
	/// Returns `true` if the blob type is known to be (or, in the case of
	/// [`MaybeCompressedBlobType::Legacy`], may be) executable code.
	pub fn is_code(&self) -> bool {
		matches!(
			self,
			MaybeCompressedBlobType::Wasm |
				MaybeCompressedBlobType::Pvm |
				MaybeCompressedBlobType::Legacy
		)
	}

	/// Returns `true` if the blob type is known to be (or, in the case of
	/// [`MaybeCompressedBlobType::Legacy`], may be) a proof-of-validity.
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

/// Decode a blob, if it indicates that it is compressed, checking that its type matches the
/// expected one. Provide a `bomb_limit`, which is the limit of bytes which should be decompressed
/// from the blob.
///
/// A blob that carries type information (either a compressed blob prefix or an uncompressed code
/// magic) is only accepted if its type is compatible with `ty`, with
/// [`MaybeCompressedBlobType::Legacy`] considered compatible with both
/// [`MaybeCompressedBlobType::Pov`] and [`MaybeCompressedBlobType::Wasm`]. A blob that does not
/// carry any type information (like an uncompressed proof-of-validity) is passed through
/// unmodified.
pub fn decompress_as(
	ty: MaybeCompressedBlobType,
	blob: &[u8],
	bomb_limit: usize,
) -> Result<Cow<'_, [u8]>, Error> {
	use MaybeCompressedBlobType::*;
	if let Ok(blob_type) = blob_type(blob) {
		match ty {
			Pov if blob_type != Pov && blob_type != Legacy => return Err(Error::Invalid),
			Wasm if blob_type != Wasm && blob_type != Legacy => return Err(Error::Invalid),
			Pvm if blob_type != Pvm => return Err(Error::Invalid),
			Legacy if blob_type != Legacy => return Err(Error::Invalid),
			_ => (),
		}
	}

	if is_compressed(blob) {
		decompress_zstd(&blob[CBLOB_PREFIX_LEN..], bomb_limit).map(Into::into)
	} else {
		Ok(blob.into())
	}
}

/// Weakly compress a blob of the given type, whose size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob,
/// as the decoder will not be able to be able to differentiate it from a compression bomb.
pub fn compress_weakly_as(
	ty: MaybeCompressedBlobType,
	blob: &[u8],
	bomb_limit: usize,
) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 3)
}

/// Strongly compress a blob of the given type, whose size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
pub fn compress_strongly_as(
	ty: MaybeCompressedBlobType,
	blob: &[u8],
	bomb_limit: usize,
) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 22)
}

/// Compress a blob of the given type, whose size is limited by `bomb_limit`.
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
#[deprecated(
	note = "Will be removed after June 2026. Use compress_strongly_as or compress_weakly_as instead"
)]
pub fn compress_as(ty: MaybeCompressedBlobType, blob: &[u8], bomb_limit: usize) -> Option<Vec<u8>> {
	compress_with_level_as(ty, blob, bomb_limit, 3)
}

/// Compress a blob of the given type, whose size is limited by `bomb_limit`, with adjustable
/// compression level.
///
/// The levels are passed through to `zstd` and can be in range [1, 22] (weakest to strongest).
///
/// If the blob's size is over the bomb limit, this will not compress the blob, as the decoder will
/// not be able to be able to differentiate it from a compression bomb.
fn compress_with_level_as(
	ty: MaybeCompressedBlobType,
	blob: &[u8],
	bomb_limit: usize,
	level: i32,
) -> Option<Vec<u8>> {
	if blob.len() > bomb_limit {
		return None;
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

/// Determine the type of a maybe compressed blob.
///
/// The type is determined either by the compressed blob prefix or, for uncompressed blobs, by
/// the code magic bytes. Uncompressed blobs not bearing any known magic bytes (like uncompressed
/// proofs-of-validity) cannot be typed, and an error is returned.
pub fn blob_type(blob: &[u8]) -> Result<MaybeCompressedBlobType, Error> {
	if blob.starts_with(&CBLOB_ZSTD_PVM_CODE) || blob.starts_with(&PVM_MAGIC) {
		Ok(MaybeCompressedBlobType::Pvm)
	} else if blob.starts_with(&CBLOB_ZSTD_WASM_CODE) || blob.starts_with(&WASM_MAGIC) {
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
	use MaybeCompressedBlobType::{Legacy, Pov, Pvm, Wasm};

	const BOMB_LIMIT: usize = 10;

	#[test]
	fn refuse_to_encode_over_limit() {
		let mut v = vec![0; BOMB_LIMIT + 1];
		assert!(compress_weakly_as(Legacy, &v, BOMB_LIMIT).is_none());
		assert!(compress_strongly_as(Legacy, &v, BOMB_LIMIT).is_none());

		let _ = v.pop();
		assert!(compress_weakly_as(Legacy, &v, BOMB_LIMIT).is_some());
		assert!(compress_strongly_as(Legacy, &v, BOMB_LIMIT).is_some());
	}

	#[test]
	fn compress_and_decompress() {
		let v = vec![0; BOMB_LIMIT];

		let compressed_weakly = compress_weakly_as(Legacy, &v, BOMB_LIMIT).unwrap();
		let compressed_strongly = compress_strongly_as(Legacy, &v, BOMB_LIMIT).unwrap();

		assert!(compressed_weakly.starts_with(&CBLOB_ZSTD_LEGACY));
		assert!(compressed_strongly.starts_with(&CBLOB_ZSTD_LEGACY));

		assert_eq!(&decompress_as(Legacy, &compressed_weakly, BOMB_LIMIT).unwrap()[..], &v[..]);
		assert_eq!(&decompress_as(Legacy, &compressed_strongly, BOMB_LIMIT).unwrap()[..], &v[..]);
	}

	#[test]
	fn typed_roundtrips_work() {
		let v = vec![0; BOMB_LIMIT];

		for ty in [Pov, Wasm, Pvm, Legacy] {
			let compressed = compress_weakly_as(ty, &v, BOMB_LIMIT).unwrap();
			assert_eq!(blob_type(&compressed), Ok(ty));
			assert_eq!(&decompress_as(ty, &compressed, BOMB_LIMIT).unwrap()[..], &v[..]);
		}
	}

	#[test]
	fn legacy_blobs_decompress_as_pov_and_wasm() {
		let v = vec![0; BOMB_LIMIT];
		let compressed = compress_weakly_as(Legacy, &v, BOMB_LIMIT).unwrap();

		assert_eq!(&decompress_as(Pov, &compressed, BOMB_LIMIT).unwrap()[..], &v[..]);
		assert_eq!(&decompress_as(Wasm, &compressed, BOMB_LIMIT).unwrap()[..], &v[..]);
		assert_eq!(decompress_as(Pvm, &compressed, BOMB_LIMIT).err(), Some(Error::Invalid));
	}

	#[test]
	fn type_mismatch_fails() {
		let v = vec![0; BOMB_LIMIT];

		let compressed_pov = compress_weakly_as(Pov, &v, BOMB_LIMIT).unwrap();
		assert_eq!(decompress_as(Wasm, &compressed_pov, BOMB_LIMIT).err(), Some(Error::Invalid));
		assert_eq!(decompress_as(Pvm, &compressed_pov, BOMB_LIMIT).err(), Some(Error::Invalid));
		assert_eq!(decompress_as(Legacy, &compressed_pov, BOMB_LIMIT).err(), Some(Error::Invalid));

		let compressed_wasm = compress_weakly_as(Wasm, &v, BOMB_LIMIT).unwrap();
		assert_eq!(decompress_as(Pov, &compressed_wasm, BOMB_LIMIT).err(), Some(Error::Invalid));
		assert_eq!(decompress_as(Pvm, &compressed_wasm, BOMB_LIMIT).err(), Some(Error::Invalid));

		let compressed_pvm = compress_weakly_as(Pvm, &v, BOMB_LIMIT).unwrap();
		assert_eq!(decompress_as(Pov, &compressed_pvm, BOMB_LIMIT).err(), Some(Error::Invalid));
		assert_eq!(decompress_as(Wasm, &compressed_pvm, BOMB_LIMIT).err(), Some(Error::Invalid));
	}

	#[test]
	fn uncompressed_code_magic_is_recognized() {
		let wasm = b"\0asm_not_a_real_wasm_blob".to_vec();
		assert_eq!(blob_type(&wasm), Ok(Wasm));
		assert_eq!(&decompress_as(Wasm, &wasm, BOMB_LIMIT).unwrap()[..], &wasm[..]);
		assert_eq!(decompress_as(Pov, &wasm, BOMB_LIMIT).err(), Some(Error::Invalid));

		let pvm = b"PVM\0not_a_real_pvm_blob".to_vec();
		assert_eq!(blob_type(&pvm), Ok(Pvm));
		assert_eq!(&decompress_as(Pvm, &pvm, BOMB_LIMIT).unwrap()[..], &pvm[..]);
		assert_eq!(decompress_as(Wasm, &pvm, BOMB_LIMIT).err(), Some(Error::Invalid));
	}

	#[test]
	fn untyped_blobs_pass_through() {
		let v = vec![1; BOMB_LIMIT + 1];

		assert_eq!(blob_type(&v), Err(Error::Invalid));
		assert_eq!(&decompress_as(Pov, &v, BOMB_LIMIT).unwrap()[..], &v[..]);
		assert_eq!(&decompress_as(Legacy, &v, BOMB_LIMIT).unwrap()[..], &v[..]);
	}

	#[test]
	fn decompresses_only_when_magic() {
		let v = vec![0; BOMB_LIMIT + 1];

		assert_eq!(&decompress_as(Legacy, &v, BOMB_LIMIT).unwrap()[..], &v[..]);
	}

	#[test]
	fn possible_bomb_fails() {
		let encoded_bigger_than_bomb = vec![0; BOMB_LIMIT + 1];
		let mut buf = CBLOB_ZSTD_LEGACY.to_vec();

		{
			let mut v = zstd::Encoder::new(&mut buf, 3).unwrap().auto_finish();
			v.write_all(&encoded_bigger_than_bomb[..]).unwrap();
		}

		assert_eq!(decompress_as(Legacy, &buf[..], BOMB_LIMIT).err(), Some(Error::PossibleBomb));
	}
}
