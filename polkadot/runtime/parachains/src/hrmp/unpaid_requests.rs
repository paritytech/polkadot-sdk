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

//! Free execution of the requests a non-system parachain makes for itself.
//!
//! A parachain makes its HRMP requests with an XCM `Transact` sent to the relay chain, and would
//! otherwise pay for its execution with funds of its own on the relay chain. A non-system
//! parachain gets in without paying, for one call at a time as itself, as long as the message
//! weighs no more than the costliest HRMP call it makes for itself:
//! [`AllowUnpaidParaRequests`] admits the message, [`MaxUnpaidParaRequestWeight`] bounds it.
//!
//! A relay chain adds the barrier at the top level of its XCM barrier, outside of any
//! `WithComputedOrigin`, since it admits only messages coming from the child parachain itself:
//!
//! ```ignore
//! pub type Barrier = TrailingSetTopicAsId<(
//! 	TakeWeightCredit,
//! 	AllowKnownQueryResponses<XcmPallet>,
//! 	hrmp::unpaid_requests::AllowUnpaidParaRequests<
//! 		hrmp::unpaid_requests::MaxUnpaidParaRequestWeight<Runtime, Weigher>,
//! 	>,
//! 	WithComputedOrigin<(/* ... */), UniversalLocation, ConstU32<8>>,
//! )>;
//! ```
//!
//! where `Weigher` is the `Weigher` of the XCM executor's configuration.

use super::{Call, Config};
use alloc::vec;
use codec::Encode;
use core::marker::PhantomData;
use frame_support::{
	traits::{Get, ProcessMessageError},
	weights::Weight,
};
use polkadot_parachain_primitives::primitives::IsSystem;
use polkadot_primitives::{HrmpChannelId, Id as ParaId};
use xcm::latest::prelude::*;
use xcm_executor::traits::{Properties, ShouldExecute, WeightBounds};

/// The witness a para may give `hrmp_cancel_open_request` for free; with more open requests
/// pending, it pays. As many as the pallet's benchmarks measure.
pub const UNPAID_CANCEL_OPEN_REQUESTS: u32 = 128;

/// The weight of the heaviest message [`AllowUnpaidParaRequests`] admits: `UnpaidExecution`, then
/// one `Transact` of the costliest HRMP call a para makes for itself, then the `SetTopic` a
/// para's router appends, weighed by `Weigher` as the executor weighs it.
///
/// The barrier is given the weight of the whole message, `SetTopic` included, even though
/// `TrailingSetTopicAsId` strips the instruction before the barrier sees it. A message without a
/// topic is thus allowed one `SetTopic` of slack.
///
/// It weighs a few messages each time it is read, which the barrier does only for the messages
/// it would admit otherwise.
pub struct MaxUnpaidParaRequestWeight<T, Weigher>(PhantomData<(T, Weigher)>);
impl<T, Weigher> Get<Weight> for MaxUnpaidParaRequestWeight<T, Weigher>
where
	T: Config,
	<T as frame_system::Config>::RuntimeCall: From<Call<T>>,
	Weigher: WeightBounds<<T as frame_system::Config>::RuntimeCall>,
{
	fn get() -> Weight {
		let para = ParaId::from(0);
		let channel_id = HrmpChannelId { sender: para, recipient: para };
		let calls = [
			Call::<T>::hrmp_init_open_channel {
				recipient: para,
				proposed_max_capacity: 0,
				proposed_max_message_size: 0,
			},
			Call::hrmp_accept_open_channel { sender: para },
			Call::hrmp_close_channel { channel_id: channel_id.clone() },
			Call::hrmp_cancel_open_request {
				channel_id,
				open_requests: UNPAID_CANCEL_OPEN_REQUESTS,
			},
			Call::poke_channel_deposits { sender: para, recipient: para },
			Call::establish_channel_with_system { target_system_chain: para },
		];
		calls.into_iter().fold(Weight::zero(), |max, call| {
			let call = <T as frame_system::Config>::RuntimeCall::from(call);
			let mut message = Xcm(vec![
				UnpaidExecution { weight_limit: Unlimited, check_origin: None },
				Transact {
					origin_kind: OriginKind::Native,
					fallback_max_weight: None,
					call: call.encode().into(),
				},
				SetTopic([0; 32]),
			]);
			// Weighing cannot fail on a message this short that decodes; were it to, a zero bound
			// admits nothing rather than everything.
			let weight = Weigher::weight(&mut message, Weight::MAX).unwrap_or(Weight::zero());
			max.max(weight)
		})
	}
}

