window.BENCHMARK_DATA = {
  "lastUpdate": 1735960737292,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "email": "41779041+alvicsam@users.noreply.github.com",
            "name": "Alexander Samusev",
            "username": "alvicsam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "721f6d97613b0ece9c8414e8ec8ba31d2f67d40c",
          "message": "[WIP] Fix networking-benchmarks (#7036)\n\ncc https://github.com/paritytech/ci_cd/issues/1094",
          "timestamp": "2025-01-03T13:19:18Z",
          "tree_id": "bec3589885e7e27d15b93c25e9283008397e0049",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/721f6d97613b0ece9c8414e8ec8ba31d2f67d40c"
        },
        "date": 1735914628128,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17243400,
            "range": "± 496233",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 17484119,
            "range": "± 365774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 18688949,
            "range": "± 313839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 22455693,
            "range": "± 428129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5322994,
            "range": "± 170673",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 31013751,
            "range": "± 1114317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 218501431,
            "range": "± 13972729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 1946804769,
            "range": "± 96309764",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yangqiwei97@gmail.com",
            "name": "Qiwei Yang",
            "username": "qiweiii"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b4f131b000e01f1aca3f023937a36dcc281d5e2",
          "message": "Replace duplicated whitelist with whitelisted_storage_keys (#7024)\n\nrelated issue: #7018\n\nreplaced duplicated whitelists with\n`AllPalletsWithSystem::whitelisted_storage_keys();` in this PR\n\n---------\n\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-03T22:22:12Z",
          "tree_id": "152f965d219452ec180463a016d757b195bd79ff",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0b4f131b000e01f1aca3f023937a36dcc281d5e2"
        },
        "date": 1735947378032,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17771421,
            "range": "± 379395",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 17796252,
            "range": "± 375294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19192305,
            "range": "± 550825",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23424069,
            "range": "± 618808",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 6000664,
            "range": "± 322927",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 35457468,
            "range": "± 1762375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 238664091,
            "range": "± 5449223",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2169025360,
            "range": "± 170481694",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "gui.thiolliere@gmail.com",
            "name": "Guillaume Thiolliere",
            "username": "gui1117"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b5a5ac4487890046d226bedb0238eaccb423ae42",
          "message": "Make `TransactionExtension` tuple of tuple transparent for implication (#7028)\n\nCurrently `(A, B, C)` and `((A, B), C)` change the order of implications\nin the transaction extension pipeline. This order is not accessible in\nthe metadata, because the metadata is just a vector of transaction\nextension, the nested structure is not visible.\n\nThis PR make the implementation for tuple of `TransactionExtension`\nbetter for tuple of tuple. `(A, B, C)` and `((A, B), C)` don't change\nthe implication for the validation A.\n\nThis is a breaking change but only when using the trait\n`TransactionExtension` the code implementing the trait is not breaking\n(surprising rust behavior but fine).\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-04T02:03:30Z",
          "tree_id": "7ceca99999c8a5065cee89a611c7b74b87f097cd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b5a5ac4487890046d226bedb0238eaccb423ae42"
        },
        "date": 1735960720711,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64B",
            "value": 17906473,
            "range": "± 402710",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/512B",
            "value": 18028249,
            "range": "± 376977",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/4KB",
            "value": 19607414,
            "range": "± 347759",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/small_payload/libp2p/serially/64KB",
            "value": 23842723,
            "range": "± 674082",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/256KB",
            "value": 5669355,
            "range": "± 326213",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/2MB",
            "value": 33259681,
            "range": "± 1448902",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/16MB",
            "value": 254199472,
            "range": "± 17716451",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_benchmark/large_payload/libp2p/serially/128MB",
            "value": 2206601933,
            "range": "± 183595326",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}