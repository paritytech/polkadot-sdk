// SPDX-License-Identifier: MIT
pragma solidity ^0.8.26;

/*

## Introduction

TODO

## Use-Cases

* Allow EVM accounts (users) to stake, and have a reasonably good experience, on-par with native accounts. As per my understanding, EVM accounts cannot yet directly transact with pallets, and ergo will only rely on this.
* Allow EVM contracts to experiment building custom pooled-staking (ala Lido).
* Allow existing parachains to (possibly) re-implement their protocol in AH (Bifrost vDOT, Acala LDOT, moonbeam stDOT).
* (experimental) A validator can put their funds into a contract that interfaces with the following. This contract is fully owned by the validator and they can do anything, except it restricts them from certain arbitrary commission changes. Require the EVM account to be able to transact with other pallets (e.g. validator needs to set session keys, claim rewards etc), and a good template contract for it.

## Implementation Notes

### Upgradability and Splitting
Precompiles are not upgradable in any meaningful way, yet we can later add more features to each, as an extension to that particular interface. This is also a motivation for the interfaces to be broken apart as much as possible, rather than one massive one, beyond a better exercise of separation of concerns. For example, I opted to not include the "blocking" mechanism that validator have access to in the interface (tentative, open to suggestions), and if need be it can be added later in a new precompile (`StakingRolesV2`) that extends the current oen (`StakingRoles`).

### Bond Limits

TODO

## Risks / Stability

TODO

## Resources

* https://research.lido.fi/t/updated-lido-kusama-polkadot-ls-by-mixbytes/877 / https://research.lido.fi/t/sunsetting-of-lido-on-polkadot-and-kusama/4067
* https://github.com/moonbeam-foundation/moonbeam/blob/0e600693f70b67c59ed6a9688deb91fa5339cd5a/precompiles/parachain-staking/StakingInterface.sol

*/

/// @title Staking Interface
/// @notice Allows users to bond, unbond, and manage their bonded amounts in the staking system. For doing more interesting things, such as nomination, see `StakingRoles`.
/// @dev Talks to the directly `pallet-staking-async`.
/// @author kianenigma
interface Staking {
	struct Stake {
		uint128 active; // The amount of tokens actively bonded.
		uint128 total; // The total tokens bonded, active + unbonding.
	}

	struct Unbonding {
		uint32 era; // The era when the unbonding will be available.
		uint128 amount; // The amount of tokens that are unbonding.
	}

	/// @notice Bonds the caller's account, increasing the bonded amount by up to `value` tokens. The caller's bonded amount after this call must be be greater than `minimum_bond()`.
	/// @param value The amount of tokens to bond.
	/// @return amount The actual amount that was additionally bonded. Returns 0 on failure.
	/// @dev At the moment, staking pallet only imposes `ED` as the minimum bond. A separate PR should parameterize this and use `ed.max(minimum_bond)` instead. This is a wrapper for `bond` and `bond_extra`.
	function bond(uint128 value) external returns (uint128 amount);

	/// @notice Unbonds up to `value` tokens from the caller's account. Note that unbonding is a two step process, and should be followed by `withdraw_unbonded()` to actually withdraw the unbonded tokens. .
	/// @dev A staker cannot request an unbond such that their leftover bond is less than `minimum_bond()`, unless if the leftover is zero. For this you can also use `full_unbond()`.
	/// @param value The amount of tokens to unbond.
	/// @return amount The actual amount that was unbonded. Returns 0 on failure.
	function unbond(uint128 value) external returns (uint128 amount);

	/// @notice Unbonds all tokens, preparing to leave the staking system with `withdraw_unbonded`.
	/// @dev A shorthand for `unbond(bond.active)`.
	/// @return amount The actual amount that was unbonded.
	function full_unbond() external returns (uint128 amount);

	/// @notice Rebonds up to `value` tokens from the caller's previously unbonded amount.
	/// @dev Essentially for "I unbonded, but I have now changed my mind" situation.
	/// @param value The amount of tokens to rebond.
	/// @return amount The actual amount that was rebonded.
	function rebond_unbonded(uint128 value) external returns (uint128 amount);

	/// @notice Withdraws any previously unbonded tokens that have passed the unbonding period.
	/// @return amount The actual amount withdrawn.
	function withdraw_unbonded() external returns (uint128 amount);

	/// @notice Sets the reward destination address for the caller.
	/// @param payee The address to receive rewards. Where and how rewards come from is not in the scope of this interface. Upon declaring a `role`, an account might receive rewards.
	/// @return bonded `true` if the caller is bonded and can set the payee, `false` otherwise.
	function set_payee(address payee) external returns (bool bonded);

	/// @notice Returns the minimum amount of tokens required to be bonded.
	/// @dev This is the minimum amount needed to be a `CHILLED` staker. Implementation: `ed.max(minimum_bond)`.
	/// @return minimumBond The minimum bond amount.
	function minimum_bond() external view returns (uint128 minimumBond);

	/// @notice Returns the current bonded stake of the caller
	/// @dev Implemented exactly as `StakingLedger`.
	/// @return stake The `Stake`d amount of the caller.
	function stake_of() external view returns (Stake memory stake);

