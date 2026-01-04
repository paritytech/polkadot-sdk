# Arkworks EC-Utils

Arkworks-compatible elliptic curve types with host-accelerated operations for
Substrate runtimes.

## Overview

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

## Supported Curves

The crate supports the following elliptic curves through feature flags:

- **BLS12-381** (`bls12-381`)
- **BLS12-377** (`bls12-377`)
- **BW6-761** (`bw6-761`)
- **Ed-on-BLS12-377** (`ed-on-bls12-377`)
- **Ed-on-BLS12-381-Bandersnatch** (`ed-on-bls12-381-bandersnatch`)

## Benchmark Results

| extrinsic                                            |  arkworkrs(µs)[^1] | ark-substrate(µs)[^2] |   speedup   |
| ---------------------------------------------------- |  ----------------- | --------------------- | ----------- |
| groth16_verification (bls12_381)                     |      23335.84      |        3569.35        |     6.54    |
| bls12_381_pairing                                    |      9092.61       |        1390.80        |     6.54    |
| bls12_381_msm_g1, 10 arguments                       |      6921.99       |        949.58         |     7.29    |
| bls12_381_msm_g1, 1000 arguments                     |      194969.80     |        30158.23       |     6.46    |
| bls12_381_msm_g2, 10 arguments                       |      21513.87      |        2870.33        |     7.57    |
| bls12_381_msm_g2, 1000 arguments                     |      621769.22     |        100801.74      |     7.50    |
| bls12_381_mul_projective_g1                          |      486.34        |        75.01          |     6.48    |
| bls12_381_mul_affine_g1                              |      420.01        |        79.26          |     5.30    |
| bls12_381_mul_projective_g2                          |      1498.84       |        210.50         |     7.12    |
| bls12_381_mul_affine_g2                              |      1234.92       |        214.00         |     5.77    |
| bls12_377_pairing                                    |      8904.20       |        1449.52        |     6.14    |
| bls12_377_msm_g1, 10 arguments                       |      6592.47       |        902.50         |     7.30    |
| bls12_377_msm_g1, 1000 arguments                     |      191793.87     |        28828.95       |     6.65    |
| bls12_377_msm_g2, 10 arguments                       |      22509.51      |        3251.84        |     6.92    |
| bls12_377_msm_g2, 1000 arguments                     |      632339.00     |        94521.78       |     6.69    |
| bls12_377_mul_projective_g1                          |      424.21        |        65.68          |     6.46    |
| bls12_377_mul_affine_g1                              |      363.85        |        65.68          |     5.54    |
| bls12_377_mul_projective_g2                          |      1339.39       |        212.20         |     6.31    |
| bls12_377_mul_affine_g2                              |      1122.08       |        208.74         |     5.38    |
| bw6_761_pairing                                      |      52065.18      |        6791.27        |     7.67    |
| bw6_761_msm_g1, 10 arguments                         |      47050.21      |        5559.53        |     8.46    |
| bw6_761_msm_g1, 1000 arguments                       |      1167536.06    |        143517.21      |     8.14    |
| bw6_761_msm_g2, 10 arguments                         |      41055.89      |        4874.46        |     8.42    |
| bw6_761_msm_g2, 1000 arguments                       |      1209593.25    |        143437.77      |     8.43    |
| bw6_761_mul_projective_g1                            |      1678.86       |        223.57         |     7.51    |
| bw6_761_mul_affine_g1                                |      1387.87       |        222.05         |     6.25    |
| bw6_761_mul_projective_g2                            |      1919.98       |        308.60         |     6.22    |
| bw6_761_mul_affine_g2                                |      1388.21       |        222.47         |     6.24    |
| ed_on_bls12_381_bandersnatch_msm_sw, 10 arguments    |      3616.81       |        557.96         |     6.48    |
| ed_on_bls12_381_bandersnatch_msm_sw, 1000 arguments  |      94473.54      |        16254.32       |     5.81    |
| ed_on_bls12_381_bandersnatch_mul_projective_sw       |      235.38        |        40.70          |     5.78    |
| ed_on_bls12_381_bandersnatch_mul_affine_sw           |      204.04        |        41.66          |     4.90    |
| ed_on_bls12_381_bandersnatch_msm_te, 10 arguments    |      5427.77       |        744.74         |     7.29    |
| ed_on_bls12_381_bandersnatch_msm_te, 1000 arguments  |      106610.20     |        16690.71       |     6.39    |
| ed_on_bls12_381_bandersnatch_mul_projective_te       |      183.29        |        34.63          |     5.29    |
| ed_on_bls12_381_bandersnatch_mul_affine_te           |      181.84        |        33.99          |     5.35    |
| ed_on_bls12_377_msm, 10 arguments                    |      5304.03       |        700.51         |     7.57    |
| ed_on_bls12_377_msm, 1000 arguments                  |      105563.53     |        15757.62       |     6.70    |
| ed_on_bls12_377_mul_projective                       |      179.54        |        32.72          |     5.49    |
| ed_on_bls12_377_mul_affine                           |      177.53        |        33.24          |     5.34    |

[^1]: Pure runtime execution of [arkworks](https://github.com/arkworks-rs/) library.
[^2]: Hostcalls hooks with [ark-extensions](https://github.com/paritytech/arkworks-extensions) library.

Refer to [ark-substrate-examples](https://github.com/davxy/ark-substrate-examples) for benchmarks code

