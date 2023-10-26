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

//! Elliptic Curves host functions to handle some of the *Arkworks* *BW6-761*
//! computationally expensive operations.

use crate::*;
use ark_bw6_761_ext::CurveHooks;
use ark_ec::{pairing::Pairing, CurveConfig};
use ark_scale::scale::{Decode, Encode};

/// TODO
pub mod g1 {
	pub use ark_bw6_761_ext::g1::{G1_GENERATOR_X, G1_GENERATOR_Y};

	/// TODO
	pub type Config = ark_bw6_761_ext::g1::Config<super::HostHooks>;
	/// TODO
	pub type G1Affine = ark_bw6_761_ext::g1::G1Affine<super::HostHooks>;
	/// TODO
	pub type G1Projective = ark_bw6_761_ext::g1::G1Projective<super::HostHooks>;
}

/// TODO
pub mod g2 {
	pub use ark_bw6_761_ext::g2::{G2_GENERATOR_X, G2_GENERATOR_Y};

	/// TODO
	pub type Config = ark_bw6_761_ext::g2::Config<super::HostHooks>;
	/// TODO
	pub type G2Affine = ark_bw6_761_ext::g2::G2Affine<super::HostHooks>;
	/// TODO
	pub type G2Projective = ark_bw6_761_ext::g2::G2Projective<super::HostHooks>;
}

pub use self::{
	g1::{Config as G1Config, G1Affine, G1Projective},
	g2::{Config as G2Config, G2Affine, G2Projective},
};

/// TODO
#[derive(Copy, Clone)]
pub struct HostHooks;

/// TODO
pub type Config = ark_bw6_761_ext::Config<HostHooks>;
/// TODO
pub type BW6_761 = ark_bw6_761_ext::BW6_761<HostHooks>;

impl CurveHooks for HostHooks {
	fn bw6_761_multi_miller_loop(
		g1: impl Iterator<Item = <BW6_761 as Pairing>::G1Prepared>,
		g2: impl Iterator<Item = <BW6_761 as Pairing>::G2Prepared>,
	) -> Result<<BW6_761 as Pairing>::TargetField, ()> {
		let g1 = ArkScale::from(g1.collect::<Vec<_>>()).encode();
		let g2 = ArkScale::from(g2.collect::<Vec<_>>()).encode();

		let res = host_calls::bw6_761_multi_miller_loop(g1, g2).unwrap_or_default();
		let res = ArkScale::<<BW6_761 as Pairing>::TargetField>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bw6_761_final_exponentiation(
		target: <BW6_761 as Pairing>::TargetField,
	) -> Result<<BW6_761 as Pairing>::TargetField, ()> {
		let target = ArkScale::from(target).encode();

		let res = host_calls::bw6_761_final_exponentiation(target).unwrap_or_default();
		let res = ArkScale::<<BW6_761 as Pairing>::TargetField>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bw6_761_msm_g1(
		bases: &[G1Affine],
		scalars: &[<G1Config as CurveConfig>::ScalarField],
	) -> Result<G1Projective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res = host_calls::bw6_761_msm_g1(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<G1Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bw6_761_msm_g2(
		bases: &[G2Affine],
		scalars: &[<G2Config as CurveConfig>::ScalarField],
	) -> Result<G2Projective, ()> {
		let bases = ArkScale::from(bases).encode();
		let scalars = ArkScale::from(scalars).encode();

		let res = host_calls::bw6_761_msm_g2(bases, scalars).unwrap_or_default();
		let res = ArkScaleProjective::<G2Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bw6_761_mul_projective_g1(base: &G1Projective, scalar: &[u64]) -> Result<G1Projective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::bw6_761_mul_projective_g1(base, scalar).unwrap_or_default();
		let res = ArkScaleProjective::<G1Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}

	fn bw6_761_mul_projective_g2(base: &G2Projective, scalar: &[u64]) -> Result<G2Projective, ()> {
		let base = ArkScaleProjective::from(base).encode();
		let scalar = ArkScale::from(scalar).encode();

		let res = host_calls::bw6_761_mul_projective_g2(base, scalar).unwrap_or_default();
		let res = ArkScaleProjective::<G2Projective>::decode(&mut res.as_slice());
		res.map(|v| v.0).map_err(|_| ())
	}
}

/// Interfaces for working with *Arkworks* *BW6-761* elliptic curve related types
/// from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from the `ark-scale` trait,
/// with `ark_scale::{ArkScale, ArkScaleProjective}`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Pairing multi Miller loop for BW6-761.
	///
	/// - Receives encoded:
	///   - `a: ArkScale<Vec<ark_ec::bls12::G1Prepared::<ark_bw6_761::Config>>>`.
	///   - `b: ArkScale<Vec<ark_ec::bls12::G2Prepared::<ark_bw6_761::Config>>>`.
	/// - Returns encoded: ArkScale<MillerLoopOutput<Bls12<ark_bw6_761::Config>>>.
	fn bw6_761_multi_miller_loop(a: Vec<u8>, b: Vec<u8>) -> Result<Vec<u8>, ()> {
		multi_miller_loop::<ark_bw6_761::BW6_761>(a, b)
	}

	/// Pairing final exponentiation for BW6-761.
	///
	/// - Receives encoded: `ArkScale<MillerLoopOutput<Bls12<ark_bw6_761::Config>>>`.
	/// - Returns encoded: `ArkScale<PairingOutput<Bls12<ark_bw6_761::Config>>>`.
	fn bw6_761_final_exponentiation(f: Vec<u8>) -> Result<Vec<u8>, ()> {
		final_exponentiation::<ark_bw6_761::BW6_761>(f)
	}

	/// Projective multiplication on G1 for BW6-761.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	fn bw6_761_mul_projective_g1(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bw6_761::g1::Config>(base, scalar)
	}

	/// Projective multiplication on G2 for BW6-761.
	///
	/// - Receives encoded:
	///   - `base`: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	///   - `scalar`: `ArkScale<&[u64]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	fn bw6_761_mul_projective_g2(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
		mul_projective_sw::<ark_bw6_761::g2::Config>(base, scalar)
	}

	/// Multi scalar multiplication on G1 for BW6-761.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bw6_761::G1Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bw6_761::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G1Projective>`.
	fn bw6_761_msm_g1(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bw6_761::g1::Config>(bases, scalars)
	}

	/// Multi scalar multiplication on G2 for BW6-761.
	///
	/// - Receives encoded:
	///   - `bases`: `ArkScale<&[ark_bw6_761::G2Affine]>`.
	///   - `scalars`: `ArkScale<&[ark_bw6_761::Fr]>`.
	/// - Returns encoded: `ArkScaleProjective<ark_bw6_761::G2Projective>`.
	fn bw6_761_msm_g2(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
		msm_sw::<ark_bw6_761::g2::Config>(bases, scalars)
	}
}
