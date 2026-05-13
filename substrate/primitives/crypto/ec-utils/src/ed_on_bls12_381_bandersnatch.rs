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

//! *Ed-on-BLS12-381-Bandersnatch* types and host functions.

use crate::utils::{self, HostcallResult, FAIL_MSG};
use alloc::vec::Vec;
use ark_ec::CurveConfig;
use ark_ed_on_bls12_381_bandersnatch_ext::CurveHooks;
use sp_runtime_interface::{
	pass_by::{PassFatPointerAndRead, PassFatPointerAndWrite},
	runtime_interface,
};

/// Group configuration.
pub type BandersnatchConfig = ark_ed_on_bls12_381_bandersnatch_ext::BandersnatchConfig<HostHooks>;

/// Group configuration for Twisted Edwards form (equal to [`BandersnatchConfig`]).
pub type EdwardsConfig = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsConfig<HostHooks>;
/// Twisted Edwards form point affine representation.
pub type EdwardsAffine = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsAffine<HostHooks>;
/// Twisted Edwards form point projective representation.
pub type EdwardsProjective = ark_ed_on_bls12_381_bandersnatch_ext::EdwardsProjective<HostHooks>;

/// Group configuration for Short Weierstrass form (equal to [`BandersnatchConfig`]).
pub type SWConfig = ark_ed_on_bls12_381_bandersnatch_ext::SWConfig<HostHooks>;
/// Short Weierstrass form point affine representation.
pub type SWAffine = ark_ed_on_bls12_381_bandersnatch_ext::SWAffine<HostHooks>;
/// Short Weierstrass form point projective representation.
pub type SWProjective = ark_ed_on_bls12_381_bandersnatch_ext::SWProjective<HostHooks>;

/// Group scalar field (Fr).
pub type ScalarField = <BandersnatchConfig as CurveConfig>::ScalarField;

