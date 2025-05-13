# Pallet Treasury

The Treasury is migrated along with all existing, active, and inactive proposals, spends and their
ids. Remote spends that were previously intended for execution on Asset Hub are now mapped to be
executed locally on Asset Hub. The Treasury on Asset Hub will use Relay Chain blocks as its internal
clock, so all previously established timeouts and periods will remain unchanged.

## User Impact

### Treasury Account

Before migration, the Treasury used two accounts: one (1) on the Relay Chain, derived from the
`PalletId` type and the `py/trsry` byte sequence, and another (2) on Asset Hub, derived from the
Treasury XCM location on the Relay Chain, as seen from Asset Hub (e.g., for Polkadot:
`Location{ parent: 1, X1(PalletInstance(19)) }`). To keep only one Treasury account on Asset Hub,
all assets from account (2) are moved to an account on Asset Hub with the same account id as (1),
and this account will be used for all future spends.

(1) - for Polkadot (13UVJyLnbVp9RBZYFwFGyDvVd1y27Tt8tkntv6Q7JVPhFsTB)
(2) - for Polkadot (14xmwinmCEz6oRrFdczHKqHgWNMiCysE2KrA4jXXAAM1Eogk)

### Spend call API

The `Beneficiary` parameter of the spend call has changed from `xcm::Location` to two dimensional
type:
``` rust
struct LocatableBeneficiary {
    // Deposit location.
    location: xcm::Location,
    // Sovereign account of the given location at deposit `location`.
    account: xcm::Location,
  }
```

On Asset Hub, we currently support only local spends, meaning the first argument will always be `xcm::Location(0, Here)`.
For more details on the reasoning and application of this API, please refer to this [document](https://github.com/paritytech/polkadot-sdk/issues/4715).

The spend call example:

``` rust
// USDT local spend
let _ = Treasury.spend(
    asset_kind: LocatableAssetId {
        // withdrawal location current chain (Asset Hub)
        location: xcm::Location { parents: 0, interior: Here },
        // USDT ID
        asset_id: xcm::Location { parents: 0, interior: X2(PalletInstance(50), GeneralIndex(1984)))
    },
    amount: 10_000_000,
    beneficiary: LocatableAssetId {
        // deposit location current chain (Asset Hub)
        location: xcm::Location { parents: 0, interior: Here },
        // some account id
        account: xcm::Location { parents: 0, interior: X1(AccountId32(0xABC...)))
    },
    valid_from, None,
);
// DOT local spend
let _ = Treasury.spend(
    asset_kind: LocatableAssetId {
        // withdrawal location current chain (Asset Hub)
        location: xcm::Location { parents: 0, interior: Here },
        // DOT
        asset_id: xcm::Location { parents: 1, interior: Here },
    },
    amount: 10_000_000_000,
    beneficiary: LocatableAssetId {
        // deposit location current chain (Asset Hub)
        location: xcm::Location { parents: 0, interior: Here },
        // some account id
        account: xcm::Location { parents: 0, interior: X1(AccountId32(0xABC...)))
    },
    valid_from, None,
);
```
