# Arkworks EC-Utils

Arkworks-compatible elliptic curve types with host-accelerated operations for
Substrate runtimes.

This crate provides elliptic curve types that are API-compatible with
[Arkworks](https://github.com/arkworks-rs), enabling easy migration from
upstream Arkworks types. The implementation leverages
[arkworks-extensions](https://github.com/paritytech/arkworks-extensions) to
redirect computationally expensive operations (pairings, MSMs, point
multiplications) to native host function implementations via Substrate's host
call interface.

The crate includes both:
- **Runtime-side**: Arkworks-compatible type definitions that can be used as
  drop-in replacements for upstream Arkworks types in runtime code. These types
  call into arkworks-extensions hooks which are implemented to redirect
  expensive operations to the host.
- **Host-side**: Host function implementations that call into the original
  Arkworks library to execute expensive operations natively on the host.

Refer to [ark-substrate-examples](https://github.com/davxy/ark-substrate-examples)
for benchmarks and usage examples.
