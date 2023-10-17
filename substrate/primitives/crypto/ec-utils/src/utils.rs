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

//! Generic executions of the operations for *Arkworks* elliptic curves.

use ark_ec::{
	pairing::{MillerLoopOutput, Pairing, PairingOutput},
	short_weierstrass,
	short_weierstrass::SWCurveConfig,
	twisted_edwards,
	twisted_edwards::TECurveConfig,
	CurveConfig, VariableBaseMSM,
};
use ark_scale::{
	hazmat::ArkScaleProjective,
	scale::{Decode, Encode},
};
use sp_std::vec::Vec;

// Scale codec type which is expected to be used by the host functions.
//
// Encoding is set to `HOST_CALL` which is a shortcut for "not-validated" and "not-compressed".
type ArkScale<T> = ark_scale::ArkScale<T, { ark_scale::HOST_CALL }>;

pub fn multi_miller_loop<Curve: Pairing>(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, ()> {
	let g1 = <ArkScale<Vec<<Curve as Pairing>::G1Affine>> as Decode>::decode(&mut g1.as_slice())
		.map_err(|_| ())?;
	let g2 = <ArkScale<Vec<<Curve as Pairing>::G2Affine>> as Decode>::decode(&mut g2.as_slice())
		.map_err(|_| ())?;

	let result = Curve::multi_miller_loop(g1.0, g2.0).0;

	let result: ArkScale<<Curve as Pairing>::TargetField> = result.into();
	Ok(result.encode())
}

pub fn final_exponentiation<Curve: Pairing>(target: Vec<u8>) -> Result<Vec<u8>, ()> {
	let target =
		<ArkScale<<Curve as Pairing>::TargetField> as Decode>::decode(&mut target.as_slice())
			.map_err(|_| ())?;

	let result = Curve::final_exponentiation(MillerLoopOutput(target.0)).ok_or(())?;

	let result: ArkScale<PairingOutput<Curve>> = result.into();
	Ok(result.encode())
}

pub fn msm_sw<Curve: SWCurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases =
		<ArkScale<Vec<short_weierstrass::Affine<Curve>>> as Decode>::decode(&mut bases.as_slice())
			.map_err(|_| ())?;
	let scalars = <ArkScale<Vec<<Curve as CurveConfig>::ScalarField>> as Decode>::decode(
		&mut scalars.as_slice(),
	)
	.map_err(|_| ())?;

	let result =
		<short_weierstrass::Projective<Curve> as VariableBaseMSM>::msm(&bases.0, &scalars.0)
			.map_err(|_| ())?;

	let result: ArkScaleProjective<short_weierstrass::Projective<Curve>> = result.into();
	Ok(result.encode())
}

pub fn msm_te<Curve: TECurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases =
		<ArkScale<Vec<twisted_edwards::Affine<Curve>>> as Decode>::decode(&mut bases.as_slice())
			.map_err(|_| ())?;
	let scalars = <ArkScale<Vec<<Curve as CurveConfig>::ScalarField>> as Decode>::decode(
		&mut scalars.as_slice(),
	)
	.map_err(|_| ())?;

	let result = <twisted_edwards::Projective<Curve> as VariableBaseMSM>::msm(&bases.0, &scalars.0)
		.map_err(|_| ())?;

	let result: ArkScaleProjective<twisted_edwards::Projective<Curve>> = result.into();
	Ok(result.encode())
}

pub fn mul_projective_sw<Group: SWCurveConfig>(
	base: Vec<u8>,
	scalar: Vec<u8>,
) -> Result<Vec<u8>, ()> {
	let base = <ArkScaleProjective<short_weierstrass::Projective<Group>> as Decode>::decode(
		&mut base.as_slice(),
	)
	.map_err(|_| ())?;
	let scalar = <ArkScale<Vec<u64>> as Decode>::decode(&mut scalar.as_slice()).map_err(|_| ())?;

	let result = <Group as SWCurveConfig>::mul_projective(&base.0, &scalar.0);

	let result: ArkScaleProjective<short_weierstrass::Projective<Group>> = result.into();
	Ok(result.encode())
}

pub fn mul_projective_te<Group: TECurveConfig>(
	base: Vec<u8>,
	scalar: Vec<u8>,
) -> Result<Vec<u8>, ()> {
	let base = <ArkScaleProjective<twisted_edwards::Projective<Group>> as Decode>::decode(
		&mut base.as_slice(),
	)
	.map_err(|_| ())?;
	let scalar = <ArkScale<Vec<u64>> as Decode>::decode(&mut scalar.as_slice()).map_err(|_| ())?;

	let result = <Group as TECurveConfig>::mul_projective(&base.0, &scalar.0);

	let result: ArkScaleProjective<twisted_edwards::Projective<Group>> = result.into();
	Ok(result.encode())
}
