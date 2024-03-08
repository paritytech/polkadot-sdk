//! # FRAME Currency Abstractions and Traits
//!
//! Currency related traits in FRAME provide standardized interfaces for implementing various
//! currency functionalities. The fundamental reason to have these traits is to allow pallets to
//! utilize tokens without relying on the implementation detail.
//! These traits enable developers to create, transfer, and manage fungible assets within the
//! blockchain environment, ensuring that the economic aspects of the chain are robust and adaptable
//! to different use cases.

//!
//! ## The Currency Trait
//!
//! The [`Currency`][4] trait was initially introduced in Substrate to manage the native token
//! balances. This trait was later deprecated in favor of the [`fungible`][3] traits in Substrate's
//! PR [#12951](https://github.com/paritytech/substrate/pull/12951), the main reason for this is
//! that the deprecated trait is only sensible in a system that has just one currency type. This
//! shift is part of a broader initiative to enhance token management capabilities within the
//! framework. This deprecation is aimed at providing improved safety and more flexibility for
//! managing assets, beyond the capabilities of the original [`Currency`][4] trait. This transition
//! enables more diverse economic models in Substrate. For more details, you can view the discussion
//! on the [Tokens Horizon issue](https://github.com/paritytech/polkadot-sdk/issues/327). The
//! [`Currency`][4] trait is still available in Substrate, but it is recommended to use the
//! **fungible** traits instead. The [deprecation PR](https://github.com/paritytech/substrate/pull/12951)
//! has a dedicated section on upgrading from **Currency** to **fungible**. Besides, this
//! [issue](https://github.com/paritytech/polkadot-sdk/issues/226) lists the pallets that have been
//! upgraded to the **fungible** traits, and the ones that are still using the [`Currency`][4]
//! trait. There one could find the relevant code examples for upgrading.
//!
//! ## Fungible Traits
//!
//! The [`fungible`][3] traits are designed for managing currency types, providing a streamlined
//! approach for single-asset operations. Fungible is currently preferred over currency as the
//! latter is deprecated.
//!
//! ### Holds and Freezes
//!
//! Learn more about this two concepts in
//! [`frame_support::traits::tokens::fungible::hold`][1] and
//! [`frame_support::traits::tokens::fungible::freeze`][2].
//!
//! ## Balances Pallet
//! The [`pallet_balances`](../../../pallet_balances/index.html) is a key component in FRAME. It
//! is designed for managing native token balances. It plays a crucial role in tracking and
//! manipulating the balance of accounts, providing functionalities essential for a wide range of
//! financial operations. The key functions of
//! [`pallet_balances`](../../../pallet_balances/index.html) include transferring tokens between
//! accounts, checking account balances, and adjusting balances for staking or fees. This pallet
//! implements the [`fungible`][3] traits, aligning with the standardized approach for asset
//! management in Substrate.
//!
//! [1]: ../../../frame_support/traits/tokens/fungible/hold/index.html
//! [2]: ../../../frame_support/traits/tokens/fungible/freeze/index.html
//! [3]: ../../../frame_support/traits/tokens/fungible/index.html
//! [4]: ../../../frame_support/traits/tokens/currency/index.html
