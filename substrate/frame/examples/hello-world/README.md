# Hello World Pallet

A simple Substrate pallet demonstrating basic concepts for workshops and learning.

## Overview

This pallet provides a basic "Hello World" example that showcases fundamental Substrate concepts:

- **Storage**: Simple value storage with `StorageValue`
- **Events**: Emitting events when actions occur
- **Calls**: Public and privileged dispatchable functions
- **Errors**: Custom error handling
- **Genesis**: Initial state configuration
- **Hooks**: Block lifecycle hooks

## Features

### Storage
- `Greeting`: A simple storage value that holds the current greeting message

### Calls
- `say_hello()`: A public call that anyone can execute
- `set_greeting()`: A privileged call that only root can execute

### Events
- `HelloSaid`: Emitted when someone calls `say_hello()`
- `GreetingUpdated`: Emitted when the greeting is updated

### Errors
- `GreetingTooShort`: When trying to set an empty greeting
- `GreetingTooLong`: When trying to set a greeting that exceeds the maximum length

## Usage

### Setting up the pallet in your runtime

1. Add the pallet to your runtime's `Cargo.toml`:

```toml
[dependencies]
pallet-example-hello-world = { path = "../frame/examples/hello-world" }
```

2. Configure the pallet in your runtime:

```rust
impl pallet_hello_world::Config for Runtime {
    type MaxGreetingLength = ConstU32<100>;
    type WeightInfo = pallet_hello_world::weights::SubstrateWeight<Runtime>;
}
```

3. Add the pallet to your `construct_runtime!` macro:

```rust
construct_runtime!(
    pub enum Runtime where
        Block = Block,
        NodeBlock = opaque::Block,
        UncheckedExtrinsic = UncheckedExtrinsic
    {
        // ... other pallets
        HelloWorld: pallet_hello_world,
    }
);
```

4. Configure genesis state:

```rust
impl pallet_hello_world::GenesisConfig for Runtime {
    fn default() -> Self {
        Self {
            greeting: b"Hello, World!".to_vec().try_into().unwrap(),
        }
    }
}
```

### Interacting with the pallet

#### Say Hello
```bash
# Call the say_hello function
substrate-node --dev --tmp
# Then use the Polkadot.js Apps or CLI to call HelloWorld.say_hello()
```

#### Set Greeting (Root only)
```bash
# Set a new greeting (requires root access)
# Use Polkadot.js Apps or CLI to call HelloWorld.set_greeting() with root origin
```

#### Query Greeting
```bash
# Query the current greeting
# Use Polkadot.js Apps or CLI to query HelloWorld.greeting()
```

## Workshop Exercises

### Exercise 1: Add a Counter
Add a storage item to track how many times `say_hello()` has been called.

### Exercise 2: Add User Greetings
Create a storage map that allows each user to set their own personal greeting.

### Exercise 3: Add Time-based Features
Add functionality that only allows `say_hello()` to be called once per block per user.

### Exercise 4: Add Fees
Modify the pallet to charge a small fee when calling `say_hello()`.

### Exercise 5: Add Validation
Add more validation to the `set_greeting()` function (e.g., no profanity, minimum length).

## Testing

Run the tests with:

```bash
cargo test -p pallet-example-hello-world
```

## Benchmarking

Generate weights with:

```bash
cargo run --release --bin substrate-node benchmark pallet \
    --pallet pallet-example-hello-world \
    --extrinsic '*' \
    --steps 50 \
    --repeat 20 \
    --output ./substrate/frame/examples/hello-world/src/weights.rs \
    --template ./.maintain/frame-weight-template.hbs
```

## License

MIT-0


