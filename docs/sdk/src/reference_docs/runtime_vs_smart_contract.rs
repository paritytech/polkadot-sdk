//! # Runtime vs. Smart Contracts
//!
//! This is a comparative analysis of Substrate-based Runtimes and Smart Contracts, highlighting
//! their main differences. Our aim is to equip you with a clear understanding of how these two
//! methods of deploying on-chain logic diverge in their design, usage, and implications.
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
//!
//! ## Weighing and Metering
//! In the context of Substrate, resource management is a critical aspect of ensuring network
//! stability and efficiency. This leads to the fundamental difference in how resources are handled
//! in FRAME-based runtimes versus smart contracts, specifically focusing on why runtime operations
//! are weighed and smart contract executions must be metered.
//!
//! ### Weighing
//! In FRAME-based runtimes, operations are "weighed". This means that each operation in the runtime
//! has a fixed cost, known in advance, determined through benchmarking. Weighing is practical here
//! because:
//!
//! - *Predictability*: Runtime operations are part of the blockchain's core logic, which is static
//!   until an upgrade occurs. This predictability allows for precise benchmarking.
//! - *Controlled Environment*: The blockchain's governance mechanisms control runtime upgrades,
//!   ensuring that any new operations or changes to existing ones are vetted for their resource
//!   usage.
//! - *Prevention of Abuse*: By having a fixed cost, it becomes infeasible for an attacker to create
//!   transactions that could unpredictably consume excessive resources.
//!
//! ### Metering
//! For smart contracts, particularly in the case of pallet_contracts and pallet_evm, resource
//! consumption is metered. Metering is essential due to:
//!
//! - *Dynamic Nature*: Unlike runtime operations, smart contracts can be deployed by any user, and
//!   their behavior isn’t known in advance. Metering dynamically measures resource consumption as
//!   the contract executes.
//! - *Safety Against Infinite Loops*: Metering protects the blockchain from poorly designed
//!   contracts that might run into infinite loops, consuming an indefinite amount of resources.
//! - *Fair Cost Allocation*: Metering ensures users deploying and interacting with smart contracts
//!   pay for the exact amount of resources their contract consumes.
//!
//! ### Implications for Developers and Users
//! - *For Runtime Developers*: Understanding the cost of each operation is essential. Misjudging
//!   the weight of operations can lead to network congestion or vulnerability exploitation.
//! - *For Smart Contract Developers*: Being mindful of the gas cost associated with contract
//!   execution is crucial. Efficiently written contracts save costs and are less likely to hit gas
//!   limits, ensuring smoother execution on the blockchain.
//!
//! Conclusion
//! The contrasting approaches to resource management - weighing in runtimes and metering in smart
//! contracts - highlight a key difference in the underlying architecture and philosophy between
//! these two methods of on-chain logic implementation in Substrate. It underscores the importance
//! of efficient and secure code, whether you're working on runtime development or smart contract
//! creation.
