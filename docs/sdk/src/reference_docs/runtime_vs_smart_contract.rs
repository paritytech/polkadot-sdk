//! # Runtime vs. Smart Contracts
//!
//! *TL;DR*: If you need to create a *Blockchain*, then write a runtime. If you need to create a
//! *DApp*, then write a Smart Contract.
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
//! #### Smart Contracts in Substrate
//! Smart Contracts are autonomous, programmable constructs deployed on the blockchain.
//! In [FRAME](frame), Smart Contracts infrastructure is implemented by the
//! [`pallet_contracts`](../../../pallet_contracts/index.html) for WASM-based contracts or the
//! [`pallet_evm`](../../../pallet_evm/index.html) for EVM-compatible contracts. These pallets
//! enable Smart Contract developers to build applications and systems on top of a Substrate-based
//! blockchain.
//!
//! #### Runtime in Substrate
//! The Runtime is the state transition function of a Substrate-based blockchain. It defines the
//! rules for processing transactions and blocks, essentially governing the behavior and
//! capabilities of a blockchain.
//!
//! ## Comparative Table
//!
//! | Aspect                | Runtime
//! | Smart Contracts                                                      |
//! |-----------------------|-------------------------------------------------------------------------|----------------------------------------------------------------------|
//! | **Design Philosophy** | Core logic of a blockchain, allowing broad and deep customization.
//! | Designed for DApps deployed on the blockchain runtime.| | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures.         | Easier to develop with knowledge of Smart Contract languages like Solidity or [ink!](https://use.ink/). |
//! | **Upgradeability and Flexibility** | Offers comprehensive upgradeability with migration logic
//! and on-chain governance, allowing modifications to the entire blockchain logic without hard
//! forks. | Less flexible in upgrade migrations but offers more straightforward deployment and
//! iteration. | | **Performance and Efficiency** | More efficient, optimized for specific needs of
//! the blockchain.        | Can be less efficient due to its generic nature (e.g. the overhead of a
//! virtual machine).     | | **Security Considerations** | Security flaws can affect the entire
//! blockchain.                        | Security risks usually localized to the individual
//! contract.        | | **Weighing and Metering** | Operations can be weighed, allowing for precise
//! benchmarking.           | Execution is metered, allowing for measurement of resource
//! consumption. |
//!
//! We will now explore these differences in more detail.
//!
//! ## Design Philosophy
//! Runtimes and Smart Contracts are designed for different purposes. Runtimes are the core logic
//! of a blockchain, while Smart Contracts are designed for DApps on top of the blockchain.
//! Runtimes can be more complex, but also more flexible and efficient, while Smart Contracts are
//! easier to develop and deploy.
//!
//! #### Runtime Design Philosophy
//! - **Core Blockchain Logic**: Runtimes are essentially the backbone of a blockchain. They define
//!   the fundamental rules, operations, and state transitions of the blockchain network.
//! - **Broad and Deep Customization**: Runtimes allow for extensive customization and flexibility.
//!   Developers can tailor the most fundamental aspects of the blockchain, like introducing an
//!   efficient transaction fee model to eliminating transaction fees completely. This level of
//!   control is essential for creating specialized or application-specific blockchains.
//!
//! #### Smart Contract Design Philosophy
//! - **DApps Development**: Smart contracts are designed primarily for developing DApps. They
//!   operate on top of the blockchain's infrastructure.
//! - **Modularity and Isolation**: Smart contracts offer a more modular approach. Each contract is
//!   an isolated piece of code, executing predefined operations when triggered. This isolation
//!   simplifies development and enhances security, as flaws in one contract do not directly
//!   compromise the entire network.
//!
//! ## Development Complexity
//! Runtimes and Smart Contracts differ in their development complexity, largely due to their
//! differing purposes and technical requirements.
//!
//! #### Runtime Development Complexity
//! - **In-depth Knowledge Requirements**: Developing a Runtime in Substrate requires a
//!   comprehensive understanding of Rust, Substrate's framework, and blockchain principles.
//! - **Complex Blockchain Architectures**: Runtime development is suitable for creating complex
//!   blockchain architectures. Developers must consider aspects like security, scalability, and
//!   network efficiency.
//!
//! #### Smart Contract Development Complexity
//! - **Accessibility**: Smart Contract development is generally more accessible, especially for
//!   those already familiar with programming concepts. Knowledge of smart contract-specific
//!   languages like Solidity or ink! is required.
//! - **Focused on Application Logic**: The development here is focused on the application logic
//!   only. This includes writing functions that execute when certain conditions are met, managing
//!   state within the contract, and ensuring security against common Smart Contract
//!   vulnerabilities.
//!
//! ## Upgradeability and Flexibility
//! Runtimes and Smart Contracts differ significantly in how they handle upgrades and flexibility,
//! each with its own advantages and constraints. Runtimes are more flexible, allowing for writing
//! migration logic for upgrades, while Smart Contracts are less flexible but offer easier
//! deployment and iteration.
//!
//! #### Runtime Upgradeability and Flexibility
//! - **Migration Logic**: One of the key strengths of runtime development is the ability to define
//!   migration logic. This allows developers to implement changes in the state or structure of the
//!   blockchain during an upgrade. Such migrations can adapt the existing state to fit new
//!   requirements or features seamlessly.
//! - **On-Chain Governance**: Upgrades in a Runtime environment are typically governed on-chain,
//!   involving validators or a governance mechanism. This allows for a democratic and transparent
//!   process for making substantial changes to the blockchain.
//! - **Broad Impact of Changes**: Changes made in Runtime affect the entire blockchain. This gives
//!   developers the power to introduce significant improvements or changes but also necessitates a
//!   high level of responsibility and scrutiny, we will talk further about it in the [Security
//!   Considerations](#security-considerations) section.
//!
//! #### Smart Contract Upgradeability and Flexibility
//! - **Deployment and Iteration**: Smart Contracts, by nature, are designed for more
//!   straightforward deployment and iteration. Developers can quickly deploy contracts.
//! - **Contract Code Updates**: Once deployed, although typically immutable, Smart Contracts can be
//!   upgraded, but lack of migration logic. The [pallet_contracts](../../../pallet_contracts/index.html)
//!   allows for contracts to be upgraded by exposing the `set_code` dispatchable. More details on this
//!   can be found in [Ink! documentation on upgradeable contracts](https://use.ink/basics/upgradeable-contracts).
//! - **Isolated Impact**: Upgrades or changes to a smart contract generally impact only that
//!   contract and its users, unlike Runtime upgrades that have a network-wide effect.
//! - **Simplicity and Rapid Development**: The development cycle for Smart Contracts is usually
//!   faster and less complex than Runtime development, allowing for rapid prototyping and
//!   deployment.
//!
//! ## Performance and Efficiency
//! Runtimes and Smart Contracts have distinct characteristics in terms of performance and
//! efficiency due to their inherent design and operational contexts. Runtimes are more efficient
//! and optimized for specific needs, while Smart Contracts are more generic and less efficient.
//!
//! #### Runtime Performance and Efficiency
//! - **Optimized for Specific Needs**: Runtime modules in Substrate are tailored to meet the
//!   specific needs of the blockchain. They are integrated directly into the blockchain's core,
//!   allowing them to operate with high efficiency and minimal overhead.
//! - **Direct Access to Blockchain State**: Runtime has direct access to the blockchain's state.
//!   This direct access enables more efficient data processing and transaction handling, as there
//!   is no additional layer between the runtime logic and the blockchain's core.
//! - **Resource Management**: Resource management is integral to runtime development to ensure that
//!   the blockchain operates smoothly and efficiently.
//!
//! #### Smart Contract Performance and Efficiency
//! - **Generic Nature and Overhead**: Smart Contracts, particularly those running in virtual
//!   machine environments, can be less efficient due to the generic nature of their execution
//!   environment. The overhead of the virtual machine can lead to increased computational and
//!   resource costs.
//! - **Isolation and Security Constraints**: Smart Contracts operate in an isolated environment to
//!   ensure security and prevent unwanted interactions with the blockchain's state. This isolation,
//!   while crucial for security, can introduce additional computational overhead.
//! - **Gas Mechanism and Metering**: The gas mechanism in Smart Contracts, used for metering
//!   computational resources, ensures that contracts don't consume excessive resources. However,
//!   this metering itself requires computational power, adding to the overall cost of contract
//!   execution.
//!
//! ## Security Considerations
//! These two methodologies, while serving different purposes, come with their own unique security
//! considerations.
//!
//! #### Runtime Security Aspects
//! Runtimes, being at the core of blockchain functionality, have profound implications for the
//! security of the entire network:
//!
//! - **Broad Impact**: Security flaws in the runtime can compromise the entire blockchain,
//!   affecting all network participants.
//! - **Governance and Upgradeability**: Runtime upgrades, while powerful, need rigorous governance
//!   and testing to ensure security. Improperly executed upgrades can introduce vulnerabilities or
//!   disrupt network operations.
//! - **Complexity and Expertise**: Developing and maintaining runtime requires a higher level of
//!   expertise in blockchain architecture and security, as mistakes can be far-reaching.
//!
//! #### Smart Contract Security Aspects
//! Smart contracts, while more isolated, bring their own set of security challenges:
//!
//! - **Isolated Impact**: Security issues in a smart contract typically affect the contract itself
//! and its users, rather than the whole network.
//! - **Contract-specific Risks**: Common issues like reentrancy
//! attacks, improper handling of external calls, and gas limit vulnerabilities are specific to
//! smart contract development.
//! - **Permissionless Deployment**: Since anyone can deploy a smart contract,
//! the ecosystem is more open to potentially malicious or vulnerable code.
//!
//! ## Weighing and Metering
//! Weighing and metering are mechanisms designed to limit the resources used by external actors.
//! However, there are fundamental differences in how these resources are handled in FRAME-based
//! Runtimes and how they are handled in Smart Contracts, while Runtime operations are weighed,
//! Smart Contract executions must be metered.
//!
//! #### Weighing
//! In FRAME-based Runtimes, operations are *weighed*. This means that each operation in the Runtime
//! has a fixed upper cost, known in advance, determined through
//! [benchmarking](crate::reference_docs::frame_benchmarking_weight). Weighing is practical here
//! because:
//!
//! - *Predictability*: Runtime operations are part of the blockchain's core logic, which is static
//!   until an upgrade occurs. This predictability allows for precise
//!   [benchmarking](crate::reference_docs::frame_benchmarking_weight).
//! - *Prevention of Abuse*: By having a fixed upper cost that corresponds to the worst-case
//!   complexity scenario of its execution (and a mechanism to refund unused weight), it becomes
//!   infeasible for an attacker to create transactions that could unpredictably consume excessive
//!   resources.
//!
//! #### Metering
//! For Smart Contracts resource consumption is metered. This is essential due to:
//!
//! - **Untrusted Nature**: Unlike Runtime operations, Smart Contracts can be deployed by any user,
//!   and their behavior isn’t known in advance. Metering dynamically measures resource consumption
//!   as the contract executes.
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!   contracts that might run into infinite loops, consuming an indefinite amount of resources.
//!
//! #### Implications for Developers and Users
//! - **For Runtime Developers**: Understanding the cost of each operation is essential. Misjudging
//!   the weight of operations can lead to network congestion or vulnerability exploitation.
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract
//!   execution is crucial. Efficiently written contracts save costs and are less likely to hit gas
//!   limits, ensuring smoother execution on the blockchain.
