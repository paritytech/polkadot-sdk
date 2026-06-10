# Substrate zombienet-sdk tests

These tests run the zombienet scenarios for the Substrate using the `zombienet-sdk`.

## Running locally

The test suite expects the Substrate integration image to be available. When
running locally you can either set `ZOMBIENET_INTEGRATION_TEST_IMAGE` to point
to an existing container image, or rely on the default
`docker.io/paritypr/substrate:latest`.

To execute the tests with the native provider:

```
ZOMBIE_PROVIDER=native cargo test --release -p substrate-zombienet-sdk-tests --features zombie-ci -- --nocapture
```

