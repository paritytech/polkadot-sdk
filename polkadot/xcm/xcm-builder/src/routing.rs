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

//! Various implementations for `SendXcm`.

use frame_system::unique;
use parity_scale_codec::Encode;
use sp_std::{marker::PhantomData, result::Result};
use xcm::prelude::*;

/// Wrapper router which, if the message does not already end with a `SetTopic` instruction,
/// appends one to the message filled with a universally unique ID. This ID is returned from a
/// successful `deliver`.
///
/// If the message does already end with a `SetTopic` instruction, then it is the responsibility
/// of the code author to ensure that the ID supplied to `SetTopic` is universally unique. Due to
/// this property, consumers of the topic ID must be aware that a user-supplied ID may not be
/// unique.
///
/// This is designed to be at the top-level of any routers, since it will always mutate the
/// passed `message` reference into a `None`. Don't try to combine it within a tuple except as the
/// last element.
pub struct WithUniqueTopic<Inner>(PhantomData<Inner>);
impl<Inner: SendXcm> SendXcm for WithUniqueTopic<Inner> {
	type Ticket = (Inner::Ticket, [u8; 32]);

	fn validate(
		destination: &mut Option<MultiLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let mut message = message.take().ok_or(SendError::MissingArgument)?;
		let unique_id = if let Some(SetTopic(id)) = message.last() {
			*id
		} else {
			let unique_id = unique(&message);
			message.0.push(SetTopic(unique_id));
			unique_id
		};
		let (ticket, assets) = Inner::validate(destination, &mut Some(message))?;
		Ok(((ticket, unique_id), assets))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let (ticket, unique_id) = ticket;
		Inner::deliver(ticket)?;
		Ok(unique_id)
	}
}

pub trait SourceTopic {
	fn source_topic(entropy: impl Encode) -> XcmHash;
}

impl SourceTopic for () {
	fn source_topic(_: impl Encode) -> XcmHash {
		[0u8; 32]
	}
}

/// Wrapper router which, if the message does not already end with a `SetTopic` instruction,
/// prepends one to the message filled with an ID from `TopicSource`. This ID is returned from a
/// successful `deliver`.
///
/// This is designed to be at the top-level of any routers, since it will always mutate the
/// passed `message` reference into a `None`. Don't try to combine it within a tuple except as the
/// last element.
pub struct WithTopicSource<Inner, TopicSource>(PhantomData<(Inner, TopicSource)>);
impl<Inner: SendXcm, TopicSource: SourceTopic> SendXcm for WithTopicSource<Inner, TopicSource> {
	type Ticket = (Inner::Ticket, [u8; 32]);

	fn validate(
		destination: &mut Option<MultiLocation>,
		message: &mut Option<Xcm<()>>,
	) -> SendResult<Self::Ticket> {
		let mut message = message.take().ok_or(SendError::MissingArgument)?;
		let unique_id = if let Some(SetTopic(id)) = message.last() {
			*id
		} else {
			let unique_id = TopicSource::source_topic(&message);
			message.0.push(SetTopic(unique_id));
			unique_id
		};
		let (ticket, assets) = Inner::validate(destination, &mut Some(message))
			.map_err(|_| SendError::NotApplicable)?;
		Ok(((ticket, unique_id), assets))
	}

	fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
		let (ticket, unique_id) = ticket;
		Inner::deliver(ticket)?;
		Ok(unique_id)
	}
}

fn split_message(message: &mut Option<Xcm<()>>) {
    if let Some(xcm) = message {
        let instructions = xcm.inner_mut();
        let mut initial_fund = false;
        let mut clear_origin_instructions = 0;

        for item in instructions.iter().enumerate() {
            match item {
                (0, WithdrawAsset(assets, ..)) if assets.len() > 1 => {
                    initial_fund = true;
                }
                (n, ClearOrigin) if n > 0 && n <= 4 => {
                    clear_origin_instructions += 1;
                }
                (n, BuyExecution { fees, .. }) if n > 0 && n <= 5 && initial_fund => {
                    if let Some(WithdrawAsset(assets)) = instructions.first() {
                        let fee_asset = MultiAssets::from(
                            assets
                                .inner()
                                .iter()
                                .filter(|asset| asset.id == fees.id)
                                .cloned()
                                .collect::<Vec<_>>(),
                        );
                        let extra_assets = MultiAssets::from(
                            assets
                                .inner()
                                .iter()
                                .filter(|asset| asset.id != fees.id)
                                .cloned()
                                .collect::<Vec<_>>(),
                        );

                        instructions[0] = WithdrawAsset(fee_asset);
                        instructions.insert(n + 1, WithdrawAsset(extra_assets));
                        if clear_origin_instructions > 0 {
                            instructions.insert(n + 2, ClearOrigin);
                            for index in 1..1 + clear_origin_instructions {
                                instructions.remove(index);
                            }
                        }
                    }
                    break;
                }
                _ => {
                    break;
                }
            }
        }
    }
}