/// FFI codec for `EdwardsProjective` that handles the `z = 0` exceptional
/// states [HWCD §3.1](https://eprint.iacr.org/2008/522) arithmetic
/// (Hisil, Wong, Carter, Dawson, *Twisted Edwards Curves Revisited*; see
/// also the [EFD page](https://www.hyperelliptic.org/EFD/g1p/auto-twisted-extended.html)
/// for the explicit `add-2008-hwcd` / `dbl-2008-hwcd` formulas) can produce
/// on this incomplete twisted Edwards curve.
///
/// Bandersnatch is *incomplete*: the HWCD add/double formulas have inputs
/// where `Z3 = F·G = 0`, reachable as soon as the arithmetic is fed a curve
/// point that lies on the curve but **outside the prime-order subgroup** —
/// e.g. anything coming out of `deserialize_with_mode(.., Validate::No)`,
/// since `Validate::No` skips the subgroup check. In the cryptography
/// literature such a point is said to have *cofactor admixture* (a non-zero
/// component in the small cofactor subgroup of order 4 that lives alongside
/// the prime-order subgroup).
/// A `z = 0` projective has no affine representative, and arkworks'
/// `From<Projective> for Affine` panics on `z.inverse().unwrap()`, so the
/// standard `(x || y)` FFI channel can't carry it. This codec can.
///
/// # Wire format
///
/// 64 bytes, two 32-byte little-endian `Fq` slots at `0..32` and `32..64`.
/// `Fq` has `MODULUS_BIT_SIZE = 255`, so bit 7 of byte 31 and bit 7 of byte
/// 63 are always zero in a canonical encoding; we use them as a 2-bit tag
/// `(S, K)`.
///
/// | S | K | Slot 0 | Slot 1 | Decoded `(X, Y, T, Z)`                   |
/// |---|---|--------|--------|------------------------------------------|
/// | 0 | 0 | `x`    | `y`    | `(x, y, x*y, 1)` (normal affine lift)    |
/// | 1 | 0 | `T`    | `Y`    | `(0, Y, T, 0)` (F-exception)             |
/// | 1 | 1 | `T`    | `X`    | `(X, 0, T, 0)` (G-exception)             |
/// | 0 | 1 | —      | —      | invalid, rejected on decode              |
///
/// Slot 0 is always `T` on the `z = 0` rows; slot 1 carries the surviving
/// `X` or `Y`. The chained-exception state `(0, 0, T, 0)` is a sub-case of
/// the G-exception row with `X = 0`: encoded as `slot 0 = T`, `slot 1 = 0`,
/// `(S, K) = (1, 1)`, decoded as `(0, 0, T, 0)` since slot 1 happens to be
/// zero. The `(0, 0)` row is byte-identical to arkworks' uncompressed
/// affine, so the normal path stays wire-compatible with the standard
/// serializer; only `z = 0` uses the tag bits. The three live rows are
/// exhaustive: `T·Z = X·Y` forces `X·Y = 0` at `Z = 0`.
///
/// How `T` is constructed: `T` is the auxiliary coordinate of the extended
/// twisted Edwards tuple `(X, Y, T, Z)` from HWCD, defined by the invariant
/// `T·Z = X·Y`. An affine point `(x, y)` enters projective form as
/// `(x, y, x·y, 1)`, so the initial `T` is `x·y`. Every subsequent `T` is
/// produced by the HWCD formulas: both `add-2008-hwcd` and `dbl-2008-hwcd`
/// write `T3 = E · H`, where for add `E = (X1+Y1)·(X2+Y2) − X1·X2 − Y1·Y2`
/// and `H = Y1·Y2 − a·X1·X2`, and for double `E = (X1+Y1)² − X1² − Y1²`
/// and `H = a·X1² − Y1²`. Both formulas preserve `T·Z = X·Y` symbolically.
/// The codec does not synthesize `T`; it transmits the value the host's
/// arithmetic actually produced, as the arkworks uncompressed little-endian
/// `Fq` encoding (32 bytes, slot 0 in the `S = 1` rows).
///
/// Why `T` is preserved: HWCD addition reads `T1, T2` directly through the
/// intermediate `C = d · T1 · T2`, which then flows into every output
/// coordinate (`F = D − C`, `G = D + C`, then `X3, Y3, T3, Z3`). For
/// `Z != 0` we could drop `T` and recover it on decode from the invariant
/// `T·Z = X·Y`. At `Z = 0` that invariant collapses to `X·Y = 0` and no
/// longer pins `T` down — any value of `T` satisfies it, so `T` becomes
/// independent information. If we encoded only `X` and `Y` the decoder
/// would have to invent a `T` (typically zero), and the next add on the
/// decoded point would compute a different `C` and produce a different
/// result than the original.
pub mod bandersnatch_codec {
	use crate::utils::Error;
	use ark_ec::{
		twisted_edwards::{Affine as TEAffine, Projective as TEProjective, TECurveConfig},
		CurveGroup,
	};
	use ark_ed_on_bls12_381_bandersnatch::Fq;
	use ark_ff::{AdditiveGroup, Zero};
	use ark_scale::ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate};

	/// Size, in bytes, of a serialized point on the wire (two 32-byte `Fq` slots).
	pub const POINT_SERIALIZED_SIZE: usize = 2 * FQ_SIZE;

	const FQ_SIZE: usize = 32;
	const SENTINEL_BYTE: usize = FQ_SIZE - 1; // 31
	const EXCEPTION_KIND_BYTE: usize = POINT_SERIALIZED_SIZE - 1; // 63
	const FLAG_MASK: u8 = 0x80;

	#[inline(always)]
	fn write_slots(out: &mut [u8], slots: (Fq, Fq)) {
		slots
			.serialize_with_mode(out, Compress::No)
			.expect("Fq pair serialization into a 64-byte slice is infallible; qed");
	}

	#[inline(always)]
	fn read_slots(bytes: &[u8]) -> Result<(Fq, Fq), Error> {
		<(Fq, Fq)>::deserialize_with_mode(bytes, Compress::No, Validate::No)
			.map_err(|_| Error::Decode)
	}

	/// Encode a projective point into the 64-byte FFI payload.
	///
	/// `out` must be at least [`POINT_SERIALIZED_SIZE`] bytes; otherwise
	/// returns `Error::Encode`.
	pub fn encode<P>(p: &TEProjective<P>, out: &mut [u8]) -> Result<(), Error>
	where
		P: TECurveConfig<BaseField = Fq>,
	{
		if out.len() < POINT_SERIALIZED_SIZE {
			return Err(Error::Encode);
		}
		if !p.z.is_zero() {
			// Normal path: standard affine.
			return p
				.into_affine()
				.serialize_with_mode(out, Compress::No)
				.map_err(|_| Error::Encode);
		}
		// Exceptional state.
		match (p.x.is_zero(), p.y.is_zero()) {
			(true, false) => {
				// F-exception: (0, Y, T, 0). Encode T in slot 0, Y in slot 1.
				write_slots(out, (p.t, p.y));
			},
			(_, true) => {
				// G-exception: (X, 0, T, 0). Encode T in slot 0, X in slot 1.
				// Chained (X = 0) lands here with slot 1 = 0.
				write_slots(out, (p.t, p.x));
				out[EXCEPTION_KIND_BYTE] |= FLAG_MASK; // K=1
			},
			// (X != 0, Y != 0) at Z = 0 violates the invariant and is rejected.
			(false, false) => return Err(Error::Encode),
		}
		// Both live exception rows have S=1
		out[SENTINEL_BYTE] |= FLAG_MASK;
		Ok(())
	}

	/// Decode a projective point from the 64-byte FFI payload.
	///
	/// `bytes` must be at least [`POINT_SERIALIZED_SIZE`] bytes; otherwise
	/// returns `Error::Decode`.
	pub fn decode<P>(bytes: &[u8]) -> Result<TEProjective<P>, Error>
	where
		P: TECurveConfig<BaseField = Fq>,
	{
		if bytes.len() < POINT_SERIALIZED_SIZE {
			return Err(Error::Decode);
		}
		let s = bytes[SENTINEL_BYTE] & FLAG_MASK != 0;
		let k = bytes[EXCEPTION_KIND_BYTE] & FLAG_MASK != 0;
		match (s, k) {
			(false, false) => {
				// Normal path: standard affine.
				TEAffine::<P>::deserialize_with_mode(bytes, Compress::No, Validate::No)
					.map(|aff| aff.into())
					.map_err(|_| Error::Decode)
			},
			// Reserved/invalid code point.
			(false, true) => Err(Error::Decode),
			(true, _) => {
				// Exceptional state: strip flag bits, decode Fq slots.
				let mut work = [0u8; POINT_SERIALIZED_SIZE];
				work.copy_from_slice(&bytes[..POINT_SERIALIZED_SIZE]);
				work[SENTINEL_BYTE] &= !FLAG_MASK;
				work[EXCEPTION_KIND_BYTE] &= !FLAG_MASK;
				let (slot0, slot1) = read_slots(&work)?;
				let (x, y) = if k {
					(slot1, Fq::ZERO) // G-exception: slot0 = T, slot1 = X. Chained (X = 0) lands here too.
				} else {
					(Fq::ZERO, slot1) // F-exception: slot0 = T, slot1 = Y.
				};
				Ok(TEProjective::<P>::new_unchecked(x, y, slot0, Fq::ZERO))
			},
		}
	}
}

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn msm_te(bases: &[EdwardsAffine], scalars: &[ScalarField]) -> EdwardsProjective {
		let mut out = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		host_calls::ed_on_bls12_381_bandersnatch_msm(
			&utils::encode(bases),
			&utils::encode(scalars),
			&mut out,
		)
		.and_then(|_| bandersnatch_codec::decode(&out))
		.expect(FAIL_MSG)
	}

	fn mul_projective_te(base: &EdwardsProjective, scalar: &[u64]) -> EdwardsProjective {
		let mut base_buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(base, &mut base_buf).expect(FAIL_MSG);
		let mut out = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		host_calls::ed_on_bls12_381_bandersnatch_mul(&base_buf, &utils::encode(scalar), &mut out)
			.and_then(|_| bandersnatch_codec::decode(&out))
			.expect(FAIL_MSG)
	}
}

