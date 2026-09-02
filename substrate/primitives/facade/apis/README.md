# Facade Runtime API Definitions

Facade APIs are Runtime APIs which are expected to have a stable interface and be implemented across different runtimes. They will be made use of by higher level Facade libraries which interact with multiple runtimes and aggregate information provided by these Facade Runtime APIs.

This crate provides the Facade APIs.
- Use the `decl-runtime-apis` feature to provide the runtime API declarations which can then be implemented in a runtime via `impl-runtime-apis`.
- Use the `metadata` feature to provide a `metadata()` function which gives information about the facade APIs.