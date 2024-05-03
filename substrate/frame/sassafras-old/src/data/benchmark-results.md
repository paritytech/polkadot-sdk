# Benchmarks High Level Results

- **Ring size**: the actual number of validators for an epoch
- **Domain size**: a value which bounds the max size of the ring (max_ring_size = domain_size - 256)

## Verify Submitted Tickets (extrinsic)

`x` = Number of tickets

### Domain=1024, Uncompressed (~ 13 ms + 11·x ms)

    Time ~=    13400
        + x    11390
                  µs

### Domain=1024, Compressed (~ 13 ms + 11·x ms)

    Time ~=    13120
        + x    11370
                  µs

### Domain=2048, Uncompressed (~ 26 ms + 11·x ms)

    Time ~=    26210
        + x    11440
                  µs

### Domain=2048, Compressed (~ 26 ms + 11·x ms)

    Time ~=    26250
        + x    11460
                  µs

### Conclusions

- Verification doesn't depend on ring size as verification key is already constructed.
- The call is fast as far as the max number of tickets which can be submitted in one shot
  is appropriately bounded.
- Currently, the bound is set equal epoch length, which iirc for Polkadot is 3600.
  In this case if all the tickets are submitted in one shot timing is expected to be
  ~39 seconds, which is not acceptable. TODO: find a sensible bound

---

## Recompute Ring Verifier Key (on epoch change)

`x` = Ring size

### Domain=1024, Uncompressed (~ 50 ms)

    Time ~=    54070
        + x    98.53
                  µs

### Domain=1024, Compressed (~ 700 ms)

    Time ~=   733700
        + x    90.49
                  µs

### Domain=2048, Uncompressed (~ 100 ms)

    Time ~=    107700
        + x    108.5
                  µs

### Domain=2048, Compressed (~ 1.5 s)

    Time ~=   1462400
        + x    65.14
                  µs

### Conclusions

- Here we load the full ring context data to recompute verification key for the epoch
- Ring size influence is marginal (e.g. for 1500 validators → ~98 ms to be added to the base time)
- This step is performed at most once per epoch (if validator set changes).
- Domain size for ring context influence the PoV size (see next paragraph)
- Decompression heavily influence timings (1.5sec vs 100ms for same domain size)

---

## Ring Context Data Size

### Domain=1024, Uncompressed

    295412 bytes = ~ 300 KiB

### Domain=1024, Compressed

    147716 bytes = ~ 150 KiB

### Domain=2048, Uncompressed

    590324 bytes = ~ 590 KiB

### Domain=2048, Compressed

    295172 bytes = ~ 300 KiB
