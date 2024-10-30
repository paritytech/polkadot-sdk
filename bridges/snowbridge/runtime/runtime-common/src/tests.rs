use crate::XcmExportFeeToSibling;
use frame_support::{parameter_types, sp_runtime::testing::H256};
use snowbridge_core::outbound::{
	v1::{Fee, Message, SendError, SendMessage},
	SendMessageFeeProvider,
};
use xcm::prelude::{
	Asset, Assets, Here, Kusama, Location, NetworkId, Parachain, XcmContext, XcmError, XcmHash,
	XcmResult,
};
use xcm_builder::HandleFee;
use xcm_executor::{
	traits::{FeeReason, TransactAsset},
	AssetsInHolding,
};

parameter_types! {
	pub EthereumNetwork: NetworkId = NetworkId::Ethereum { chain_id: 11155111 };
	pub TokenLocation: Location = Location::parent();
}

struct MockOkOutboundQueue;
impl SendMessage for MockOkOutboundQueue {
	type Ticket = ();

	fn validate(_: &Message) -> Result<(Self::Ticket, Fee<Self::Balance>), SendError> {
		Ok(((), Fee { local: 1, remote: 1 }))
	}

	fn deliver(_: Self::Ticket) -> Result<H256, SendError> {
		Ok(H256::zero())
	}
}

impl SendMessageFeeProvider for MockOkOutboundQueue {
	type Balance = u128;

	fn local_fee() -> Self::Balance {
		1
	}
}
struct MockErrOutboundQueue;
impl SendMessage for MockErrOutboundQueue {
	type Ticket = ();

	fn validate(_: &Message) -> Result<(Self::Ticket, Fee<Self::Balance>), SendError> {
		Err(SendError::MessageTooLarge)
	}

	fn deliver(_: Self::Ticket) -> Result<H256, SendError> {
		Err(SendError::MessageTooLarge)
	}
}

impl SendMessageFeeProvider for MockErrOutboundQueue {
	type Balance = u128;

	fn local_fee() -> Self::Balance {
		1
	}
}

pub struct MockAssetTransactor;
impl TransactAsset for MockAssetTransactor {
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn can_check_out(_dest: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		Ok(())
	}

	fn deposit_asset(_what: &Asset, _who: &Location, _context: Option<&XcmContext>) -> XcmResult {
		Ok(())
	}

	fn withdraw_asset(
		_what: &Asset,
		_who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(Assets::default().into())
	}

	fn internal_transfer_asset(
		_what: &Asset,
		_from: &Location,
		_to: &Location,
		_context: &XcmContext,
	) -> Result<AssetsInHolding, XcmError> {
		Ok(Assets::default().into())
	}
}

#[test]
fn handle_fee_success() {
	let fee: Assets = Asset::from((Location::parent(), 10_u128)).into();
	let ctx = XcmContext {
		origin: Some(Location::new(1, Parachain(1000))),
		message_id: XcmHash::default(),
		topic: None,
	};
	let reason = FeeReason::Export { network: EthereumNetwork::get(), destination: Here };
	let result = XcmExportFeeToSibling::<
		u128,
		u64,
		TokenLocation,
		EthereumNetwork,
		MockAssetTransactor,
		MockOkOutboundQueue,
	>::handle_fee(fee, Some(&ctx), reason);
	let local_fee = Asset::from((Location::parent(), MockOkOutboundQueue::local_fee())).into();
	// assert only local fee left
	assert_eq!(result, local_fee)
}

#[test]
fn handle_fee_success_but_not_for_ethereum() {
	let fee: Assets = Asset::from((Location::parent(), 10_u128)).into();
	let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
	// invalid network not for ethereum
	let reason = FeeReason::Export { network: Kusama, destination: Here };
	let result = XcmExportFeeToSibling::<
		u128,
		u64,
		TokenLocation,
		EthereumNetwork,
		MockAssetTransactor,
		MockOkOutboundQueue,
	>::handle_fee(fee.clone(), Some(&ctx), reason);
	// assert fee not touched and just forward to the next handler
	assert_eq!(result, fee)
}

#[test]
fn handle_fee_success_even_from_an_invalid_none_origin_location() {
	let fee: Assets = Asset::from((Location::parent(), 10_u128)).into();
	// invalid origin None here not from a sibling chain
	let ctx = XcmContext { origin: None, message_id: XcmHash::default(), topic: None };
	let reason = FeeReason::Export { network: EthereumNetwork::get(), destination: Here };
	let result = XcmExportFeeToSibling::<
		u128,
		u64,
		TokenLocation,
		EthereumNetwork,
		MockAssetTransactor,
		MockOkOutboundQueue,
	>::handle_fee(fee.clone(), Some(&ctx), reason);
	assert_eq!(result, fee)
}

#[test]
fn handle_fee_success_even_when_fee_insufficient() {
	// insufficient fee not cover the (local_fee + remote_fee) required
	let fee: Assets = Asset::from((Location::parent(), 1_u128)).into();
	let ctx = XcmContext {
		origin: Some(Location::new(1, Parachain(1000))),
		message_id: XcmHash::default(),
		topic: None,
	};
	let reason = FeeReason::Export { network: EthereumNetwork::get(), destination: Here };
	let result = XcmExportFeeToSibling::<
		u128,
		u64,
		TokenLocation,
		EthereumNetwork,
		MockAssetTransactor,
		MockOkOutboundQueue,
	>::handle_fee(fee.clone(), Some(&ctx), reason);
	assert_eq!(result, fee)
}
