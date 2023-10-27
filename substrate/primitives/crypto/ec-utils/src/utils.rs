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

// As not all functions are used by each elliptic curve and some elliptic
// curve may be excluded by the build we resort to `#[allow(unused)]` to
// suppress the expected warning.

use ark_ec::{
	pairing::{MillerLoopOutput, Pairing},
	short_weierstrass::{Affine as SWAffine, Projective as SWProjective, SWCurveConfig},
	twisted_edwards::{Affine as TEAffine, Projective as TEProjective, TECurveConfig},
	CurveConfig, VariableBaseMSM,
};
use ark_scale::{
	ark_serialize::{CanonicalDeserialize, CanonicalSerialize, Compress, Validate},
	scale::{Decode, Encode},
};
use sp_std::vec::Vec;

// SCALE encoding parameters shared by all the enabled modules
const SCALE_USAGE: u8 = ark_scale::make_usage(Compress::No, Validate::No);
type ArkScale<T> = ark_scale::ArkScale<T, SCALE_USAGE>;
type ArkScaleProjective<T> = ark_scale::hazmat::ArkScaleProjective<T>;

#[inline(always)]
pub fn encode<T: CanonicalSerialize>(val: T) -> Vec<u8> {
	ArkScale::from(val).encode()
}

#[inline(always)]
pub fn decode<T: CanonicalDeserialize>(buf: Vec<u8>) -> Result<T, ()> {
	ArkScale::<T>::decode(&mut &buf[..]).map_err(|_| ()).map(|v| v.0)
}

#[inline(always)]
pub fn encode_proj_sw<T: SWCurveConfig>(val: &SWProjective<T>) -> Vec<u8> {
	ArkScaleProjective::from(val).encode()
}

#[inline(always)]
pub fn decode_proj_sw<T: SWCurveConfig>(buf: Vec<u8>) -> Result<SWProjective<T>, ()> {
	ArkScaleProjective::decode(&mut &buf[..]).map_err(|_| ()).map(|v| v.0)
}

#[inline(always)]
pub fn encode_proj_te<T: TECurveConfig>(val: &TEProjective<T>) -> Vec<u8> {
	ArkScaleProjective::from(val).encode()
}

#[inline(always)]
pub fn decode_proj_te<T: TECurveConfig>(buf: Vec<u8>) -> Result<TEProjective<T>, ()> {
	ArkScaleProjective::decode(&mut &buf[..]).map_err(|_| ()).map(|v| v.0)
}

#[allow(unused)]
pub fn multi_miller_loop<T: Pairing>(g1: Vec<u8>, g2: Vec<u8>) -> Result<Vec<u8>, ()> {
	let g1 = decode::<Vec<<T as Pairing>::G1Affine>>(g1)?;
	let g2 = decode::<Vec<<T as Pairing>::G2Affine>>(g2)?;
	let res = T::multi_miller_loop(g1, g2);
	Ok(encode(res.0))
}

#[allow(unused)]
pub fn final_exponentiation<T: Pairing>(target: Vec<u8>) -> Result<Vec<u8>, ()> {
	let target = decode::<<T as Pairing>::TargetField>(target)?;
	let res = T::final_exponentiation(MillerLoopOutput(target)).ok_or(())?;
	Ok(encode(res.0))
}

#[allow(unused)]
pub fn msm_sw<T: SWCurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases = decode::<Vec<SWAffine<T>>>(bases)?;
	let scalars = decode::<Vec<<T as CurveConfig>::ScalarField>>(scalars)?;
	let res = <SWProjective<T> as VariableBaseMSM>::msm(&bases, &scalars).map_err(|_| ())?;
	Ok(encode_proj_sw(&res))
}

#[allow(unused)]
pub fn msm_te<T: TECurveConfig>(bases: Vec<u8>, scalars: Vec<u8>) -> Result<Vec<u8>, ()> {
	let bases = decode::<Vec<TEAffine<T>>>(bases)?;
	let scalars = decode::<Vec<<T as CurveConfig>::ScalarField>>(scalars)?;
	let res = <TEProjective<T> as VariableBaseMSM>::msm(&bases, &scalars).map_err(|_| ())?;
	Ok(encode_proj_te(&res))
}

#[allow(unused)]
pub fn mul_projective_sw<T: SWCurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
	let base = decode_proj_sw::<T>(base)?;
	let scalar = decode::<Vec<u64>>(scalar)?;
	let res = <T as SWCurveConfig>::mul_projective(&base, &scalar);
	Ok(encode_proj_sw(&res))
}

#[allow(unused)]
pub fn mul_projective_te<T: TECurveConfig>(base: Vec<u8>, scalar: Vec<u8>) -> Result<Vec<u8>, ()> {
	let base = decode_proj_te::<T>(base)?;
	let scalar = decode::<Vec<u64>>(scalar)?;
	let res = <T as TECurveConfig>::mul_projective(&base, &scalar);
	Ok(encode_proj_te(&res))
}
