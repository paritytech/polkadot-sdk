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
			target: "xcm::transactor::erc20::withdraw",
			?what, ?who,
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
		tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?gas_consumed, "Gas consumed by withdraw_asset");
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = GasLimit::get().saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?surplus, "GasLimit - gas_consumed");
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::withdraw", ?return_value, "Return value by withdraw_asset");
			let has_reverted = return_value.flags.contains(ReturnFlags::REVERT);
			if has_reverted {
				tracing::error!(target: "xcm::transactor::erc20::withdraw", "ERC20 contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success =
					bool::abi_decode(&return_value.data, true).expect("Failed to ABI decode");
				if is_success {
					Ok((what.clone().into(), surplus))
				} else {
					tracing::error!(target: "xcm::transactor::erc20::withdraw", "ERC20 contract transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::error!(target: "xcm::transactor::erc20::withdraw", ?result, "An error occured in ERC20Transactor::withdraw_asset");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}

	fn deposit_asset_with_surplus(
		what: &Asset,
		who: &Location,
		_context: Option<&XcmContext>,
	) -> Result<Weight, XcmError> {
		tracing::trace!(
			target: "xcm::transactor::erc20::deposit",
			?what, ?who,
		);
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
		tracing::trace!(target: "xcm::transactor::erc20::deposit", ?gas_consumed, "Gas consumed");
		// We need to return this surplus for the executor to allow refunding it.
		let surplus = GasLimit::get().saturating_sub(gas_consumed);
		tracing::trace!(target: "xcm::transactor::erc20::deposit", ?surplus, "GasLimit - gas_consumed");
		if let Ok(return_value) = result {
			tracing::trace!(target: "xcm::transactor::erc20::deposit", ?return_value, "Return value");
			let has_reverted = return_value.flags.contains(ReturnFlags::REVERT);
			if has_reverted {
				tracing::error!(target: "xcm::transactor::erc20::deposit", "Contract reverted");
				Err(XcmError::FailedToTransactAsset("ERC20 contract reverted"))
			} else {
				let is_success =
					bool::abi_decode(&return_value.data, false).expect("Failed to ABI decode");
				if is_success {
					Ok(surplus)
				} else {
					tracing::error!(target: "xcm::transactor::erc20::deposit", "Transfer failed");
					Err(XcmError::FailedToTransactAsset("ERC20 contract transfer failed"))
				}
			}
		} else {
			tracing::error!(target: "xcm::transactor::erc20::deposit", ?result, "An error occured");
			// This error could've been duplicate smart contract, out of gas, etc.
			// If the issue is gas, there's nothing the user can change in the XCM
			// that will make this work since there's a hardcoded gas limit.
			Err(XcmError::FailedToTransactAsset("ERC20 contract execution errored"))
		}
	}
}
