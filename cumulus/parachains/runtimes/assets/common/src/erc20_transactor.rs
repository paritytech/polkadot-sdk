use alloy_core::{
	primitives::{Address, U256 as EU256},
	sol,
	sol_types::*,
};
use core::marker::PhantomData;
use frame_support::{pallet_prelude::Zero, traits::{OriginTrait, fungible::Inspect}};
use pallet_revive::{AddressMapper, ContractResult, DepositLimit, MomentOf};
use pallet_revive_uapi::ReturnFlags;
use sp_core::{Get, H160, H256, U256};
use sp_runtime::Weight;
use xcm_executor::{AssetsInHolding, traits::{MatchesFungibles, TransactAsset, Error as MatchError, ConvertLocation}};
use xcm::latest::prelude::*;

// ERC20 interface.
sol! {
	function totalSupply() public view virtual returns (uint256);
	function balanceOf(address account) public view virtual returns (uint256);
	function transfer(address to, uint256 value) public virtual returns (bool);
}

type BalanceOf<T> = <<T as pallet_revive::Config>::Currency as Inspect<<T as frame_system::Config>::AccountId>>::Balance;

pub struct ERC20Transactor<T, Matcher, AccountIdConverter, GasLimit, AccountId>(PhantomData<(T, Matcher, AccountIdConverter, GasLimit, AccountId)>);

impl<
	AccountId: Eq + Clone,
	T: pallet_revive::Config<AccountId = AccountId>,
	AccountIdConverter: ConvertLocation<AccountId>,
	Matcher: MatchesFungibles<H160, u128>,
	GasLimit: Get<Weight>,
> TransactAsset for ERC20Transactor<T, Matcher, AccountIdConverter, GasLimit, AccountId>
where
	BalanceOf<T>: Into<U256> + TryFrom<U256>,
	MomentOf<T>: Into<U256>,
	T::Hash: frame_support::traits::IsType<H256>
{
	fn can_check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// TODO.
		Ok(())
	}

	fn check_in(_origin: &Location, _what: &Asset, _context: &XcmContext) {
		// TODO.
	}

	fn can_check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) -> XcmResult {
		// TODO.
		Ok(())
	}

	fn check_out(_destination: &Location, _what: &Asset, _context: &XcmContext) {
		// TODO.
	}

	fn withdraw_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<(AssetsInHolding, Weight), XcmError> {
		tracing::trace!(
			target: "xcm::transactor",
			?what, ?who,
			"withdraw_asset"
		);
		let checking_account_eth = T::AddressMapper::to_address(&T::CheckingAccount::get());
		let checking_address = Address::from(Into::<[u8; 20]>::into(checking_account_eth));
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let data = transferCall { to: checking_address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, gas_consumed, .. } = pallet_revive::Pallet::<T>::bare_call(
			T::RuntimeOrigin::signed(who.clone()),
			asset_id,
			BalanceOf::<T>::zero(),
			GasLimit::get(),
			DepositLimit::Unchecked,
			data,
		);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = GasLimit::get().saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20", ?gas_consumed, "Gas consumed by withdraw_asset");
		if let Ok(return_value) = result {
			let has_reverted = return_value.flags.contains(ReturnFlags::REVERT);
			if has_reverted {
				// TODO: Can actually match errors from contract call
				// to provide better errors here.
				// For now we assume it's because we hit the gas limit.
				Err(XcmError::TooExpensive)
			} else {
				let is_success =
					bool::abi_decode(&return_value.data, true).expect("Failed to ABI decode");
				if is_success {
					Ok((what.clone().into(), surplus))
				} else {
					// TODO: Can actually match errors from contract call
					// to provide better errors here.
					Err(XcmError::FailedToTransactAsset(""))
				}
			}
		} else {
			// TODO: Don't know what this case is for.
			Err(XcmError::Unimplemented)
		}
	}

	fn deposit_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, XcmError> {
		let who = AccountIdConverter::convert_location(who)
			.ok_or(MatchError::AccountIdConversionFailed)?;
		let eth_address = T::AddressMapper::to_address(&who);
		let address = Address::from(Into::<[u8; 20]>::into(eth_address));
		let (asset_id, amount) = Matcher::matches_fungibles(what)?;
		let data = transferCall { to: address, value: EU256::from(amount) }.abi_encode();
		let ContractResult { result, gas_consumed, .. } = pallet_revive::Pallet::<T>::bare_call(
			T::RuntimeOrigin::signed(T::CheckingAccount::get()),
			asset_id,
			BalanceOf::<T>::zero(),
			GasLimit::get(),
			DepositLimit::Unchecked,
			data,
		);
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = GasLimit::get().saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20", ?gas_consumed, "Gas consumed by withdraw_asset");
		if let Ok(return_value) = result {
			let has_reverted = return_value.flags.contains(ReturnFlags::REVERT);
			if has_reverted {
				// TODO: Can actually match errors from contract call
				// to provide better errors here.
				// For now we assume it's because we hit the gas limit.
				Err(XcmError::TooExpensive)
			} else {
				let is_success =
					bool::abi_decode(&return_value.data, false).expect("Failed to ABI decode");
				if is_success {
					Ok(surplus)
				} else {
					// TODO: Can actually match errors from contract call
					// to provide better errors here.
					Err(XcmError::FailedToTransactAsset(""))
				}
			}
		} else {
			Err(XcmError::Unimplemented)
		}
	}
}
