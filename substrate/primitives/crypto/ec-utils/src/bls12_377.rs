use crate::*;
use ark_bls12_377_ext::CurveHooks;
use ark_ec::{pairing::Pairing, CurveConfig};
use ark_scale::scale::{Decode, Encode};

/// TODO
pub mod g1 {
	pub use ark_bls12_377_ext::g1::{
		G1_GENERATOR_X, G1_GENERATOR_Y, TE_GENERATOR_X, TE_GENERATOR_Y,
	};

	/// TODO
	pub type Config = ark_bls12_377_ext::g1::Config<super::HostHooks>;
	/// TODO
	pub type G1Affine = ark_bls12_377_ext::g1::G1Affine<super::HostHooks>;
	/// TODO
	pub type G1Projective = ark_bls12_377_ext::g1::G1Projective<super::HostHooks>;
	/// TODO
	pub type G1SWAffine = ark_bls12_377_ext::g1::G1SWAffine<super::HostHooks>;
	/// TODO
	pub type G1SWProjectove = ark_bls12_377_ext::g1::G1SWProjective<super::HostHooks>;
	/// TODO
	pub type G1TEAffine = ark_bls12_377_ext::g1::G1TEAffine<super::HostHooks>;
	/// TODO
	pub type G1TEProjective = ark_bls12_377_ext::g1::G1TEProjective<super::HostHooks>;
}

/// TODO
pub mod g2 {
	pub use ark_bls12_377_ext::g2::{
		G2_GENERATOR_X, G2_GENERATOR_X_C0, G2_GENERATOR_X_C1, G2_GENERATOR_Y, G2_GENERATOR_Y_C0,
		G2_GENERATOR_Y_C1,
	};

	/// TODO
	pub type Config = ark_bls12_377_ext::g2::Config<super::HostHooks>;
	/// TODO
	pub type G2Affine = ark_bls12_377_ext::g2::G2Affine<super::HostHooks>;
	/// TODO
	pub type G2Projective = ark_bls12_377_ext::g2::G2Projective<super::HostHooks>;
}

pub use self::{
	g1::{Config as G1Config, G1Affine, G1Projective},
	g2::{Config as G2Config, G2Affine, G2Projective},
};

/// TODO
#[derive(Copy, Clone)]
pub struct HostHooks;

/// TODO
pub type Config = ark_bls12_377_ext::Config<HostHooks>;
/// TODO
pub type Bls12_377 = ark_bls12_377_ext::Bls12_377<HostHooks>;

impl CurveHooks for HostHooks {
	fn bls12_377_multi_miller_loop(
		g1: impl Iterator<Item = <Bls12_377 as Pairing>::G1Prepared>,
		g2: impl Iterator<Item = <Bls12_377 as Pairing>::G2Prepared>,
	) -> Result<<Bls12_377 as Pairing>::TargetField, ()> {
		let g1 = ArkScale::from(g1.collect::<Vec<_>>()).encode();
		let g2 = ArkScale::from(g2.collect::<Vec<_>>()).encode();

		let res = host_calls::bls12_377_multi_miller_loop(g1, g2).unwrap_or_default();
		let res = ArkScale::<<Bls12_377 as Pairing>::TargetField>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bls12_377_final_exponentiation(
		target: <Bls12_377 as Pairing>::TargetField,
	) -> Result<<Bls12_377 as Pairing>::TargetField, ()> {
		let target = ArkScale::from(target).encode();

		let res = host_calls::bls12_377_final_exponentiation(target).unwrap_or_default();
		let res = ArkScale::<<Bls12_377 as Pairing>::TargetField>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bls12_377_msm_g1(
		bases: &[G1Affine],
		scalars: &[<G1Config as CurveConfig>::ScalarField],
	) -> Result<G1Projective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res = host_calls::bls12_377_msm_g1(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<G1Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bls12_377_msm_g2(
		bases: &[G2Affine],
		scalars: &[<G2Config as CurveConfig>::ScalarField],
	) -> Result<G2Projective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res = host_calls::bls12_377_msm_g2(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<G2Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bls12_377_mul_projective_g1(
		base: &G1Projective,
		scalar: &[u64],
	) -> Result<G1Projective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::bls12_377_mul_projective_g1(base, scalar).unwrap_or_default();
		let res = ArkScaleProjective::<G1Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bls12_377_mul_projective_g2(
		base: &G2Projective,
		scalar: &[u64],
	) -> Result<G2Projective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::bls12_377_mul_projective_g2(base, scalar).unwrap_or_default();
		let res = ArkScaleProjective::<G2Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}
}

/// Interfaces for working with *Arkworks* *Ed-on-BLS12-381-Bandersnatch* elliptic curve
/// related types from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from the `ark-scale` trait,
/// with `ark_scale::{ArkScale, ArkScaleProjective}`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Pairing multi Miller loop for BLS12-377.
	///
	/// - Receives encoded:
	///   - `a: ArkScale<Vec<ark_ec::bls12::G1Prepared::<ark_bls12_377::Config>>>`.
	///   - `b: ArkScale<Vec<ark_ec::bls12::G2Prepared::<ark_bls12_377::Config>>>`.
	/// - Returns encoded: ArkScale<MillerLoopOutput<Bls12<ark_bls12_377::Config>>>.
	fn bls12_377_multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Vec<u8>, ()> {
		multi_miller_loop::<ark_bls12_377::Bls12_377>(a, b)
	}

	/// Pairing final exponentiation for BLS12-377.
	///
	/// - Receives encoded: `ArkScale<MillerLoopOutput<Bls12<ark_bls12_377::Config>>>`.
	/// - Returns encoded: `ArkScale<PairingOutput<Bls12<ark_bls12_377::Config>>>`.
	fn bls12_377_final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, ()> {
		final_exponentiation::<ark_bls12_377::Bls12_377>(f)
	}

	/// Projective multiplication on G1 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	fn bls12_377_mul_projective_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_377::g1::Config>(base, scalar)
	}

	/// Projective multiplication on G2 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	fn bls12_377_mul_projective_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bls12_377::g2::Config>(base, scalar)
	}

	/// Multi scalar multiplication on G1 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bls12_377::G1Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bls12_377::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G1Projective>`.
	fn bls12_377_msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_377::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on G2 for BLS12-377.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bls12_377::G2Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bls12_377::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bls12_377::G2Projective>`.
	fn bls12_377_msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bls12_377::g2::Config>(bases, scalars)
	}
}