/// Admits, without payment, the message a non-system child parachain sends to make one call for
/// itself: `UnpaidExecution` covering the message, then one `Transact` with `OriginKind::Native`,
/// the whole weighing at most `MaxWeight`.
///
/// `Native` keeps it to the para's own origin: free execution can never reach a signed call of
/// its sovereign account, only the calls that accept the para's own origin. `MaxWeight` bounds
/// what one such message costs the relay chain, see [`MaxUnpaidParaRequestWeight`].
///
/// NOTE: the executor bills a message the barrier refuses its full weight all the same, so this
/// bounds what an admitted message costs, not what a refused one does.
pub struct AllowUnpaidParaRequests<MaxWeight>(PhantomData<MaxWeight>);
impl<MaxWeight: Get<Weight>> ShouldExecute for AllowUnpaidParaRequests<MaxWeight> {
	fn should_execute<RuntimeCall>(
		origin: &Location,
		instructions: &mut [Instruction<RuntimeCall>],
		max_weight: Weight,
		_properties: &mut Properties,
	) -> Result<(), ProcessMessageError> {
		if !matches!(origin.unpack(), (0, [Parachain(id)]) if !ParaId::from(*id).is_system()) {
			return Err(ProcessMessageError::Unsupported);
		}
		let covers = |limit: &WeightLimit| match limit {
			WeightLimit::Unlimited => true,
			WeightLimit::Limited(limit) => limit.all_gte(max_weight),
		};
		// A trailing `SetTopic` is expected to be stripped already, by `TrailingSetTopicAsId`.
		match instructions {
			[UnpaidExecution { weight_limit, .. }, Transact { origin_kind: OriginKind::Native, .. }]
				if covers(weight_limit) && max_weight.all_lte(MaxWeight::get()) =>
			{
				Ok(())
			},
			_ => Err(ProcessMessageError::Unsupported),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mock::{RuntimeCall, Test};
	use alloc::vec::Vec;
	use frame_support::assert_ok;
	use xcm::DoubleEncoded;

	/// Weighs each instruction 1, plus a `Transact` the weight [`call_weight`] gives its call.
	struct TestWeigher;
	impl WeightBounds<RuntimeCall> for TestWeigher {
		fn weight(
			message: &mut Xcm<RuntimeCall>,
			_weight_limit: Weight,
		) -> Result<Weight, InstructionError> {
			message
				.0
				.iter_mut()
				.enumerate()
				.try_fold(Weight::zero(), |total, (index, instr)| {
					let weight = Self::instr_weight(instr)
						.map_err(|error| InstructionError { index: index as u8, error })?;
					Ok(total.saturating_add(weight))
				})
		}

		fn instr_weight(instruction: &mut Instruction<RuntimeCall>) -> Result<Weight, XcmError> {
			let call = match instruction {
				Transact { call, .. } => {
					call_weight(call.ensure_decoded().map_err(|_| XcmError::FailedToDecode)?)
				},
				_ => Weight::zero(),
			};
			Ok(Weight::from_parts(1, 1).saturating_add(call))
		}
	}

	/// The cancel is the heaviest in ref time, the poke in proof size.
	fn call_weight(call: &RuntimeCall) -> Weight {
		match call {
			RuntimeCall::Hrmp(Call::hrmp_cancel_open_request { open_requests, .. }) => {
				Weight::from_parts(10 + *open_requests as u64, 1)
			},
			RuntimeCall::Hrmp(Call::poke_channel_deposits { .. }) => Weight::from_parts(5, 20),
			_ => Weight::from_parts(1, 1),
		}
	}

	#[test]
	fn max_weight_is_the_heaviest_request() {
		// Three instructions, `SetTopic` included, plus the heaviest call in each dimension.
		assert_eq!(
			MaxUnpaidParaRequestWeight::<Test, TestWeigher>::get(),
			Weight::from_parts(3 + 10 + UNPAID_CANCEL_OPEN_REQUESTS as u64, 3 + 20),
		);
	}

	frame_support::parameter_types! {
		pub const Cap: Weight = Weight::from_parts(1_000, 1_000);
	}

	#[test]
	fn unpaid_para_requests_are_bounded() {
		type Allow = AllowUnpaidParaRequests<Cap>;
		let cap = Cap::get();

		let call: DoubleEncoded<RuntimeCall> =
			RuntimeCall::System(frame_system::Call::remark { remark: Vec::new() })
				.encode()
				.into();
		let transact =
			|origin_kind| Transact { origin_kind, fallback_max_weight: None, call: call.clone() };
		let message = |weight_limit| {
			vec![UnpaidExecution { weight_limit, check_origin: None }, transact(OriginKind::Native)]
		};
		let run = |origin: Location, mut instructions: Vec<Instruction<RuntimeCall>>, weight| {
			let mut properties = Properties { weight_credit: Weight::zero(), message_id: None };
			Allow::should_execute(&origin, &mut instructions, weight, &mut properties)
		};
		let para: Location = Parachain(2000).into();
		let one = Weight::from_parts(1, 1);
		let unsupported = Err(ProcessMessageError::Unsupported);

		assert_ok!(run(para.clone(), message(Unlimited), cap));
		assert_ok!(run(para.clone(), message(Limited(cap)), cap));
		// Heavier than any HRMP call: the para pays.
		assert_eq!(run(para.clone(), message(Unlimited), cap.saturating_add(one)), unsupported);
		// The para must cover the whole message.
		assert_eq!(run(para.clone(), message(Limited(cap.saturating_sub(one))), cap), unsupported);
		// Only as the para itself.
		let as_account = vec![
			UnpaidExecution { weight_limit: Unlimited, check_origin: None },
			transact(OriginKind::SovereignAccount),
		];
		assert_eq!(run(para.clone(), as_account, cap), unsupported);
		// Only one call, and nothing else.
		let mut two_calls = message(Unlimited);
		two_calls.push(transact(OriginKind::Native));
		assert_eq!(run(para.clone(), two_calls, cap), unsupported);
		assert_eq!(run(para.clone(), vec![transact(OriginKind::Native)], cap), unsupported);
		// Only non-system child parachains; system parachains have a barrier of their own.
		assert_eq!(run(Parachain(1000).into(), message(Unlimited), cap), unsupported);
		let sibling = Location::new(1, [Parachain(2000)]);
		assert_eq!(run(sibling, message(Unlimited), cap), unsupported);
	}
}
