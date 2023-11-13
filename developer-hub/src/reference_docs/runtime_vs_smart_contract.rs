//! # Runtime vs. Smart Contracts
//!
//! Notes:
//!
//! Why one can be weighed, and one MUST be metered.
//! https://forum.polkadot.network/t/where-contracts-fail-and-runtimes-chains-are-needed/4464/3
//!
//!
//! Here you will find a comparative analysis of Substrate-based Runtimes and Smart Contracts,
//! highlighting their main differences. The aim is to equip you with a clear understanding of how
//! these two methods of deploying on-chain logic diverge in their design, usage, and implications.
//!
//! Both Runtimes and Smart Contracts serve distinct purposes. Runtimes offer deep customization for
//! blockchain development, while Smart Contracts provide a more accessible approach for
//! decentralized applications. Understanding their differences is crucial in choosing the right
//! approach for your specific solution.
//!
//! ## Substrate
//! Substrate is a modular framework that enables the creation of purpose-specific blockchains. In
//! the Polkadot ecosystem, leveraging Substrate, you can find two distinct approaches for on-chain
//! code execution: [Runtime Development](#runtime-in-substrate) and [Smart
//! Contracts](#smart-contracts).
//!
//! ## Smart Contracts
//! Smart Contracts are autonomous, programmable constructs deployed on the blockchain.
//! In Substrate, Smart Contracts capabilities are realized through the [`pallet_contracts`] for
//! WASM-based contracts and [`pallet_evm`] for EVM-compatible contracts. This functionality enables
//! developers to build a wide range of decentralized applications and systems on top of the
//! Substrate blockchain, leveraging its inherent security and decentralized nature.
//!
//! ## Runtime in Substrate
//! A FRAME-based runtime is the state transition function of a Substrate blockchain. It defines
//! the rules for processing transactions and blocks, essentially governing the behavior and
//! capabilities of a blockchain.
//!
//! ## Comparative Table
//!
//! | Aspect                | Runtime                                                                 | Smart Contracts                                                      |
//! |-----------------------|-------------------------------------------------------------------------|----------------------------------------------------------------------|
//! | **Design Philosophy** | Core logic of a blockchain, allowing broad and deep customization.      | Designed for decentralized applications within the blockchain.       |
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures. | Easier to develop with knowledge of smart contract languages like Solidity. |
//! | **Upgradeability and Flexibility** | Offers seamless upgradeability, allowing entire blockchain logic modifications without hard forks. | Less flexible in upgrading but offers more straightforward deployment and iteration. |
//! | **Performance and Efficiency** | More efficient, optimized for specific needs of the blockchain.        | Can be less efficient due to the overhead of a virtual machine.     |
//! | **Security Considerations** | Security flaws can affect the entire blockchain.                        | Security risks usually localized to the individual contract.        |
//! | **Use Cases and Applicability** | Ideal for creating new blockchains or fundamentally altering blockchain functionality. | Best suited for decentralized applications on an existing blockchain infrastructure. |
