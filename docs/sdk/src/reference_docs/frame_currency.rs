//! FRAME Currency Abstractions and Traits
//!
//! TODO:
//!
//! - [x] History, `Currency` trait.
//! - [ ] `Hold` and `Freeze` with diagram.
//! - [x] `HoldReason` and `FreezeReason`
//! - [ ] This footgun: https://github.com/paritytech/polkadot-sdk/pull/1900#discussion_r1363783609
//!
//!
//!
//! Currency related traits in FRAME provide standardized interfaces for implementing various
//! currency functionalities. These traits enable developers to create, transfer, and manage
//! different forms of digital assets within the blockchain environment, ensuring that the economic
//! aspects of the chain are robust and adaptable to different use cases.
//!
//! ## The `Currency` Trait
//!
//! The [`Currency`](../../../frame_support/traits/tokens/currency/index.html) trait was initially
//! introduced in Substrate to manage the native token balances. This trait was later deprecated in
//! favor of the [`fungible`](../../../frame_support/traits/tokens/fungible/index.html) and
//! [`fungibles`](../../../frame_support/traits/tokens/fungibles/index.html) traits in Substrate's
//! PR [#12951](https://github.com/paritytech/substrate/pull/12951). This shift is part of a broader
//! initiative to enhance token management capabilities within the framework. This deprecation is
//! aimed at providing improved safety and more flexibility for managing assets, beyond the
//! capabilities of the original
//! [`Currency`](../../../frame_support/traits/tokens/currency/index.html) trait. This transition
//! enables more diverse economic models in Substrate. For more details, you can view the discussion
//! on the [Tokens Horizon issue](https://github.com/paritytech/polkadot-sdk/issues/327). The
//! [`Currency`](../../../frame_support/traits/tokens/currency/index.html) trait is still available
//! in Substrate, but it is recommended to use the **fungible** traits instead. The
//! [deprecation PR](https://github.com/paritytech/substrate/pull/12951) has a dedicated section on
//! upgrading from **Currency** to **fungible**. Besides, this [issue](https://github.com/paritytech/polkadot-sdk/issues/226)
//! lists the pallets that have been upgraded to the **fungible** traits, and the ones that are
//! still using the [`Currency`](../../../frame_support/traits/tokens/currency/index.html) trait.
//! There one could find the relevant code examples for upgrading.
//!
//! ## Fungible and Fungibles Traits
//!
//! The [`fungible`](../../../frame_support/traits/tokens/fungible/index.html) trait is designed for
//! managing single currency types, providing a streamlined approach for single-asset operations.
//! The [`fungibles`](../../../frame_support/traits/tokens/fungibles/index.html) trait, in contrast,
//! allows for handling multiple types of currencies or assets, offering greater flexibility in
//! multi-asset environments.
//!
//! #### Fungible Trait:
//!
//! This trait includes key methods for asset management, like transfer, balance check, and minting.
//! It's particularly useful in scenarios involving a single currency type, simplifying the
//! implementation and management process.
//!
//! #### Fungibles Trait:
//!
//! Offers a comprehensive solution for managing multiple asset types within a single system.
//! It's more complex than the
//! [`fungible`](../../../frame_support/traits/tokens/fungible/index.html) trait, suited for
//! environments where diverse asset types coexist and interact. This trait is essential in
//! multi-currency contexts, providing the necessary tools for intricate asset management.
//!
//! #### Holds and Freezes
//!
//! *Holds* are explicitly designed to be infallibly slashed. They do not contribute to the ED but
//! do require a provider reference, removing any possibility of account reference counting from
//! being problematic for a slash. They are also always named, ensuring different holds do not
//! accidentally slash each other's balances. E.g. some balance is held when it is put to staking,
//! it does not contribute to the ED, but it is slashed when the staker misbehaves.
//!
//! *Freezes* can overlap with *Holds*. Since *Holds* are designed to be infallibly slashed, this
//! means that any logic using a *Freeze* must handle the possibility of the frozen amount being
//! reduced, potentially to zero. A permissionless function should be provided in order to allow
//! bookkeeping to be updated in this instance. E.g. some balance is frozen when it is used for
//! voting, one could use held balance for voting, but nothing prevents this frozen balance from
//! being reduced if the overlapping hold is slashed.
//!
//! Both *Holds* and *Freezes* require an identifier, `HoldReason` and `FreezeReason` respectively,
//! which is configurable and is expected to be an enum aggregated across all pallet instances of
//! the runtime.
//!
//! To understand this with an example
//!
//! ## Common Pallets
//!
//! #### Pallet Balances
//! The [`pallet_balances`](../../../pallet_balances/index.html) is a key component in FRAME. It
//! is designed for managing native token balances. It plays a crucial role in tracking and
//! manipulating the balance of accounts, providing functionalities essential for a wide range of
//! financial operations. The key functions of
//! [`pallet_balances`](../../../pallet_balances/index.html) include transferring tokens between
//! accounts, checking account balances, and adjusting balances for staking or fees. This pallet
//! integrates with the [`fungible`](../../../frame_support/traits/tokens/fungible/index.html)
//! trait, aligning with the standardized approach for asset management in Substrate.
//!
//! #### Pallet Assets
//! The [`pallet_assets`](../../../pallet_assets/index.html) is designed to manage
//! multiple asset types within a blockchain. It provides functionalities for asset creation,
//! management, and destruction. This pallet allows for the handling of a diverse range of assets,
//! facilitating complex economic models and token systems. Its interaction with the
//! [`fungibles`](../../../frame_support/traits/tokens/fungibles/index.html) trait enables seamless
//! integration with multi-asset systems.