	/// @notice Returns the total amount that the caller can further stake, considering balance and existence requirements.
	/// @dev Ensures enough balance is left in the account for existence requirements, but does not leave anything for gas fee payment.
	/// @return stakeable The maximum amount the caller can stake.
	function stake_able() external view returns (uint128 stakeable);

	/// @notice Returns the payee account for the contract address.
	/// @return payee The address that will receive rewards.
	function payee() external view returns (address payee);

	/// @notice Returns the current `era` number of the staking system.
	/// @dev The era number is the unit of time in the staking system. It does not have a 100% fixed duration, but it aims to be at a parameterized value. For example, eras in Polkadot eras are aiming to be 24 hours. Implementation: `active_era.index`
	/// @return era The current era number.
	function era() external view returns (uint32);

	/// @notice Returns the unbonding queue of the caller.
	/// @dev a list of `(era, amount)`. Once any of the said eras are reached (check via `era`), the amount is withdraw-able via `withdraw_unbonded()`.
	/// @return unbondingQueue An array of Unbonding structs containing the era and amount scheduled to be unbonded in that (probably future) era.
	function unbonding_queue() external view returns (Unbonding[] memory unbondingQueue);
}

/// @title StakingRoles Interface
/// @notice Allows setting roles in the Staking system for validators and nominators.
/// @author kianenigma
interface StakingRoles {
  /// @title StakingRoles Interface
  /// @notice Used for setting the roles in the staking system.
  enum Roles {
	/// @notice Staked, but not doing anything.
	CHILLED,
	/// @notice Staked and validating.
	VALIDATOR,
	/// @notice Staked and nominating.
	NOMINATOR
  }

  /// @notice Declare the sender's intention to be a nominator, selecting `targets` as their preference.
  /// @dev Effects can be checked with `role` and `nominations`.
  /// @param targets An array of addresses to nominate.
  /// @return bonded True if the caller is bonded and therefore can nominate, false otherwise.
  function nominate(address[] calldata targets) external returns (bool bonded);

  /// @notice Declare the sender's intention to be a validator, selecting `commission` as their preference.
  /// @dev Effects can be checked with `role` and `commission`.
  /// @param commission The commission rate for the validator.
  /// @return bonded Returns true if the caller is bonded and therefore can validate, false otherwise.
  function validate(uint32 commission) external returns (bool bonded);

  /// @notice Nullifies the effect of `validate` or `nominate`.
  /// @dev Has to be called before `unbond` if the intention is to fully leave the staking system.
  /// @return bonded True if the caller is bonded and therefore can chill, false otherwise.
  function chill() external returns (bool bonded);

  /// @notice the maximum number of nomination targets that a staker can declare upon `bond`.
  /// @dev the upper bound of the size of the `targets` array in `nominate`. It might be dynamic based on the runtime, or even the caller's staked balance.
  function max_nominations() external view returns (uint32 maxNominations);

  /// @notice Returns the role of the caller.
  /// @return role The role of the caller from the `Roles` enum.
  function role() external view returns (Roles role);

  /// @notice Returns the addresses that the caller is nominating.
  /// @return nominations An array of addresses.
  function nominations() external view returns (address[] memory nominations);

  /// @notice Returns the commission rate of the caller, if they are a validator.
  /// @return commission The validator commission.
  function commission() external view returns (uint32 commission);

  /// @notice Returns the minimum bond required for validator
  /// @return uint128 The amount of minimum stake required
  function minimum_validator_bond() external view returns (uint128);

  /// @notice Returns the minimum bond required for validator
  /// @return uint128 The amount of minimum stake required
  function minimum_nominator_bond() external view returns (uint128);

  /// @notice Return the last known value for the smallest nominator who managed to be actively receiving rewards.
  /// @dev If `0`, it means we don't have a guess. Otherwise, this value should be treated as a best effort estimate of the minimum "reasonable" amount that a nominator should have staked. Bonding a value less than this will likely cause the nominator to not be eligible for receiving rewards. In such cases, one should use `PoolStaking` instead. Please refer to `NPoS` section in the Polkadot wiki for more info, notably: <https://wiki.polkadot.network/learn/learn-nominator/#minimum-active-nomination-to-receive-staking-rewards>
  /// @return uint128 the estimate minimum staked value for a nominator to earn rewards.
  function minimum_active_nominator_bond() external view returns (uint128);
}

/// @title StakingRewards Interface
/// @notice not needed, just for demonstration purposes. Potentially for claiming staking rewards. Reading rewards is an expensive operation best done off-chain.
/// @dev Claiming rewards is permissionless and can be done by any account. The payout pays the reward of (validator, era, page-of-nominators), not just a single nominator.
interface StakingRewards {
  /// @notice Payout rewards, is an expensive operation because of the large storage query, and thus it is recommended that this function is not used.
  /// @return rewards The amount of rewards paid out
  function payout() external returns (uint128 rewards);

  /// @notice Get the number of the queue of stakers rewards are pending from, as this has an expensive storage query, it is recommended that is not use.
  function pending() external view returns (uint128);
}
