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

//! XCM sender for relay chain.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use core::marker::PhantomData;
use frame_support::traits::Get;
use frame_system::pallet_prelude::BlockNumberFor;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::{
	configuration::{self, HostConfiguration},
	dmp, FeeTracker,
};
use sp_runtime::FixedPointNumber;
use xcm::prelude::*;
use xcm_builder::InspectMessageQueues;
use SendError::*;

/// Simple value-bearing trait for determining/expressing the assets required to be paid for a
/// messages to be delivered to a parachain.
pub trait PriceForMessageDelivery {
	/// Type used for charging different prices to different destinations
	type Id;
	/// Return the assets required to deliver `message` to the given `para` destination.
	fn price_for_delivery(id: Self::Id, message: &Xcm<()>) -> Assets;
}
impl PriceForMessageDelivery for () {
	type Id = ();

	fn price_for_delivery(_: Self::Id, _: &Xcm<()>) -> Assets {
		Assets::new()
	}
}

pub struct NoPriceForMessageDelivery<Id>(PhantomData<Id>);
impl<Id> PriceForMessageDelivery for NoPriceForMessageDelivery<Id> {
	type Id = Id;

	fn price_for_delivery(_: Self::Id, _: &Xcm<()>) -> Assets {
		Assets::new()
	}
}

/// Implementation of [`PriceForMessageDelivery`] which returns a fixed price.
pub struct ConstantPrice<T>(core::marker::PhantomData<T>);
impl<T: Get<Assets>> PriceForMessageDelivery for ConstantPrice<T> {
	type Id = ();

	fn price_for_delivery(_: Self::Id, _: &Xcm<()>) -> Assets {
		T::get()
	}
}

/// Implementation of [`PriceForMessageDelivery`] which returns an exponentially increasing price.
/// The formula for the fee is based on the sum of a base fee plus a message length fee, multiplied
/// by a specified factor. In mathematical form:
///
/// `F * (B + encoded_msg_len * M)`
///
/// Thus, if F = 1 and M = 0, this type is equivalent to [`ConstantPrice<B>`].
///
/// The type parameters are understood as follows:
///
/// - `A`: Used to denote the asset ID that will be used for paying the delivery fee.
/// - `B`: The base fee to pay for message delivery.
/// - `M`: The fee to pay for each and every byte of the message after encoding it.
/// - `F`: A fee factor multiplier. It can be understood as the exponent term in the formula.
pub struct ExponentialPrice<A, B, M, F>(core::marker::PhantomData<(A, B, M, F)>);
impl<A: Get<AssetId>, B: Get<u128>, M: Get<u128>, F: FeeTracker> PriceForMessageDelivery
	for ExponentialPrice<A, B, M, F>
{
	type Id = F::Id;

	fn price_for_delivery(id: Self::Id, msg: &Xcm<()>) -> Assets {
		let msg_fee = (msg.encoded_size() as u128).saturating_mul(M::get());
		let fee_sum = B::get().saturating_add(msg_fee);
		let amount = F::get_fee_factor(id).saturating_mul_int(fee_sum);
		(A::get(), amount).into()
	}
}

/// XCM sender for relay chain. It only sends downward message.
pub struct ChildParachainRouter<T, W, P>(PhantomData<(T, W, P)>);

impl<T: configuration::Config + dmp::Config, W: xcm::WrapVersion, P> SendXcm
	for ChildParachainRouter<T, W, P>
where
	P: PriceForMessageDelivery<Id = ParaId>,
{
	type Ticket = (HostConfiguration<BlockNumberFor<T>>, ParaId, Vec<u8>);

	fn validate(
		dest: &mut Option<Location>,
		msg: &mut Option<Xcm<()>>,
	) -> SendResult<(HostConfiguration<BlockNumberFor<T>>, ParaId, Vec<u8>)> {
		let d = dest.take().ok_or(MissingArgument)?;
		let id = if let (0, [Parachain(id)]) = d.unpack() {
			*id
		} else {
			*dest = Some(d);
			return Err(NotApplicable)
		};

		// Downward message passing.
		let xcm = msg.take().ok_or(MissingArgument)?;
		let config = configuration::ActiveConfig::<T>::get();
		let para = id.into();
		let price = P::price_for_delivery(para, &xcm);
		let versioned_xcm = W::wrap_version(&d, xcm).map_err(|()| DestinationUnsupported)?;
		versioned_xcm.validate_xcm_nesting().map_err(|()| ExceedsMaxMessageSize)?;
		let blob = versioned_xcm.encode();
		dmp::Pallet::<T>::can_queue_downward_message(&config, &para, &blob)
			.map_err(Into::<SendError>::into)?;

		Ok(((config, para, blob), price))
	}

	fn deliver(
		(config, para, blob): (HostConfiguration<BlockNumberFor<T>>, ParaId, Vec<u8>),
	) -> Result<XcmHash, SendError> {
		let hash = sp_io::hashing::blake2_256(&blob[..]);
		dmp::Pallet::<T>::queue_downward_message(&config, para, blob)
			.map(|()| hash)
			.map_err(|_| SendError::Transport(&"Error placing into DMP queue"))
	}
}

