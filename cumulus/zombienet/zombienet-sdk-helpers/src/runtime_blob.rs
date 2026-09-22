// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! Inflates runtime blobs for runtime-upgrade tests.
//!
//! A runtime upgrade only triggers the full-core logic when the new runtime is bigger than the PoV
//! limit of a single core, so tests pad the runtime blob until it crosses that threshold. WASM
//! blobs also bump the embedded `spec_version`, because `apply_authorized_upgrade` on Polkadot
//! rejects a runtime whose `spec_version` did not increase. JAM tests instead use
//! `System::authorize_upgrade_without_checks`, so no version bump is required; padding alone
//! changes the blob's blake2b-256 hash, which is what the JAM code-upgrade lifecycle keys on.

use anyhow::anyhow;

/// The magic bytes every PolkaVM program blob starts with.
const POLKAVM_MAGIC: &[u8; 4] = b"PVM\0";

/// The size of one encoded runtime API info entry: 8 bytes id and 4 bytes version.
const RUNTIME_API_INFO_SIZE: usize = 12;

/// Pads `blob` until it reaches `min_size`, bumping the embedded `spec_version` where required.
///
/// Both formats are supported:
/// - WASM blobs are decompressed, the `spec_version` is bumped, the blob is padded with a
///   pseudo-random `padding` custom section, and then re-compressed until the compressed size
///   reaches `min_size`.
/// - PolkaVM program blobs (`PVM\0` magic) are uncompressed and grown with at least one
///   [`sp_polkavm_blob::PADDING_SECTION`] section, and as many more as `min_size` needs. No version
///   bump: JAM tests use `System::authorize_upgrade_without_checks`, so the padding alone has to
///   make the result differ from the running code.
pub fn inflate_runtime_wasm(blob: &[u8], min_size: usize) -> Result<Vec<u8>, anyhow::Error> {
	if blob.starts_with(POLKAVM_MAGIC) {
		inflate_polkavm_blob(blob, min_size)
	} else {
		inflate_wasm_blob(blob, min_size)
	}
}

fn inflate_polkavm_blob(blob: &[u8], min_size: usize) -> Result<Vec<u8>, anyhow::Error> {
	let chunk_size = 256 * 1024;
	let mut blob = blob.to_vec();
	// At least one chunk, even when the blob already exceeds `min_size`: with no `spec_version` to
	// bump, padding is the only thing that distinguishes the upgrade target from the running code,
	// and a target that hashes to the active code is reported as applied without any preimage.
	loop {
		blob = sp_polkavm_blob::append_custom_section(
			&blob,
			sp_polkavm_blob::PADDING_SECTION,
			&vec![0; chunk_size],
		);
		if blob.len() >= min_size {
			break;
		}
	}
	Ok(blob)
}

fn inflate_wasm_blob(compressed_wasm: &[u8], min_size: usize) -> Result<Vec<u8>, anyhow::Error> {
	let mut wasm = sp_maybe_compressed_blob::decompress(compressed_wasm, 50 * 1024 * 1024)
		.map_err(|e| anyhow!("Decompression failed: {:?}", e))?
		.into_owned();

	// Bump the `spec_version` so that `apply_authorized_upgrade`'s version check passes.
	// On chain nothing will change, as we only change the runtime version stored inside the wasm
	// file.
	let mut version = read_wasm_embedded_version(&wasm)?;
	version.spec_version += 1;
	wasm = sp_version::embed::embed_runtime_version(&wasm, version)?;

	let mut rng_state: u64 = 0xdeadbeef;
	let mut padding = Vec::new();
	let chunk_size = 256 * 1024;
	loop {
		padding.extend((0..chunk_size).map(|_| {
			// xorshift64
			rng_state ^= rng_state << 13;
			rng_state ^= rng_state >> 7;
			rng_state ^= rng_state << 17;
			rng_state as u8
		}));

		let mut module: parity_wasm::elements::Module =
			parity_wasm::deserialize_buffer(&wasm).map_err(|e| anyhow!("wasm parse: {e:?}"))?;
		module.set_custom_section("padding", padding.clone());
		wasm = parity_wasm::serialize(module).map_err(|e| anyhow!("wasm serialize: {e:?}"))?;

		let compressed = sp_maybe_compressed_blob::compress_weakly(&wasm, 50 * 1024 * 1024)
			.ok_or_else(|| anyhow!("Compression failed"))?;
		log::info!(
			"Inflated WASM: uncompressed={} bytes, compressed={} bytes (target={})",
			wasm.len(),
			compressed.len(),
			min_size,
		);
		if compressed.len() >= min_size {
			return Ok(compressed);
		}
	}
}