/// Interfaces for working with *Arkworks* *Ed-on-BLS12-381-Bandersnatch* elliptic curve related
/// types from within the runtime.
///
/// Point inputs and outputs use the `bandersnatch_ffi` 64-byte projective
/// codec, which can round-trip the `z = 0` exceptional states that HWCD
/// arithmetic on non-subgroup inputs can produce. For points with
/// `z != 0` the wire format is byte-identical to arkworks' uncompressed
/// affine encoding (and thus to `ArkScale<EdwardsAffine>`).
///
/// Scalars are still `ArkScale`-encoded ("not-validated", "not-compressed").
#[runtime_interface]
pub trait HostCalls {
	/// Twisted Edwards multi scalar multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<EdwardsAffine>` via `ArkScale`.
	/// - `scalars`: `Vec<ScalarField>` via `ArkScale`.
	/// Writes a 64-byte `bandersnatch_ffi`-encoded projective to `out`.
	fn ed_on_bls12_381_bandersnatch_msm(
		bases: PassFatPointerAndRead<&[u8]>,
		scalars: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		use ark_ec::twisted_edwards::{Affine as TEAffine, TECurveConfig};
		use ark_ed_on_bls12_381_bandersnatch::EdwardsConfig as RawEdwardsConfig;
		let bases = utils::decode::<Vec<TEAffine<RawEdwardsConfig>>>(bases)?;
		let scalars =
			utils::decode::<Vec<<RawEdwardsConfig as CurveConfig>::ScalarField>>(scalars)?;
		let res = <RawEdwardsConfig as TECurveConfig>::msm(&bases, &scalars)
			.map_err(|_| utils::Error::LengthMismatch)?;
		bandersnatch_codec::encode(&res, out)
	}

