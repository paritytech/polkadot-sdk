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

//! *Ed-on-BLS12-377* types and host functions.

use crate::utils::{self, FAIL_MSG};
use alloc::vec::Vec;
use ark_ec::{AffineRepr, CurveConfig, CurveGroup};
use ark_ed_on_bls12_377_ext::CurveHooks;
use sp_runtime_interface::{
	pass_by::{AllocateAndReturnByCodec, PassFatPointerAndRead},
	runtime_interface,
};

/// Group configuration.
pub type EdwardsConfig = ark_ed_on_bls12_377_ext::EdwardsConfig<HostHooks>;
/// Twisted Edwards form point affine representation.
pub type EdwardsAffine = ark_ed_on_bls12_377_ext::EdwardsAffine<HostHooks>;
/// Twisted Edwards form point projective representation.
pub type EdwardsProjective = ark_ed_on_bls12_377_ext::EdwardsProjective<HostHooks>;

/// Group scalar field (Fr).
pub type ScalarField = <EdwardsConfig as CurveConfig>::ScalarField;

/// Curve hooks jumping into [`host_calls`] host functions.
#[derive(Copy, Clone)]
pub struct HostHooks;

impl CurveHooks for HostHooks {
	fn msm(bases: &[EdwardsAffine], scalars: &[ScalarField]) -> EdwardsProjective {
		host_calls::ed_on_bls12_377_te_msm(utils::encode(bases), utils::encode(scalars))
			.and_then(|res| utils::decode::<EdwardsAffine>(res))
			.expect(FAIL_MSG)
			.into_group()
	}

	fn mul_projective(base: &EdwardsProjective, scalar: &[u64]) -> EdwardsProjective {
		host_calls::ed_on_bls12_377_te_mul_affine(
			utils::encode(base.into_affine()),
			utils::encode(scalar),
		)
		.and_then(|res| utils::decode::<EdwardsAffine>(res))
		.expect(FAIL_MSG)
		.into_group()
	}
}

/// Interfaces for working with *Arkworks* *Ed-on-BLS12-377* elliptic curve related types
/// from within the runtime.
///
/// All types are (de-)serialized through the wrapper types from `ark-scale`.
///
/// `ArkScale`'s `Usage` generic parameter is expected to be set to "not-validated"
/// and "not-compressed".
#[runtime_interface]
pub trait HostCalls {
	/// Twisted Edwards multi scalar multiplication for *Ed-on-BLS12-377*.
	///
	/// Receives encoded:
	/// - `bases`: `Vec<EdwardsAffine>`.
	/// - `scalars`: `Vec<ScalarField>`.
	/// Returns encoded: `EdwardsAffine`.
	fn ed_on_bls12_377_te_msm(
		bases: PassFatPointerAndRead<Vec<u8>>,
		scalars: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::msm_te::<ark_ed_on_bls12_377::EdwardsConfig>(bases, scalars)
	}

	/// Twisted Edwards affine multiplication for *Ed-on-BLS12-377*.
	///
	/// Receives encoded:
	/// - `base`: `EdwardsAffine`.
	/// - `scalar`: `BigInteger`.
	/// Returns encoded: `EdwardsAffine`.
	fn ed_on_bls12_377_te_mul_affine(
		base: PassFatPointerAndRead<Vec<u8>>,
		scalar: PassFatPointerAndRead<Vec<u8>>,
	) -> AllocateAndReturnByCodec<Result<Vec<u8>, ()>> {
		utils::mul_affine_te::<ark_ed_on_bls12_377::EdwardsConfig>(base, scalar)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::utils::testing::*;

	#[test]
	fn mul_works() {
		mul_te_test::<EdwardsAffine, ark_ed_on_bls12_377::EdwardsAffine>();
	}

	#[test]
	fn msm_works() {
		msm_te_test::<EdwardsAffine, ark_ed_on_bls12_377::EdwardsAffine>();
	}
}
