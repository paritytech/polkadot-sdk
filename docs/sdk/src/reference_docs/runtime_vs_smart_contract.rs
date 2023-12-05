//! # Runtime vs. Smart Contracts
//!
//! This is a comparative analysis of Substrate-based Runtimes and Smart Contracts, highlighting
//! their main differences. Our aim is to equip you with a clear understanding of how these two
//! methods of deploying on-chain logic diverge in their design, usage, and implications.
//!
//! Both Runtimes and Smart Contracts serve distinct purposes. Runtimes offer deep customization for
//! blockchain development, while Smart Contracts provide a more accessible approach for
//! decentralized applications. Understanding their differences is crucial in choosing the right
//! approach for a specific solution.
//!
//! ## Substrate
//! Substrate is a modular framework that enables the creation of purpose-specific blockchains. In
//! the Polkadot ecosystem you can find two distinct approaches for on-chain code execution:
//! [Runtime Development](#runtime-in-substrate) and [Smart Contracts](#smart-contracts).
//!
//! ## Smart Contracts
//! Smart Contracts are autonomous, programmable constructs deployed on the blockchain.
//! In [FRAME](frame), Smart Contracts capabilities can be realized through the
//! [`pallet_contracts`](../../../pallet_contracts/index.html) for WASM-based contracts or the
//! [`pallet_evm`](../../../pallet_evm/index.html) for EVM-compatible contracts. These pallets
//! enable Smart Contract developers to build applications and systems on top of a Substrate-based
//! blockchain.
//!
//! ## Runtime in Substrate
//! The Runtime is the state transition function of a Substrate-based blockchain. It defines the
//! rules for processing transactions and blocks, essentially governing the behavior and
//! capabilities of a blockchain.
//!
//! ## Comparative Table
//!
//! | Aspect                | Runtime                                                                 | Smart Contracts                                                      |
//! |-----------------------|-------------------------------------------------------------------------|----------------------------------------------------------------------|
//! | **Design Philosophy** | Core logic of a blockchain, allowing broad and deep customization.      | Designed for Dapps on top of the the blockchain.|
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures.         | Easier to develop with knowledge of Smart Contract languages like Solidity or [ink!](https://use.ink/). |
//! | **Upgradeability and Flexibility** | Offers seamless upgradeability, allowing entire blockchain logic modifications without hard forks. | Less flexible in upgrading but offers more straightforward deployment and iteration. |
//! | **Performance and Efficiency** | More efficient, optimized for specific needs of the blockchain.        | Can be less efficient due to its generic nature (e.g. the overhead of a virtual machine).     |
//! | **Security Considerations** | Security flaws can affect the entire blockchain.                        | Security risks usually localized to the individual contract.        |
//!
//! ## Weighing and Metering
//! Both mechanisms designed to limit the resources used by external actors. However, there are
//! fundamental differences in how these resources are handled in FRAME-based Runtimes and how
//! they are handled in Smart Contracts, while Runtime operations are weighed, Smart Contract
//! executions must be metered.
//!
//! ### Weighing
//! In FRAME-based Runtimes, operations are *weighed*. This means that each operation in the Runtime
//! has a fixed cost, known in advance, determined through
//! [benchmarking](crate::reference_docs::frame_benchmarking_weight). Weighing is practical here
//! because:
//!
//! - *Predictability*: Runtime operations are part of the blockchain's core logic, which is static
//!   until an upgrade occurs. This predictability allows for precise
//!   [benchmarking](crate::reference_docs::frame_benchmarking_weight).
//! - *Prevention of Abuse*: By having a fixed cost (although unused weight can be refunded), it
//!   becomes infeasible for an attacker to create transactions that could unpredictably consume
//!   excessive resources.
//!
//! ### Metering
//! For Smart Contracts resource consumption is metered. This is essential due to:
//!
//! - **Dynamic Nature**: Unlike Runtime operations, Smart Contracts can be deployed by any user,
//!   and their behavior isn’t known in advance. Metering dynamically measures resource consumption
//!   as the contract executes.
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!   contracts that might run into infinite loops, consuming an indefinite amount of resources.
//!
//! ### Implications for Developers and Users
//! - **For Runtime Developers**: Understanding the cost of each operation is essential. Misjudging
//!   the weight of operations can lead to network congestion or vulnerability exploitation.
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract
//!   execution is crucial. Efficiently written contracts save costs and are less likely to hit gas
//!   limits, ensuring smoother execution on the blockchain.
