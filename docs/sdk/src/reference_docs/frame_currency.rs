//! FRAME Currency Abstractions and Traits
//!
//! Notes:
//!
//! - History, `Currency` trait.
//! - `Hold` and `Freeze` with diagram.
//! - `HoldReason` and `FreezeReason`
//! 	- This footgun: https://github.com/paritytech/polkadot-sdk/pull/1900#discussion_r1363783609
//!
//!
//!
//! This document provides an in-depth guide for Substrate developers on the implementation and
//! functionalities of currency management in FRAME-based runtimes. It focuses on the transition
//! from the `Currency` trait to the `fungible` traits, and the utilization of
//! pallet_balances and pallet_assets.
//!
//! 1. Overview of FRAME-based Runtimes
//!   - Brief introduction to FRAME
//!   - Importance of currency management in blockchain development
//!   - Overview of currency traits in Substrate
//! 2. **The `Currency` Trait**
//!
//!    The `Currency` trait was initially introduced in Substrate to manage the native token
//!    balances. This trait was later deprecated in favor of the `fungible` traits in Substrate's PR
//!    [#12951](https://github.com/paritytech/substrate/pull/12951). This shift is part of a broader
//!    initiative to enhance token management capabilities within the framework. This deprecation is
//!    aimed at providing improved safety and more flexibility for managing assets, beyond the
//!    capabilities of the original `Currency` trait. This transition enables more diverse economic
//!    models in Substrate. For more details, you can view the discussion on the
//!    [Tokens Horizon issue](https://github.com/paritytech/polkadot-sdk/issues/327).
//!    The `Currency` trait is still available in Substrate, but it is recommended to use the
//!    `fungible` traits instead. The [deprecation PR](https://github.com/paritytech/substrate/pull/12951)
//!    has a dedicated section on upgrading from `Currency` to `fungible`. Besides, this [issue](https://github.com/paritytech/polkadot-sdk/issues/226)
//!    lists the pallets that have been upgraded to the `fungible` traits, and the ones that are
//!    still using the `Currency` trait. There one could find the relevant code examples for
//!    upgrading.
//!
//! 3. The fungible and fungibles Traits
//!   - Definition and distinction between fungible and fungibles
//!   - fungible: For managing single currency types
//!   - fungibles: For handling multiple types of currencies or assets
//! 3.1 fungible Trait
//!   - Detailed explanation of the trait
//!   - Key methods and their functionalities
//!   - Use cases and implementation examples
//! 3.2 fungibles Trait
//!   - Comprehensive overview
//!   - Comparison with the fungible trait
//!   - Implementation in multi-currency contexts
//! 4. Pallet Balances (pallet_balances)
//!   - Introduction to pallet_balances
//!   - Role in managing native token balances
//!   - Key functions and their uses
//!   - Integration with the fungible trait
//!   - Example code snippets and use cases
//! 5. Pallet Assets (pallet_assets)
//!   - Purpose and functionalities of pallet_assets
//!   - Handling multiple asset types
//!   - Interaction with fungibles trait
//!   - Configuration and customization options
//!   - Practical examples and code snippets
//! 6. Migration Strategies
//!   - Guidance for migrating from Currency to fungible and fungibles
//!   - Best practices and common pitfalls
//!   - Case studies or examples of successful migrations
//! 7. Advanced Topics
//!   - Cross-chain asset management
//!   - Interaction with other FRAME pallets
//!   - Security considerations and best practices
//! 8. Conclusion
//!   - Recap of key points
//!   - Future developments in currency management in Substrate
//!   - Additional resources and community support
//!   - References
//!   - Official Substrate documentation
//!   - Relevant GitHub repositories and code examples
//!   - Community discussions and tutorials
