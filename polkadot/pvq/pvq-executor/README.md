# PVQ Executor

Executes PVQ programs on top of PolkaVM.

This crate provides [`pvq_executor::PvqExecutor`], a small wrapper around `polkavm` that instantiates a PolkaVM program
blob, passes argument bytes through the module's auxiliary data region, and calls the guest entrypoint `"pvq"`.

## API

- `PvqExecutor`: Executes a program with optional gas metering.
- `PvqExecutorContext`: Registers host functions and provides user data passed to host calls.
- `PvqExecutorError`: Error type returned by the executor.

## Usage

### Execute a program

```rust
use pvq_executor::{PvqExecutor, PvqExecutorContext};
use polkavm::{Config, Linker};

struct Ctx {
    data: (),
}

impl PvqExecutorContext for Ctx {
    type UserData = ();
    type UserError = core::convert::Infallible;

    fn register_host_functions(&mut self, _linker: &mut Linker<Self::UserData, Self::UserError>) {}
    fn data(&mut self) -> &mut Self::UserData {
        &mut self.data
    }
}

let mut executor = PvqExecutor::new(Config::default(), Ctx { data: () });
let program = std::fs::read("program.polkavm")?;
let args = b"\x01\x02\x03";

let (result, gas_remaining) = executor.execute(&program, args, None);
println!("result={result:?} gas={gas_remaining:?}");
# Ok::<(), std::io::Error>(())
```

### Gas metering

Pass `Some(gas_limit)` to enable PolkaVM gas metering:

```rust
# use pvq_executor::{PvqExecutor, PvqExecutorContext};
# use polkavm::{Config, Linker};
# struct Ctx { data: () }
# impl PvqExecutorContext for Ctx {
#     type UserData = ();
#     type UserError = core::convert::Infallible;
#     fn register_host_functions(&mut self, _linker: &mut Linker<Self::UserData, Self::UserError>) {}
#     fn data(&mut self) -> &mut Self::UserData { &mut self.data }
# }
# let mut executor = PvqExecutor::new(Config::default(), Ctx { data: () });
# let program = vec![];
# let args = vec![];
let (result, gas_remaining) = executor.execute(&program, &args, Some(1_000_000));
# let _ = (result, gas_remaining);
```

## Related crates

- `../pvq-extension/`: A higher-level extension system built on top of this executor.
- `../pvq-program/`: Guest-side program framework and macros.
- `../pvq-primitives/`: Shared types (including the `PvqError` that the executor can convert into).

## Development

```bash
cargo test -p pvq-executor
```