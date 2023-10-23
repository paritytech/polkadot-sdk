use crate::*;
use ark_ed_on_bls12_381_bandersnatch_ext::{
	BandersnatchConfig as BandersnatchConfigHost, CurveHooks, EdwardsAffine as EdwardsAffineHost,
	EdwardsConfig as EdwardsConfigHost, EdwardsProjective as EdwardsProjectiveHost,
	SWAffine as SWAffineHost, SWConfig as SWConfigHost, SWProjective as SWProjectiveHost,
};

pub type EdwardsAffine = EdwardsAffineHost<Host>;
pub type EdwardsProjective = EdwardsProjectiveHost<Host>;

pub type SWAffine = SWAffineHost<Host>;
pub type SWProjective = SWProjectiveHost<Host>;

pub type SWConfig = SWConfigHost<Host>;
pub type EdwardsConfig = EdwardsConfigHost<Host>;
pub type BandersnatchConfig = BandersnatchConfigHost<Host>;

impl CurveHooks for Host {
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		host_calls::ed_on_bls12_381_bandersnatch_te_msm(bases, scalars)
	}
	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		host_calls::ed_on_bls12_381_bandersnatch_sw_msm(bases, scalars)
	}
	fn ed_on_bls12_381_bandersnatch_te_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		host_calls::ed_on_bls12_381_bandersnatch_te_mul_projective(base, scalar)
	}
	fn ed_on_bls12_381_bandersnatch_sw_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		host_calls::ed_on_bls12_381_bandersnatch_sw_mul_projective(base, scalar)
	}
}

/// Interfaces for working with *Arkworks* ed-on-bls12-381-bandersnatch elliptic curve
/// related types from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from the `ark-scale` trait,
/// with `ark_scale::{ArkScale, ArkScaleProjective}`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to `HOST_CALL`, which is
/// a shortcut for "not-validated" and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Short Weierstrass projective multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(base, scalar)
	}

	/// Twisted Edwards projective multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded:
	///   `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	fn ed_on_bls12_381_bandersnatch_te_mul_projective(
		base: Vec<u8>,
		scalar: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		mul_projective_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(base, scalar)
	}

	/// Short Weierstrass multi scalar multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::SWAffine]>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::SWProjective>`.
	fn ed_on_bls12_381_bandersnatch_sw_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_ed_on_bls12_381_bandersnatch::SWConfig>(bases, scalars)
	}

	/// Twisted Edwards multi scalar multiplication for Ed-on-BLS12-381-Bandersnatch.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	///   - `scalars`: `ArkScale<&[ark_ed_on_bls12_381_bandersnatch::Fr]>`.
	/// - Returns encoded:
	///   `ArkScaleProjective<ark_ed_on_bls12_381_bandersnatch::EdwardsProjective>`.
	fn ed_on_bls12_381_bandersnatch_te_msm(
		bases: Vec<u8>,
		scalars: Vec<u8>,
	) -> Result<Vec<u8>, ()> {
		msm_te::<ark_ed_on_bls12_381_bandersnatch::EdwardsConfig>(bases, scalars)
	}
}
