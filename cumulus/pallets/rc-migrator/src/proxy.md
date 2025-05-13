## Pallet Proxy

Information on the migration of the `Proxy` pallet from Polkadot Relay Chain to Polkadot Asset Hub.

## User Impact

- 🚨 Proxy delegations are **migrated** to the Asset Hub and **deleted** from the Relay Chain.
- 🚨 Proxy announcements are **not migrated** to the Asset Hub and **deleted** from the Relay Chain.
- The delays of proxies are now always measured in Relay Chain blocks. This means that the delay of a proxy on Asset Hub will be translated to a Relay Chain block duration.
- Existing proxies on Asset Hub will have more permissions and will be able to access the new pallets as well. For example, the `NonTransfer` proxy will also be able to use nomination pools. This may affect security assumptions of previously created proxies. Users are advised to review the new proxy permissions.
- Pure proxies are treated like any other proxy. In order to access them on the Relay Chain, you need to use the AHM account recovery mechanism (todo) or remote proxy pallet (todo). There should be no use in accessing them on the Relay Chain though, since nearly all balances are transferred to Asset Hub.

## Proxy Delegations

The [Proxies](https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/proxy/src/lib.rs#L564-L579) storage maps a delegator to its delegatees. It is migrated one-to-one by mapping the `ProxyType` and `Delay` fields.

### Translation Of The Permission

The different permissions that are available to a proxy are a [runtime injected type](https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/proxy/src/lib.rs#L119-L125). Since these are different for each runtime, we need a converter that maps the Relay to AH `ProxyType` as close as possible to keep the original intention. The Relay kind is defined [here](https://github.com/polkadot-fellows/runtimes/blob/dde99603d7dbd6b8bf541d57eb30d9c07a4fce32/relay/polkadot/src/lib.rs#L1000-L1010) and the AH version [here](https://github.com/polkadot-fellows/runtimes/blob/fd8d0c23d83a7b512e721b1fde2ba3737a3478d5/system-parachains/asset-hubs/asset-hub-polkadot/src/lib.rs#L453-L468). This is done by injecting a `RcToProxyType` converter into the Asset Hub migration pallet.

The idea is to keep the **intention** of the proxy permission. This means that a `NonTransfer` proxy can still do anything that is not a direct transfer to a user account. This implies that some proxies on Asset Hub will receive new permissions without any further user action.

The permissions with their indices and how they will be migrated, are:

| Index | Relay Chain                | Asset Hub    | Index Available | Migration         |
| ----- | -------------------------- | ------------ | --------------- | --------------- |
| 0     | Any                        | Any          | ✅         | As-is   |
| 1     | NonTransfer                | NonTransfer  | ✅         | Intention kept   |
| 2     | Governance                 | CancelProxy  | ❌         | Translate index |
| 3     | Staking                    | Assets       | ❌         | Translate index |
| 4     | -                          | AssetOwner   | ✅         | As-is   |
| 5     | -                          | AssetManager | ✅         | As-is   |
| 6     | CancelProxy                | Collator     | ❌         | Translate index |
| 7     | Auction                    | TBD          | ✅         | As-is   |
| 8     | NominationPools            | TBD          | ✅         | As-is   |
| 9     | NominationParaRegistration | TBD          | ✅         | As-is   |

### Translation of the Delay

The [delay of a ProxyDefinition](https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/proxy/src/lib.rs#L77) is currently measured in Relay Chain blocks. This will change and be measured in Asset Hub blocks after the migration. The delays are translated by dividing them by two.

## Announcements

The [Announcements](https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/proxy/src/lib.rs#L581-L592) storage maps delegator AccountIDs to [Accouncement](https://github.com/paritytech/polkadot-sdk/blob/7c5224cb01710d0c14c87bf3463cc79e49b3e7b5/substrate/frame/proxy/src/lib.rs#L80-L89). Since an announcement contains a call hash, we cannot translate them for the same reason as with the Multisigs; the preimage of the hash would be either undecodable, decode to something else (security issue) or accidentally decode to the same thing.