	/// Twisted Edwards projective multiplication for *Ed-on-BLS12-381-Bandersnatch*.
	///
	/// Receives:
	/// - `base`: a 64-byte `bandersnatch_ffi`-encoded projective.
	/// - `scalar`: `BigInteger` via `ArkScale`.
	/// Writes a 64-byte `bandersnatch_ffi`-encoded projective to `out`.
	fn ed_on_bls12_381_bandersnatch_mul(
		base: PassFatPointerAndRead<&[u8]>,
		scalar: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> HostcallResult {
		use ark_ec::twisted_edwards::TECurveConfig;
		use ark_ed_on_bls12_381_bandersnatch::EdwardsConfig as RawEdwardsConfig;
		let base = bandersnatch_codec::decode::<RawEdwardsConfig>(base)?;
		let scalar = utils::decode::<utils::BigInteger>(scalar)?;
		let res = <RawEdwardsConfig as TECurveConfig>::mul_projective(&base, &scalar);
		bandersnatch_codec::encode(&res, out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::testing::*;
	use ark_ec::{twisted_edwards::Projective as TEProjective, AffineRepr};
	use ark_ed_on_bls12_381_bandersnatch::{EdwardsConfig as RawEdwardsConfig, Fq, Fr};
	use ark_ff::{AdditiveGroup, PrimeField, UniformRand, Zero};
	use ark_std::test_rng;

	#[test]
	fn mul_works() {
		mul_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn msm_works() {
		msm_te_test::<EdwardsAffine, ark_ed_on_bls12_381_bandersnatch::EdwardsAffine>();
	}

	#[test]
	fn mul_works_sw() {
		mul_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}

	#[test]
	fn msm_works_sw() {
		msm_test::<SWAffine, ark_ed_on_bls12_381_bandersnatch::SWAffine>();
	}

	type RawProjective = TEProjective<RawEdwardsConfig>;

	fn quadruple_eq(a: &RawProjective, b: &RawProjective) -> bool {
		a.x == b.x && a.y == b.y && a.t == b.t && a.z == b.z
	}

	#[test]
	fn normal_path_round_trip() {
		// A random prime-subgroup point, taken through the encoder/decoder.
		let mut rng = test_rng();
		let base = ark_ed_on_bls12_381_bandersnatch::EdwardsAffine::generator();
		let scalar = Fr::rand(&mut rng);
		let p: RawProjective = base * scalar;
		assert!(!p.z.is_zero());

		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(&p, &mut buf).unwrap();
		assert_eq!(buf[31] & 0x80, 0, "sentinel bit set on a normal-path encoding");
		assert_eq!(buf[63] & 0x80, 0, "kind bit set on a normal-path encoding");

		let q: RawProjective = bandersnatch_codec::decode(&buf).unwrap();
		// PartialEq cross-multiplies through Z, so a pre-normalization input
		// compares equal to the post-normalization output.
		assert_eq!(p, q);
	}

	#[test]
	fn f_exception_round_trip() {
		let mut rng = test_rng();
		let y = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p = RawProjective::new_unchecked(Fq::ZERO, y, t, Fq::ZERO);

		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(&p, &mut buf).unwrap();
		assert_ne!(buf[31] & 0x80, 0, "S flag not set on F-exception");
		assert_eq!(buf[63] & 0x80, 0, "K flag should be 0 on F-exception");

		let q: RawProjective = bandersnatch_codec::decode(&buf).unwrap();
		assert!(quadruple_eq(&p, &q), "F-exception quadruple not preserved");
	}

	#[test]
	fn g_exception_round_trip() {
		let mut rng = test_rng();
		let x = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p = RawProjective::new_unchecked(x, Fq::ZERO, t, Fq::ZERO);

		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(&p, &mut buf).unwrap();
		assert_ne!(buf[31] & 0x80, 0, "S flag not set on G-exception");
		assert_ne!(buf[63] & 0x80, 0, "K flag not set on G-exception");

		let q: RawProjective = bandersnatch_codec::decode(&buf).unwrap();
		assert!(quadruple_eq(&p, &q), "G-exception quadruple not preserved");
	}

	#[test]
	fn invalid_sentinel_bits_rejected() {
		// (S=0, K=1) is a reserved/invalid code point: decoder must reject.
		let base = ark_ed_on_bls12_381_bandersnatch::EdwardsAffine::generator();
		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		let proj: RawProjective = base.into_group();
		bandersnatch_codec::encode(&proj, &mut buf).unwrap();
		buf[31] &= 0x7f; // S=0
		buf[63] |= 0x80; // K=1
		assert!(bandersnatch_codec::decode::<RawEdwardsConfig>(&buf).is_err());
	}

	#[test]
	fn out_of_range_field_element_rejected() {
		// All-0xff bytes in slot 0 with S=1, K=0 (so we hit the exceptional
		// decoder). After masking off the S flag, slot 0 still encodes a
		// value >= q, which Fq::deserialize_with_mode must reject.
		let mut buf = [0xffu8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		// Clear K so we take the F-exception branch (which is otherwise valid
		// shape-wise; we want the Fq check to be the rejecting step).
		buf[63] &= 0x7f; // K=0
		assert!(bandersnatch_codec::decode::<RawEdwardsConfig>(&buf).is_err());
	}

	#[test]
	fn chained_exception_round_trip() {
		// (0, 0, T, 0) folds into the G-exception row with slot 1 = 0.
		let mut rng = test_rng();
		let t = Fq::rand(&mut rng);
		let p = RawProjective::new_unchecked(Fq::ZERO, Fq::ZERO, t, Fq::ZERO);
		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(&p, &mut buf).unwrap();
		assert_ne!(buf[31] & 0x80, 0, "S flag should be 1 on chained exception (G-row)");
		assert_ne!(buf[63] & 0x80, 0, "K flag should be 1 on chained exception (G-row)");

		let q: RawProjective = bandersnatch_codec::decode(&buf).unwrap();
		assert!(quadruple_eq(&p, &q), "chained-exception quadruple not preserved");
	}

	#[test]
	fn invariant_violating_z_zero_rejected_on_encode() {
		// X != 0 AND Y != 0 with Z = 0 violates T*Z = X*Y; encoder must reject.
		let mut rng = test_rng();
		let x = Fq::rand(&mut rng);
		let y = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p = RawProjective::new_unchecked(x, y, t, Fq::ZERO);
		let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		assert!(bandersnatch_codec::encode(&p, &mut buf).is_err());
	}

	/// Covers call site `HostHooks::mul_projective_te` (WASM-side input encoding)
	/// with `z = 0`. Uses scalar = 0 so the host returns identity without ever
	/// invoking HWCD addition on the `z = 0` operand.
	#[test]
	fn mul_projective_te_z_zero_input() {
		let mut rng = test_rng();
		let y = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);
		let p_ext: EdwardsProjective = EdwardsProjective::new_unchecked(Fq::ZERO, y, t, Fq::ZERO);

		let result = <HostHooks as CurveHooks>::mul_projective_te(&p_ext, &[0u64]);
		// Identity in TE projective form is (0, 1, 0, 1); z != 0.
		assert!(!result.z.is_zero(), "expected identity, got a z=0 projective");
		assert!(result.is_zero(), "expected identity result for scalar=0");
	}

	/// A `z = 0` projective enters the mul host fn as input alongside a
	/// non-trivial scalar. The pre-fix code would have panicked inside
	/// `into_affine()` before the bytes ever crossed the FFI; with the
	/// codec in place:
	/// - the host call completes,
	/// - the host-fn result matches running `mul_projective` directly on the same `z = 0` quadruple
	///   (quadruple-eq cross-check),
	/// - both F-exception and G-exception input shapes collapse to the chained-exception state `(0,
	///   0, 0, 0)` (the absorbing element of HWCD on already-degenerate inputs), and
	/// - re-encoding the decoded result reproduces the host's output buffer byte-for-byte (codec
	///   determinism).
	#[test]
	fn host_mul_with_z_zero_input() {
		let mut rng = test_rng();
		let nonzero_a = Fq::rand(&mut rng);
		let nonzero_b = Fq::rand(&mut rng);
		let t = Fq::rand(&mut rng);

		let cases = [
			("F-exception", RawProjective::new_unchecked(Fq::ZERO, nonzero_a, t, Fq::ZERO)),
			("G-exception", RawProjective::new_unchecked(nonzero_b, Fq::ZERO, t, Fq::ZERO)),
		];

		for (name, p) in cases {
			let scalar_bigint: Vec<u64> = Fr::rand(&mut rng).into_bigint().0.to_vec();

			let mut input_buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
			bandersnatch_codec::encode(&p, &mut input_buf).unwrap();
			let scalar_enc = utils::encode(scalar_bigint.clone());

			let mut output_buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
			host_calls::ed_on_bls12_381_bandersnatch_mul(&input_buf, &scalar_enc, &mut output_buf)
				.unwrap();
			let r_host: RawProjective = bandersnatch_codec::decode(&output_buf).unwrap();

			// 1. HWCD on a z=0 input with any non-zero scalar collapses to the absorbing element
			//    (0, 0, 0, 0): the chained-exception state encoded as the G-row with slot 1 = 0.
			let zero = RawProjective::new_unchecked(Fq::ZERO, Fq::ZERO, Fq::ZERO, Fq::ZERO);
			assert!(quadruple_eq(&r_host, &zero), "{name}: expected (0, 0, 0, 0), got {r_host:?}",);

			// 2. Cross-check against direct mul_projective on the same quadruple.
			let r_direct =
				<RawEdwardsConfig as ark_ec::twisted_edwards::TECurveConfig>::mul_projective(
					&p,
					&scalar_bigint,
				);
			assert!(
				quadruple_eq(&r_host, &r_direct),
				"{name}: host result diverges from direct mul_projective",
			);

			// 3. Re-encoding the decoded result must reproduce the host's output.
			let mut reencoded = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
			bandersnatch_codec::encode(&r_host, &mut reencoded).unwrap();
			assert_eq!(reencoded, output_buf, "{name}: re-encoded bytes differ from host output");
		}
	}

	/// Covers the call site with a real `z = 0` result, via the trigger: the `y = 2`
	/// non-subgroup Bandersnatch point times the scalar-field order.
	#[test]
	fn host_mul_and_msm_with_y2_cofactor_admixture() {
		let p_aff = ark_ed_on_bls12_381_bandersnatch::EdwardsAffine::get_point_from_y_unchecked(
			Fq::from(2u64),
			false,
		)
		.unwrap();
		let proj: RawProjective = p_aff.into_group();
		let scalar_bigint: Vec<u64> = Fr::MODULUS.0.to_vec();

		// --- mul path: produces z = 0 output.
		let mut mul_input = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		bandersnatch_codec::encode(&proj, &mut mul_input).unwrap();
		let scalar_enc = utils::encode(scalar_bigint);
		let mut mul_output = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		host_calls::ed_on_bls12_381_bandersnatch_mul(&mul_input, &scalar_enc, &mut mul_output)
			.unwrap();
		let r_mul: RawProjective = bandersnatch_codec::decode(&mul_output).unwrap();
		assert!(r_mul.z.is_zero(), "y=2 * Fr::MODULUS via mul_projective should land at z=0");
		// Cross-check: same operation via direct arkworks call should give the
		// same quadruple. (PartialEq cross-multiplies through Z and is vacuous
		// at Z = 0; compare field-by-field.)
		let r_direct = <RawEdwardsConfig as ark_ec::twisted_edwards::TECurveConfig>::mul_projective(
			&proj,
			Fr::MODULUS.0.as_ref(),
		);
		assert!(
			quadruple_eq(&r_mul, &r_direct),
			"host fn result diverges from direct mul_projective",
		);

		// --- msm path: same trigger, integration coverage only (Pippenger keeps z != 0 here).
		let scalar_fr = Fr::from_bigint(Fr::MODULUS).unwrap_or(Fr::ZERO);
		let bases_enc = utils::encode(vec![p_aff]);
		let scalars_enc = utils::encode(vec![scalar_fr]);
		let mut msm_output = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
		host_calls::ed_on_bls12_381_bandersnatch_msm(&bases_enc, &scalars_enc, &mut msm_output)
			.unwrap();
		let r_msm: RawProjective = bandersnatch_codec::decode(&msm_output).unwrap();
		// Cross-check: same operation via direct arkworks msm.
		let r_msm_direct = <RawEdwardsConfig as ark_ec::twisted_edwards::TECurveConfig>::msm(
			&[p_aff],
			&[scalar_fr],
		)
		.unwrap();
		assert_eq!(r_msm, r_msm_direct, "host msm result diverges from direct msm");
	}

	#[test]
	fn normal_path_tag_bits_are_zero() {
		// 10 random subgroup encodings, none of which should have either
		// flag bit set.
		let mut rng = test_rng();
		let base = ark_ed_on_bls12_381_bandersnatch::EdwardsAffine::generator();
		for _ in 0..10 {
			let p: RawProjective = base * Fr::rand(&mut rng);
			let mut buf = [0u8; bandersnatch_codec::POINT_SERIALIZED_SIZE];
			bandersnatch_codec::encode(&p, &mut buf).unwrap();
			assert_eq!(buf[31] & 0x80, 0);
			assert_eq!(buf[63] & 0x80, 0);
		}
	}
}
