// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Tests that a prepared message reserves enough weight to cover a barrier rejection: since the
//! barrier-check weight can exceed a cheap message's execution weight, `weight_of()` must report
//! the maximum of the two so a caller that reserves it never overruns on rejection.

use super::mock::*;
use crate::{
	traits::{Properties, ShouldExecute, WeightBounds},
	Config, XcmExecutor,
};
use frame_support::traits::{Everything, Nothing, ProcessMessageError};
use xcm::prelude::*;

const EXEC_WEIGHT: Weight = Weight::from_parts(2, 2);
/// Deliberately larger than `EXEC_WEIGHT` in both dimensions: the worst-case barrier scan can cost
/// more than executing a trivial message.
const BARRIER_WEIGHT: Weight = Weight::from_parts(100, 100);

/// Weigher whose `barrier_check_weight()` exceeds the per-message execution weight.
pub struct BarrierWeigher;
impl<C> WeightBounds<C> for BarrierWeigher {
	fn weight(_message: &mut Xcm<C>, _weight_limit: Weight) -> Result<Weight, InstructionError> {
		Ok(EXEC_WEIGHT)
	}
	fn instr_weight(_instruction: &mut Instruction<C>) -> Result<Weight, XcmError> {
		Ok(EXEC_WEIGHT)
	}
	fn barrier_check_weight() -> Option<Weight> {
		Some(BARRIER_WEIGHT)
	}
}

/// Barrier that rejects every message, so `execute` always takes the barrier-rejection path.
pub struct RejectingBarrier;
impl ShouldExecute for RejectingBarrier {
	fn should_execute<Call>(
		_origin: &Location,
		_instructions: &mut [Instruction<Call>],
		_max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		Err(ProcessMessageError::Unsupported)
	}
}

/// Like the shared mock `XcmConfig`, but with a weigher that reports a barrier-check weight and a
/// barrier that always rejects.
pub struct BarrierWeightConfig;
impl Config for BarrierWeightConfig {
	type RuntimeCall = TestCall;
	type XcmSender = TestSender;
	type XcmEventEmitter = ();
	type AssetTransactor = TestAssetTransactor;
	type OriginConverter = ();
	type IsReserve = ();
	type IsTeleporter = ();
	type UniversalLocation = UniversalLocation;
	type Barrier = RejectingBarrier;
	type Weigher = BarrierWeigher;
	type Trader = TestTrader;
	type ResponseHandler = ();
	type AssetTrap = TestAssetTrap;
	type AssetLocker = ();
	type AssetExchanger = ();
	type SubscriptionService = ();
	type PalletInstancesInfo = ();
	type MaxAssetsIntoHolding = MaxAssetsIntoHolding;
	type FeeManager = TestFeeManager;
	type MessageExporter = ();
	type UniversalAliases = Nothing;
	type CallDispatcher = Self::RuntimeCall;
	type SafeCallFilter = Everything;
	type Aliasers = Nothing;
	type TransactionalProcessor = TestTransactionalProcessor;
	type HrmpNewChannelOpenRequestHandler = ();
	type HrmpChannelAcceptedHandler = ();
	type HrmpChannelClosingHandler = ();
	type XcmRecorder = ();
}

#[test]
fn weight_of_reserves_for_barrier_rejection() {
	let message = Xcm::<TestCall>(vec![ClearOrigin]);

	// `weight_of()` is the worst case over execute-or-reject: max(EXEC_WEIGHT, BARRIER_WEIGHT).
	let prepared = XcmExecutor::<BarrierWeightConfig>::prepare(message.clone(), Weight::MAX)
		.expect("weighing succeeds");
	let reserved = prepared.weight_of();
	assert_eq!(reserved, BARRIER_WEIGHT);

	// Executing rejects at the barrier and charges `barrier_check_weight()`, which must not exceed
	// what `weight_of()` reserved.
	let mut hash = [0u8; 32];
	let outcome =
		XcmExecutor::<BarrierWeightConfig>::execute(Here, prepared, &mut hash, Weight::zero());
	let used = match outcome {
		Outcome::Incomplete { used, error: InstructionError { error: XcmError::Barrier, .. } } => {
			used
		},
		other => panic!("expected barrier rejection, got {other:?}"),
	};
	assert_eq!(used, BARRIER_WEIGHT);
	assert!(used.all_lte(reserved), "consumed weight {used:?} exceeds reserved {reserved:?}");
}

#[test]
fn weight_of_unchanged_without_barrier_check_weight() {
	// The shared mock `XcmConfig` uses a weigher whose `barrier_check_weight()` is `None`, so
	// `weight_of()` must equal the plain execution weight — no regression for runtimes that do not
	// opt into precise barrier weighting.
	let message = Xcm::<TestCall>(vec![ClearOrigin]);
	let exec_weight =
		<TestWeigher as WeightBounds<TestCall>>::weight(&mut message.clone(), Weight::MAX).unwrap();
	assert_eq!(<TestWeigher as WeightBounds<TestCall>>::barrier_check_weight(), None);

	let reserved = XcmExecutor::<XcmConfig>::prepare(message, Weight::MAX)
		.expect("weighing succeeds")
		.weight_of();
	assert_eq!(reserved, exec_weight);
}
