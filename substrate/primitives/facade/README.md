# Polkadot Facade

Facade APIs are Runtime APIs which are expected to have a stable interface and be implemented across different runtimes. They will be made use of by higher level Facade libraries which interact with multiple runtimes and aggregate information provided by these Facade Runtime APIs.

- The [`apis`](./apis/README.md) folder here provides the Facade APIs.
- The [`apis-macro`](./apis-macro/README.md) folder provides a macro for defining these.
