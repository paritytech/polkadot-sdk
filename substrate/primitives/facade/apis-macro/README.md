# Facade Runtime APIs macro

This crate provides a proc macro called `define_facade_apis!` which defines a set of runtime APIs that can be implemented, as well as providing metadata about them. It: 
- Enforces certain conventions to help prevent breaking changes to the APIs that have been defined, and enforce naming.
- Generates metadata about the facade APIs which can be used to check runtimes for compatibility and such.

The macro is not expected to be used outside of the `sp-facade-apis` crate.