/// Reads the runtime version embedded in a WASM blob.
///
/// Mirrors the WASM branch of `sc_executor::read_embedded_version` without pulling the executor
/// into this crate. The `runtime_version` section always carries an empty `apis` field, so the
/// `Core` api version from the `runtime_apis` section is required as a decoding hint.
fn read_wasm_embedded_version(wasm: &[u8]) -> Result<sp_version::RuntimeVersion, anyhow::Error> {
	let module: parity_wasm::elements::Module =
		parity_wasm::deserialize_buffer(wasm).map_err(|e| anyhow!("wasm parse: {e:?}"))?;

	let version_section = module
		.custom_sections()
		.find(|section| section.name() == "runtime_version")
		.map(|section| section.payload())
		.ok_or_else(|| anyhow!("No runtime version found?"))?;

	let apis: sp_version::ApisVec = module
		.custom_sections()
		.find(|section| section.name() == "runtime_apis")
		.map(|section| section.payload())
		.unwrap_or(&[])
		.chunks(RUNTIME_API_INFO_SIZE)
		.map(|chunk| {
			<[u8; RUNTIME_API_INFO_SIZE]>::try_from(chunk)
				.map(deserialize_runtime_api_info)
				.map_err(|_| anyhow!("the `runtime_apis` section is not a multiple of 12 bytes"))
		})
		.collect::<Result<Vec<_>, _>>()?
		.into();

	let core_version = sp_version::core_version_from_apis(&apis);
	let mut version = sp_version::RuntimeVersion::decode_with_version_hint(
		&mut &version_section[..],
		core_version,
	)
	.map_err(|e| anyhow!("failed to decode the `runtime_version` section: {e}"))?;
	version.apis = apis;

	Ok(version)
}

/// Deserializes a runtime API info entry serialized by `sp_api::serialize_runtime_api_info`.
fn deserialize_runtime_api_info(bytes: [u8; RUNTIME_API_INFO_SIZE]) -> ([u8; 8], u32) {
	let id: [u8; 8] = bytes[0..8]
		.try_into()
		.expect("the source slice size is equal to the dest array length; qed");

	let version = u32::from_le_bytes(
		bytes[8..12]
			.try_into()
			.expect("the source slice size is equal to the array length; qed"),
	);

	(id, version)
}

#[cfg(test)]
mod tests {
	use super::*;

	/// Section ids as defined by `polkavm-common`.
	const SECTION_MEMORY_CONFIG: u8 = 1;
	const SECTION_RO_DATA: u8 = 2;
	const SECTION_RW_DATA: u8 = 3;
	const SECTION_EXPORTS: u8 = 5;
	const SECTION_CODE_AND_JUMP_TABLE: u8 = 6;

	/// Builds a minimal but structurally valid program blob.
	fn valid_blob() -> Vec<u8> {
		let mut blob = Vec::new();
		blob.extend_from_slice(b"PVM\0");
		blob.push(0); // `InstructionSetKind::ReviveV1`
		blob.extend_from_slice(&[0; 8]); // `blob_len`, patched below

		// Memory config: empty ro/rw data and stack.
		blob.extend_from_slice(&[SECTION_MEMORY_CONFIG, 3, 0, 0, 0]);
		// Empty ro data, rw data and exports.
		blob.extend_from_slice(&[SECTION_RO_DATA, 0, SECTION_RW_DATA, 0, SECTION_EXPORTS, 0]);
		// Code and jump table: no jump table entries, no code.
		blob.extend_from_slice(&[SECTION_CODE_AND_JUMP_TABLE, 3, 0, 0, 0]);
		blob.push(0); // end-of-file marker
		patch_blob_len(&mut blob);
		blob
	}

	fn patch_blob_len(blob: &mut [u8]) {
		let len = (blob.len() as u64).to_le_bytes();
		blob[5..13].copy_from_slice(&len);
	}

	#[test]
	fn pvm_blob_is_inflated_to_min_size() {
		let blob = valid_blob();
		let min_size = 512 * 1024;

		let inflated = inflate_runtime_wasm(&blob, min_size).expect("inflation must succeed");

		assert!(inflated.len() >= min_size, "inflated blob too small: {}", inflated.len());
		polkavm::ProgramBlob::parse(inflated.as_slice().into())
			.expect("the inflated blob must still be a valid PolkaVM program");
		// `0xC8` was the runtime-version section id. Nothing writes it any more; this guards
		// against inflation growing a version section again.
		assert!(
			sp_polkavm_blob::custom_section(&inflated, 0xC8).is_none(),
			"no runtime version section should be present"
		);
		assert_ne!(sp_crypto_hashing::blake2_256(&inflated), sp_crypto_hashing::blake2_256(&blob),);
	}

	/// The real template blob is ~6.7 MiB, well over the ~3 MiB `MIN_RUNTIME_SIZE_BYTES` the
	/// block-bundling test asks for. Returning it unpadded would make the upgrade target hash to
	/// the code already running, and the service would report the upgrade as applied without any
	/// preimage ever being submitted.
	#[test]
	fn pvm_blob_already_over_min_size_still_changes_hash() {
		let mut blob = valid_blob();
		blob = sp_polkavm_blob::append_custom_section(
			&blob,
			sp_polkavm_blob::PADDING_SECTION,
			&vec![0; 64 * 1024],
		);

		let inflated = inflate_runtime_wasm(&blob, 1024).expect("inflation must succeed");

		polkavm::ProgramBlob::parse(inflated.as_slice().into())
			.expect("the inflated blob must still be a valid PolkaVM program");
		assert_ne!(sp_crypto_hashing::blake2_256(&inflated), sp_crypto_hashing::blake2_256(&blob),);
	}
}
