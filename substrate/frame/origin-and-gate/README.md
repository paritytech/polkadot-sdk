# Origin "And Gate" Pallet

"And Gate" Substrate pallet that implements a mechanism for `EnsureOrigin` that requires two independent origins to approve a dispatch before it executes.

## Overview

The pallet provides a stateful mechanism for tracking proposal approvals from multiple origins across different blocks. Inspired by the multisig pallet pattern, it is adapted specifically for origin types rather than signatories.

The primary use case is to enforce that a dispatch has been approved by two different origin types (for example, requiring both governance council approval and technical committee approval).

## License

Apache 2.0