pub struct SplitXcmRouter<InnerRouter>(InnerRouter);

impl<InnerRouter: SendXcm> SendXcm for SplitXcmRouter<InnerRouter> {
    type Ticket = InnerRouter::Ticket;

    fn validate(destination: &mut Option<MultiLocation>, message: &mut Option<Xcm<()>>) -> SendResult<Self::Ticket> {
        split_message(message);
        InnerRouter::validate(destination, message)
    }

    fn deliver(ticket: Self::Ticket) -> Result<XcmHash, SendError> {
        InnerRouter::deliver(ticket)
    }
}

#[cfg(test)]
mod tests {
	use super::*;
	use core::cell::RefCell;

	fn fake_message_hash<T>(message: &Xcm<T>) -> XcmHash {
		message.using_encoded(sp_io::hashing::blake2_256)
	}
	thread_local! {
		pub static SENT_XCM: RefCell<Vec<(MultiLocation, opaque::Xcm, XcmHash)>> = RefCell::new(Vec::new());
	}
	pub fn sent_xcm() -> Vec<(MultiLocation, opaque::Xcm, XcmHash)> {
		SENT_XCM.with(|q| (*q.borrow()).clone())
	}
	pub struct TestSendXcm;
	impl SendXcm for TestSendXcm {
		type Ticket = (MultiLocation, Xcm<()>, XcmHash);
		fn validate(
			dest: &mut Option<MultiLocation>,
			msg: &mut Option<Xcm<()>>,
		) -> SendResult<(MultiLocation, Xcm<()>, XcmHash)> {
			let msg = msg.take().unwrap();
			let hash = fake_message_hash(&msg);
			let triplet = (dest.take().unwrap(), msg, hash);
			Ok((triplet, MultiAssets::new()))
		}
		fn deliver(triplet: (MultiLocation, Xcm<()>, XcmHash)) -> Result<XcmHash, SendError> {
			let hash = triplet.2;
			SENT_XCM.with(|q| q.borrow_mut().push(triplet));
			Ok(hash)
		}
	}

	#[test]
	fn split_xcm_router_works() {
		// Split XCM router wrapping a test sender
		type Router = SplitXcmRouter<TestSendXcm>;

		let fee_asset: MultiAsset = (GeneralIndex(2), 100u128).into();
		let multiple_assets: MultiAssets = vec![
			(GeneralIndex(1), 100u128).into(),
			fee_asset.clone(),
			(GeneralIndex(3), 100u128).into(),
		].into();
		let message = Xcm(vec![
			WithdrawAsset(multiple_assets.clone()),
			ClearOrigin,
			BuyExecution { fees: fee_asset.clone(), weight_limit: Unlimited },
			DepositAsset { assets: AllCounted(3).into(), beneficiary: AccountId32 { id: [0u8; 32], network: None }.into() },
		]);
		let multiple_assets_without_fee: MultiAssets = vec![
			(GeneralIndex(1), 100u128).into(),
			(GeneralIndex(3), 100u128).into(),
		].into();
		let expected_message = Xcm(vec![
			WithdrawAsset(fee_asset.clone().into()),
			BuyExecution { fees: fee_asset.clone(), weight_limit: Unlimited },
			WithdrawAsset(multiple_assets_without_fee),
			ClearOrigin,
			DepositAsset { assets: AllCounted(3).into(), beneficiary: AccountId32 { id: [0u8; 32], network: None }.into() },
		]);
		let (ticket, _) = Router::validate(&mut Some(MultiLocation::parent()), &mut Some(message)).unwrap();
		let _ = Router::deliver(ticket).unwrap();
		let sent_xcms = sent_xcm();
		let (_, message_sent, _) = sent_xcms.first().unwrap();
		assert_eq!(message_sent, &expected_message);
	}
}
