# Changelog

All notable changes and migrations to pallet-staking will be documented in this file.

The format is loosely based
on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/). We maintain a
single integer version number for staking pallet to keep track of all storage
migrations.

## [v15]

### Added

- New trait `DisablingStrategy` which is responsible for making a decision which offenders should be
  disabled on new offence.
- Default implementation of `DisablingStrategy` - `UpToLimitDisablingStrategy`. It
  disables each new offender up to a threshold (1/3 by default). Offenders are not runtime disabled for
  offences in previous era(s). But they will be low-priority node-side disabled for dispute initiation.
- `OffendingValidators` storage item is replaced with `DisabledValidators`. The former keeps all
  offenders and if they are disabled or not. The latter just keeps a list of all offenders as they
  are disabled by default.

### Deprecated

- `enum DisableStrategy` is no longer needed because disabling is not related to the type of the
  offence anymore. A decision if a offender is disabled or not is made by a `DisablingStrategy`
  implementation.

## [v14]

### Added

- New item `ErasStakersPaged` that keeps up to `MaxExposurePageSize`
  individual nominator exposures by era, validator and page.
- New item `ErasStakersOverview` complementary to `ErasStakersPaged` which keeps
  state of own and total stake of the validator across pages.
- New item `ClaimedRewards` to support paged rewards payout.

### Deprecated

- `ErasStakers` and `ErasStakersClipped` is deprecated, will not be used any longer for the exposures of the new era
  post v14 and can be removed after 84 eras once all the exposures are stale.
- Field `claimed_rewards` in item `Ledger` is renamed
  to `legacy_claimed_rewards` and can be removed after 84 eras.

[v14]: https://github.com/paritytech/substrate/pull/13498