impl<T: dmp::Config, W, P> InspectMessageQueues for ChildParachainRouter<T, W, P> {
	fn clear_messages() {
		// Best effort.
		let _ = dmp::DownwardMessageQueues::<T>::clear(u32::MAX, None);
	}

	fn get_messages() -> Vec<(VersionedLocation, Vec<VersionedXcm<()>>)> {
		dmp::DownwardMessageQueues::<T>::iter()
			.map(|(para_id, messages)| {
				let decoded_messages: Vec<VersionedXcm<()>> = messages
					.iter()
					.map(|downward_message| {
						let message = VersionedXcm::<()>::decode(&mut &downward_message.msg[..]).unwrap();
						log::trace!(target: "xcm::DownwardMessageQueues::get_messages", "Message: {:?}, sent at: {:?}", message, downward_message.sent_at);
						message
					})
					.collect();
				(VersionedLocation::from(Location::from(Parachain(para_id.into()))), decoded_messages)
			})
			.collect()
	}
}

/// Implementation of `xcm_builder::EnsureDelivery` which helps to ensure delivery to the
/// `ParaId` parachain (sibling or child). Deposits existential deposit for origin (if needed).
/// Deposits estimated fee to the origin account (if needed).
/// Allows to trigger additional logic for specific `ParaId` (e.g. open HRMP channel) (if needed).
#[cfg(feature = "runtime-benchmarks")]
pub struct ToParachainDeliveryHelper<
	XcmConfig,
	ExistentialDeposit,
	PriceForDelivery,
	ParaId,
	ToParaIdHelper,
>(
	core::marker::PhantomData<(
		XcmConfig,
		ExistentialDeposit,
		PriceForDelivery,
		ParaId,
		ToParaIdHelper,
	)>,
);

#[cfg(feature = "runtime-benchmarks")]
impl<
		XcmConfig: xcm_executor::Config,
		ExistentialDeposit: Get<Option<Asset>>,
		PriceForDelivery: PriceForMessageDelivery<Id = ParaId>,
		Parachain: Get<ParaId>,
		ToParachainHelper: EnsureForParachain,
	> xcm_builder::EnsureDelivery
	for ToParachainDeliveryHelper<
		XcmConfig,
		ExistentialDeposit,
		PriceForDelivery,
		Parachain,
		ToParachainHelper,
	>
{
	fn ensure_successful_delivery(
		origin_ref: &Location,
		dest: &Location,
		fee_reason: xcm_executor::traits::FeeReason,
	) -> (Option<xcm_executor::FeesMode>, Option<Assets>) {
		use xcm_executor::{
			traits::{FeeManager, TransactAsset},
			FeesMode,
		};

		// check if the destination matches the expected `Parachain`.
		if let Some(Parachain(para_id)) = dest.first_interior() {
			if ParaId::from(*para_id) != Parachain::get().into() {
				return (None, None)
			}
		} else {
			return (None, None)
		}

		let mut fees_mode = None;
		if !XcmConfig::FeeManager::is_waived(Some(origin_ref), fee_reason) {
			// if not waived, we need to set up accounts for paying and receiving fees

			// mint ED to origin if needed
			if let Some(ed) = ExistentialDeposit::get() {
				XcmConfig::AssetTransactor::deposit_asset(&ed, &origin_ref, None).unwrap();
			}

			// overestimate delivery fee
			let overestimated_xcm = alloc::vec![ClearOrigin; 128].into();
			let overestimated_fees =
				PriceForDelivery::price_for_delivery(Parachain::get(), &overestimated_xcm);

			// mint overestimated fee to origin
			for fee in overestimated_fees.inner() {
				XcmConfig::AssetTransactor::deposit_asset(&fee, &origin_ref, None).unwrap();
			}

			// allow more initialization for target parachain
			ToParachainHelper::ensure(Parachain::get());

			// expected worst case - direct withdraw
			fees_mode = Some(FeesMode { jit_withdraw: true });
		}
		(fees_mode, None)
	}
}

