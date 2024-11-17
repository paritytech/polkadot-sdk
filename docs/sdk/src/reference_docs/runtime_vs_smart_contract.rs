//! # Runtime vs. Smart Contracts
//!
//!
//!
//! approach for a specific solution.
//!
//! [`Runtime Development`] and [`Smart Contracts`].
//!
//! [`pallet_contracts`] for WASM-based contracts or the
//!
//! capabilities of a blockchain.
//!
//!
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures. | Easier to develop with knowledge of Smart Contract languages like Solidity or [`ink!`]. |
//! | **Weighing and Metering** | Operations can be weighed, allowing for precise benchmarking. | Execution is metered, allowing for measurement of resource consumption. |
//!
//!
//! Runtimes can be more complex, but also more flexible and efficient, while Smart Contracts are
//!
//! - **Broad and Deep Customization**: Runtimes allow for extensive customization and flexibility.
//!
//! - **Modularity and Isolation**: Smart contracts offer a more modular approach. Each contract is
//!
//!
//! - **Complex Blockchain Architectures**: Runtime development is suitable for creating complex
//!
//!   languages like Solidity or ink! is required.
//!   vulnerabilities.
//!
//! migration logic for upgrades, while Smart Contracts are less flexible but offer easier
//!
//!   blockchain during an upgrade. Such migrations can adapt the existing state to fit new
//!   process for making substantial changes to the blockchain.
//!   Considerations](#security-considerations) section.
//!
//! - **Contract Code Updates**: Once deployed, although typically immutable, Smart Contracts can be
//! - **Isolated Impact**: Upgrades or changes to a smart contract generally impact only that
//!   deployment.
//!
//! and optimized for specific needs, while Smart Contracts are more generic and less efficient.
//!
//!   allowing them to operate with high efficiency and minimal overhead.
//! - **Resource Management**: Resource management is integral to runtime development to ensure that
//!
//!   environment. The overhead of the virtual machine can lead to increased computational and
//!   while crucial for security, can introduce additional computational overhead.
//!   execution.
//!
//!
//!
//!   and testing to ensure security. Improperly executed upgrades can introduce vulnerabilities or
//!
//!
//! attacks, improper handling of external calls, and gas limit vulnerabilities are specific to
//!
//! Runtimes and how they are handled in Smart Contracts, while Runtime operations are weighed,
//!
//! [`benchmarking`]. Weighing is practical here
//!
//! - *Prevention of Abuse*: By having a fixed upper cost that corresponds to the worst-case
//!
//!
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract







//!
//!
//!
//! approach for a specific solution.
//!
//! [`Runtime Development`] and [`Smart Contracts`].
//!
//! [`pallet_contracts`] for WASM-based contracts or the
//!
//! capabilities of a blockchain.
//!
//!
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures. | Easier to develop with knowledge of Smart Contract languages like Solidity or [`ink!`]. |
//! | **Weighing and Metering** | Operations can be weighed, allowing for precise benchmarking. | Execution is metered, allowing for measurement of resource consumption. |
//!
//!
//! Runtimes can be more complex, but also more flexible and efficient, while Smart Contracts are
//!
//! - **Broad and Deep Customization**: Runtimes allow for extensive customization and flexibility.
//!
//! - **Modularity and Isolation**: Smart contracts offer a more modular approach. Each contract is
//!
//!
//! - **Complex Blockchain Architectures**: Runtime development is suitable for creating complex
//!
//!   languages like Solidity or ink! is required.
//!   vulnerabilities.
//!
//! migration logic for upgrades, while Smart Contracts are less flexible but offer easier
//!
//!   blockchain during an upgrade. Such migrations can adapt the existing state to fit new
//!   process for making substantial changes to the blockchain.
//!   Considerations](#security-considerations) section.
//!
//! - **Contract Code Updates**: Once deployed, although typically immutable, Smart Contracts can be
//! - **Isolated Impact**: Upgrades or changes to a smart contract generally impact only that
//!   deployment.
//!
//! and optimized for specific needs, while Smart Contracts are more generic and less efficient.
//!
//!   allowing them to operate with high efficiency and minimal overhead.
//! - **Resource Management**: Resource management is integral to runtime development to ensure that
//!
//!   environment. The overhead of the virtual machine can lead to increased computational and
//!   while crucial for security, can introduce additional computational overhead.
//!   execution.
//!
//!
//!
//!   and testing to ensure security. Improperly executed upgrades can introduce vulnerabilities or
//!
//!
//! attacks, improper handling of external calls, and gas limit vulnerabilities are specific to
//!
//! Runtimes and how they are handled in Smart Contracts, while Runtime operations are weighed,
//!
//! [`benchmarking`]. Weighing is practical here
//!
//! - *Prevention of Abuse*: By having a fixed upper cost that corresponds to the worst-case
//!
//!
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract








//!
//!
//!
//! approach for a specific solution.
//!
//! [`Runtime Development`] and [`Smart Contracts`].
//!
//! [`pallet_contracts`] for WASM-based contracts or the
//!
//! capabilities of a blockchain.
//!
//!
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures. | Easier to develop with knowledge of Smart Contract languages like Solidity or [`ink!`]. |
//! | **Weighing and Metering** | Operations can be weighed, allowing for precise benchmarking. | Execution is metered, allowing for measurement of resource consumption. |
//!
//!
//! Runtimes can be more complex, but also more flexible and efficient, while Smart Contracts are
//!
//! - **Broad and Deep Customization**: Runtimes allow for extensive customization and flexibility.
//!
//! - **Modularity and Isolation**: Smart contracts offer a more modular approach. Each contract is
//!
//!
//! - **Complex Blockchain Architectures**: Runtime development is suitable for creating complex
//!
//!   languages like Solidity or ink! is required.
//!   vulnerabilities.
//!
//! migration logic for upgrades, while Smart Contracts are less flexible but offer easier
//!
//!   blockchain during an upgrade. Such migrations can adapt the existing state to fit new
//!   process for making substantial changes to the blockchain.
//!   Considerations](#security-considerations) section.
//!
//! - **Contract Code Updates**: Once deployed, although typically immutable, Smart Contracts can be
//! - **Isolated Impact**: Upgrades or changes to a smart contract generally impact only that
//!   deployment.
//!
//! and optimized for specific needs, while Smart Contracts are more generic and less efficient.
//!
//!   allowing them to operate with high efficiency and minimal overhead.
//! - **Resource Management**: Resource management is integral to runtime development to ensure that
//!
//!   environment. The overhead of the virtual machine can lead to increased computational and
//!   while crucial for security, can introduce additional computational overhead.
//!   execution.
//!
//!
//!
//!   and testing to ensure security. Improperly executed upgrades can introduce vulnerabilities or
//!
//!
//! attacks, improper handling of external calls, and gas limit vulnerabilities are specific to
//!
//! Runtimes and how they are handled in Smart Contracts, while Runtime operations are weighed,
//!
//! [`benchmarking`]. Weighing is practical here
//!
//! - *Prevention of Abuse*: By having a fixed upper cost that corresponds to the worst-case
//!
//!
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract







//!
//!
//!
//! approach for a specific solution.
//!
//! [`Runtime Development`] and [`Smart Contracts`].
//!
//! [`pallet_contracts`] for WASM-based contracts or the
//!
//! capabilities of a blockchain.
//!
//!
//! | **Development Complexity** | Requires in-depth knowledge of Rust and Substrate. Suitable for complex blockchain architectures. | Easier to develop with knowledge of Smart Contract languages like Solidity or [`ink!`]. |
//! | **Weighing and Metering** | Operations can be weighed, allowing for precise benchmarking. | Execution is metered, allowing for measurement of resource consumption. |
//!
//!
//! Runtimes can be more complex, but also more flexible and efficient, while Smart Contracts are
//!
//! - **Broad and Deep Customization**: Runtimes allow for extensive customization and flexibility.
//!
//! - **Modularity and Isolation**: Smart contracts offer a more modular approach. Each contract is
//!
//!
//! - **Complex Blockchain Architectures**: Runtime development is suitable for creating complex
//!
//!   languages like Solidity or ink! is required.
//!   vulnerabilities.
//!
//! migration logic for upgrades, while Smart Contracts are less flexible but offer easier
//!
//!   blockchain during an upgrade. Such migrations can adapt the existing state to fit new
//!   process for making substantial changes to the blockchain.
//!   Considerations](#security-considerations) section.
//!
//! - **Contract Code Updates**: Once deployed, although typically immutable, Smart Contracts can be
//! - **Isolated Impact**: Upgrades or changes to a smart contract generally impact only that
//!   deployment.
//!
//! and optimized for specific needs, while Smart Contracts are more generic and less efficient.
//!
//!   allowing them to operate with high efficiency and minimal overhead.
//! - **Resource Management**: Resource management is integral to runtime development to ensure that
//!
//!   environment. The overhead of the virtual machine can lead to increased computational and
//!   while crucial for security, can introduce additional computational overhead.
//!   execution.
//!
//!
//!
//!   and testing to ensure security. Improperly executed upgrades can introduce vulnerabilities or
//!
//!
//! attacks, improper handling of external calls, and gas limit vulnerabilities are specific to
//!
//! Runtimes and how they are handled in Smart Contracts, while Runtime operations are weighed,
//!
//! [`benchmarking`]. Weighing is practical here
//!
//! - *Prevention of Abuse*: By having a fixed upper cost that corresponds to the worst-case
//!
//!
//! - **Safety Against Infinite Loops**: Metering protects the blockchain from poorly designed
//!
//! - **For Smart Contract Developers**: Being mindful of the gas cost associated with contract












// [`pallet_evm`]: pallet_evm

// [`Runtime Development`]: #runtime-in-substrate
// [`Smart Contracts`]: #smart-contracts
// [`benchmarking`]: crate::reference_docs::frame_benchmarking_weight
// [`ink!`]: https://use.ink/
// [`pallet_contracts`]: pallet_contracts
// [`pallet_evm`]: pallet_evm
