# PVQ Runtime API

Substrate runtime API definition for PVQ (PolkaVM Query). This crate exposes a single runtime API trait that runtimes implement to execute PVQ programs and expose PVQ-related metadata.

## API definition

The trait exactly as defined in this crate:

```rust
use alloc::vec::Vec;
use pvq_primitives::PvqResult;
use sp_api::decl_runtime_apis;

decl_runtime_apis! {
    /// The runtime API for the PVQ module.
    pub trait PvqApi {
        /// Execute a PVQ program with arguments.
        ///
        /// - `program`: PolkaVM bytecode of the guest program
        /// - `args`: SCALE-encoded call data
        /// - `gas_limit`: Optional execution gas limit; `None` means use the default time boundary
        fn execute_query(program: Vec<u8>, args: Vec<u8>, gas_limit: Option<i64>) -> PvqResult;

        /// Return PVQ extensions metadata as an opaque byte blob.
        fn metadata() -> Vec<u8>;
    }
}
```

Notes

- `PvqResult` is defined in `pvq-primitives` as `Result<PvqResponse, PvqError>` where `PvqResponse = Vec<u8>`.
- The `args` buffer must match the PVQ guest ABI (see `pvq-program`): first byte is the entrypoint index, followed by SCALE-encoded arguments.

## Minimal runtime implementation

```rust
use sp_api::impl_runtime_apis;
use pvq_runtime_api::PvqApi;
use pvq_primitives::PvqResult;

impl_runtime_apis! {
    impl PvqApi<Block> for Runtime {
        fn execute_query(program: Vec<u8>, args: Vec<u8>, gas_limit: Option<i64>) -> PvqResult {
            // Integrate with your PVQ executor here, e.g. pvq-executor
            // Ok(result_bytes) or Err(pvq_error)
            unimplemented!()
        }

        fn metadata() -> Vec<u8> {
            // Return extension metadata bytes (format decided by the runtime)
            Vec::new()
        }
    }
}
```

## Metadata

`metadata()` returns a byte vector. The encoding and schema are defined by the runtime. Recommended format is either SCALE or JSON of the structure defined in `pvq-extension` (`pvq-extension/src/metadata.rs`).

Shape

```json
{
  "types": { /* scale-info PortableRegistry */ },
  "extensions": {
    "<extension_id_as_string>": {
      "name": "<extension_name>",
      "functions": [
        {
          "name": "<fn_name>",
          "inputs": [ { "name": "<arg>", "ty": <type_id_u32> } ],
          "output": <type_id_u32>
        }
      ]
    }
  }
}
```

Notes

- `types` is a `scale-info` PortableRegistry describing all referenced types.
- `ty` and `output` are numeric type IDs that reference entries inside `types`.
- The `extensions` object is keyed by stringified extension IDs (u64 -> string) and maps to per-extension metadata: name and function signatures.