/// Ensure more initialization for `ParaId`. (e.g. open HRMP channels, ...)
#[cfg(feature = "runtime-benchmarks")]
pub trait EnsureForParachain {
	fn ensure(para_id: ParaId);
}
#[cfg(feature = "runtime-benchmarks")]
impl EnsureForParachain for () {
	fn ensure(_: ParaId) {
		// doing nothing
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::integration_tests::new_test_ext;
	use alloc::vec;
	use frame_support::{assert_ok, parameter_types};
	use polkadot_runtime_parachains::FeeTracker;
	use sp_runtime::FixedU128;
	use xcm::MAX_XCM_DECODE_DEPTH;

	parameter_types! {
		pub const BaseDeliveryFee: u128 = 300_000_000;
		pub const TransactionByteFee: u128 = 1_000_000;
		pub FeeAssetId: AssetId = AssetId(Here.into());
	}

	struct TestFeeTracker;
	impl FeeTracker for TestFeeTracker {
		type Id = ParaId;

		fn get_fee_factor(_: Self::Id) -> FixedU128 {
			FixedU128::from_rational(101, 100)
		}

		fn increase_fee_factor(_: Self::Id, _: FixedU128) -> FixedU128 {
			FixedU128::from_rational(101, 100)
		}

		fn decrease_fee_factor(_: Self::Id) -> FixedU128 {
			FixedU128::from_rational(101, 100)
		}
	}

	type TestExponentialPrice =
		ExponentialPrice<FeeAssetId, BaseDeliveryFee, TransactionByteFee, TestFeeTracker>;

	#[test]
	fn exponential_price_correct_price_calculation() {
		let id: ParaId = 123.into();
		let b: u128 = BaseDeliveryFee::get();
		let m: u128 = TransactionByteFee::get();

		// F * (B + msg_length * M)
		// message_length = 1
		let result: u128 = TestFeeTracker::get_fee_factor(id).saturating_mul_int(b + m);
		assert_eq!(
			TestExponentialPrice::price_for_delivery(id, &Xcm(vec![])),
			(FeeAssetId::get(), result).into()
		);

		// message size = 2
		let result: u128 = TestFeeTracker::get_fee_factor(id).saturating_mul_int(b + (2 * m));
		assert_eq!(
			TestExponentialPrice::price_for_delivery(id, &Xcm(vec![ClearOrigin])),
			(FeeAssetId::get(), result).into()
		);

		// message size = 4
		let result: u128 = TestFeeTracker::get_fee_factor(id).saturating_mul_int(b + (4 * m));
		assert_eq!(
			TestExponentialPrice::price_for_delivery(
				id,
				&Xcm(vec![SetAppendix(Xcm(vec![ClearOrigin]))])
			),
			(FeeAssetId::get(), result).into()
		);
	}

	#[test]
	fn child_parachain_router_validate_nested_xcm_works() {
		let dest = Parachain(5555);

		type Router = ChildParachainRouter<
			crate::integration_tests::Test,
			(),
			NoPriceForMessageDelivery<ParaId>,
		>;

		// Message that is not too deeply nested:
		let mut good = Xcm(vec![ClearOrigin]);
		for _ in 0..MAX_XCM_DECODE_DEPTH - 1 {
			good = Xcm(vec![SetAppendix(good)]);
		}

		new_test_ext().execute_with(|| {
			configuration::ActiveConfig::<crate::integration_tests::Test>::mutate(|c| {
				c.max_downward_message_size = u32::MAX;
			});

			// Check that the good message is validated:
			assert_ok!(<Router as SendXcm>::validate(
				&mut Some(dest.into()),
				&mut Some(good.clone())
			));

			// Nesting the message one more time should reject it:
			let bad = Xcm(vec![SetAppendix(good)]);
			assert_eq!(
				Err(ExceedsMaxMessageSize),
				<Router as SendXcm>::validate(&mut Some(dest.into()), &mut Some(bad))
			);
		});
	}
}
