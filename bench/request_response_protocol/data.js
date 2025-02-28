window.BENCHMARK_DATA = {
  "lastUpdate": 1740765402507,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "request_response_protocol": [
      {
        "commit": {
          "author": {
            "email": "eresav@me.com",
            "name": "Andrei Eres",
            "username": "AndreiEres"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "6bfe4523acf597ef47dfdcefd11b0eee396bc5c5",
          "message": "networking-bench: Update benchmarks payload (#7056)\n\n# Description\n\n- Used 10 notifications and requests within the benchmarks. After moving\nthe network workers' initialization out of the benchmarks, it is\nacceptable to use this small number without losing precision.\n- Removed the 128MB payload that consumed most of the execution time.",
          "timestamp": "2025-01-09T18:20:07Z",
          "tree_id": "903477b959e883dd8daeff00beda8441cd55d58d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6bfe4523acf597ef47dfdcefd11b0eee396bc5c5"
        },
        "date": 1736449911762,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17163862,
            "range": "± 170203",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17446070,
            "range": "± 164252",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18730101,
            "range": "± 98579",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22547421,
            "range": "± 293148",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50727588,
            "range": "± 643035",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 300331693,
            "range": "± 3948705",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2091391647,
            "range": "± 54342059",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14256263,
            "range": "± 109675",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14412977,
            "range": "± 77653",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14771320,
            "range": "± 98653",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18936940,
            "range": "± 137625",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49662562,
            "range": "± 413487",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 291586625,
            "range": "± 4639808",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2308878584,
            "range": "± 8781236",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "franciscoaguirreperez@gmail.com",
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "e051f3edd3d6a0699a9261c8f8985d2e8e95c276",
          "message": "Add XCM benchmarks to collectives-westend (#6820)\n\nCollectives-westend was using `FixedWeightBounds`, meaning the same\nweight per instruction. Added proper benchmarks.\n\n---------\n\nCo-authored-by: GitHub Action <action@github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-01-10T02:20:01Z",
          "tree_id": "442535aa9d038b646a8bc597f5baf489661c544a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e051f3edd3d6a0699a9261c8f8985d2e8e95c276"
        },
        "date": 1736478677328,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17513611,
            "range": "± 231894",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17747614,
            "range": "± 158066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19342746,
            "range": "± 92774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23054691,
            "range": "± 164988",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52100650,
            "range": "± 407725",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 313787855,
            "range": "± 2904933",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2337544694,
            "range": "± 119956966",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14631080,
            "range": "± 84241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14840693,
            "range": "± 117272",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15424874,
            "range": "± 209023",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19730480,
            "range": "± 136949",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50191870,
            "range": "± 441053",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 299081098,
            "range": "± 1880044",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2405092455,
            "range": "± 9514900",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "738282a2c4127f5e6a1c8d50235ba126b9f05025",
          "message": "Fix incorrected deprecated message (#7118)",
          "timestamp": "2025-01-11T10:32:50Z",
          "tree_id": "b2498b9f12218fdc6dbd1e34243d44889f118bc3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/738282a2c4127f5e6a1c8d50235ba126b9f05025"
        },
        "date": 1736594645848,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18044700,
            "range": "± 153767",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18331354,
            "range": "± 167473",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20147906,
            "range": "± 344420",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23977172,
            "range": "± 319112",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53129643,
            "range": "± 635022",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 317087858,
            "range": "± 4521386",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2413386156,
            "range": "± 31594654",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15036676,
            "range": "± 118681",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15074889,
            "range": "± 104668",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15857748,
            "range": "± 235167",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20039519,
            "range": "± 173065",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50820511,
            "range": "± 692420",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 308376455,
            "range": "± 3036490",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2460718408,
            "range": "± 6624429",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7d8e3a434ea1e760190456e8df1359aa8137e16a",
          "message": "reference-docs: Start `state` and mention well known keys (#7037)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/7033",
          "timestamp": "2025-01-13T12:32:01Z",
          "tree_id": "3dd18660d4fd66b37863cb6862e37aa0dcd4908c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7d8e3a434ea1e760190456e8df1359aa8137e16a"
        },
        "date": 1736775176661,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17639219,
            "range": "± 234844",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17906936,
            "range": "± 188639",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19398134,
            "range": "± 139774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23110768,
            "range": "± 179179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55345609,
            "range": "± 1069731",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 320737970,
            "range": "± 8971466",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2344482664,
            "range": "± 65530518",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14592616,
            "range": "± 191900",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14855276,
            "range": "± 112557",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15342068,
            "range": "± 151346",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19772082,
            "range": "± 159814",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52121899,
            "range": "± 296503",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 318478991,
            "range": "± 3338407",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2513740445,
            "range": "± 22113207",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ba572ae892d4e4fae89ca053d8a137117b0f3a17",
          "message": "[pallet-revive] Update gas encoding (#6689)\n\nUpdate the current approach to attach the `ref_time`, `pov` and\n`deposit` parameters to an Ethereum transaction.\nPreviously we will pass these 3 parameters along with the signed\npayload, and check that the fees resulting from `gas x gas_price` match\nthe actual fees paid by the user for the extrinsic.\n\nThis approach unfortunately can be attacked. A malicious actor could\nforce such a transaction to fail by injecting low values for some of\nthese extra parameters as they are not part of the signed payload.\n\nThe new approach encodes these 3 extra parameters in the lower digits of\nthe transaction gas, approximating the the log2 of the actual values to\nencode each components on 2 digits\n\n---------\n\nCo-authored-by: GitHub Action <action@github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-13T14:49:37Z",
          "tree_id": "4a9a0a7887bb994eca8d3e08360d73900bc2f4d3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba572ae892d4e4fae89ca053d8a137117b0f3a17"
        },
        "date": 1736784087882,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18018663,
            "range": "± 273071",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17634888,
            "range": "± 67745",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19368755,
            "range": "± 298109",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23258470,
            "range": "± 289207",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51975347,
            "range": "± 470598",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 291360469,
            "range": "± 2392874",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2433093474,
            "range": "± 17063120",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14358250,
            "range": "± 67345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14626759,
            "range": "± 124568",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14901564,
            "range": "± 137575",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18956808,
            "range": "± 86040",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50747715,
            "range": "± 593781",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 292926166,
            "range": "± 3960874",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2370491534,
            "range": "± 41198195",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0e0fa4782e2872ea74d8038ebedb9f6e6be53457",
          "message": "`fatxpool`: rotator cache size now depends on pool's limits (#7102)\n\n# Description\n\nThis PR modifies the hard-coded size of extrinsics cache within\n[`PoolRotator`](https://github.com/paritytech/polkadot-sdk/blob/cdf107de700388a52a17b2fb852c98420c78278e/substrate/client/transaction-pool/src/graph/rotator.rs#L36-L45)\nto be inline with pool limits.\n\nThe problem was, that due to small size (comparing to number of txs in\nsingle block) of hard coded size:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/cdf107de700388a52a17b2fb852c98420c78278e/substrate/client/transaction-pool/src/graph/rotator.rs#L34\nexcessive number of unnecessary verification were performed in\n`prune_tags`:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/cdf107de700388a52a17b2fb852c98420c78278e/substrate/client/transaction-pool/src/graph/pool.rs#L369-L370\n\nThis was resulting in quite long durations of `prune_tags` execution\ntime (which was ok for 6s, but becomes noticable for 2s blocks):\n```\nPruning at HashAndNumber { number: 83, ... }. Resubmitting transactions: 6142, reverification took: 237.818955ms    \nPruning at HashAndNumber { number: 84, ... }. Resubmitting transactions: 5985, reverification took: 222.118218ms    \nPruning at HashAndNumber { number: 85, ... }. Resubmitting transactions: 5981, reverification took: 215.546847ms\n```\n\nThe fix reduces the overhead:\n```\nPruning at HashAndNumber { number: 92, ... }. Resubmitting transactions: 6325, reverification took: 14.728354ms    \nPruning at HashAndNumber { number: 93, ... }. Resubmitting transactions: 7030, reverification took: 23.973607ms    \nPruning at HashAndNumber { number: 94, ... }. Resubmitting transactions: 4465, reverification took: 9.532472ms    \n```\n\n## Review Notes\nI decided to leave the hardocded `EXPECTED_SIZE` for the legacy\ntransaction pool. Removing verification of transactions during\nre-submission may negatively impact the behavior of the legacy\n(single-state) pool. As in long-term we probably want to deprecate old\npool, I did not invest time to assess the impact of rotator change in\nbehavior of the legacy pool.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-01-13T17:42:22Z",
          "tree_id": "206d7c3d681e324e45101018e7b758dffe9f5f15",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0e0fa4782e2872ea74d8038ebedb9f6e6be53457"
        },
        "date": 1736793223534,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17037566,
            "range": "± 91575",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17227602,
            "range": "± 113959",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18721782,
            "range": "± 91561",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22471893,
            "range": "± 186497",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50006160,
            "range": "± 1505467",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 289683491,
            "range": "± 2550640",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2228352738,
            "range": "± 112499268",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14104640,
            "range": "± 97418",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14108451,
            "range": "± 58367",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14556976,
            "range": "± 92937",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18683920,
            "range": "± 127622",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47644153,
            "range": "± 318348",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 281509357,
            "range": "± 1293849",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2244328548,
            "range": "± 11468921",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alin@parity.io",
            "name": "Alin Dima",
            "username": "alindima"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ddffa027d7b78af330a2d3d18b7dfdbd00e431f0",
          "message": "forbid v1 descriptors with UMP signals (#7127)",
          "timestamp": "2025-01-14T08:40:50Z",
          "tree_id": "b2e6007a7f07c47680adfebdb3e238ecd4482c7f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ddffa027d7b78af330a2d3d18b7dfdbd00e431f0"
        },
        "date": 1736847226606,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19462857,
            "range": "± 598925",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20225289,
            "range": "± 1177085",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21414985,
            "range": "± 516276",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26637736,
            "range": "± 1089928",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 58545195,
            "range": "± 1893684",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 356625882,
            "range": "± 4046990",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2354886616,
            "range": "± 147484564",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14799189,
            "range": "± 145343",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15280767,
            "range": "± 64215",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15823890,
            "range": "± 133161",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 24771051,
            "range": "± 853048",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 61867496,
            "range": "± 3128815",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 392129254,
            "range": "± 18397344",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2952516885,
            "range": "± 51666969",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "105c5b94f5d3bf394a3ddf1d10ab0932ce93181b",
          "message": "litep2p: Sufix litep2p to the identify agent version for visibility (#7133)\n\nThis PR adds the `(litep2p)` suffix to the agent version (user agent) of\nthe identify protocol.\n\nThe change is needed to gain visibility into network backends and\ndetermine exactly the number of validators that are running litep2p.\nUsing tools like subp2p-explorer, we can determine if the validators are\nrunning litep2p nodes.\n\nThis reflects on the identify protocol:\n\n```\ninfo=Identify {\n  protocol_version: Some(\"/substrate/1.0\"),\n  agent_version: Some(\"polkadot-parachain/v1.17.0-967989c5d94 (kusama-node-name-01) (litep2p)\")\n  ...\n}\n```\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-01-14T13:30:05Z",
          "tree_id": "ce6a5b4d320c19e7d556b2046408e9e26f92cc72",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/105c5b94f5d3bf394a3ddf1d10ab0932ce93181b"
        },
        "date": 1736865049178,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18258120,
            "range": "± 168392",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18475138,
            "range": "± 261401",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20109765,
            "range": "± 141889",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23463644,
            "range": "± 281495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52862185,
            "range": "± 447627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 311528806,
            "range": "± 9209973",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2378817914,
            "range": "± 95238790",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14930493,
            "range": "± 115190",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15193451,
            "range": "± 79032",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15605536,
            "range": "± 122620",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19487293,
            "range": "± 278402",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50674982,
            "range": "± 478004",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 301902494,
            "range": "± 3710570",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2434776357,
            "range": "± 25814766",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "023763da2043333c3524bd7f12ac6c7b2d084b39",
          "message": "[pallet-revive-eth-rpc] persist eth transaction hash (#6836)\n\nAdd an option to persist EVM transaction hash to a SQL db.\nThis should make it possible to run a full archive ETH RPC node\n(assuming the substrate node is also a full archive node)\n\nSome queries such as eth_getTransactionByHash,\neth_getBlockTransactionCountByHash, and other need to work with a\ntransaction hash indexes, which are not stored in Substrate and need to\nbe stored by the eth-rpc proxy.\n\nThe refactoring break down the Client into a `BlockInfoProvider` and\n`ReceiptProvider`\n- BlockInfoProvider does not need any persistence data, as we can fetch\nall block info from the source substrate chain\n- ReceiptProvider comes in two flavor, \n  - An in memory cache implementation - This is the one we had so far.\n- A DB implementation - This one persist rows with the block_hash, the\ntransaction_index and the transaction_hash, so that we can later fetch\nthe block and extrinsic for that receipt and reconstruct the ReceiptInfo\nobject.\n\nThis PR also adds a new binary eth-indexer, that iterate past and new\nblocks and write the receipt hashes to the DB using the new\nReceiptProvider.\n\n---------\n\nCo-authored-by: GitHub Action <action@github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-14T13:41:24Z",
          "tree_id": "94241c9c7ef55673e81804dca424fbf2ece937d7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/023763da2043333c3524bd7f12ac6c7b2d084b39"
        },
        "date": 1736866697263,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18334732,
            "range": "± 117265",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18478726,
            "range": "± 201625",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20085042,
            "range": "± 234649",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23438229,
            "range": "± 146186",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52948075,
            "range": "± 702613",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 316621583,
            "range": "± 7963195",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2525076169,
            "range": "± 103338396",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14845301,
            "range": "± 114393",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14939408,
            "range": "± 122964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15419460,
            "range": "± 157795",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19386973,
            "range": "± 224999",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50332012,
            "range": "± 248799",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 300579505,
            "range": "± 3167964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2453529695,
            "range": "± 21238059",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6878ba1f399b628cf456ad3abfe72f2553422e1f",
          "message": "Retry approval on availability failure if the check is still needed (#6807)\n\nRecovering the POV can fail in situation where the node just restart and\nthe DHT topology wasn't fully discovered yet, so the current node can't\nconnect to most of its Peers. This is bad because for gossiping the\nassignment you need to be connected to just a few peers, so because we\ncan't approve the candidate and other nodes will see this as a no show.\n\nThis becomes bad in the scenario where you've got a lot of nodes\nrestarting at the same time, so you end up having a lot of no-shows in\nthe network that are never covered, in that case it makes sense for\nnodes to actually retry approving the candidate at a later data in time\nand retry several times if the block containing the candidate wasn't\napproved.\n\n## TODO\n- [x] Add a subsystem test.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2025-01-14T14:52:49Z",
          "tree_id": "a840a94db44fe19bd889ebdf7f2861865680ee1a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6878ba1f399b628cf456ad3abfe72f2553422e1f"
        },
        "date": 1736869550482,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19779909,
            "range": "± 439717",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19894140,
            "range": "± 312859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21667174,
            "range": "± 294044",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25449483,
            "range": "± 304446",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61802876,
            "range": "± 1301090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 380588393,
            "range": "± 5535300",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2594898336,
            "range": "± 80868651",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16271569,
            "range": "± 190294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16417142,
            "range": "± 135317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16976098,
            "range": "± 138724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21446072,
            "range": "± 379223",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 55648752,
            "range": "± 750986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 341691336,
            "range": "± 2639587",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2675359995,
            "range": "± 26922907",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d38bb9533b70abb7eff4e8770177d7840899ca86",
          "message": "approval-voting: Fix sending of assignments after restart (#6973)\n\nThere is a problem on restart where nodes will not trigger their needed\nassignment if they were offline while the time of the assignment passed.\n\nThat happens because after restart we will hit this condition\nhttps://github.com/paritytech/polkadot-sdk/blob/4e805ca05067f6ed970f33f9be51483185b0cc0b/polkadot/node/core/approval-voting/src/lib.rs#L2495\nand considered will be `tick_now` which is already higher than the tick\nof our assignment.\n\nThe fix is to schedule a wakeup for untriggered assignments at restart\nand let the logic of processing an wakeup decide if it needs to trigger\nthe assignment or not.\n\nOne thing that we need to be careful here is to make sure we don't\nschedule the wake up immediately after restart because, the node would\nstill be behind with all the assignments that should have received and\nmight make it wrongfully decide it needs to trigger its assignment, so I\nadded a `RESTART_WAKEUP_DELAY: Tick = 12` which should be more than\nenough for the node to catch up.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: Andrei Eres <eresav@me.com>",
          "timestamp": "2025-01-14T17:10:27Z",
          "tree_id": "5f3c488550900117e030d5e7268c0775e4479292",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d38bb9533b70abb7eff4e8770177d7840899ca86"
        },
        "date": 1736877700755,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17175140,
            "range": "± 144091",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18112687,
            "range": "± 313904",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19316438,
            "range": "± 86244",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23070150,
            "range": "± 117672",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53363617,
            "range": "± 2016094",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 314185597,
            "range": "± 8152695",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2318622229,
            "range": "± 62064152",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14991231,
            "range": "± 229294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15276196,
            "range": "± 137495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15675463,
            "range": "± 301759",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20346687,
            "range": "± 164529",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51042604,
            "range": "± 383701",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304804620,
            "range": "± 2184821",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2420487989,
            "range": "± 11266299",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ba36b2d2293d72d087072254e6371d9089f192b7",
          "message": "CI: Only format umbrella crate during umbrella check (#7139)\n\nThe umbrella crate quick-check was always failing whenever there was\nsomething misformated in the whole codebase.\nThis leads to an error that indicates that a new crate was added, even\nwhen it was not.\n\nAfter this PR we only apply `cargo fmt` to the newly generated umbrella\ncrate `polkadot-sdk`. This results in this check being independent from\nthe fmt job which should check the entire codebase.",
          "timestamp": "2025-01-14T17:56:30Z",
          "tree_id": "4c9d54a88060cf1ed429532b9096d0521c6d6278",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba36b2d2293d72d087072254e6371d9089f192b7"
        },
        "date": 1736880397020,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18165065,
            "range": "± 190980",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18286268,
            "range": "± 210677",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20021115,
            "range": "± 392823",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23443791,
            "range": "± 150845",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53241323,
            "range": "± 410480",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 306596809,
            "range": "± 2418646",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2533432411,
            "range": "± 43407896",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14947791,
            "range": "± 96630",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15258953,
            "range": "± 136218",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15845731,
            "range": "± 164053",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19511629,
            "range": "± 138963",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51047941,
            "range": "± 1254996",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 307126744,
            "range": "± 2700022",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2426136602,
            "range": "± 16201609",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "carlosalag@protonmail.com",
            "name": "Carlo Sala",
            "username": "carlosala"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "85c244f6e6e59db23bdfcfef903fd9145f0546ad",
          "message": "xcm: convert properly assets in xcmpayment apis (#7134)\n\nPort #6459 changes to relays as well, which were probably forgotten in\nthat PR.\nThanks!\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-14T19:57:05Z",
          "tree_id": "8a81e263b00e7faaa9ef4265fa398e217a9717f4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/85c244f6e6e59db23bdfcfef903fd9145f0546ad"
        },
        "date": 1736887781488,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19326492,
            "range": "± 192239",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19568984,
            "range": "± 237771",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21620253,
            "range": "± 195700",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25280870,
            "range": "± 198142",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55589853,
            "range": "± 621510",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 362998845,
            "range": "± 8023013",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2565082540,
            "range": "± 118976847",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15942265,
            "range": "± 114508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15985412,
            "range": "± 79738",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16293529,
            "range": "± 146380",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20539328,
            "range": "± 187988",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52601713,
            "range": "± 427856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 332849201,
            "range": "± 2675278",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2550310247,
            "range": "± 11374664",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "5f391db8af50a79db83acfe37f73c7202177d71c",
          "message": "PRDOC: Document `validate: false` (#7117)",
          "timestamp": "2025-01-14T20:22:52Z",
          "tree_id": "b90cddafe7426e86d589d89aed6845397d18d474",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5f391db8af50a79db83acfe37f73c7202177d71c"
        },
        "date": 1736889236001,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18706619,
            "range": "± 165714",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19255104,
            "range": "± 345992",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20911502,
            "range": "± 164935",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25370905,
            "range": "± 475262",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63313080,
            "range": "± 1410083",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 406209658,
            "range": "± 7306113",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2504138350,
            "range": "± 106585985",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15681205,
            "range": "± 449218",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15945439,
            "range": "± 119874",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16523680,
            "range": "± 134349",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21422392,
            "range": "± 180345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 59244545,
            "range": "± 1008938",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 363103114,
            "range": "± 10768721",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2817715625,
            "range": "± 46035714",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d5539aa63edc8068eff9c4cbb78214c3a5ab66b2",
          "message": "Parachains: Use relay chain slot for velocity measurement (#6825)\n\ncloses #3967 \n\n## Changes\nWe now use relay chain slots to measure velocity on chain. Previously we\nwere storing the current parachain slot. Then in `on_state_proof` of the\n`ConsensusHook` we were checking how many blocks were athored in the\ncurrent parachain slot. This works well when the parachain slot time and\nrelay chain slot time is the same. With elastic scaling, we can have\nparachain slot times lower than that of the relay chain. In these cases\nwe want to measure velocity in relation to the relay chain. This PR\nadjusts that.\n\n\n##  Migration\nThis PR includes a migration. Storage item `SlotInfo` of pallet\n`aura-ext` is renamed to `RelaySlotInfo` to better reflect its new\ncontent. A migration has been added that just kills the old storage\nitem. `RelaySlotInfo` will be `None` initially but its value will be\nadjusted after one new relay chain slot arrives.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-14T22:47:19Z",
          "tree_id": "cbcdcd56a70e6bd67dc20a556f4fa69acba96164",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5539aa63edc8068eff9c4cbb78214c3a5ab66b2"
        },
        "date": 1736897829845,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17722201,
            "range": "± 116282",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18012556,
            "range": "± 177550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19381997,
            "range": "± 109964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22946110,
            "range": "± 181601",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51386664,
            "range": "± 435439",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 292305767,
            "range": "± 4514382",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2352164298,
            "range": "± 65310014",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14230879,
            "range": "± 107403",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14377533,
            "range": "± 96294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15098952,
            "range": "± 112444",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19361823,
            "range": "± 184034",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50307501,
            "range": "± 409590",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 297428866,
            "range": "± 1987586",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2470574263,
            "range": "± 17857659",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d660a420fbc11a90cde5aa4e43ce2027b502162",
          "message": "approval-voting: Make importing of duplicate assignment idempotent (#6971)\n\nNormally, approval-voting wouldn't receive duplicate assignments because\napproval-distribution makes sure of it, however in the situation where\nwe restart we might receive the same assignment again and since\napproval-voting already persisted it we will end up inserting it twice\nin `ApprovalEntry.tranches.assignments` because that's an array.\n\nFix this by making sure duplicate assignments are a noop if the\nvalidator already had an assignment imported at the same tranche.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: ordian <write@reusable.software>",
          "timestamp": "2025-01-15T09:13:23Z",
          "tree_id": "65fbb6d76e92ed10477d288c458b69c0ad8e281a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d660a420fbc11a90cde5aa4e43ce2027b502162"
        },
        "date": 1736935766797,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18408807,
            "range": "± 355811",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18457808,
            "range": "± 263289",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19801345,
            "range": "± 292748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24000614,
            "range": "± 260191",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53346613,
            "range": "± 493793",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 315930032,
            "range": "± 6811735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2596476385,
            "range": "± 28124692",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14977752,
            "range": "± 526668",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15249615,
            "range": "± 241734",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15739770,
            "range": "± 150608",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19654452,
            "range": "± 197491",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50821643,
            "range": "± 225568",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304634582,
            "range": "± 3193676",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2454352580,
            "range": "± 10160829",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ef064a357c97c2635f05295aac1698a91fa2f4fd",
          "message": "req-resp/litep2p: Reject inbound requests from banned peers (#7158)\n\nThis PR rejects inbound requests from banned peers (reputation is below\nthe banned threshold).\n\nThis mirrors the request-response implementation from the libp2p side.\nI won't expect this to get triggered too often, but we'll monitor this\nmetric.\n\nWhile at it, have registered a new inbound failure metric to have\nvisibility into this.\n\nDiscovered during the investigation of:\nhttps://github.com/paritytech/polkadot-sdk/issues/7076#issuecomment-2589613046\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-01-15T11:04:37Z",
          "tree_id": "9d3ca09a7c9aa59dab2d7fb614d4ab978d516a2c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ef064a357c97c2635f05295aac1698a91fa2f4fd"
        },
        "date": 1736944100208,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17611124,
            "range": "± 168550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17922403,
            "range": "± 90481",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19266958,
            "range": "± 261691",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22983394,
            "range": "± 234550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51394167,
            "range": "± 442708",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 292396407,
            "range": "± 2873177",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2323722628,
            "range": "± 75890400",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14422717,
            "range": "± 174704",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14605702,
            "range": "± 104698",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15182823,
            "range": "± 149796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18811332,
            "range": "± 188290",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49651859,
            "range": "± 261695",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 293475543,
            "range": "± 2014055",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2379715345,
            "range": "± 13962787",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alexandre.balde@parity.io",
            "name": "Alexandre R. Baldé",
            "username": "rockbmb"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "cb0d8544dc8828c7b5e7f6a5fc20ce8c6ef9bbb4",
          "message": "Remove 0 as a special case in gas/storage meters (#6890)\n\nCloses #6846 .\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-01-15T13:14:54Z",
          "tree_id": "7962b0041a87ad5b6b5a3dbb5c26e4703b291285",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cb0d8544dc8828c7b5e7f6a5fc20ce8c6ef9bbb4"
        },
        "date": 1736950854839,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18128549,
            "range": "± 117859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18179551,
            "range": "± 176411",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19698538,
            "range": "± 211171",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23526145,
            "range": "± 216141",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52949353,
            "range": "± 783449",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 309730096,
            "range": "± 6435843",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2310416654,
            "range": "± 67131835",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14746141,
            "range": "± 265145",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14624380,
            "range": "± 151870",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15303350,
            "range": "± 171911",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19004387,
            "range": "± 115996",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49388843,
            "range": "± 535637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 296602487,
            "range": "± 4244718",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2402669328,
            "range": "± 26473664",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d822e07d51dda41982291dc6582a8c4a34821e94",
          "message": "[pallet-revive] Bump asset-hub westend spec version (#7176)\n\nBump asset-hub westend spec version\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-15T13:48:38Z",
          "tree_id": "e10cbef466384a55cb70d8a06279b92e9e89f55d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d822e07d51dda41982291dc6582a8c4a34821e94"
        },
        "date": 1736952526432,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18254286,
            "range": "± 121382",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18609699,
            "range": "± 316668",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19709322,
            "range": "± 175699",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23949767,
            "range": "± 242271",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53978128,
            "range": "± 543126",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 316571212,
            "range": "± 4624985",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2421168844,
            "range": "± 58228106",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15287954,
            "range": "± 166398",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15295784,
            "range": "± 178520",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15771912,
            "range": "± 189753",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20025963,
            "range": "± 331182",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51565054,
            "range": "± 781983",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 309242359,
            "range": "± 4312550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2474618025,
            "range": "± 21670842",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ece32e38a1a37aa354d51b16c07a42c66f23976e",
          "message": "[pallet-revive] Remove debug buffer (#7163)\n\nRemove the `debug_buffer` feature\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Cyrill Leutwiler <cyrill@parity.io>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-01-15T17:37:59Z",
          "tree_id": "7d68de4fdbfafcb85dea33ba480521078b4fdd6b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ece32e38a1a37aa354d51b16c07a42c66f23976e"
        },
        "date": 1736965673094,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17939690,
            "range": "± 156078",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18148230,
            "range": "± 187454",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19709998,
            "range": "± 140061",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23489686,
            "range": "± 220391",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53548192,
            "range": "± 529866",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 298153051,
            "range": "± 3285810",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2452752745,
            "range": "± 66316983",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14627982,
            "range": "± 97753",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14801115,
            "range": "± 102672",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15265155,
            "range": "± 187311",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19238177,
            "range": "± 206352",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49708940,
            "range": "± 622625",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 298148610,
            "range": "± 3889401",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2414731202,
            "range": "± 22682292",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "5be65872188a4ac1bf76333af3958b65f2a9629e",
          "message": "[pallet-revive] Remove revive events (#7164)\n\nRemove all pallet::events except for the `ContractEmitted` event that is\nemitted by contracts\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-01-15T19:23:54Z",
          "tree_id": "b90c0c104eba03064ac578fc573c153eb3bec52e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5be65872188a4ac1bf76333af3958b65f2a9629e"
        },
        "date": 1736972027688,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18372717,
            "range": "± 210143",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18462748,
            "range": "± 117987",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19652310,
            "range": "± 121453",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23562077,
            "range": "± 162544",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53478752,
            "range": "± 1090746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 316001707,
            "range": "± 3132883",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2331506604,
            "range": "± 110256047",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14653478,
            "range": "± 173976",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14731829,
            "range": "± 125117",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15386650,
            "range": "± 139801",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19365751,
            "range": "± 213118",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49751117,
            "range": "± 398216",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 294616068,
            "range": "± 2625649",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2408988939,
            "range": "± 28171907",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "liam.aharon@hotmail.com",
            "name": "liamaharon",
            "username": "liamaharon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be2404cccd9923c41e2f16bfe655f19574f1ae0e",
          "message": "Implement `pallet-asset-rewards` (#3926)\n\nCloses #3149 \n\n## Description\n\nThis PR introduces `pallet-asset-rewards`, which allows accounts to be\nrewarded for freezing `fungible` tokens. The motivation for creating\nthis pallet is to allow incentivising LPs.\n\nSee the pallet docs for more info about the pallet.\n\n## Runtime changes\n\nThe pallet has been added to\n- `asset-hub-rococo`\n- `asset-hub-westend`\n\nThe `NativeAndAssets` `fungibles` Union did not contain `PoolAssets`, so\nit has been renamed `NativeAndNonPoolAssets`\n\nA new `fungibles` Union `NativeAndAllAssets` was created to encompass\nall assets and the native token.\n\n## TODO\n- [x] Emulation tests\n- [x] Fill in Freeze logic (blocked\nhttps://github.com/paritytech/polkadot-sdk/issues/3342) and re-run\nbenchmarks\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: muharem <ismailov.m.h@gmail.com>\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>",
          "timestamp": "2025-01-16T06:26:59Z",
          "tree_id": "aa90529d06d73e2ad5d12708830213302e23ac6a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/be2404cccd9923c41e2f16bfe655f19574f1ae0e"
        },
        "date": 1737012046386,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19157804,
            "range": "± 203729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19553379,
            "range": "± 170674",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21033152,
            "range": "± 248042",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25787654,
            "range": "± 625285",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 65422726,
            "range": "± 1271536",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 407165643,
            "range": "± 9054261",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2538724429,
            "range": "± 165946431",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16091513,
            "range": "± 213653",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16174168,
            "range": "± 190838",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16682004,
            "range": "± 181863",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21732154,
            "range": "± 861978",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53994686,
            "range": "± 3273159",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 374258182,
            "range": "± 21142073",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2830854255,
            "range": "± 83814457",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "giuseppe.re@parity.io",
            "name": "Giuseppe Re",
            "username": "re-gius"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "64abc745d9a7e7d6bea471e7bd2e895c503199c2",
          "message": "Update `parity-publish` to v0.10.4 (#7193)\n\nThe changes from v0.10.3 are only related to dependencies version. This\nshould fix some failing CIs.\n\nThis PR also updates the Rust cache version in CI.",
          "timestamp": "2025-01-16T14:00:59Z",
          "tree_id": "0ae678c987baad6fd5d34d15ec036bf49638c7a8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/64abc745d9a7e7d6bea471e7bd2e895c503199c2"
        },
        "date": 1737039043274,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17584553,
            "range": "± 248876",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17675994,
            "range": "± 83401",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19180723,
            "range": "± 112837",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22694589,
            "range": "± 103193",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50526010,
            "range": "± 410379",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 287769114,
            "range": "± 1719335",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2196478209,
            "range": "± 21133490",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14319864,
            "range": "± 141768",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14407371,
            "range": "± 91929",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14886917,
            "range": "± 71010",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18950257,
            "range": "± 253863",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49476953,
            "range": "± 218857",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 294394284,
            "range": "± 2003880",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2349405061,
            "range": "± 37038070",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "88332432+dastansam@users.noreply.github.com",
            "name": "Dastan",
            "username": "dastansam"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f7baa84f48aa72b96e8c9a9ec8a1934431de6709",
          "message": "[FRAME] `pallet_asset_tx_payment`: replace `AssetId` bound from `Copy` to `Clone` (#7194)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/6911",
          "timestamp": "2025-01-16T15:12:41Z",
          "tree_id": "76e82138f2681d38c1837774260368084f3321d0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f7baa84f48aa72b96e8c9a9ec8a1934431de6709"
        },
        "date": 1737044377210,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19945056,
            "range": "± 426716",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20032928,
            "range": "± 425297",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22292701,
            "range": "± 643098",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 28099924,
            "range": "± 1394811",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 68451246,
            "range": "± 2794289",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 428333459,
            "range": "± 9289883",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2574827317,
            "range": "± 213720637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16584153,
            "range": "± 336899",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16546325,
            "range": "± 237883",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17876652,
            "range": "± 307422",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23219119,
            "range": "± 1084064",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 63226169,
            "range": "± 894853",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 395282150,
            "range": "± 13401989",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2857880129,
            "range": "± 66590516",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "363911+pepoviola@users.noreply.github.com",
            "name": "Javier Viola",
            "username": "pepoviola"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "77ad8abb4a3aada3362fc4d5780db1844cc2e15d",
          "message": "Migrate substrate zombienet test poc (#7178)\n\nZombienet substrate tests PoC (using native provider).\n\ncc: @emamihe @alvicsam",
          "timestamp": "2025-01-16T16:09:24Z",
          "tree_id": "60adef081ef6ce5f5746930839f47c859bb25317",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/77ad8abb4a3aada3362fc4d5780db1844cc2e15d"
        },
        "date": 1737046705925,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18168708,
            "range": "± 121752",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18533278,
            "range": "± 137708",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20148580,
            "range": "± 497248",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23597340,
            "range": "± 191963",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52244377,
            "range": "± 471469",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 301079748,
            "range": "± 2459829",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2495852104,
            "range": "± 42486797",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15064551,
            "range": "± 90439",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15126610,
            "range": "± 210687",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15605269,
            "range": "± 113899",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19447320,
            "range": "± 195116",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50621768,
            "range": "± 394179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 306056897,
            "range": "± 3453280",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2435107403,
            "range": "± 17259492",
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
          "distinct": true,
          "id": "4b2febe18c6f2180a31a902433c00c30f8903ef7",
          "message": "Make frame crate not use the feature experimental (#7177)\n\nWe already use it for lots of pallet.\n\nKeeping it feature gated by experimental means we lose the information\nof which pallet was using experimental before the migration to frame\ncrate usage.\n\nWe can consider `polkadot-sdk-frame` crate unstable but let's not use\nthe feature `experimental`.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-17T11:46:28Z",
          "tree_id": "d3c04961bfcb06080b83e764b72bd06a609a2c84",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4b2febe18c6f2180a31a902433c00c30f8903ef7"
        },
        "date": 1737118318915,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18088388,
            "range": "± 228949",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18042711,
            "range": "± 141304",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19455012,
            "range": "± 170870",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23911112,
            "range": "± 467793",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56609937,
            "range": "± 1310417",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 336995393,
            "range": "± 6659536",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2778740486,
            "range": "± 103991073",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14889067,
            "range": "± 244241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15126065,
            "range": "± 212413",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16249191,
            "range": "± 354192",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20056750,
            "range": "± 450755",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52206620,
            "range": "± 1214898",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 321376342,
            "range": "± 9783716",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2606503249,
            "range": "± 67805264",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d62a90c8c729acd98c7e9a5cab9803b8b211ffc5",
          "message": "pallet_revive: Bump PolkaVM (#7203)\n\nUpdate to PolkaVM `0.19`. This version renumbers the opcodes in order to\nbe in-line with the grey paper. Hopefully, for the last time. This means\nthat it breaks existing contracts.\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: command-bot <>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-01-17T14:36:28Z",
          "tree_id": "968e43038c8fb4da1fd52c21631862be5ec7491f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d62a90c8c729acd98c7e9a5cab9803b8b211ffc5"
        },
        "date": 1737128852258,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18364193,
            "range": "± 881274",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18608187,
            "range": "± 162908",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20405683,
            "range": "± 633656",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24566062,
            "range": "± 315871",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57668950,
            "range": "± 1268082",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 354008420,
            "range": "± 8009030",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2572046074,
            "range": "± 109938897",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15443759,
            "range": "± 183487",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15350356,
            "range": "± 130783",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15940296,
            "range": "± 164453",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19961632,
            "range": "± 259145",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52153794,
            "range": "± 660727",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 316226044,
            "range": "± 5220342",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2523189047,
            "range": "± 20851527",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "0@mcornholio.ru",
            "name": "Yuri Volkov",
            "username": "mutantcornholio"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c2531dc12dedfb345c16200229038ef8d04972cc",
          "message": "review-bot upgrade (#7214)\n\nUpgrading PAPI in review-bot:\nhttps://github.com/paritytech/review-bot/issues/140",
          "timestamp": "2025-01-17T17:00:04Z",
          "tree_id": "aba80af7d686cb9ce7a7c829d22c93e3f3cdd9f3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c2531dc12dedfb345c16200229038ef8d04972cc"
        },
        "date": 1737136366940,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18304738,
            "range": "± 191073",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18527236,
            "range": "± 281673",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20043720,
            "range": "± 226665",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23781324,
            "range": "± 162492",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54567179,
            "range": "± 817817",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 313420659,
            "range": "± 4555487",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2605308384,
            "range": "± 45571261",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14677700,
            "range": "± 141201",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14765049,
            "range": "± 86961",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15236255,
            "range": "± 124141",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19210241,
            "range": "± 115039",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51714726,
            "range": "± 831038",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 307169871,
            "range": "± 2961139",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2438498238,
            "range": "± 11898175",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7702fdd1bd869e518bf176ccf0268f83f8927f9b",
          "message": "[pallet-revive] Add  tracing support (1/3) (#7166)\n\nAdd foundation for supporting call traces in pallet_revive\n\nFollow up:\n- PR #7167 Add changes to eth-rpc to introduce debug endpoint that will\nuse pallet-revive tracing features\n- PR #6727 Add new RPC to the client and implement tracing runtime API\nthat can capture traces on previous blocks\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-01-17T18:21:38Z",
          "tree_id": "c83fae415391294d96d84614537a8454d6a2a84b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7702fdd1bd869e518bf176ccf0268f83f8927f9b"
        },
        "date": 1737147608745,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17979571,
            "range": "± 229661",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18244692,
            "range": "± 150519",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19763823,
            "range": "± 132968",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23496696,
            "range": "± 504601",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52131354,
            "range": "± 457737",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 309407290,
            "range": "± 2527716",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2406364477,
            "range": "± 86731526",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14765115,
            "range": "± 105120",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14854341,
            "range": "± 119648",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15258502,
            "range": "± 72031",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19361630,
            "range": "± 242393",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50663308,
            "range": "± 489173",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304482389,
            "range": "± 3026713",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2386263416,
            "range": "± 31593728",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32275622+seemantaggarwal@users.noreply.github.com",
            "name": "seemantaggarwal",
            "username": "seemantaggarwal"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "4937f779068d1ab947c9eada8e1d3f5b7191eb94",
          "message": "Use docify export for parachain template hardcoded configuration and embed it in its README #6333 (#7093)\n\nUse docify export for parachain template hardcoded configuration and\nembed it in its README #6333\n\nDocify currently has a limitation of not being able to embed a\nvariable/const in its code, without embedding it's definition, even if\ndo something in a string like\n\n\"this is a sample string ${sample_variable}\"\n\nIt will embed the entire string \n\"this is a sample string ${sample_variable}\"\nwithout replacing the value of sample_variable from the code\n\nHence, the goal was just to make it obvious in the README where the\nPARACHAIN_ID value is coming from, so a note has been added at the start\nfor the same, so whenever somebody is running these commands, they will\nbe aware about the value and replace accordingly.\n\nTo make it simpler, we added a \nrust ignore block so the user can just look it up in the readme itself\nand does not have to scan through the runtime directory for the value.\n\n---------\n\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-01-20T10:21:29Z",
          "tree_id": "bf73d3be67c48f088a2d8ea09a7f98b9d05ef959",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4937f779068d1ab947c9eada8e1d3f5b7191eb94"
        },
        "date": 1737372833952,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21112001,
            "range": "± 726247",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20952942,
            "range": "± 1053150",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21958949,
            "range": "± 332781",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26553717,
            "range": "± 641259",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63482603,
            "range": "± 2311791",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 430378516,
            "range": "± 15308694",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2896325497,
            "range": "± 235769895",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15668955,
            "range": "± 485238",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16594168,
            "range": "± 423578",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17661021,
            "range": "± 438277",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22989128,
            "range": "± 895428",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 60803065,
            "range": "± 1734343",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 409673043,
            "range": "± 12780549",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2947073658,
            "range": "± 63584767",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "d5d9b1276a088a6bd7a8c2c698320dad3d0ee2c4",
          "message": "Stabilize `ensure_execute_processes_have_correct_num_threads` test (#7253)\n\nSaw this test flake a few times, last time\n[here](https://github.com/paritytech/polkadot-sdk/actions/runs/12834432188/job/35791830215).\n\nWe first fetch all processes in the test, then query `/proc/<pid>/stat`\nfor every one of them. When the file was not found, we would error. Now\nwe tolerate not finding this file. Ran 200 times locally without error,\nbefore would fail a few times, probably depending on process fluctuation\n(which I expect to be high on CI runners).",
          "timestamp": "2025-01-20T11:02:59Z",
          "tree_id": "ae157145721731ae535a6f5633902334e6ce545a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5d9b1276a088a6bd7a8c2c698320dad3d0ee2c4"
        },
        "date": 1737374176727,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17422732,
            "range": "± 119219",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17708208,
            "range": "± 100861",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19427012,
            "range": "± 141533",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22912638,
            "range": "± 170760",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52536572,
            "range": "± 770324",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 325779334,
            "range": "± 5149128",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2300108714,
            "range": "± 72743446",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14426000,
            "range": "± 142949",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14492808,
            "range": "± 97496",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15027981,
            "range": "± 91093",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19263461,
            "range": "± 116918",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50104357,
            "range": "± 590082",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 293283706,
            "range": "± 1721853",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2420889348,
            "range": "± 11376355",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ea27696aeed8e76cfb82492f6f3665948d766fe5",
          "message": "[pallet-revive] eth-rpc error logging (#7251)\n\nLog error instead of failing with an error when block processing fails\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-20T11:47:29Z",
          "tree_id": "e73af823265c0bd183ed33d3d4170ee727a2b722",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ea27696aeed8e76cfb82492f6f3665948d766fe5"
        },
        "date": 1737376869300,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18771338,
            "range": "± 242423",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18791059,
            "range": "± 256843",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20144976,
            "range": "± 291039",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23690349,
            "range": "± 153020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52603663,
            "range": "± 2269605",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 319913688,
            "range": "± 9578750",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2538452292,
            "range": "± 105117903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15262462,
            "range": "± 171255",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15562067,
            "range": "± 177331",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16148001,
            "range": "± 245144",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20052853,
            "range": "± 200616",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51687520,
            "range": "± 573038",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 308609914,
            "range": "± 5033150",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2434308809,
            "range": "± 13042974",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yrong1997@gmail.com",
            "name": "Ron",
            "username": "yrong"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "569ce71e2c759b26601608f145d9b5efcb906919",
          "message": "Migrate pallet-mmr to umbrella crate (#7081)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/6504",
          "timestamp": "2025-01-20T14:16:57Z",
          "tree_id": "92132f125d5f9934b551da362af5b39250a3c53f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/569ce71e2c759b26601608f145d9b5efcb906919"
        },
        "date": 1737385973042,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 16879660,
            "range": "± 165255",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17188718,
            "range": "± 101153",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18712548,
            "range": "± 170822",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22439961,
            "range": "± 179765",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50331570,
            "range": "± 432563",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 295769453,
            "range": "± 1602481",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2300071111,
            "range": "± 21113938",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14220818,
            "range": "± 153346",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14293497,
            "range": "± 67882",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14745967,
            "range": "± 124850",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18616681,
            "range": "± 137998",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47651938,
            "range": "± 472286",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 282228495,
            "range": "± 1470248",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2247880839,
            "range": "± 7725470",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "runcomet@protonmail.com",
            "name": "runcomet",
            "username": "runcomet"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "711e6ff33373bc08b026446ce19b73920bfe068c",
          "message": "Migrate `pallet-assets-freezer` to umbrella crate (#6599)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/6504\n\n### Added modules\n\n- `utility`: Traits not tied to any direct operation in the runtime.\n\npolkadot address: 14SRqZTC1d8rfxL8W1tBTnfUBPU23ACFVPzp61FyGf4ftUFg\n\n---------\n\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>",
          "timestamp": "2025-01-20T16:12:44Z",
          "tree_id": "4f4b01b3189d08a7662c4986bcd35a0cdf12aac6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/711e6ff33373bc08b026446ce19b73920bfe068c"
        },
        "date": 1737393330228,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18055831,
            "range": "± 359936",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18157697,
            "range": "± 399030",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19826214,
            "range": "± 426007",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23375077,
            "range": "± 590290",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53265825,
            "range": "± 830529",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 311195248,
            "range": "± 7229491",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2508621951,
            "range": "± 37513773",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15123148,
            "range": "± 247343",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15039146,
            "range": "± 340111",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15525050,
            "range": "± 253736",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19459771,
            "range": "± 317878",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51177997,
            "range": "± 1324533",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 314552697,
            "range": "± 10126109",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2504317267,
            "range": "± 15786540",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "benjamin@gallois.cc",
            "name": "Benjamin Gallois",
            "username": "bgallois"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2c4ceccebe2c338029eef243645455d525a5a78b",
          "message": "Fix `frame-benchmarking-cli` not buildable without rocksdb (#7263)\n\n## Description\n\nThe `frame-benchmarking-cli` crate has not been buildable without the\n`rocksdb` feature since version 1.17.0.\n\n**Error:**  \n```rust\nself.database()?.unwrap_or(Database::RocksDb),\n                             ^^^^^^^ variant or associated item not found in `Database`\n```\n\nThis issue is also related to the `rocksdb` feature bleeding (#3793),\nwhere the `rocksdb` feature was always activated even when compiling\nthis crate with `--no-default-features`.\n\n**Fix:**  \n- Resolved the error by choosing `paritydb` as the default database when\ncompiled without the `rocksdb` feature.\n- Fixed the issue where the `sc-cli` crate's `rocksdb` feature was\nalways active, even compiling `frame-benchmarking-cli` with\n`--no-default-features`.\n\n## Review Notes\n\nFix the crate to be built without rocksdb, not intended to solve #3793.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-20T21:19:48Z",
          "tree_id": "42c3bf3888529a0b4b0b85c87a9d5814dfb30c18",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2c4ceccebe2c338029eef243645455d525a5a78b"
        },
        "date": 1737410999170,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18488390,
            "range": "± 123986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18795468,
            "range": "± 144980",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20219790,
            "range": "± 116871",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23654992,
            "range": "± 75707",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53554239,
            "range": "± 427420",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 319764511,
            "range": "± 5692119",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2472618270,
            "range": "± 27808887",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15085718,
            "range": "± 109104",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14982627,
            "range": "± 95447",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15703470,
            "range": "± 171769",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19530417,
            "range": "± 185213",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50712341,
            "range": "± 358775",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 305581430,
            "range": "± 1468290",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2442363349,
            "range": "± 23000516",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cbf3925e1fe1383b998cfb428038c46da1577501",
          "message": "[eth-indexer] subscribe to finalize blocks instead of best blocks (#7260)\n\nFor eth-indexer, it's probably safer to use `subscribe_finalized` and\nindex these blocks into the DB rather than `subscribe_best`\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-20T22:58:21Z",
          "tree_id": "27475d6dbd249e2e1b4b930038f1a1cd4be00564",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cbf3925e1fe1383b998cfb428038c46da1577501"
        },
        "date": 1737417148279,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17271208,
            "range": "± 78850",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17734759,
            "range": "± 303563",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18990916,
            "range": "± 92565",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22585603,
            "range": "± 132385",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51221295,
            "range": "± 562658",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 283401812,
            "range": "± 2346370",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2249084970,
            "range": "± 66563554",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14261144,
            "range": "± 85620",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14359887,
            "range": "± 56477",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14723154,
            "range": "± 83250",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18532906,
            "range": "± 390122",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48480680,
            "range": "± 387844",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 286442338,
            "range": "± 1784820",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2304917879,
            "range": "± 6397679",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jose@blockdeep.io",
            "name": "José Molina Colmenero",
            "username": "Moliholy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "12ed0f4ffe4dcf3a8fe8928e3791141a110fad8b",
          "message": "Add an extra_constant to pallet-collator-selection (#7206)\n\nCurrently `pallet-collator-selection` does not expose a way to query the\nassigned pot account derived from the `PotId` configuration item.\nWithout it, it is not possible to transfer the existential deposit to\nit.\n\nThis PR addresses this issue by exposing an extra constant.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-21T09:49:09Z",
          "tree_id": "606f8c7eb20cf23b7b299cdde264f8503415b819",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12ed0f4ffe4dcf3a8fe8928e3791141a110fad8b"
        },
        "date": 1737456413662,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20306283,
            "range": "± 256890",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20767347,
            "range": "± 297831",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21873982,
            "range": "± 301200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26871701,
            "range": "± 451244",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 66525852,
            "range": "± 1640230",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 421653578,
            "range": "± 20746923",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 3071179289,
            "range": "± 36423565",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16236375,
            "range": "± 315976",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16315818,
            "range": "± 202432",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16626545,
            "range": "± 246160",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20993207,
            "range": "± 257717",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57320684,
            "range": "± 1201455",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 358290060,
            "range": "± 4793232",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2707290220,
            "range": "± 28684018",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "claravanstaden64@gmail.com",
            "name": "Clara van Staden",
            "username": "claravanstaden"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c0c0632c2efca435e973a1f6788e24235fe0e2a6",
          "message": "Snowbridge - Copy Rococo integration tests to Westend (#7108)\n\nCopies all the integration tests from Rococo to Westend.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/6389",
          "timestamp": "2025-01-21T14:11:50Z",
          "tree_id": "245cc885df6e82b498175778a667548dce9f9a09",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c0c0632c2efca435e973a1f6788e24235fe0e2a6"
        },
        "date": 1737471726801,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17536198,
            "range": "± 121212",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17822261,
            "range": "± 158020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19370525,
            "range": "± 182193",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23222848,
            "range": "± 297414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53059338,
            "range": "± 931496",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 298218217,
            "range": "± 4239266",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2425768999,
            "range": "± 143731600",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14610346,
            "range": "± 100562",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14753882,
            "range": "± 175556",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15126000,
            "range": "± 63805",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19213816,
            "range": "± 239018",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50386080,
            "range": "± 454901",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 293042735,
            "range": "± 3270371",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2402432828,
            "range": "± 19039944",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ludovic.domingues96@gmail.com",
            "name": "Ludovic_Domingues",
            "username": "Krayt78"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9edaef09a69e39b0785f8339f93a3ed6a1f6e023",
          "message": "Migrate pallet-paged-list-fuzzer to umbrella crate (#6930)\n\nPart of  #6504\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>",
          "timestamp": "2025-01-21T17:36:04Z",
          "tree_id": "8f4410fe7bbfba61ebfee9ff25593496596d86d9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9edaef09a69e39b0785f8339f93a3ed6a1f6e023"
        },
        "date": 1737484329186,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18168111,
            "range": "± 277561",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18310431,
            "range": "± 223262",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19844693,
            "range": "± 340508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23443474,
            "range": "± 272682",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53665757,
            "range": "± 906228",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 329267549,
            "range": "± 7203906",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2294490715,
            "range": "± 15736389",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14754657,
            "range": "± 102915",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15044332,
            "range": "± 204200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15471691,
            "range": "± 234583",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20123475,
            "range": "± 213903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51503133,
            "range": "± 1159902",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304912064,
            "range": "± 4478959",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2438996186,
            "range": "± 18266391",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "363911+pepoviola@users.noreply.github.com",
            "name": "Javier Viola",
            "username": "pepoviola"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2345eb9a5b5e2145ac1c04fd9cf1fcf12b7278b6",
          "message": "Bump zombienet version to `v1.3.119` (#7283)\n\nThis version include a fix that make test\n`zombienet-polkadot-malus-0001-dispute-valid` green again.\nThx!",
          "timestamp": "2025-01-21T21:24:05Z",
          "tree_id": "79041ea24648703c8978b3b5268e00f102249ee6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2345eb9a5b5e2145ac1c04fd9cf1fcf12b7278b6"
        },
        "date": 1737497719905,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17484087,
            "range": "± 99771",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17653061,
            "range": "± 152585",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19324279,
            "range": "± 181899",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23097042,
            "range": "± 248508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52763241,
            "range": "± 787546",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 301224907,
            "range": "± 2977872",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2238912573,
            "range": "± 206256952",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14594413,
            "range": "± 304915",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14864407,
            "range": "± 103757",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15766852,
            "range": "± 126979",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20077951,
            "range": "± 141129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52139893,
            "range": "± 814045",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 308676962,
            "range": "± 4359186",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2496544685,
            "range": "± 13279994",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1bdb817f2b140b0c2573396146fd7bbfb936af10",
          "message": "Enable BEEFY `report_fork_voting()` (#6856)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4523\n\nFollow-up for: https://github.com/paritytech/polkadot-sdk/pull/5188\n\nReopening https://github.com/paritytech/polkadot-sdk/pull/6732 as a new\nPR\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-22T10:01:28Z",
          "tree_id": "a014a8246dab85d5b371f306aac68609b4ac6947",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1bdb817f2b140b0c2573396146fd7bbfb936af10"
        },
        "date": 1737543139601,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17224821,
            "range": "± 116992",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17448861,
            "range": "± 99986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18817058,
            "range": "± 153966",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22614653,
            "range": "± 149322",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50725963,
            "range": "± 743826",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 296387354,
            "range": "± 1899947",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2285090349,
            "range": "± 88699847",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14238295,
            "range": "± 90789",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14374416,
            "range": "± 148270",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14936959,
            "range": "± 112108",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18747434,
            "range": "± 150100",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49110568,
            "range": "± 509353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 287227092,
            "range": "± 2175720",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2350864227,
            "range": "± 15553045",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "59443568+sw10pa@users.noreply.github.com",
            "name": "Stephane Gurgenidze",
            "username": "sw10pa"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4eb9228840be0abef1c45cf8fa8bc44b5f95200a",
          "message": "collation-generation: resolve mismatch between descriptor and commitments core index (#7104)\n\n## Issue\n[[#7107] Core Index Mismatch in Commitments and\nDescriptor](https://github.com/paritytech/polkadot-sdk/issues/7107)\n\n## Description\nThis PR resolves a bug where normal (non-malus) undying collators failed\nto generate and submit collations, resulting in the following error:\n\n`ERROR tokio-runtime-worker parachain::collation-generation: Failed to\nconstruct and distribute collation: V2 core index check failed: The core\nindex in commitments doesn't match the one in descriptor.`\n\nMore details about the issue and reproduction steps are described in the\n[related issue](https://github.com/paritytech/polkadot-sdk/issues/7107).\n\n## Summary of Fix\n- When core selectors are provided in the UMP signals, core indexes will\nbe chosen using them;\n- The fix ensures that functionality remains unchanged for parachains\nnot using UMP signals;\n- Added checks to stop processing if the same core is selected\nrepeatedly.\n\n## TODO\n- [X] Implement the fix;\n- [x] Add tests;\n- [x] Add PRdoc.",
          "timestamp": "2025-01-22T11:00:50Z",
          "tree_id": "73f36ea0a295d0adf3f15e8201dc0dddb9de2443",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4eb9228840be0abef1c45cf8fa8bc44b5f95200a"
        },
        "date": 1737547225841,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17591905,
            "range": "± 77275",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17730249,
            "range": "± 104764",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19462014,
            "range": "± 179451",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23200596,
            "range": "± 635097",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54191053,
            "range": "± 918803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 318372087,
            "range": "± 6020272",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2513454153,
            "range": "± 88076012",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14662274,
            "range": "± 228111",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14750960,
            "range": "± 143480",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15411144,
            "range": "± 143059",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19954494,
            "range": "± 209327",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51675146,
            "range": "± 785096",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 297467984,
            "range": "± 10443551",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2431374621,
            "range": "± 8103863",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "69342343+MrishoLukamba@users.noreply.github.com",
            "name": "Mrisho Lukamba",
            "username": "MrishoLukamba"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "634a17b6f67c71e589f921b0ddd4c23bbed883f1",
          "message": "Unify Import verifier usage across parachain template and omninode (#7195)\n\nCloses #7055\n\n@skunert @bkchr\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-01-22T15:06:18Z",
          "tree_id": "7730c1e3bb98148039a8322f04aef3fa9dfcd179",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/634a17b6f67c71e589f921b0ddd4c23bbed883f1"
        },
        "date": 1737561444305,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18108598,
            "range": "± 165680",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18310336,
            "range": "± 140417",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19678926,
            "range": "± 302175",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23109195,
            "range": "± 268722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52575587,
            "range": "± 716830",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 306634454,
            "range": "± 5461515",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2333421654,
            "range": "± 140510288",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14288670,
            "range": "± 96746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14420693,
            "range": "± 72385",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14965166,
            "range": "± 153477",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18890398,
            "range": "± 273190",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48738512,
            "range": "± 616012",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 291343607,
            "range": "± 3171234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2367260950,
            "range": "± 27798952",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "fd64a1e7768ba6e8676cbbf25c4e821a901c0a7f",
          "message": "net/libp2p: Enforce outbound request-response timeout limits (#7222)\n\nThis PR enforces that outbound requests are finished within the\nspecified protocol timeout.\n\nThe stable2412 version running libp2p 0.52.4 contains a bug which does\nnot track request timeouts properly:\n- https://github.com/libp2p/rust-libp2p/pull/5429\n\nThe issue has been detected while submitting libp2p -> litep2p requests\nin kusama. This aims to check that pending outbound requests have not\ntimedout. Although the issue has been fixed in libp2p, there might be\nother cases where this may happen. For example:\n- https://github.com/libp2p/rust-libp2p/pull/5417\n\nFor more context see:\nhttps://github.com/paritytech/polkadot-sdk/issues/7076#issuecomment-2596085096\n\n\n1. Ideally, the force-timeout mechanism in this PR should never be\ntriggered in production. However, origin/stable2412 occasionally\nencounters this issue. When this happens, 2 warnings may be generated:\n- one warning introduced by this PR wrt force timeout terminating the\nrequest\n- possible one warning when the libp2p decides (if at all) to provide\nthe response back to substrate (as mentioned by @alexggh\n[here](https://github.com/paritytech/polkadot-sdk/pull/7222/files#diff-052aeaf79fef3d9a18c2cfd67006aa306b8d52e848509d9077a6a0f2eb856af7L769)\nand\n[here](https://github.com/paritytech/polkadot-sdk/pull/7222/files#diff-052aeaf79fef3d9a18c2cfd67006aa306b8d52e848509d9077a6a0f2eb856af7L842)\n\n2. This implementation does not propagate to the substrate service the\n`RequestFinished { error: .. }`. That event is only used internally by\nsubstrate to increment metrics. However, we don't have the peer\ninformation available to propagate the event properly when we\nforce-timeout the request. Considering this should most likely not\nhappen in production (origin/master) and that we'll be able to extract\ninformation by warnings, I would say this is a good tradeoff for code\nsimplicity:\n\n\nhttps://github.com/paritytech/polkadot-sdk/blob/06e3b5c6a7696048d65f1b8729f16b379a16f501/substrate/client/network/src/service.rs#L1543\n\n\n### Testing\n\nAdded a new test to ensure the timeout is reached properly, even if\nlibp2p does not produce a response in due time.\n\nI've also transitioned the tests to using `tokio::test` due to a\nlimitation of\n[CI](https://github.com/paritytech/polkadot-sdk/actions/runs/12832055737/job/35784043867)\n\n```\n--- TRY 1 STDERR:        sc-network request_responses::tests::max_response_size_exceeded ---\nthread 'request_responses::tests::max_response_size_exceeded' panicked at /usr/local/cargo/registry/src/index.crates.io-6f17d22bba15001f/tokio-1.40.0/src/time/interval.rs:139:26:\nthere is no reactor running, must be called from the context of a Tokio 1.x runtime\n```\n\n\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-22T16:51:59Z",
          "tree_id": "17935494a17a9360cc2d6485a7009724bcb76fef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd64a1e7768ba6e8676cbbf25c4e821a901c0a7f"
        },
        "date": 1737567871593,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17332101,
            "range": "± 115518",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17632435,
            "range": "± 101645",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18894011,
            "range": "± 137294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22823920,
            "range": "± 195523",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50696494,
            "range": "± 448459",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 294930390,
            "range": "± 3920965",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2129239854,
            "range": "± 51248077",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14267666,
            "range": "± 106998",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14283890,
            "range": "± 49221",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14761581,
            "range": "± 117305",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18980312,
            "range": "± 141705",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49340374,
            "range": "± 277101",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 291494936,
            "range": "± 2458394",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2325664147,
            "range": "± 22865042",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "FereMouSiopi@proton.me",
            "name": "FereMouSiopi",
            "username": "FereMouSiopi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "89b022842c7ab922de5bf026cd45e43b9cd8c654",
          "message": "Migrate `pallet-insecure-randomness-collective-flip` to umbrella crate (#6738)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/6504\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-22T18:08:59Z",
          "tree_id": "cc44d8d5ba3fdf2339d181c13dcd627e344d1111",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/89b022842c7ab922de5bf026cd45e43b9cd8c654"
        },
        "date": 1737572766849,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17237123,
            "range": "± 106362",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17671202,
            "range": "± 177271",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19406711,
            "range": "± 185884",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23367225,
            "range": "± 597672",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52198169,
            "range": "± 646307",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 305708857,
            "range": "± 3552246",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2361844739,
            "range": "± 89160907",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14173896,
            "range": "± 229627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14449298,
            "range": "± 173353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15026028,
            "range": "± 253742",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19080091,
            "range": "± 130106",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49440458,
            "range": "± 287564",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 293193870,
            "range": "± 2678446",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2404568047,
            "range": "± 50529481",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "5772b9dbde8f88718ec5c6409f444d6e5b4e4e03",
          "message": "[pallet-revive] fee estimation fixes (#7281)\n\n- Fix the EVM fee cost estimation.\nThe estimation shown in EVM wallet was using Native instead of EVM\ndecimals\n- Remove the precise code length estimation in dry run call.\nOver-estimating is fine, since extra gas is refunded anyway.\n- Ensure that the estimated fee calculated from gas_price x gas use the\nencoded weight & deposit limit instead of the exact one calculated by\nthe dry-run. Else we can end up with a fee that is lower than the actual\nfee paid by the user\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-23T09:57:06Z",
          "tree_id": "b25a28f13b631208eec39649193f984dbb68820e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5772b9dbde8f88718ec5c6409f444d6e5b4e4e03"
        },
        "date": 1737629826349,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18781640,
            "range": "± 179580",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19050687,
            "range": "± 175550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20798941,
            "range": "± 177402",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24204258,
            "range": "± 295582",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 58562282,
            "range": "± 760222",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 358961761,
            "range": "± 6718371",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2721471337,
            "range": "± 120880540",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15383674,
            "range": "± 112010",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15589071,
            "range": "± 144749",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16359587,
            "range": "± 88849",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20643808,
            "range": "± 181466",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53042867,
            "range": "± 1177312",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 336537600,
            "range": "± 6918551",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2604747510,
            "range": "± 16206172",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "runcomet@protonmail.com",
            "name": "runcomet",
            "username": "runcomet"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "04847d515ef56da4d0801c9b89a4241dfa827b33",
          "message": "Balances: Configurable Number of Genesis Accounts with Specified Balances for Benchmarking (#6267)\n\n# Derived Dev Accounts\n\nResolves https://github.com/paritytech/polkadot-sdk/issues/6040\n\n## Description\nThis update introduces support for creating an arbitrary number of\ndeveloper accounts at the genesis block based on a specified derivation\npath. This functionality is gated by the runtime-benchmarks feature,\nensuring it is only enabled during benchmarking scenarios.\n\n### Key Features\n- Arbitrary Dev Accounts at Genesis: Developers can now specify any\nnumber of accounts to be generated at genesis using a hard derivation\npath.\n\n- Default Derivation Path: If no derivation path is provided (i.e., when\n`Option<dev_accounts: (..., None)>` is set to `Some` at genesis), the\nsystem will default to the path `//Sender//{}`.\n\n- No Impact on Total Token Issuance: Developer accounts are excluded\nfrom the total issuance of the token supply at genesis, ensuring they do\nnot affect the overall balance or token distribution.\n\npolkadot address: 14SRqZTC1d8rfxL8W1tBTnfUBPU23ACFVPzp61FyGf4ftUFg\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-01-23T10:38:15Z",
          "tree_id": "2e6227b4cd51ae9aad6ed4b03538e4eb4ed049f5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/04847d515ef56da4d0801c9b89a4241dfa827b33"
        },
        "date": 1737632211537,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18932331,
            "range": "± 151984",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19281512,
            "range": "± 185162",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20812886,
            "range": "± 143042",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24880598,
            "range": "± 250352",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 60380771,
            "range": "± 1167993",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 375000230,
            "range": "± 6402899",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2787588806,
            "range": "± 95369334",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15338349,
            "range": "± 149803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15580666,
            "range": "± 183796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15935156,
            "range": "± 181187",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20625851,
            "range": "± 125753",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 56529413,
            "range": "± 1499200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 362834088,
            "range": "± 5877065",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2714415511,
            "range": "± 60071106",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "a.khssnv@gmail.com",
            "name": "Alisher A. Khassanov",
            "username": "khssnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "66bd26d35c21ad260120129776c86870ff1dd220",
          "message": "Add `offchain_localStorageClear` RPC method (#7266)\n\n# Description\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/7265.\n\n## Integration\n\nRequires changes in\n`https://github.com/polkadot-js/api/packages/{rpc-augment,types-support,types}`\nto be visible in Polkadot\\Substrate Portal and in other libraries where\nwe should explicitly state RPC methods.\n\nAccompany PR to `polkadot-js/api`:\nhttps://github.com/polkadot-js/api/pull/6070.\n\n## Review Notes\n\nPlease put the right label on my PR.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-23T11:01:55Z",
          "tree_id": "a68a17a4e6d0a5320ce2ea2e0421515e4421751a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/66bd26d35c21ad260120129776c86870ff1dd220"
        },
        "date": 1737633469336,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17793999,
            "range": "± 126295",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18105917,
            "range": "± 110302",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19539554,
            "range": "± 266454",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23535606,
            "range": "± 163742",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53388152,
            "range": "± 3292138",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 321196958,
            "range": "± 4978903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2480094138,
            "range": "± 45485034",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14253662,
            "range": "± 54257",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14291859,
            "range": "± 193633",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14835416,
            "range": "± 118965",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18904611,
            "range": "± 157572",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49232196,
            "range": "± 623740",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 288564137,
            "range": "± 2429554",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2360878487,
            "range": "± 14353806",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "085da479dee8e09ad3de83dbc59b304bd36b4ebe",
          "message": "Bridges small nits/improvements (#7307)\n\nThis PR contains small fixes identified during work on the larger PR:\nhttps://github.com/paritytech/polkadot-sdk/issues/6906.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-23T11:55:14Z",
          "tree_id": "36c525ca26c1e465f4fe088589d1e30b3f02fe2a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/085da479dee8e09ad3de83dbc59b304bd36b4ebe"
        },
        "date": 1737636767248,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18789188,
            "range": "± 132891",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19169474,
            "range": "± 151668",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20735723,
            "range": "± 207118",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24751662,
            "range": "± 427873",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57624261,
            "range": "± 1444681",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 369108787,
            "range": "± 8881839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2738784992,
            "range": "± 85629670",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15402871,
            "range": "± 64531",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15659988,
            "range": "± 104368",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16207852,
            "range": "± 207429",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20314007,
            "range": "± 141289",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52159553,
            "range": "± 545489",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 338681235,
            "range": "± 5778508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2584877417,
            "range": "± 61762454",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alin@parity.io",
            "name": "Alin Dima",
            "username": "alindima"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "cfc5b6f59a1fa46aa55144bff5eb7fca14e27e2b",
          "message": "bump lookahead to 3 for testnet genesis (#7252)\n\nThis is the right value after\nhttps://github.com/paritytech/polkadot-sdk/pull/4880, which corresponds\nto an allowedAncestryLen of 2 (which is the default)\n\nWIll fix https://github.com/paritytech/polkadot-sdk/issues/7105",
          "timestamp": "2025-01-23T13:00:31Z",
          "tree_id": "7f68012322ae7652b91643eedd929cdded5937be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cfc5b6f59a1fa46aa55144bff5eb7fca14e27e2b"
        },
        "date": 1737640514895,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19348123,
            "range": "± 223452",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19550012,
            "range": "± 175021",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20966958,
            "range": "± 316762",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25330550,
            "range": "± 562622",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57067195,
            "range": "± 1862717",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 368858853,
            "range": "± 5529146",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2731726867,
            "range": "± 60806804",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15425503,
            "range": "± 107234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15459858,
            "range": "± 167566",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15937410,
            "range": "± 178311",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20663978,
            "range": "± 182366",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52456608,
            "range": "± 760680",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 330437967,
            "range": "± 3329495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2524818790,
            "range": "± 12921281",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1177472+mordamax@users.noreply.github.com",
            "name": "Maksym H",
            "username": "mordamax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "6091330ae6d799bcf34d366acda7aff91c609ab1",
          "message": "Refactor command bot and drop rejecting non paritytech members (#7231)\n\nAims to \n- close #7049 \n- close https://github.com/paritytech/opstooling/issues/449\n- close https://github.com/paritytech/opstooling/issues/463\n\nWhat's changed:\n- removed is paritytech member check as required prerequisite to run a\ncommand\n- run the cmd.py script taking it from master, if someone who run this\nis not a member of paritytech, and from current branch, if is a member.\nThat keeps the developer experience & easy testing if paritytech members\nare contributing to cmd.py\n- isolate the cmd job from being able to access GH App token or PR\ntoken- currently the fmt/bench/prdoc are being run with limited\npermissions scope, just to generate output, which then is uploaded to\nartifacts, and then the other job which doesn't run any files from repo,\ndoes a commit/push more securely",
          "timestamp": "2025-01-23T13:30:26Z",
          "tree_id": "da45e8f46110ea08661552bbc0795a8fa2c5a8a5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6091330ae6d799bcf34d366acda7aff91c609ab1"
        },
        "date": 1737642047643,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17646943,
            "range": "± 279730",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17874774,
            "range": "± 162539",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19354280,
            "range": "± 142041",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23000629,
            "range": "± 114160",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51387600,
            "range": "± 744773",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 295605833,
            "range": "± 4715702",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2111008169,
            "range": "± 62805932",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14047498,
            "range": "± 90950",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14265292,
            "range": "± 201824",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14677955,
            "range": "± 32615",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18979005,
            "range": "± 98397",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49091699,
            "range": "± 292918",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 292106444,
            "range": "± 1461272",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2278335567,
            "range": "± 13883628",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1177472+mordamax@users.noreply.github.com",
            "name": "Maksym H",
            "username": "mordamax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3a7f3c0af63b1a7566ca29c59fa4ac274bd911f1",
          "message": "Fix setting the image properly (#7315)\n\nFixed condition which sets weights/large images",
          "timestamp": "2025-01-23T16:08:32Z",
          "tree_id": "ddf15c0baa7aad24dfe8dde9c77711bbf57959c7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3a7f3c0af63b1a7566ca29c59fa4ac274bd911f1"
        },
        "date": 1737651588012,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17764904,
            "range": "± 151591",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17906625,
            "range": "± 98109",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19295712,
            "range": "± 81280",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22717280,
            "range": "± 185742",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51590228,
            "range": "± 536510",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 310371598,
            "range": "± 2513197",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2396185613,
            "range": "± 80891417",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14477255,
            "range": "± 139053",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14676122,
            "range": "± 55407",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14915548,
            "range": "± 227341",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19285700,
            "range": "± 111626",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50536645,
            "range": "± 168011",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 303567042,
            "range": "± 5384957",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2391610028,
            "range": "± 36893923",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "23600076de203dad498d815ff4b7ed2968217c10",
          "message": "Nits for collectives-westend XCM benchmarks setup (#7215)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/2904\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-24T12:32:19Z",
          "tree_id": "1ab71e2b09edd1f19fb9eb5b7bea0f4a189d5e9c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/23600076de203dad498d815ff4b7ed2968217c10"
        },
        "date": 1737724841001,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17473009,
            "range": "± 320867",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17936649,
            "range": "± 238976",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19345985,
            "range": "± 113839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23209482,
            "range": "± 215900",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51683620,
            "range": "± 496347",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 291500929,
            "range": "± 3401467",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2276206301,
            "range": "± 53558901",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14536212,
            "range": "± 118408",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14487537,
            "range": "± 97065",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15424759,
            "range": "± 106552",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19356225,
            "range": "± 92541",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49665002,
            "range": "± 239212",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 300639653,
            "range": "± 10178373",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2417398906,
            "range": "± 7877540",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "a2c63e8d8a512eca28ed24c3c58ea7609c28b9ee",
          "message": "fix(cmd bench-omni): build omni-bencher with production profile (#7299)\n\n# Description\n\nThis PR builds frame-omni-bencher with `production` profile when calling\n`/cmd bench-omni` to compute benchmarks for pallets.\nFix proposed by @bkchr , thanks!\n\nCloses #6797.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nMore info on #6797, and related to how the fix was tested:\nhttps://github.com/paritytech/polkadot-sdk/issues/6797#issuecomment-2611903102.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-24T13:29:25Z",
          "tree_id": "93aa5d24fb63b13db179ce2b696deb7f7d4f2ba1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a2c63e8d8a512eca28ed24c3c58ea7609c28b9ee"
        },
        "date": 1737728389007,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17994941,
            "range": "± 209909",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18502222,
            "range": "± 305355",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19725907,
            "range": "± 178657",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23687299,
            "range": "± 315762",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54516118,
            "range": "± 1274550",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 356218003,
            "range": "± 5036676",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2548213776,
            "range": "± 179747240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15019952,
            "range": "± 251372",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15198984,
            "range": "± 306715",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15498005,
            "range": "± 233856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20412934,
            "range": "± 1009217",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54354828,
            "range": "± 2342088",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 300197674,
            "range": "± 4267982",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2423662945,
            "range": "± 51811157",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7710483541ce273df892c77a6e300aaa2efa1dca",
          "message": "Bridges: emulated tests small nits/improvements (#7322)\n\nThis PR includes minor fixes identified during work on the larger PR:\n[https://github.com/paritytech/polkadot-sdk/issues/6906](https://github.com/paritytech/polkadot-sdk/issues/6906).\n\nSpecifically, this PR removes the use of\n`open_bridge_between_asset_hub_rococo_and_asset_hub_westend`, which is\nno longer relevant for BridgeHubs, as bridges are now created with\ngenesis settings. This function was used in the generic\n`test_dry_run_transfer_across_pk_bridge` macro, which could cause\ncompilation issues when used in other contexts (e.g. fellows repo).\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-24T15:05:36Z",
          "tree_id": "2598c5e554465a1a38bd317b69c21942a0a9174f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7710483541ce273df892c77a6e300aaa2efa1dca"
        },
        "date": 1737735369351,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19148256,
            "range": "± 117629",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19858520,
            "range": "± 141843",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20938779,
            "range": "± 305929",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24758946,
            "range": "± 306548",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53560409,
            "range": "± 565962",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 324506386,
            "range": "± 2687549",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2412081602,
            "range": "± 96594616",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15259724,
            "range": "± 105672",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15217990,
            "range": "± 102013",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15792854,
            "range": "± 196497",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19996642,
            "range": "± 324071",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50838364,
            "range": "± 376307",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 311810507,
            "range": "± 3665689",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2509459781,
            "range": "± 37029672",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "ccd6337f1bfef8ff9da9020fefc25db5a6508da7",
          "message": "sync-templates: enable syncing from stable release patches (#7227)\n\n# Description\n\nWe're unable to sync templates repos with what's in\npolkadot-sdk/templates for stable2412 because the tag which references\nthe release (`polkadot-stable2412`) is missing the Plan.toml file, which\nis needed by PSVM, ran when syncing, to update the templates\ndependencies versions in Cargo.tomls. This PR adds a workflow `patch`\ninput, to enable the workflow to use PSVM with a tag corresponding to a\npatch stable release (e.g. `polkadot-stable2412-1`), which will contain\nthe `Plan.toml` file.\n\n## Integration\n\nThis enables the templates repos update with the contents of latest\nstable2412 release, in terms of polkadot-sdk/templates, which is\nrelevant for getting-started docs.\n\n## Review Notes\n\nThis PR adds a `patch` input for the `misc-sync-templates.yml` workflow,\nwhich if set will be used with `psvm` accordingly to update templates\nrepos' dependencies versions based on upcomming patch stable2412-1,\nwhich contains the `Plan.toml`. The workflow will be ran manually after\nstable2412-1 is out and this work is tracked under #6329 .\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-01-24T16:29:17Z",
          "tree_id": "adb23e6ebd52248f86ca4a252dc335d5510a50d8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ccd6337f1bfef8ff9da9020fefc25db5a6508da7"
        },
        "date": 1737739274216,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17989872,
            "range": "± 96767",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18030381,
            "range": "± 199197",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19599614,
            "range": "± 110390",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23213848,
            "range": "± 172393",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51806173,
            "range": "± 543681",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 305101961,
            "range": "± 3064762",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2282159735,
            "range": "± 114146463",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14811018,
            "range": "± 117986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14879800,
            "range": "± 71008",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15445769,
            "range": "± 141927",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19323631,
            "range": "± 162473",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50181179,
            "range": "± 272971",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 302392649,
            "range": "± 2396137",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2395376793,
            "range": "± 7045855",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "223bd28896cfa7ece1068c70da9f433a08da5554",
          "message": "[pallet-revive] eth-rpc minor fixes (#7325)\n\n- Add option to specify database_url using DATABASE_URL environment\nvariable\n- Add a eth-rpc-tester rust bin that can be used to test deployment\nbefore releasing eth-rpc\n- make evm_block non fallible so that it can return an Ok response for\nolder blocks when the runtime API is not available\n- update cargo.lock to integrate changes from\nhttps://github.com/paritytech/subxt/pull/1904\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-24T16:34:15Z",
          "tree_id": "e4cb0c9a140ced6d3db9021c0ff6c4142e4d2e7a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/223bd28896cfa7ece1068c70da9f433a08da5554"
        },
        "date": 1737741178862,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18766548,
            "range": "± 177375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18843713,
            "range": "± 106638",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20305541,
            "range": "± 147758",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23957624,
            "range": "± 259766",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52919702,
            "range": "± 595440",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 300688644,
            "range": "± 6034894",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2553907638,
            "range": "± 91466770",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14826069,
            "range": "± 131739",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14978212,
            "range": "± 101041",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15178576,
            "range": "± 133191",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19407594,
            "range": "± 163535",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50369022,
            "range": "± 136603",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 295892540,
            "range": "± 2755675",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2385374718,
            "range": "± 19082449",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dcbea60cc7a280f37986f2f815ec3fcff4758be5",
          "message": "revive: Fix compilation of `uapi` crate when `unstable-hostfn` is not set (#7318)\n\nThis regression was introduced with some of the recent PRs. Regression\nfixed and test added.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-24T18:20:09Z",
          "tree_id": "4394d8c652cb545eeade1843252669d35df034aa",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dcbea60cc7a280f37986f2f815ec3fcff4758be5"
        },
        "date": 1737745781134,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18519448,
            "range": "± 181558",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18651701,
            "range": "± 223661",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20117667,
            "range": "± 144822",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23976890,
            "range": "± 371256",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52533035,
            "range": "± 680890",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 310029347,
            "range": "± 2946867",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2344511282,
            "range": "± 25542703",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14875225,
            "range": "± 85450",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14985258,
            "range": "± 103168",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15554057,
            "range": "± 184086",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19432891,
            "range": "± 136395",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50381632,
            "range": "± 421111",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 302236759,
            "range": "± 2468129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2423896450,
            "range": "± 16039820",
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
          "distinct": true,
          "id": "682f8cd22f5bcb76d1b98820b62be49d11deae10",
          "message": "`set_validation_data` register weight manually, do not use refund when the pre dispatch is zero. (#7327)\n\nRelated https://github.com/paritytech/polkadot-sdk/issues/6772\n\nFor an extrinsic, in the post dispatch info, the actual weight is only\nused to reclaim unused weight. If the actual weight is more than the pre\ndispatch weight, then the extrinsic is using the minimum, e.g., the\nweight used registered in pre dispatch.\n\nIn parachain-system pallet one call is `set_validation_data`. This call\nis returning an actual weight, but the pre-dispatch weight is 0.\n\nThis PR fix the disregard of actual weight of `set_validation_data` by\nregistering it manually.",
          "timestamp": "2025-01-25T03:04:45Z",
          "tree_id": "6ae4f129204d616ae2ed51523c5be5354ef1f203",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/682f8cd22f5bcb76d1b98820b62be49d11deae10"
        },
        "date": 1737777381965,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18912233,
            "range": "± 185987",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19422943,
            "range": "± 142004",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21039592,
            "range": "± 214317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24946168,
            "range": "± 360245",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56396812,
            "range": "± 644627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 357928141,
            "range": "± 6654997",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2349414602,
            "range": "± 118998794",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15633374,
            "range": "± 190602",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15761837,
            "range": "± 228526",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16197987,
            "range": "± 113316",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20574813,
            "range": "± 237079",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53506831,
            "range": "± 643230",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 320152054,
            "range": "± 3254518",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2560653792,
            "range": "± 29546124",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c95e49c4c9848c42d5cbfd261de0d22eec9c2bf6",
          "message": "Removed unused dependencies (partial progress) (#7329)\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/6906",
          "timestamp": "2025-01-26T21:18:43Z",
          "tree_id": "943db5ce05aa4d97b3c5e7ea5869fcacd90d22cd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c95e49c4c9848c42d5cbfd261de0d22eec9c2bf6"
        },
        "date": 1737929318171,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17687776,
            "range": "± 110092",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17884789,
            "range": "± 106254",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19274166,
            "range": "± 93613",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22901890,
            "range": "± 141621",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51170624,
            "range": "± 673932",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 305106656,
            "range": "± 2788383",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2169934372,
            "range": "± 49386154",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14455115,
            "range": "± 107618",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14510624,
            "range": "± 157945",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14991726,
            "range": "± 93740",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18915683,
            "range": "± 183350",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49045989,
            "range": "± 412726",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 288318186,
            "range": "± 1627277",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2332909330,
            "range": "± 10469725",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dmitry@markin.tech",
            "name": "Dmitry Markin",
            "username": "dmitry-markin"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "ee30ec723ee22e247014217e48513a2e7690c953",
          "message": "[sync] Let new subscribers know about already connected peers (backward-compatible) (#7344)\n\nRevert https://github.com/paritytech/polkadot-sdk/pull/7011 and replace\nit with a backward-compatible solution suitable for backporting to a\nrelease branch.\n\n### Review notes\nIt's easier to review this PR per commit: the first commit is just a\nrevert, so it's enough to review only the second one, which is almost a\none-liner.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-27T12:29:49Z",
          "tree_id": "baef02a556e3f6c8de2d365edc34fca484ab88c1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ee30ec723ee22e247014217e48513a2e7690c953"
        },
        "date": 1737984545865,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17526977,
            "range": "± 82215",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17777856,
            "range": "± 75036",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19153537,
            "range": "± 111124",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22797406,
            "range": "± 115553",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51207521,
            "range": "± 613208",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 281927743,
            "range": "± 2645068",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2197573218,
            "range": "± 60137326",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14231024,
            "range": "± 158609",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14300696,
            "range": "± 92234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14785617,
            "range": "± 144625",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18637523,
            "range": "± 118355",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48374998,
            "range": "± 216277",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 285183024,
            "range": "± 6294103",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2240049866,
            "range": "± 5150239",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "30932534+EleisonC@users.noreply.github.com",
            "name": "christopher k",
            "username": "EleisonC"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d85147d013e112feae5000816932d0543aee95da",
          "message": "Add development chain-spec file for minimal/parachain templates for Omni Node compatibility (#6529)\n\n# Description\n\nThis PR adds development chain specs for the minimal and parachain\ntemplates.\n[#6334](https://github.com/paritytech/polkadot-sdk/issues/6334)\n\n\n## Integration\n\nThis PR adds development chain specs for the minimal and para chain\ntemplate runtimes, ensuring synchronization with runtime code. It\nupdates zombienet-omni-node.toml, zombinet.toml files to include valid\nchain spec paths, simplifying configuration for zombienet in the\nparachain and minimal template.\n\n## Review Notes\n\n1. Overview of Changes:\n- Added development chain specs for use in the minimal and parachain\ntemplate.\n- Updated zombienet-omni-node.toml and zombinet.toml files in the\nminimal and parachain templates to include paths to the new dev chain\nspecs.\n\n2. Integration Guidance:\n**NB: Follow the templates' READMEs from the polkadot-SDK master branch.\nPlease build the binaries and runtimes based on the polkadot-SDK master\nbranch.**\n- Ensure you have set up your runtimes `parachain-template-runtime` and\n`minimal-template-runtime`\n- Ensure you have installed the nodes required ie\n`parachain-template-node` and `minimal-template-node`\n- Set up [Zombinet](https://paritytech.github.io/zombienet/intro.html)\n- For running the parachains, you will need to install the polkadot\n`cargo install --path polkadot` remember from the polkadot-SDK master\nbranch.\n- Inside the template folders minimal or parachain, run the command to\nstart with `Zombienet with Omni Node`, `Zombienet with\nminimal-template-node` or `Zombienet with parachain-template-node`\n\n*Include your leftover TODOs, if any, here.*\n* [ ] Test the syncing of chain specs with runtime's code.\n\n---------\n\nSigned-off-by: EleisonC <ckalule7@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-01-27T13:01:49Z",
          "tree_id": "bb0c4f4d2dc63c5625bb9d9f83eb433b8841ccfd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d85147d013e112feae5000816932d0543aee95da"
        },
        "date": 1737986904681,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17532848,
            "range": "± 75845",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17840417,
            "range": "± 88265",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19082205,
            "range": "± 188877",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22674127,
            "range": "± 219360",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50550666,
            "range": "± 550212",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 289806509,
            "range": "± 1409440",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2212678529,
            "range": "± 53106097",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14234407,
            "range": "± 424445",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14430245,
            "range": "± 121524",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14872095,
            "range": "± 83990",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18958523,
            "range": "± 114787",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49365651,
            "range": "± 455691",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 283256394,
            "range": "± 1302441",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2252443633,
            "range": "± 14833984",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ludovic.domingues96@gmail.com",
            "name": "Ludovic_Domingues",
            "username": "Krayt78"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "db3ff60b5af2a9017cb968a4727835f3d00340f0",
          "message": "Migrating polkadot-runtime-common slots benchmarking to v2 (#6614)\n\n#Description\nMigrated polkadot-runtime-parachains slots benchmarking to the new\nbenchmarking syntax v2.\nThis is part of #6202\n\n---------\n\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>\nCo-authored-by: seemantaggarwal <32275622+seemantaggarwal@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-27T14:37:00Z",
          "tree_id": "030b3f496c64e746f2206b1a93b3c9c7355e9d32",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/db3ff60b5af2a9017cb968a4727835f3d00340f0"
        },
        "date": 1737991956222,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19039262,
            "range": "± 316116",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19703668,
            "range": "± 248963",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21454237,
            "range": "± 349142",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25634866,
            "range": "± 387429",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 60722428,
            "range": "± 1405052",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 392761009,
            "range": "± 6347830",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2593888993,
            "range": "± 238548391",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15663939,
            "range": "± 171528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16219740,
            "range": "± 147638",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15850638,
            "range": "± 108392",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20266253,
            "range": "± 159007",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54089026,
            "range": "± 1474414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 347010832,
            "range": "± 7133089",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2655118948,
            "range": "± 43590682",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "yrong1997@gmail.com",
            "name": "Ron",
            "username": "yrong"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b30aa3193048d6bbdf21408bd0cc4503010fe3f8",
          "message": "xcm: fix for DenyThenTry Barrier (#7169)\n\nResolves (partially):\nhttps://github.com/paritytech/polkadot-sdk/issues/7148 (see _Problem 1 -\n`ShouldExecute` tuple implementation and `Deny` filter tuple_)\n\nThis PR changes the behavior of `DenyThenTry` from the pattern\n`DenyIfAllMatch` to `DenyIfAnyMatch` for the tuple.\n\nI would expect the latter is the right behavior so make the fix in\nplace, but we can also add a dedicated Impl with the legacy one\nuntouched.\n\n## TODO\n- [x] add unit-test for `DenyReserveTransferToRelayChain`\n- [x] add test and investigate/check `DenyThenTry` as discussed\n[here](https://github.com/paritytech/polkadot-sdk/pull/6838#discussion_r1914553990)\nand update documentation if needed\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-01-27T17:31:05Z",
          "tree_id": "7fa236a2b3152a85bc32cfbff3e0d953f468640f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b30aa3193048d6bbdf21408bd0cc4503010fe3f8"
        },
        "date": 1738002160859,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19057756,
            "range": "± 240746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19385610,
            "range": "± 201914",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21116234,
            "range": "± 181567",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24447847,
            "range": "± 300767",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55453511,
            "range": "± 787528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 344712904,
            "range": "± 6068170",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2602381207,
            "range": "± 91929769",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15305235,
            "range": "± 282369",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15482347,
            "range": "± 130456",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15970249,
            "range": "± 123239",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20275916,
            "range": "± 136522",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52533449,
            "range": "± 681248",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 316827719,
            "range": "± 2646875",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2491906245,
            "range": "± 19724829",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alin@parity.io",
            "name": "Alin Dima",
            "username": "alindima"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "e6aad5b010e630dbac7d86873fef45580630b406",
          "message": "cumulus: bump PARENT_SEARCH_DEPTH and add test for 12-core elastic scaling (#6983)\n\nOn top of https://github.com/paritytech/polkadot-sdk/pull/6757\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/6858 by bumping\nthe `PARENT_SEARCH_DEPTH` constant to a larger value (30) and adds a\nzombienet-sdk test that exercises the 12-core scenario.\n\nThis is a node-side limit that restricts the number of allowed pending\navailability candidates when choosing the parent parablock during\nauthoring.\nThis limit is rather redundant, as the parachain runtime already\nrestricts the unincluded segment length to the configured value in the\n[FixedVelocityConsensusHook](https://github.com/paritytech/polkadot-sdk/blob/88d900afbff7ebe600dfe5e3ee9f87fe52c93d1f/cumulus/pallets/aura-ext/src/consensus_hook.rs#L35)\n(which ideally should be equal to this `PARENT_SEARCH_DEPTH`).\n\nFor 12 cores, a value of 24 should be enough, but I bumped it to 30 to\nhave some extra buffer.\n\nThere are two other potential ways of fixing this:\n- remove this constant altogether, as the parachain runtime already\nmakes those guarantees. Chose not to do this, as it can't hurt to have\nan extra safeguard\n- set this value to be equal to the uninlcuded segment size. This value\nhowever is not exposed to the node-side and would require a new runtime\nAPI, which seems overkill for a redundant check.\n\n---------\n\nCo-authored-by: Javier Viola <javier@parity.io>",
          "timestamp": "2025-01-28T08:32:00Z",
          "tree_id": "04f7690e702bfca2ab234b18b2575a391c81bc75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6aad5b010e630dbac7d86873fef45580630b406"
        },
        "date": 1738056986238,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18873383,
            "range": "± 124739",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19182615,
            "range": "± 193243",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21315067,
            "range": "± 422890",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24621128,
            "range": "± 295102",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 58959309,
            "range": "± 1302129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 370100525,
            "range": "± 6197056",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2687361918,
            "range": "± 146904065",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15282910,
            "range": "± 99839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15775035,
            "range": "± 257471",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15983792,
            "range": "± 155563",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20563914,
            "range": "± 387928",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57486725,
            "range": "± 1332872",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 327580929,
            "range": "± 10257178",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2582603547,
            "range": "± 41273481",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cyrill@parity.io",
            "name": "xermicus",
            "username": "xermicus"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4302f74f7874e6a894578731142a7b310a1449b0",
          "message": "[pallet-revive] pack exceeding syscall arguments into registers (#7319)\n\nThis PR changes how we call runtime API methods with more than 6\narguments: They are no longer spilled to the stack but packed into\nregisters instead. Pointers are 32 bit wide so we can pack two of them\ninto a single 64 bit register. Since we mostly pass pointers, this\ntechnique effectively increases the number of arguments we can pass\nusing the available registers.\n\nTo make this work for `instantiate` too we now pass the code hash and\nthe call data in the same buffer, akin to how the `create` family\nopcodes work in the EVM. The code hash is fixed in size, implying the\nstart of the constructor call data.\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: command-bot <>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-01-28T09:03:21Z",
          "tree_id": "f6c41a646675532e87a6cd6ee428fe7a14feb512",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4302f74f7874e6a894578731142a7b310a1449b0"
        },
        "date": 1738058069481,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18320678,
            "range": "± 251106",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18596163,
            "range": "± 142344",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20100255,
            "range": "± 191673",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23693472,
            "range": "± 265744",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53710714,
            "range": "± 937672",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 310448795,
            "range": "± 2869645",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2445689604,
            "range": "± 52490864",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14980713,
            "range": "± 163517",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14954285,
            "range": "± 145291",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15836339,
            "range": "± 146787",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19879774,
            "range": "± 192166",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51619258,
            "range": "± 2601436",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304231458,
            "range": "± 4066503",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2440813504,
            "range": "± 18495773",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ascjones@gmail.com",
            "name": "Andrew Jones",
            "username": "ascjones"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0b8d744109a3c29d97a28e768a027e3438c8a69a",
          "message": "Implement pallet view function queries (#4722)\n\nCloses #216.\n\nThis PR allows pallets to define a `view_functions` impl like so:\n\n```rust\n#[pallet::view_functions]\nimpl<T: Config> Pallet<T>\nwhere\n\tT::AccountId: From<SomeType1> + SomeAssociation1,\n{\n\t/// Query value no args.\n\tpub fn get_value() -> Option<u32> {\n\t\tSomeValue::<T>::get()\n\t}\n\n\t/// Query value with args.\n\tpub fn get_value_with_arg(key: u32) -> Option<u32> {\n\t\tSomeMap::<T>::get(key)\n\t}\n}\n```\n### `QueryId`\n\nEach view function is uniquely identified by a `QueryId`, which for this\nimplementation is generated by:\n\n```twox_128(pallet_name) ++ twox_128(\"fn_name(fnarg_types) -> return_ty\")```\n\nThe prefix `twox_128(pallet_name)` is the same as the storage prefix for pallets and take into account multiple instances of the same pallet.\n\nThe suffix is generated from the fn type signature so is guaranteed to be unique for that pallet impl. For one of the view fns in the example above it would be `twox_128(\"get_value_with_arg(u32) -> Option<u32>\")`. It is a known limitation that only the type names themselves are taken into account: in the case of type aliases the signature may have the same underlying types but a different id; for generics the concrete types may be different but the signatures will remain the same.\n\nThe existing Runtime `Call` dispatchables are addressed by their concatenated indices `pallet_index ++ call_index`, and the dispatching is handled by the SCALE decoding of the `RuntimeCallEnum::PalletVariant(PalletCallEnum::dispatchable_variant(payload))`. For `view_functions` the runtime/pallet generated enum structure is replaced by implementing the `DispatchQuery` trait on the outer (runtime) scope, dispatching to a pallet based on the id prefix, and the inner (pallet) scope dispatching to the specific function based on the id suffix.\n\nFuture implementations could also modify/extend this scheme and routing to pallet agnostic queries.\n\n### Executing externally\n\nThese view functions can be executed externally via the system runtime api:\n\n```rust\npub trait ViewFunctionsApi<QueryId, Query, QueryResult, Error> where\n\tQueryId: codec::Codec,\n\tQuery: codec::Codec,\n\tQueryResult: codec::Codec,\n\tError: codec::Codec,\n{\n\t/// Execute a view function query.\nfn execute_query(query_id: QueryId, query: Query) -> Result<QueryResult,\nError>;\n}\n```\n### `XCQ`\nCurrently there is work going on by @xlc to implement [`XCQ`](https://github.com/open-web3-stack/XCQ/) which may eventually supersede this work.\n\nIt may be that we still need the fixed function local query dispatching in addition to XCQ, in the same way that we have chain specific runtime dispatchables and XCM.\n\nI have kept this in mind and the high level query API is agnostic to the underlying query dispatch and execution. I am just providing the implementation for the `view_function` definition.\n\n### Metadata\nCurrently I am utilizing the `custom` section of the frame metadata, to avoid modifying the official metadata format until this is standardized.\n\n### vs `runtime_api`\nThere are similarities with `runtime_apis`, some differences being:\n- queries can be defined directly on pallets, so no need for boilerplate declarations and implementations\n- no versioning, the `QueryId` will change if the signature changes. \n- possibility for queries to be executed from smart contracts (see below)\n\n### Calling from contracts\nFuture work would be to add `weight` annotations to the view function queries, and a host function to `pallet_contracts` to allow executing these queries from contracts.\n\n### TODO\n\n- [x] Consistent naming (view functions pallet impl, queries, high level api?)\n- [ ] End to end tests via `runtime_api`\n- [ ] UI tests\n- [x] Mertadata tests\n- [ ] Docs\n\n---------\n\nCo-authored-by: kianenigma <kian@parity.io>\nCo-authored-by: James Wilson <james@jsdw.me>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>\nCo-authored-by: Guillaume Thiolliere <guillaume.thiolliere@parity.io>",
          "timestamp": "2025-01-28T11:52:43Z",
          "tree_id": "8ea2b6aefaeb17d6c3f8fd7ddb6062f79faf963e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0b8d744109a3c29d97a28e768a027e3438c8a69a"
        },
        "date": 1738069524562,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18588949,
            "range": "± 118208",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18716675,
            "range": "± 253287",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20345091,
            "range": "± 75331",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24155868,
            "range": "± 254381",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53220559,
            "range": "± 516115",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 305909091,
            "range": "± 3900803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2291607050,
            "range": "± 152270682",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15594917,
            "range": "± 98181",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15626624,
            "range": "± 159037",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16413063,
            "range": "± 128568",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20436633,
            "range": "± 343335",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52542817,
            "range": "± 738231",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 306787514,
            "range": "± 2778379",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2482806302,
            "range": "± 14526712",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dmitry@markin.tech",
            "name": "Dmitry Markin",
            "username": "dmitry-markin"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "758db43c524605bd81c39777de6c402ee5e0a5e3",
          "message": "[net/libp2p] Use raw `Identify` observed addresses to discover external addresses (#7338)\n\nInstead of using libp2p-provided external address candidates,\nsusceptible to address translation issues, use litep2p-backend approach\nbased on confirming addresses observed by multiple peers as external.\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/7207.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-28T15:01:36Z",
          "tree_id": "52e0ef38f16211c70bc3826722aff7b7754ec0b7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/758db43c524605bd81c39777de6c402ee5e0a5e3"
        },
        "date": 1738079551530,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18185303,
            "range": "± 140152",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18407208,
            "range": "± 161341",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19980357,
            "range": "± 77900",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23783758,
            "range": "± 304670",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53081543,
            "range": "± 340450",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 310321415,
            "range": "± 3793622",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2597584959,
            "range": "± 124083946",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14794710,
            "range": "± 184057",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15117423,
            "range": "± 136474",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15714765,
            "range": "± 149885",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19733332,
            "range": "± 131143",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51319725,
            "range": "± 535754",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 310561285,
            "range": "± 3073102",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2451232713,
            "range": "± 26310496",
            "unit": "ns/iter"
          }
        ]
      },
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
          "distinct": true,
          "id": "0dcb580e3012c2ee61a91390c5f69451a096a820",
          "message": "ci: fix workflow permissions (#7366)\n\ncc https://github.com/paritytech/ci_cd/issues/1101",
          "timestamp": "2025-01-28T17:12:18Z",
          "tree_id": "ffb6f41b530b33a3d146c91f5a7c8236c521324c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0dcb580e3012c2ee61a91390c5f69451a096a820"
        },
        "date": 1738087358252,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19919252,
            "range": "± 1005983",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22642536,
            "range": "± 512734",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 24074646,
            "range": "± 1421374",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 29638504,
            "range": "± 715851",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 67674746,
            "range": "± 1435350",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 481552871,
            "range": "± 11163632",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2725405505,
            "range": "± 66336200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17269628,
            "range": "± 217682",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17541415,
            "range": "± 276150",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17911403,
            "range": "± 292062",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23375408,
            "range": "± 252263",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 61169543,
            "range": "± 2173096",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 427448211,
            "range": "± 5748993",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2989494934,
            "range": "± 117995070",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1177472+mordamax@users.noreply.github.com",
            "name": "Maksym H",
            "username": "mordamax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9ab00b15b2e98d008822fe6addaaad148f513bea",
          "message": "remove old bench & revert the frame-weight-template (#7362)\n\n- remove old bench from cmd.py and left alias for backward compatibility\n- reverted the frame-wight-template as the problem was that it umbrella\ntemplate wasn't picked correctly in the old benchmarks, in\nframe-omni-bench it correctly identifies the dependencies and uses\ncorrect template\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-28T19:32:46Z",
          "tree_id": "32c65a77194bfad2b966d1d9371feba6b395693c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9ab00b15b2e98d008822fe6addaaad148f513bea"
        },
        "date": 1738096035807,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21080392,
            "range": "± 543286",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20179438,
            "range": "± 501048",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22297576,
            "range": "± 829511",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26064828,
            "range": "± 497345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63050651,
            "range": "± 2563665",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 457473568,
            "range": "± 15678258",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2818007918,
            "range": "± 209595760",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16988780,
            "range": "± 633541",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17085191,
            "range": "± 514925",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 18366026,
            "range": "± 633296",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22719978,
            "range": "± 408447",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 58100354,
            "range": "± 1079915",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 396459112,
            "range": "± 12313142",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2765952434,
            "range": "± 62285113",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "v@lery.dev",
            "name": "Valery Gantchev",
            "username": "vgantchev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f373af0d1c1e296c1b07486dd74710b40089250e",
          "message": "Use checked math in frame-balances named_reserve (#7365)\n\nThis PR modifies `named_reserve()` in frame-balances to use checked math\ninstead of defensive saturating math.\n\nThe use of saturating math relies on the assumption that the sum of the\nvalues will always fit in `u128::MAX`. However, there is nothing\npreventing the implementing pallet from passing a larger value which\noverflows. This can happen if the implementing pallet does not validate\nuser input and instead relies on `named_reserve()` to return an error\n(this saves an additional read)\n\nThis is not a security concern, as the method will subsequently return\nan error thanks to `<Self as ReservableCurrency<_>>::reserve(who,\nvalue)?;`. However, the `defensive_saturating_add` will panic in\n`--all-features`, creating false positive crashes in fuzzing operations.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-29T09:11:04Z",
          "tree_id": "9f39b6fef5a83298029fc92ace7de587549fff02",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f373af0d1c1e296c1b07486dd74710b40089250e"
        },
        "date": 1738145055878,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18383998,
            "range": "± 174513",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18855389,
            "range": "± 200752",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20670575,
            "range": "± 146587",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24183981,
            "range": "± 747237",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53759449,
            "range": "± 860246",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 332367385,
            "range": "± 10252029",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2476034886,
            "range": "± 20377312",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15297137,
            "range": "± 155959",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15407758,
            "range": "± 244170",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15851092,
            "range": "± 204035",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20563324,
            "range": "± 379446",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52298041,
            "range": "± 605129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 325274705,
            "range": "± 6822607",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2531492282,
            "range": "± 33491179",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ludovic.domingues96@gmail.com",
            "name": "Ludovic_Domingues",
            "username": "Krayt78"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "57f0b95978a0eed283cc894724a4ba1c5d4ca258",
          "message": "Migrating cumulus-pallet-session-benchmarking to Benchmarking V2  (#6564)\n\n# Description\n\nMigrating cumulus-pallet-session-benchmarking to the new benchmarking\nsyntax v2.\nThis is a part of #6202\n\n---------\n\nCo-authored-by: seemantaggarwal <32275622+seemantaggarwal@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-29T11:13:56Z",
          "tree_id": "a9541d14e5745408a36a5cf4acedc08ab3370a9f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/57f0b95978a0eed283cc894724a4ba1c5d4ca258"
        },
        "date": 1738152570583,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18196840,
            "range": "± 119271",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18209776,
            "range": "± 97156",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19924752,
            "range": "± 136784",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23827875,
            "range": "± 191723",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54684366,
            "range": "± 603665",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 323847988,
            "range": "± 3572691",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2484463672,
            "range": "± 83498964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14995104,
            "range": "± 154582",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15144572,
            "range": "± 136593",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15535801,
            "range": "± 85181",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19948983,
            "range": "± 139430",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52405835,
            "range": "± 871079",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 312976799,
            "range": "± 1914813",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2454262599,
            "range": "± 44887496",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bkontur@gmail.com",
            "name": "Branislav Kontur",
            "username": "bkontur"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ada12be652a4fa8f60fc54e8cfe9ca81e09ad28b",
          "message": "Bridges small nits/improvements (#7383)\n\nThis PR contains small fixes and backwards compatibility issues\nidentified during work on the larger PR:\nhttps://github.com/paritytech/polkadot-sdk/issues/6906.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-29T14:33:08Z",
          "tree_id": "b81dbda9d653a05c991f6e994b685def500719ac",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ada12be652a4fa8f60fc54e8cfe9ca81e09ad28b"
        },
        "date": 1738164251861,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19988279,
            "range": "± 284652",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20358986,
            "range": "± 256375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21504455,
            "range": "± 405024",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25358822,
            "range": "± 1233829",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55348457,
            "range": "± 674909",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 361470012,
            "range": "± 6635033",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2382522018,
            "range": "± 23599191",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16026454,
            "range": "± 272515",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16168098,
            "range": "± 277781",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16989771,
            "range": "± 264112",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21169823,
            "range": "± 358849",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54397430,
            "range": "± 767347",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 335382456,
            "range": "± 5211204",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2583129549,
            "range": "± 30927017",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "manuel.mauro@protonmail.com",
            "name": "Manuel Mauro",
            "username": "manuelmauro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "80e30ec3cdccae8e9099bd67840ff8737b043496",
          "message": "Add support for feature `pallet_balances/insecure_zero_ed` in benchmarks and testing (#7379)\n\n# Description\n\nCurrently benchmarks and tests on pallet_balances would fail when the\nfeature insecure_zero_ed is enabled. This PR allows to run such\nbenchmark and tests keeping into account the fact that accounts would\nnot be deleted when their balance goes below a threshold.\n\n## Integration\n\n*In depth notes about how this PR should be integrated by downstream\nprojects. This part is mandatory, and should be\nreviewed by reviewers, if the PR does NOT have the `R0-Silent` label. In\ncase of a `R0-Silent`, it can be ignored.*\n\n## Review Notes\n\n*In depth notes about the **implementation** details of your PR. This\nshould be the main guide for reviewers to\nunderstand your approach and effectively review it. If too long, use\n\n[`<details>`](https://developer.mozilla.org/en-US/docs/Web/HTML/Element/details)*.\n\n*Imagine that someone who is depending on the old code wants to\nintegrate your new code and the only information that\nthey get is this section. It helps to include example usage and default\nvalue here, with a `diff` code-block to show\npossibly integration.*\n\n*Include your leftover TODOs, if any, here.*\n\n# Checklist\n\n* [x] My PR includes a detailed description as outlined in the\n\"Description\" and its two subsections above.\n* [x] My PR follows the [labeling requirements](\n\nhttps://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process\n) of this project (at minimum one label for `T` required)\n* External contributors: ask maintainers to put the right label on your\nPR.\n* [x] I have made corresponding changes to the documentation (if\napplicable)\n* [x] I have added tests that prove my fix is effective or that my\nfeature works (if applicable)\n\nYou can remove the \"Checklist\" section once all have been checked. Thank\nyou for your contribution!\n\n✄\n-----------------------------------------------------------------------------\n\n---------\n\nCo-authored-by: Rodrigo Quelhas <rodrigo_quelhas@outlook.pt>",
          "timestamp": "2025-01-29T22:11:52Z",
          "tree_id": "2c966a8913ba0f6518561501ee0c5ee85dabe1d1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/80e30ec3cdccae8e9099bd67840ff8737b043496"
        },
        "date": 1738191882110,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18491560,
            "range": "± 294221",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18591258,
            "range": "± 176414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20176753,
            "range": "± 161509",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24236155,
            "range": "± 275971",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56281190,
            "range": "± 1004761",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 363778697,
            "range": "± 4949512",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2384941034,
            "range": "± 207063680",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15202601,
            "range": "± 274578",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15258162,
            "range": "± 156030",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15865030,
            "range": "± 170177",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20050148,
            "range": "± 143554",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53145612,
            "range": "± 1085925",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 326434516,
            "range": "± 6641801",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2590711174,
            "range": "± 20205537",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1177472+mordamax@users.noreply.github.com",
            "name": "Maksym H",
            "username": "mordamax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e9e425175e9db6b84f3ed2fd96f0326d798c25b2",
          "message": "Improvements for Weekly bench (#7390)\n\n- added 3 links for subweight comparison - now, ~1 month ago release, ~3\nmonth ago release tag\n- added `--3way --ours` flags for `git apply` to resolve potential\nconflict\n- stick to the weekly branch from the start until the end, to prevent\nrace condition with conflicts",
          "timestamp": "2025-01-30T11:46:36Z",
          "tree_id": "6c09ea8ab8d141712e8be35575592597e75de84d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e9e425175e9db6b84f3ed2fd96f0326d798c25b2"
        },
        "date": 1738240636708,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18759515,
            "range": "± 171491",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18821252,
            "range": "± 133765",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20424271,
            "range": "± 132917",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24576631,
            "range": "± 240502",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56535335,
            "range": "± 1548966",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 335812232,
            "range": "± 11496025",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2367400086,
            "range": "± 36351349",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15211683,
            "range": "± 149199",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15207600,
            "range": "± 198150",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15851553,
            "range": "± 91666",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19931194,
            "range": "± 233915",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52676211,
            "range": "± 897189",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 311842346,
            "range": "± 4854517",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2536761564,
            "range": "± 14675189",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "59443568+sw10pa@users.noreply.github.com",
            "name": "Stephane Gurgenidze",
            "username": "sw10pa"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "48f69cca47fcb95f65b5a4e4165e9c3bf359e82b",
          "message": "malus-collator: implement malicious collator submitting same collation to all backing groups (#6924)\n\n## Issues\n- [[#5049] Elastic scaling: zombienet\ntests](https://github.com/paritytech/polkadot-sdk/issues/5049)\n- [[#4526] Add zombienet tests for malicious\ncollators](https://github.com/paritytech/polkadot-sdk/issues/4526)\n\n## Description\nModified the undying collator to include a malus mode, in which it\nsubmits the same collation to all assigned backing groups.\n\n## TODO\n* [X] Implement malicious collator that submits the same collation to\nall backing groups;\n* [X] Avoid the core index check in the collation generation subsystem:\nhttps://github.com/paritytech/polkadot-sdk/blob/master/polkadot/node/collation-generation/src/lib.rs#L552-L553;\n* [X] Resolve the mismatch between the descriptor and the commitments\ncore index: https://github.com/paritytech/polkadot-sdk/pull/7104\n* [X] Implement `duplicate_collations` test with zombienet-sdk;\n* [X] Add PRdoc.",
          "timestamp": "2025-01-30T12:42:17Z",
          "tree_id": "306c378f0b701fbf00ec994741b6065288d97bdf",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48f69cca47fcb95f65b5a4e4165e9c3bf359e82b"
        },
        "date": 1738244214292,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18614231,
            "range": "± 203435",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18738877,
            "range": "± 221722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20483864,
            "range": "± 175232",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24406252,
            "range": "± 234607",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56063792,
            "range": "± 872454",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 348090252,
            "range": "± 7000361",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2722067233,
            "range": "± 51565476",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15604985,
            "range": "± 163024",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15776732,
            "range": "± 116346",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16425221,
            "range": "± 104093",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20739460,
            "range": "± 217008",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53113203,
            "range": "± 2068108",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 320590045,
            "range": "± 3215370",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2518022768,
            "range": "± 19308525",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "write@reusable.software",
            "name": "ordian",
            "username": "ordian"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0d35be7bc80a9c375db52585866601f4294b1e3d",
          "message": "fix pre-dispatch PoV underweight for ParasInherent (#7378)\n\nThis should fix the error log related to PoV pre-dispatch weight being\nlower than post-dispatch for `ParasInherent`:\n```\nERROR tokio-runtime-worker runtime::frame-support: Post dispatch weight is greater than pre dispatch weight. Pre dispatch weight may underestimating the actual weight. Greater post dispatch weight components are ignored.\n                                        Pre dispatch weight: Weight { ref_time: 47793353978, proof_size: 1019 },\n                                        Post dispatch weight: Weight { ref_time: 5030321719, proof_size: 135395 }\n```\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-01-30T14:15:12Z",
          "tree_id": "299d8defabb798b6e6ecf908a253bf8aac6e41f2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d35be7bc80a9c375db52585866601f4294b1e3d"
        },
        "date": 1738249856580,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17719376,
            "range": "± 284124",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17819333,
            "range": "± 120561",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19491074,
            "range": "± 64236",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23176207,
            "range": "± 148340",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51500221,
            "range": "± 639735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 283377191,
            "range": "± 2338955",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2161390029,
            "range": "± 104197880",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14424634,
            "range": "± 167620",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14640386,
            "range": "± 98097",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15230192,
            "range": "± 111105",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19014124,
            "range": "± 137495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49043193,
            "range": "± 396536",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 287688955,
            "range": "± 1032827",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2274671651,
            "range": "± 11386467",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "698d9ae5b32785d3a5a55b770e973bbdb59ad271",
          "message": "[Backport] Version bumps from stable2412-1 + prdocs reorg (#7401)\n\nThis PR backports regular version bumps and prdoc reorganization from\nstable release branch back to master",
          "timestamp": "2025-01-31T09:25:44Z",
          "tree_id": "7f4bfd29059cb41939b169de88b552717b7fe607",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/698d9ae5b32785d3a5a55b770e973bbdb59ad271"
        },
        "date": 1738318525252,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17571216,
            "range": "± 66281",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17820416,
            "range": "± 116090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19391714,
            "range": "± 140495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23180450,
            "range": "± 220676",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51606746,
            "range": "± 517856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 281643876,
            "range": "± 2687722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2209798017,
            "range": "± 14023580",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14625407,
            "range": "± 98757",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14768972,
            "range": "± 151965",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15241097,
            "range": "± 68287",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19128589,
            "range": "± 86506",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49546038,
            "range": "± 342320",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 290224484,
            "range": "± 2559268",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2337080112,
            "range": "± 20426357",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "23833ccee9c1c2062123e60901e6dc1076e0697d",
          "message": "Remove warnings by cleaning up the `Cargo.toml` (#7416)",
          "timestamp": "2025-01-31T22:47:28Z",
          "tree_id": "598b03b65537f6ebea91fc5510ac5377f25177ea",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/23833ccee9c1c2062123e60901e6dc1076e0697d"
        },
        "date": 1738366713975,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18298571,
            "range": "± 42834",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18599005,
            "range": "± 200978",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20185896,
            "range": "± 100699",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24175254,
            "range": "± 260492",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56491099,
            "range": "± 619664",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 364679788,
            "range": "± 5032156",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2691488175,
            "range": "± 68717654",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15063679,
            "range": "± 103753",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15313021,
            "range": "± 132087",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15894250,
            "range": "± 74578",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20556891,
            "range": "± 288447",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51986149,
            "range": "± 761601",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 316657391,
            "range": "± 3597564",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2555486379,
            "range": "± 31068912",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alin@parity.io",
            "name": "Alin Dima",
            "username": "alindima"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4cd07c56378291fddb9fceab3b508cf99034126a",
          "message": "deprecate AsyncBackingParams (#7254)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/5079.\n\nRemoves all usage of the static async backing params, replacing them\nwith dynamically computed equivalent values (based on the claim queue\nand scheduling lookahead).\n\nAdds a new runtime API for querying the scheduling lookahead value. If\nnot present, falls back to 3 (the default value that is backwards\ncompatible with values we have on production networks for\nallowed_ancestry_len)\n\nAlso resolves most of\nhttps://github.com/paritytech/polkadot-sdk/issues/4447, removing code\nthat handles async backing not yet being enabled.\nWhile doing this, I removed the support for collation protocol version 1\non collators, as it only worked for leaves not supporting async backing\n(which are none).\nI also unhooked the legacy v1 statement-distribution (for the same\nreason as above). That subsystem is basically dead code now, so I had to\nremove some of its tests as they would no longer pass (since the\nsubsystem no longer sends messages to the legacy variant). I did not\nremove the entire legacy subsystem yet, as that would pollute this PR\ntoo much. We can remove the entire v1 and v2 validation protocols in a\nfollow up PR.\n\nIn another PR: remove test files with names `prospective_parachains`\n(it'd pollute this PR if we do now)\n\nTODO:\n- [x] add deprecation warnings\n- [x] prdoc\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-03T09:01:15Z",
          "tree_id": "59d7a1f1357bdfdea64825e74e9e190c719369b1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4cd07c56378291fddb9fceab3b508cf99034126a"
        },
        "date": 1738576654321,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19161830,
            "range": "± 225404",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19211005,
            "range": "± 248252",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20595357,
            "range": "± 228179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25249681,
            "range": "± 181885",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57348263,
            "range": "± 1163996",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 363240048,
            "range": "± 7752350",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2516448991,
            "range": "± 174750024",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15187923,
            "range": "± 190366",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15234033,
            "range": "± 183770",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15559085,
            "range": "± 209272",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20313902,
            "range": "± 152045",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52335229,
            "range": "± 1105592",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 334219922,
            "range": "± 3983897",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2525541799,
            "range": "± 19920480",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1177472+mordamax@users.noreply.github.com",
            "name": "Maksym H",
            "username": "mordamax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4f4f6f82bd54748231384128c8d8b37e51bd8367",
          "message": "add allow(dead_code) to substrate weight templates (#7408)\n\naddress failed CI after full regeneration\n\nExample https://github.com/paritytech/polkadot-sdk/pull/7406\nFailed CI\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/13070646240\n\nMonkey-patched weights which have been overridden by automation\n\n![image](https://github.com/user-attachments/assets/ecf69173-f4dd-4113-a319-4f29d779ecae)",
          "timestamp": "2025-02-03T12:25:21Z",
          "tree_id": "f05c358e926a472916d44609a3427998ab630799",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4f4f6f82bd54748231384128c8d8b37e51bd8367"
        },
        "date": 1738588436370,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18254533,
            "range": "± 69172",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18444588,
            "range": "± 121746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20089489,
            "range": "± 118387",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23710658,
            "range": "± 184739",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53123898,
            "range": "± 837581",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 339460004,
            "range": "± 5087973",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2468318970,
            "range": "± 167253844",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14922583,
            "range": "± 85090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14803702,
            "range": "± 104048",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15413288,
            "range": "± 57396",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19751385,
            "range": "± 183172",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51061139,
            "range": "± 448223",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 314036201,
            "range": "± 3268249",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2299951396,
            "range": "± 81687971",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cyrill@parity.io",
            "name": "xermicus",
            "username": "xermicus"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "274a781e8ca1a9432c7ec87593bd93214abbff50",
          "message": "[pallet-revive] do not trap the caller on instantiations with duplicate contracts (#7414)\n\nThis PR changes the behavior of `instantiate` when the resulting\ncontract address already exists (because the caller tried to instantiate\nthe same contract with the same salt multiple times): Instead of\ntrapping the caller, return an error code.\n\nSolidity allows `catch`ing this, which doesn't work if we are trapping\nthe caller. For example, the change makes the following snippet work:\n\n```Solidity\ntry new Foo{salt: hex\"00\"}() returns (Foo) {\n    // Instantiation was successful (contract address was free and constructor did not revert)\n} catch {\n    // This branch is expected to be taken if the instantiation failed because of a duplicate salt\n}\n```\n\n`revive` PR: https://github.com/paritytech/revive/pull/188\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-03T15:59:39Z",
          "tree_id": "bba40945cfb044e8b8752fd3493329ea7900a423",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/274a781e8ca1a9432c7ec87593bd93214abbff50"
        },
        "date": 1738601329122,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17108468,
            "range": "± 200699",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17352423,
            "range": "± 83027",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18676278,
            "range": "± 81456",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22253857,
            "range": "± 78032",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50033011,
            "range": "± 565403",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 299412604,
            "range": "± 3125253",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2166004946,
            "range": "± 68346575",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 13854550,
            "range": "± 83076",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 13972928,
            "range": "± 121053",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14442676,
            "range": "± 73897",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18202123,
            "range": "± 200356",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 46594466,
            "range": "± 421451",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 276843280,
            "range": "± 3102891",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2212415722,
            "range": "± 9351694",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "0e386bed464b37b20c3e0c3d27b7f92d1a476a88",
          "message": "fix(sync-templates): keep parachain-template's workspace Cargo.toml (#7439)\n\n# Description\n\nAnother small fix for sync-templates. We're copying the `polkadot-sdk`'s\n`parachain-template` files (including the `parachain-template-docs`'s\nCargo.toml) to the directory where we're creating the workspace with all\n`parachain-template` members crates, and workspace's toml. The error is\nthat in this directory for the workspace we first create the workspace's\nCargo.toml, and then copy the files of the `polkadot-sdk`'s\n`parachain-template`, including the `Cargo.toml` of the\n`parachain-template-docs` crate, which overwrites the workspace\nCargo.toml. In the end we delete the `Cargo.toml` (which we assume it is\nof the `parachain-template-docs` crate), forgetting that previously\nthere should've been a workspace Cargo.toml, which should still be kept\nand committed to the template's repository.\n\nThe error happens here:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/13111697690/job/36577834127\n\n## Integration\n\nN/A\n\n## Review Notes\n\nOnce again, merging this into master requires re-running sync templates\nbased on latest version on master. Hopefully this will be the last issue\nrelated to the workflow itself.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-02-04T09:35:22Z",
          "tree_id": "6d46731b2cc2fe15bf064c61d358d3768accade8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0e386bed464b37b20c3e0c3d27b7f92d1a476a88"
        },
        "date": 1738665036745,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17183604,
            "range": "± 141405",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17508243,
            "range": "± 98384",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19108828,
            "range": "± 237448",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22694578,
            "range": "± 190960",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51265417,
            "range": "± 541930",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 315057053,
            "range": "± 3231663",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2505053867,
            "range": "± 54592651",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14019224,
            "range": "± 68717",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14239358,
            "range": "± 82562",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14697149,
            "range": "± 57769",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19033573,
            "range": "± 94061",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47974483,
            "range": "± 291013",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 284807490,
            "range": "± 3745939",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2297630052,
            "range": "± 23722380",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "d6aa157888902fdfcee3995e5ff209847977c696",
          "message": "Fix Message codec indexes (#7437)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/7400",
          "timestamp": "2025-02-04T10:02:45Z",
          "tree_id": "b6bb1dd7d23dfab61aedc83640af615e7f5a7e60",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6aa157888902fdfcee3995e5ff209847977c696"
        },
        "date": 1738666332273,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18995345,
            "range": "± 350626",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19098306,
            "range": "± 251977",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21083338,
            "range": "± 226485",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24976534,
            "range": "± 513456",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57040156,
            "range": "± 994729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 336804204,
            "range": "± 5016611",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2647136191,
            "range": "± 100589986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15201626,
            "range": "± 92262",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15502481,
            "range": "± 424301",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16237478,
            "range": "± 242045",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20718887,
            "range": "± 356345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52981474,
            "range": "± 432222",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 326760797,
            "range": "± 4835449",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2574330732,
            "range": "± 24969898",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "aa42debebaf3bf8e6979497256bc1fbad2db0e11",
          "message": "`fatxpool`: do not use individual transaction listeners (#7316)\n\n#### Description\nDuring 2s block investigation it turned out that\n[ForkAwareTxPool::register_listeners](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/client/transaction-pool/src/fork_aware_txpool/fork_aware_txpool.rs#L1036)\ncall takes significant amount of time.\n```\nregister_listeners: at HashAndNumber { number: 12, hash: 0xe9a1...0b1d2 } took 200.041933ms\nregister_listeners: at HashAndNumber { number: 13, hash: 0x5eb8...a87c6 } took 264.487414ms\nregister_listeners: at HashAndNumber { number: 14, hash: 0x30cb...2e6ec } took 340.525566ms\nregister_listeners: at HashAndNumber { number: 15, hash: 0x0450...4f05c } took 405.686659ms\nregister_listeners: at HashAndNumber { number: 16, hash: 0xfa6f...16c20 } took 477.977836ms\nregister_listeners: at HashAndNumber { number: 17, hash: 0x5474...5d0c1 } took 483.046029ms\nregister_listeners: at HashAndNumber { number: 18, hash: 0x3ca5...37b78 } took 482.715468ms\nregister_listeners: at HashAndNumber { number: 19, hash: 0xbfcc...df254 } took 484.206999ms\nregister_listeners: at HashAndNumber { number: 20, hash: 0xd748...7f027 } took 414.635236ms\nregister_listeners: at HashAndNumber { number: 21, hash: 0x2baa...f66b5 } took 418.015897ms\nregister_listeners: at HashAndNumber { number: 22, hash: 0x5f1d...282b5 } took 423.342397ms\nregister_listeners: at HashAndNumber { number: 23, hash: 0x7a18...f2d03 } took 472.742939ms\nregister_listeners: at HashAndNumber { number: 24, hash: 0xc381...3fd07 } took 489.625557ms\n```\n\nThis PR implements the idea outlined in #7071. Instead of having a\nseparate listener for every transaction in each view, we now use a\nsingle stream of aggregated events per view, with each stream providing\nevents for all transactions in that view. Each event is represented as a\ntuple: (transaction-hash, transaction-status). This significantly reduce\nthe time required for `maintain`.\n\n#### Review Notes\n- single aggregated stream, provided by the individual view delivers\nevents in form of `(transaction-hash, transaction-status)`,\n- `MultiViewListener` now has a task. This task is responsible for:\n- polling the stream map (which consists of individual view's aggregated\nstreams) and the `controller_receiver` which provides side-channel\n[commands](https://github.com/paritytech/polkadot-sdk/blob/2b18e080cfcd6b56ee638c729f891154e566e52e/substrate/client/transaction-pool/src/fork_aware_txpool/multi_view_listener.rs#L68-L95)\n(like `AddView` or `FinalizeTransaction`) sent from the _transaction\npool_.\n- dispatching individual transaction statuses and control commands into\nthe external (created via API, e.g. over RPC) listeners of individual\ntransactions,\n- external listener is responsible for status handling _logic_ (e.g.\ndeduplication of events, or ignoring some of them) and triggering\nstatuses to external world (_this was not changed_).\n- level of debug messages was adjusted (per-tx messages shall be\n_trace_),\n\nCloses #7071\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-02-04T12:18:04Z",
          "tree_id": "1f096529c55a66eabfa7b44d957e89929ba57029",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa42debebaf3bf8e6979497256bc1fbad2db0e11"
        },
        "date": 1738675209380,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18332381,
            "range": "± 163011",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18444365,
            "range": "± 120891",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20104777,
            "range": "± 157369",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23318108,
            "range": "± 414724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55176838,
            "range": "± 1001803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 348842046,
            "range": "± 5447214",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2560968954,
            "range": "± 195609442",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14612539,
            "range": "± 132285",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14717740,
            "range": "± 127478",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15209345,
            "range": "± 126240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19756409,
            "range": "± 139850",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49312412,
            "range": "± 561656",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 292119992,
            "range": "± 8291465",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2403101085,
            "range": "± 26711463",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "claravanstaden64@gmail.com",
            "name": "Clara van Staden",
            "username": "claravanstaden"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8834a9bf7dcb1e5a7498b7148787c544136609b2",
          "message": "Snowbridge: Remove fee amount check from tests (#7436)\n\nRemove the specific fee amount checks in integration tests, since it\nchanges every time weights are regenerated.",
          "timestamp": "2025-02-04T12:55:44Z",
          "tree_id": "0190e4e4cae113a163a5e44b5cf5abe61e883076",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8834a9bf7dcb1e5a7498b7148787c544136609b2"
        },
        "date": 1738676694050,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19111990,
            "range": "± 157497",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19479525,
            "range": "± 541876",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21385708,
            "range": "± 634502",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25570026,
            "range": "± 483730",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61971032,
            "range": "± 1807242",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 371750805,
            "range": "± 10093760",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2476047504,
            "range": "± 247584347",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15204729,
            "range": "± 264706",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15190263,
            "range": "± 211046",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16014681,
            "range": "± 312206",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20237694,
            "range": "± 290647",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51993638,
            "range": "± 921961",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 326780004,
            "range": "± 7124765",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2554076057,
            "range": "± 16711381",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alexandre.balde@parity.io",
            "name": "Alexandre R. Baldé",
            "username": "rockbmb"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a883475944b26c24704df7f0ff329121e396a6bb",
          "message": "Add missing events to nomination pool extrinsincs (#7377)\n\nFound via\nhttps://github.com/open-web3-stack/polkadot-ecosystem-tests/pull/165.\n\nCloses #7370 .\n\n# Description\n\nSome extrinsics from `pallet_nomination_pools` were not emitting events:\n* `set_configs`\n* `set_claim_permission`\n* `set_metadata`\n* `chill`\n* `nominate`\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>",
          "timestamp": "2025-02-04T16:04:23Z",
          "tree_id": "aae1d07ef8a941d519f3fea9bdacc4ad5bcd5219",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a883475944b26c24704df7f0ff329121e396a6bb"
        },
        "date": 1738687947554,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17696986,
            "range": "± 134575",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18331507,
            "range": "± 198489",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19815581,
            "range": "± 226253",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23632211,
            "range": "± 438442",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56400269,
            "range": "± 1135508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 329356543,
            "range": "± 9564315",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2526506870,
            "range": "± 112734335",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14797460,
            "range": "± 138988",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14954809,
            "range": "± 152639",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15320011,
            "range": "± 242542",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19644431,
            "range": "± 198215",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51138744,
            "range": "± 489762",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 306157824,
            "range": "± 4630452",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2501599884,
            "range": "± 15188473",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "4c28354be1f5eec875d116a1ab7d9429023ef83b",
          "message": "revive: Include immutable storage deposit into the contracts `storage_base_deposit` (#7230)\n\nThis PR is centered around a main fix regarding the base deposit and a\nbunch of drive by or related fixtures that make sense to resolve in one\ngo. It could be broken down more but I am constantly rebasing this PR\nand would appreciate getting those fixes in as-one.\n\n**This adds a multi block migration to Westend AssetHub that wipes the\npallet state clean. This is necessary because of the changes to the\n`ContractInfo` storage item. It will not delete the child storage\nthough. This will leave a tiny bit of garbage behind but won't cause any\nproblems. They will just be orphaned.**\n\n## Record the deposit for immutable data into the `storage_base_deposit`\n\nThe `storage_base_deposit` are all the deposit a contract has to pay for\nexisting. It included the deposit for its own metadata and a deposit\nproportional (< 1.0x) to the size of its code. However, the immutable\ncode size was not recorded there. This would lead to the situation where\non terminate this portion wouldn't be refunded staying locked into the\ncontract. It would also make the calculation of the deposit changes on\n`set_code_hash` more complicated when it updates the immutable data (to\nbe done in #6985). Reason is because it didn't know how much was payed\nbefore since the storage prices could have changed in the mean time.\n\nIn order for this solution to work I needed to delay the deposit\ncalculation for a new contract for after the contract is done executing\nis constructor as only then we know the immutable data size. Before, we\njust charged this eagerly in `charge_instantiate` before we execute the\nconstructor. Now, we merely send the ED as free balance before the\nconstructor in order to create the account. After the constructor is\ndone we calculate the contract base deposit and charge it. This will\nmake `set_code_hash` much easier to implement.\n\nAs a side effect it is now legal to call `set_immutable_data` multiple\ntimes per constructor (even though I see no reason to do so). It simply\noverrides the immutable data with the new value. The deposit accounting\nwill be done after the constructor returns (as mentioned above) instead\nof when setting the immutable data.\n\n## Don't pre-charge for reading immutable data\n\nI noticed that we were pre-charging weight for the max allowable\nimmutable data when reading those values and then refunding after read.\nThis is not necessary as we know its length without reading the storage\nas we store it out of band in contract metadata. This makes reading it\nfree. Less pre-charging less problems.\n\n## Remove delegate locking\n\nFixes #7092\n\nThis is also in the spirit of making #6985 easier to implement. The\nlocking complicates `set_code_hash` as we might need to block settings\nthe code hash when locks exist. Check #7092 for further rationale.\n\n## Enforce \"no terminate in constructor\" eagerly\n\nWe used to enforce this rule after the contract execution returned. Now\nwe error out early in the host call. This makes it easier to be sure to\nargue that a contract info still exists (wasn't terminated) when a\nconstructor successfully returns. All around this his just much simpler\nthan dealing this check.\n\n## Moved refcount functions to `CodeInfo`\n\nThey never really made sense to exist on `Stack`. But now with the\nlocking gone this makes even less sense. The refcount is stored inside\n`CodeInfo` to lets just move them there.\n\n## Set `CodeHashLockupDepositPercent` for test runtime\n\nThe test runtime was setting `CodeHashLockupDepositPercent` to zero.\nThis was trivializing many code paths and excluded them from testing. I\nset it to `30%` which is our default value and fixed up all the tests\nthat broke. This should give us confidence that the lockup doeposit\ncollections properly works.\n\n## Reworked the `MockExecutable` to have both a `deploy` and a `call`\nentry point\n\nThis type used for testing could only have either entry points but not\nboth. In order to fix the `immutable_data_set_overrides` I needed to a\nnew function `add_both` to `MockExecutable` that allows to have both\nentry points. Make sure to make use of it in the future :)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-02-04T21:20:20Z",
          "tree_id": "41b307281d8f8a96e38748949c7a6d35b45114dc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4c28354be1f5eec875d116a1ab7d9429023ef83b"
        },
        "date": 1738707023961,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18628091,
            "range": "± 357524",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18833054,
            "range": "± 171009",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19970801,
            "range": "± 121299",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24043717,
            "range": "± 175090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55391941,
            "range": "± 1046241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 383998348,
            "range": "± 8690056",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2620868532,
            "range": "± 106589377",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14959405,
            "range": "± 98830",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15002662,
            "range": "± 176659",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15342416,
            "range": "± 182747",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19767620,
            "range": "± 177523",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51074348,
            "range": "± 829954",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 318208175,
            "range": "± 4910716",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2518987971,
            "range": "± 40154787",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "31abe6117f2deed5928816b1606b9a42aab8838d",
          "message": "Remove pallet_revive benchmarks from Westend AssetHub (#7454)\n\nWe are using the substrate weights on the test net. Removing the benches\nso that they are not generated by accident and then not used.",
          "timestamp": "2025-02-05T09:56:28Z",
          "tree_id": "4e690dea8f1cd88d928fdaa87711785394855f39",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/31abe6117f2deed5928816b1606b9a42aab8838d"
        },
        "date": 1738752213194,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18276046,
            "range": "± 442383",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18936878,
            "range": "± 189083",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20563831,
            "range": "± 340711",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24270332,
            "range": "± 320989",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57354424,
            "range": "± 1793775",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 353219969,
            "range": "± 8712440",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2632476504,
            "range": "± 194899086",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14934277,
            "range": "± 147094",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15129956,
            "range": "± 142507",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15712314,
            "range": "± 154901",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20347510,
            "range": "± 213139",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51537852,
            "range": "± 778449",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 322586237,
            "range": "± 4274460",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2525596551,
            "range": "± 17326014",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9c474d5452a855adc843785c71fc842f81eeed56",
          "message": "omni-node: Adjust manual seal parameters (#7451)\n\nThis PR will make omni-node dev-mode once again compatible with older\nruntimes.\n\nThe changes introduced in\nhttps://github.com/paritytech/polkadot-sdk/pull/6825 changed constraints\nthat are enforced in the runtime. For normal chains this should work\nfine, since we have real parameters there, like relay chain slots and\nparachain slots.\n\nFor these manual seal parameters we need to respect the constraints,\nwhile faking all the parameters. This PR should fix manual seal in\nomni-node to work with runtime build before and after\nhttps://github.com/paritytech/polkadot-sdk/pull/6825 (I tested that).\n\nIn the future, we should look into improving the parameterization here,\npossibly by introducing proper aura pre-digests so that the parachain\nslot moves forward. This will require quite a bit of refactoring on the\nmanual seal node side however. Issue:\nhttps://github.com/paritytech/polkadot-sdk/issues/7453\n\nAlso, the dev chain spec in parachain template is updated. This makes it\nwork with stable2412-1 and master omni-node. Once the changes here are\nbackported and in a release, all combinations will work again.\n\nfixes https://github.com/paritytech/polkadot-sdk/issues/7341\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-05T14:53:36Z",
          "tree_id": "8dc156f5199bc4cd7ab1c87438688c6a6a842c9b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9c474d5452a855adc843785c71fc842f81eeed56"
        },
        "date": 1738770344195,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17706165,
            "range": "± 204452",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17847458,
            "range": "± 236778",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19562072,
            "range": "± 239902",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23173817,
            "range": "± 538527",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52907836,
            "range": "± 612245",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 326633228,
            "range": "± 5907073",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2468624155,
            "range": "± 77819724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14420794,
            "range": "± 233274",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14497851,
            "range": "± 161787",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14809024,
            "range": "± 109447",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19247918,
            "range": "± 235724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49943680,
            "range": "± 1855438",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 292451054,
            "range": "± 3869357",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2372247471,
            "range": "± 13340884",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "14218860+iulianbarbu@users.noreply.github.com",
            "name": "Iulian Barbu",
            "username": "iulianbarbu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "87f4f3f0df5fc0cc72f69e612909d4d213965820",
          "message": "omni-node: add offchain worker (#7479)\n\n# Description\n\nCopy pasted the `parachain-template-node` offchain worker setup to\nomni-node-lib for both aura and manual seal nodes.\n\nCloses #7447 \n\n## Integration\n\nEnabled offchain workers for both `polkadot-omni-node` and\n`polkadot-parachain` nodes. This would allow executing offchain logic in\nthe runtime and considering it on the node side.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-05T19:17:15Z",
          "tree_id": "82637de79a33cbf7f3f057f8f1f391847419b821",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/87f4f3f0df5fc0cc72f69e612909d4d213965820"
        },
        "date": 1738786000038,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17567153,
            "range": "± 215373",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17896432,
            "range": "± 173096",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19378233,
            "range": "± 203515",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22607933,
            "range": "± 205433",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54846312,
            "range": "± 1205344",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 320798028,
            "range": "± 3157450",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2542409736,
            "range": "± 111184035",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14222516,
            "range": "± 147897",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14200981,
            "range": "± 229130",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14989409,
            "range": "± 122456",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19478265,
            "range": "± 220913",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48803858,
            "range": "± 977086",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 290728282,
            "range": "± 3493594",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2367845896,
            "range": "± 14454988",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2f44779a54ebdf068496a1c1b651cf11434c8740",
          "message": "litep2p: Increase keep-alive to 10 seconds to mirror libp2p (#7488)\n\nThis PR ensures that litep2p will keep an idle connection alive for 10\nseconds.\n\nThe bump from 5 seconds is done to mirror the libp2p behavior and\npotentially improve connection stability:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/a07fb323bc0cfb5c2fb4c8fbe9d20e344cc8eeaf/substrate/client/network/src/service.rs#L542-L549\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-02-06T11:42:40Z",
          "tree_id": "964d93f847b24ae933d6310580ceaeda21af0b9a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f44779a54ebdf068496a1c1b651cf11434c8740"
        },
        "date": 1738845118986,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17473254,
            "range": "± 130317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17611370,
            "range": "± 102972",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19345738,
            "range": "± 415317",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22728141,
            "range": "± 211790",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51564189,
            "range": "± 449657",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 292322357,
            "range": "± 3078529",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2235582652,
            "range": "± 61135720",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14116246,
            "range": "± 178218",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14181613,
            "range": "± 128125",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14504844,
            "range": "± 70493",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18440290,
            "range": "± 112129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47879127,
            "range": "± 440859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 281225188,
            "range": "± 2476583",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2307007349,
            "range": "± 30646947",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "917052e58fbcd22f4320e2b722631ddf8ac74960",
          "message": "[pallet-revive] tx fee fixes (#7463)\n\nApply some fixes to properly estimate ethereum tx fees:\n\n- Set the `extension_weight` on the dispatch_info to properly calculate\nthe fee with pallet_transaction_payment\n- Expose the gas_price through Runtime API, just in case we decide to\ntweak the value in future updates, it should be read from the chain\nrather than be a shared constant exposed by the crate\n- add a `evm_gas_to_fee` utility function to properly convert gas to\nsubstrate fee\n- Fix some minor gas encoding for edge cases\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-06T16:56:51Z",
          "tree_id": "61e74e9cf200dba354367f8c94021e6984384210",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/917052e58fbcd22f4320e2b722631ddf8ac74960"
        },
        "date": 1738864054392,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18386869,
            "range": "± 163768",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18500326,
            "range": "± 222659",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20327358,
            "range": "± 209062",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24333263,
            "range": "± 378893",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56872169,
            "range": "± 922372",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 360727226,
            "range": "± 10301849",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2691809542,
            "range": "± 74153275",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14759705,
            "range": "± 117733",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15144359,
            "range": "± 67696",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15569351,
            "range": "± 112805",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19827802,
            "range": "± 266731",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51485795,
            "range": "± 923777",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 321997647,
            "range": "± 4257218",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2527146488,
            "range": "± 14502221",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dharjeezy@gmail.com",
            "name": "dharjeezy",
            "username": "dharjeezy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f08bf1a1ff0a891c1a06ca6ff4d4be679e7346e0",
          "message": "Update Pallet Referenda to support  Block Number Provider (#6338)\n\nThis PR introduces BlockNumberProvider config for the referenda pallet.\ncloses part of https://github.com/paritytech/polkadot-sdk/issues/6297\n\nPolkadot address: 12GyGD3QhT4i2JJpNzvMf96sxxBLWymz4RdGCxRH5Rj5agKW\n\n---------\n\nCo-authored-by: muharem <ismailov.m.h@gmail.com>",
          "timestamp": "2025-02-07T07:59:51Z",
          "tree_id": "4c4a414b869a7d6c04d00e75ad2cf10f63726ff3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f08bf1a1ff0a891c1a06ca6ff4d4be679e7346e0"
        },
        "date": 1738918125062,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19815185,
            "range": "± 173543",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20273649,
            "range": "± 244803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21974297,
            "range": "± 135866",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25663909,
            "range": "± 189277",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 60186094,
            "range": "± 1043538",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 385589598,
            "range": "± 4494384",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2771296871,
            "range": "± 43092304",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15933656,
            "range": "± 178922",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16030651,
            "range": "± 186182",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16234218,
            "range": "± 192161",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20941753,
            "range": "± 155334",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53440630,
            "range": "± 629735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 343584875,
            "range": "± 4731357",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2608348815,
            "range": "± 21640464",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dharjeezy@gmail.com",
            "name": "dharjeezy",
            "username": "dharjeezy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "fddb6a2c36042cf896d3006492bb15d2e2b56bae",
          "message": "Update pallet society to support Block Number Provider (#6623)\n\nThis PR introduces BlockNumberProvider config for pallet society.\ncloses part of https://github.com/paritytech/polkadot-sdk/issues/6297",
          "timestamp": "2025-02-07T08:41:18Z",
          "tree_id": "d49648e9d8da408dfc4d4c97b69f101206f4c70a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fddb6a2c36042cf896d3006492bb15d2e2b56bae"
        },
        "date": 1738922651516,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19481556,
            "range": "± 129197",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19556663,
            "range": "± 181222",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21310456,
            "range": "± 231651",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25496292,
            "range": "± 361509",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61973612,
            "range": "± 2829009",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 387242254,
            "range": "± 9668890",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2628107847,
            "range": "± 117951788",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16138782,
            "range": "± 551958",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16072688,
            "range": "± 384316",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16819476,
            "range": "± 173945",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22184161,
            "range": "± 796543",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 58039873,
            "range": "± 2471581",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 368697558,
            "range": "± 11393294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2634713403,
            "range": "± 48345214",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "4ec221eb92a17eb682bf4124b5157d65208775bd",
          "message": "[pallet-revive] fix eth-rpc indexing (#7493)\n\n- Fix a deadlock on the RWLock cache\n- Remove eth-indexer, we won't need it anymore, the indexing will be\nstarted from within eth-rpc directly\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-07T15:32:14Z",
          "tree_id": "c51b2735dc55f194b5f34a920215feba290be6b2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4ec221eb92a17eb682bf4124b5157d65208775bd"
        },
        "date": 1738945948202,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17274486,
            "range": "± 122923",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17635240,
            "range": "± 135653",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19276139,
            "range": "± 137081",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22764620,
            "range": "± 249100",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50833108,
            "range": "± 279295",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 291480046,
            "range": "± 2515722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2313499016,
            "range": "± 70324547",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14085689,
            "range": "± 102934",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14204158,
            "range": "± 229214",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14645913,
            "range": "± 146528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18303835,
            "range": "± 63624",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47772716,
            "range": "± 661305",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 280277943,
            "range": "± 1814195",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2342376144,
            "range": "± 7425673",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "363911+pepoviola@users.noreply.github.com",
            "name": "Javier Viola",
            "username": "pepoviola"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "135b7183626e4ac675fe368cf11496050b9e7821",
          "message": "Zombienet gha substrate migration (#7217)\n\nMigrate subtrate's zombienet test from gitlab to gha.\n\n---------\n\nCo-authored-by: alvicsam <alvicsam@gmail.com>",
          "timestamp": "2025-02-07T16:48:31Z",
          "tree_id": "443d3e39a34ee1f6b6f14649ed62816887704c38",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/135b7183626e4ac675fe368cf11496050b9e7821"
        },
        "date": 1738950712306,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17794544,
            "range": "± 178298",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18159511,
            "range": "± 142444",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19633212,
            "range": "± 189111",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23650965,
            "range": "± 270736",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55294429,
            "range": "± 1058008",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 348999413,
            "range": "± 5716493",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2601124641,
            "range": "± 242034710",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14674137,
            "range": "± 212378",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14939281,
            "range": "± 183864",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15497429,
            "range": "± 211050",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20083877,
            "range": "± 276670",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51109426,
            "range": "± 1084804",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 309349244,
            "range": "± 6217310",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2543888017,
            "range": "± 20065290",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "49e381a959e4573b0856608576a5e81ce9ee1184",
          "message": "Fix compilation warnings  (#7507)\n\nThis should fix some compilation warnings discovered under rustc 1.83\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-07T21:08:31Z",
          "tree_id": "8f808890daf1fac5dc67944fdd6f8a7a0c06ff0e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/49e381a959e4573b0856608576a5e81ce9ee1184"
        },
        "date": 1738965622742,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19484390,
            "range": "± 415962",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19967367,
            "range": "± 178326",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19881647,
            "range": "± 283086",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24060256,
            "range": "± 392020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56564580,
            "range": "± 1974038",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 318136923,
            "range": "± 24911910",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2450618271,
            "range": "± 71286507",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14215787,
            "range": "± 223857",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14557004,
            "range": "± 173892",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14980677,
            "range": "± 192544",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18770578,
            "range": "± 150492",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48357088,
            "range": "± 571107",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 282493201,
            "range": "± 1760011",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2353371475,
            "range": "± 26165368",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dharjeezy@gmail.com",
            "name": "dharjeezy",
            "username": "dharjeezy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "10b77c95429c98412a2bb0a39436519cbf3b0950",
          "message": "Update pallet nomination pool to support Block Number Provider (#6715)\n\nThis PR introduces BlockNumberProvider config for the nomination pool\npallet.\ncloses part of https://github.com/paritytech/polkadot-sdk/issues/6297",
          "timestamp": "2025-02-08T04:44:46Z",
          "tree_id": "46b7c67c0eaf8444e6b0a2d25336cefff2a6c2f9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/10b77c95429c98412a2bb0a39436519cbf3b0950"
        },
        "date": 1738993136760,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17761463,
            "range": "± 142727",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18110432,
            "range": "± 195917",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19815378,
            "range": "± 232779",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23871802,
            "range": "± 438517",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56803617,
            "range": "± 1390798",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 338220823,
            "range": "± 10059263",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2692888568,
            "range": "± 76946720",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14895500,
            "range": "± 130563",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15158172,
            "range": "± 186828",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15823488,
            "range": "± 107936",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20188914,
            "range": "± 192117",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 52935352,
            "range": "± 1371834",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 321007234,
            "range": "± 4184245",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2521447032,
            "range": "± 27863895",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ea51bbf996c6e79f674f751491292309eb9eead9",
          "message": "pallet-revive-fixtures: Support compilation on stable (#7419)\n\nLet's burry out the old `RUSTC_BOOTSTRAP` hack. This is required when\nyou don't use `rustup` which automatically switches to the nightly\ntoolchain when it detects nightly CLI args.\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-02-08T12:03:02Z",
          "tree_id": "f4c9595774d53f40151c614aa8fd43c4826c3159",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ea51bbf996c6e79f674f751491292309eb9eead9"
        },
        "date": 1739019848352,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20376459,
            "range": "± 271806",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20170477,
            "range": "± 216206",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22244807,
            "range": "± 371914",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 27050681,
            "range": "± 340753",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63141318,
            "range": "± 1288835",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 428187974,
            "range": "± 4396824",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2689699252,
            "range": "± 147211172",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16100479,
            "range": "± 150115",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16389755,
            "range": "± 135579",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17011985,
            "range": "± 274795",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22398559,
            "range": "± 256854",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 56436482,
            "range": "± 717009",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 392755308,
            "range": "± 8149834",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2820618038,
            "range": "± 30185221",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "109800286+StackOverflowExcept1on@users.noreply.github.com",
            "name": "StackOverflowExcept1on",
            "username": "StackOverflowExcept1on"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "2970ab151402a94c146800c769953cf6fdb6ef1d",
          "message": "feat(wasm-builder): add support for new `wasm32v1-none` target (#7008)\n\n# Description\n\nResolves #5777\n\nPreviously `wasm-builder` used hacks such as `-Zbuild-std` (required\n`rust-src` component) and `RUSTC_BOOTSTRAP=1` to build WASM runtime\nwithout WASM features: `sign-ext`, `multivalue` and `reference-types`,\nbut since Rust 1.84 (will be stable on 9 January, 2025) the situation\nhas improved as there is new\n[`wasm32v1-none`](https://doc.rust-lang.org/beta/rustc/platform-support/wasm32v1-none.html)\ntarget that disables all \"post-MVP\" WASM features except\n`mutable-globals`.\n\nPreviously, your `rust-toolchain.toml` looked like this:\n\n```toml\n[toolchain]\nchannel = \"stable\"\ncomponents = [\"rust-src\"]\ntargets = [\"wasm32-unknown-unknown\"]\nprofile = \"default\"\n```\n\nIt should now be updated to something like this:\n\n```toml\n[toolchain]\nchannel = \"stable\"\ntargets = [\"wasm32v1-none\"]\nprofile = \"default\"\n```\n\nTo build the runtime:\n\n```bash\ncargo build --package minimal-template-runtime --release\n```\n\n## Integration\n\nIf you are using Rust 1.84 and above, then install the `wasm32v1-none`\ntarget instead of `wasm32-unknown-unknown` as shown above. You can also\nremove the unnecessary `rust-src` component.\n\nAlso note the slight differences in conditional compilation:\n- `wasm32-unknown-unknown`: `#[cfg(all(target_family = \"wasm\", target_os\n= \"unknown\"))]`\n- `wasm32v1-none`: `#[cfg(all(target_family = \"wasm\", target_os =\n\"none\"))]`\n\nAvoid using `target_os = \"unknown\"` in `#[cfg(...)]` or\n`#[cfg_attr(...)]` and instead use `target_family = \"wasm\"` or\n`target_arch = \"wasm32\"` in the runtime code.\n\n## Review Notes\n\nWasm builder requires the following prerequisites for building the WASM\nbinary:\n- Rust >= 1.68 and Rust < 1.84:\n  - `wasm32-unknown-unknown` target\n  - `rust-src` component\n- Rust >= 1.84:\n  - `wasm32v1-none` target\n- no more `-Zbuild-std` and `RUSTC_BOOTSTRAP=1` hacks and `rust-src`\ncomponent requirements!\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-02-09T00:08:41Z",
          "tree_id": "4a43522910c91f653ac69fa4718807e18edf15a3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2970ab151402a94c146800c769953cf6fdb6ef1d"
        },
        "date": 1739063861416,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17769129,
            "range": "± 196785",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18194721,
            "range": "± 105413",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19551680,
            "range": "± 166894",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23536341,
            "range": "± 468552",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55130805,
            "range": "± 934768",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 311959506,
            "range": "± 5782290",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2445777212,
            "range": "± 124159665",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14350933,
            "range": "± 186322",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14656613,
            "range": "± 147626",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14921955,
            "range": "± 149577",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19036707,
            "range": "± 169221",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48920198,
            "range": "± 470613",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 290449378,
            "range": "± 3303479",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2393788258,
            "range": "± 6190253",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7b0ac746e4fc26a9959b0dc1aeac2db32d3f289f",
          "message": "[pallet-revive] Add eth_get_logs (#7506)\n\nAdd support for eth_get_logs rpc method\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-02-10T15:33:16Z",
          "tree_id": "6c995cfd7bfa5866f034e7366f2f7109d9e6ae4e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7b0ac746e4fc26a9959b0dc1aeac2db32d3f289f"
        },
        "date": 1739204501701,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17305835,
            "range": "± 68415",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17757999,
            "range": "± 110722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19401288,
            "range": "± 114544",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22941082,
            "range": "± 239785",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51348854,
            "range": "± 917495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 321380355,
            "range": "± 3636831",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2277699152,
            "range": "± 60680389",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14247916,
            "range": "± 112141",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14171477,
            "range": "± 117200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14768848,
            "range": "± 184299",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19192291,
            "range": "± 254721",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48461334,
            "range": "± 362952",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 286243349,
            "range": "± 2292241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2356729213,
            "range": "± 18357815",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "dhiraj.kumar2990@gmail.com",
            "name": "Dhiraj Sah",
            "username": "dhirajs0"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f96da6f3c37318568a1a662f39511afa76c13791",
          "message": "transfer function Preservation is changed to Expendable  (#7243)\n\n# Description\n\nFixes #7039\n\nThe Preservation of transfer method of fungible and fungibles adapters\nis changed from Preserve to Expendable. So the behavior of the\nTransferAsset will be consistent with the WithdrawAsset function, as in\n[fungible](https://github.com/paritytech/polkadot-sdk/blob/f3ab3854e1df9e0498599f01ba4f9f152426432a/polkadot/xcm/xcm-builder/src/fungible_adapter.rs#L217)\nand [fungibles](https://github.com/paritytech/polkadot-sdk/issues/url)\nadapter.\n\nThis pull request includes changes to the `fungible_adapter.rs` and\n`fungibles_adapter.rs` files in the `polkadot/xcm/xcm-builder`\ndirectory. The main change involves modifying the transfer method to use\nthe `Expendable` strategy instead of the `Preserve` strategy.\n\nChanges to transfer strategy:\n\n*\n[`polkadot/xcm/xcm-builder/src/fungible_adapter.rs`](diffhunk://#diff-6ebd77385441f2c8b023c480e818a01c4b43ae892c73ca30144cd64ee960bd66L67-R67):\nChanged the transfer method to use `Expendable` instead of `Preserve`.\n*\n[`polkadot/xcm/xcm-builder/src/fungibles_adapter.rs`](diffhunk://#diff-82221429de4c4c88be3d2976ece6475ef4fa56a32abc70290911bd47191f8e17L61-R61):\nChanged the transfer method to use `Expendable` instead of `Preserve`.\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-02-10T16:10:47Z",
          "tree_id": "4a2e65443eece20df30b63234a762f525207ed16",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f96da6f3c37318568a1a662f39511afa76c13791"
        },
        "date": 1739207269189,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18700718,
            "range": "± 1188991",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18552025,
            "range": "± 164362",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20046234,
            "range": "± 220819",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24028222,
            "range": "± 308850",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57495423,
            "range": "± 957353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 344989742,
            "range": "± 6484948",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2509979862,
            "range": "± 141471467",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14740868,
            "range": "± 130049",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14890443,
            "range": "± 160096",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15571617,
            "range": "± 222818",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19720389,
            "range": "± 151858",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51957406,
            "range": "± 740276",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 310016355,
            "range": "± 6688160",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2512605474,
            "range": "± 25739482",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "bd1b02f15a65227266540df48ac22e32f90ebc34",
          "message": "update readme to link to the new polkadot-docs (#7411)\n\ncloses https://github.com/polkadot-developers/polkadot-docs/issues/238\n\n---------\n\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>",
          "timestamp": "2025-02-10T17:43:09Z",
          "tree_id": "ba50ca6abeea5f67bc9ae7d530c2302210e87598",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd1b02f15a65227266540df48ac22e32f90ebc34"
        },
        "date": 1739212386840,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 25352371,
            "range": "± 1780026",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 25569013,
            "range": "± 2512495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 25160521,
            "range": "± 1950659",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 33917021,
            "range": "± 3254009",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 82336596,
            "range": "± 4766442",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 481039617,
            "range": "± 15964107",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2896248388,
            "range": "± 411492631",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17453107,
            "range": "± 697001",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 18577581,
            "range": "± 857239",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 18882596,
            "range": "± 1480233",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 24606624,
            "range": "± 930495",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 71280404,
            "range": "± 4255336",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 453746718,
            "range": "± 34298853",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 3616276605,
            "range": "± 154839424",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "71c19e4c7ca2b411722ec7d9c1392e1bd81b5681",
          "message": "Use rpc_port in bridge tests (#7520)\n\nUse `rpc_port` instead of `ws_port` in bridge tests since `ws_port` is\ndeprecated.",
          "timestamp": "2025-02-10T19:59:03Z",
          "tree_id": "96c4647f75e0df500deb6ee9124dc02bcb7e51a5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/71c19e4c7ca2b411722ec7d9c1392e1bd81b5681"
        },
        "date": 1739220433471,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18639933,
            "range": "± 127735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19200320,
            "range": "± 162195",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20552098,
            "range": "± 163354",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25032827,
            "range": "± 336341",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 59414977,
            "range": "± 1085523",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 368204463,
            "range": "± 7413210",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2558992008,
            "range": "± 149879931",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15440844,
            "range": "± 188777",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15361692,
            "range": "± 284661",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16074619,
            "range": "± 186510",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21100268,
            "range": "± 192435",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54762255,
            "range": "± 769641",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 331816409,
            "range": "± 3485331",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2645787474,
            "range": "± 15877082",
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
          "distinct": true,
          "id": "6786bbcba95e1c2cc634f38d7276c077f8087f4a",
          "message": "Warn about cargo remote not copying hidden file by default (#7429)\n\nadd a warning about hidden file not transfered.\n\ncargo remote is not really configurable so I just use my own fork for\nnow: https://github.com/sgeisler/cargo-remote/pull/25",
          "timestamp": "2025-02-12T06:38:10Z",
          "tree_id": "7f6cf22c06218f727ea8421253c26cda6d320e3e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6786bbcba95e1c2cc634f38d7276c077f8087f4a"
        },
        "date": 1739345177796,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18900931,
            "range": "± 132617",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19082833,
            "range": "± 184005",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20689832,
            "range": "± 135906",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24548273,
            "range": "± 149856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55982067,
            "range": "± 859599",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 347566093,
            "range": "± 6140824",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2325348952,
            "range": "± 35892907",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15016623,
            "range": "± 98951",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15192497,
            "range": "± 157532",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15689665,
            "range": "± 80108",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20163073,
            "range": "± 173298",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50944860,
            "range": "± 592886",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 312717744,
            "range": "± 2065648",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2482403153,
            "range": "± 15653946",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "42a61672c11e61b241deb843bc7fca7fc824befe",
          "message": "[pallet-revive] Add tracing support (2/2) (#7167)\n\nAdd debug endpoint to eth-rpc for capturing a block or a single\ntransaction traces\n\nSee:\n-  PR #7166\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Yuri Volkov <0@mcornholio.ru>\nCo-authored-by: Maksym H <1177472+mordamax@users.noreply.github.com>\nCo-authored-by: Santi Balaguer <santiago.balaguer@gmail.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-02-12T07:23:49Z",
          "tree_id": "920fb4c8be27dfd7164ac87f04a0c31149cad777",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/42a61672c11e61b241deb843bc7fca7fc824befe"
        },
        "date": 1739347951849,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18778763,
            "range": "± 160761",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19252836,
            "range": "± 233507",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20368484,
            "range": "± 231992",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24776848,
            "range": "± 275693",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56806160,
            "range": "± 1069607",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 363880796,
            "range": "± 6594066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2661187163,
            "range": "± 82498980",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14840072,
            "range": "± 236240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15210417,
            "range": "± 145462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15575510,
            "range": "± 166361",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20051287,
            "range": "± 261185",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51422697,
            "range": "± 1296242",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 334954253,
            "range": "± 6649211",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2526537028,
            "range": "± 25777763",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73715684+Szegoo@users.noreply.github.com",
            "name": "Sergej Sakac",
            "username": "Szegoo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f22c30c9e308ee3780652adcd788264a6242e0ac",
          "message": "On-demand credits (#5990)\n\nImplementation of on-demand credits as described in\n[RFC-1](https://github.com/polkadot-fellows/RFCs/blob/main/text/0001-agile-coretime.md#instantaneous-coretime)\n\n---------\n\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>",
          "timestamp": "2025-02-12T14:26:01Z",
          "tree_id": "f79ab0a80f27e4cffa2e4ed65077e6f1340ffab1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f22c30c9e308ee3780652adcd788264a6242e0ac"
        },
        "date": 1739373511368,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19455136,
            "range": "± 154992",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19596731,
            "range": "± 219541",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21650079,
            "range": "± 233176",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26245143,
            "range": "± 436911",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61189863,
            "range": "± 1130976",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 383777273,
            "range": "± 6003198",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2492466803,
            "range": "± 211455185",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16318172,
            "range": "± 212489",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16461728,
            "range": "± 245686",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17118324,
            "range": "± 197229",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21751868,
            "range": "± 241321",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 56727591,
            "range": "± 982586",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 336773749,
            "range": "± 4453196",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2702103410,
            "range": "± 11902435",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "af4c745b52dbf2a4945d2ae2bda5fa22390e16c1",
          "message": "Update some dependencies (#7548)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/7360\n\nUpdate some dependencies needed for implementing\n`DecodeWithMemTracking`:\n`parity-scale-codec` -> 3.7.4\n`finality-grandpa` -> 0.16.3\n`bounded-collections` -> 0.2.3\n`impl-codec` -> 0.7.1",
          "timestamp": "2025-02-12T14:49:42Z",
          "tree_id": "7bf3440a50eb7709e873765774a445dcb145a5b8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/af4c745b52dbf2a4945d2ae2bda5fa22390e16c1"
        },
        "date": 1739375481441,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19933922,
            "range": "± 240446",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20572233,
            "range": "± 333630",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22529739,
            "range": "± 236542",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26301534,
            "range": "± 588499",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63345288,
            "range": "± 1709246",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 375540126,
            "range": "± 10231831",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2960039080,
            "range": "± 109879759",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17036682,
            "range": "± 218964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17076168,
            "range": "± 241063",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17674954,
            "range": "± 107594",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22876883,
            "range": "± 331743",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 59443540,
            "range": "± 1141046",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 356099372,
            "range": "± 4935117",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2773567297,
            "range": "± 19243635",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "848875ad7dbb1b14a6c83e603a64c8a236e0b2f2",
          "message": "Fix build-linux-substrate (#7552)\n\nFix `build-linux-substrate` when opening PRs from a `polkadot-sdk` fork\n\nFailed CI job example:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/13284026730/job/37088673786?pr=7548",
          "timestamp": "2025-02-12T16:33:26Z",
          "tree_id": "e5f7886fb0c479c8b4724fc004cad703504dcda8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/848875ad7dbb1b14a6c83e603a64c8a236e0b2f2"
        },
        "date": 1739381032856,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21817719,
            "range": "± 405547",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22697072,
            "range": "± 346729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 24464527,
            "range": "± 449133",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 29397360,
            "range": "± 649767",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 72194838,
            "range": "± 2356031",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 456366136,
            "range": "± 12500778",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 3073226971,
            "range": "± 132416375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 18270140,
            "range": "± 212331",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 18655892,
            "range": "± 188270",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 18905083,
            "range": "± 315172",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 26121038,
            "range": "± 1291766",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 75840277,
            "range": "± 2311993",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 465720555,
            "range": "± 8368971",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 3517774819,
            "range": "± 55069779",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "363911+pepoviola@users.noreply.github.com",
            "name": "Javier Viola",
            "username": "pepoviola"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "43ae08fa0783ff5974925b709d623d30990a2667",
          "message": "Move zombienet cumulus pipeline to gha (#7529)\n\nIncludes a fix on the `wait` job (for waiting images to be ready).",
          "timestamp": "2025-02-12T17:19:44Z",
          "tree_id": "ba83065812fb887de310a527aab79b18dafa969d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/43ae08fa0783ff5974925b709d623d30990a2667"
        },
        "date": 1739383979655,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21232704,
            "range": "± 203579",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22630488,
            "range": "± 639684",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 23838334,
            "range": "± 471410",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 28197931,
            "range": "± 403671",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 66784539,
            "range": "± 918836",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 422394063,
            "range": "± 5262306",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2871310797,
            "range": "± 28031001",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17417693,
            "range": "± 128575",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17890119,
            "range": "± 311796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 18894725,
            "range": "± 258971",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 24127276,
            "range": "± 236414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 64055823,
            "range": "± 1193669",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 398177408,
            "range": "± 7382207",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2994152299,
            "range": "± 31673950",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32275622+seemantaggarwal@users.noreply.github.com",
            "name": "seemantaggarwal",
            "username": "seemantaggarwal"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "645a6f40927c8acea3f32a6363242456de009321",
          "message": "Update Scheduler to have a configurable block provider #7434 (#7441)\n\nFollow up from\nhttps://github.com/paritytech/polkadot-sdk/pull/6362#issuecomment-2629744365\n\nThe goal of this PR is to have the scheduler pallet work on a parachain\nwhich does not produce blocks on a regular schedule, thus can use the\nrelay chain as a block provider.\n\nBecause blocks are not produced regularly, we cannot make the assumption\nthat block number increases monotonically, and thus have new logic to\nhandle multiple spend periods passing between blocks.\n\nRequirement: \n\ninstead of using the hard coded system block number. We add an\nassociated type BlockNumberProvider\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-02-13T00:30:37Z",
          "tree_id": "bb63eea1a582dc3683e7e2c88069cabec74fbcf2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/645a6f40927c8acea3f32a6363242456de009321"
        },
        "date": 1739409714182,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21962881,
            "range": "± 336599",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22059755,
            "range": "± 269903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 24516713,
            "range": "± 337644",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 28967300,
            "range": "± 294189",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 67165648,
            "range": "± 1351212",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 424846445,
            "range": "± 6485355",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2652548087,
            "range": "± 51396667",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17999003,
            "range": "± 324761",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 18147632,
            "range": "± 299101",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 19193373,
            "range": "± 192658",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 24620592,
            "range": "± 409503",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 63491932,
            "range": "± 1843220",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 404710148,
            "range": "± 4745960",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2940256166,
            "range": "± 52279150",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e5df3306a5e741bc97b21daa60f25870be897b8c",
          "message": "`fatxpool`: transaction statuses metrics added (#7505)\n\n#### Overview\n\nThis PR introduces a new mechanism to capture and report metrics related\nto timings of transaction lifecycle events, which are currently not\navailable. By exposing these timings, we aim to augment transaction-pool\nreliability dashboards and extend existing Grafana boards.\n\nA new `unknown_from_block_import_txs` metric is also introduced. It\nprovides the number of transactions in imported block which are not\nknown to the node's transaction pool. It allows to monitor alignment of\ntransaction pools across the nodes in the network.\n\n#### Notes for reviewers\n- **[Per-event\nMetrics](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L84-L105)\nCollection**: implemented by[\n`EventsMetricsCollector`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L353-L358)\nwhich allows to capture both submission timestamps and transaction\nstatus updates. An asynchronous\n[`EventsMetricsCollectorTask`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L503-L526)\nprocesses the metrics-related messages sent by the\n`EventsMetricsCollector` and reports the timings of transaction statuses\nupdates to Prometheus. This task implements event[\nde-duplication](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L458)\nusing a `HashMap` of\n[`TransactionEventMetricsData`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L424-L435)\nentries which also holds transaction submission timestamps used to\n[compute\ntimings](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L489-L495).\nTransaction-related items are removed when transaction's final status is\n[reported](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L496).\n- Transaction submission timestamp is reusing the timestamp of\n`TimedTransactionSource` kept in mempool. It is reported to\n`EventsMetricsCollector` in\n[`submit_at`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/fork_aware_txpool.rs#L735)\nand\n[`submit_and_watch`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/fork_aware_txpool.rs#L836)\nmethods of `ForkAwareTxPool`.\n- Transaction updates are reported to `EventsMetricsCollector` from\n`MultiViewListener`\n[task](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/multi_view_listener.rs#L494).\nThis allows to gather metrics for _watched_ and _non-watched_\ntransactions (what enables metrics on non-rpc-enabled collators).\n- New metric\n([`unknown_from_block_import_txs`](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/metrics.rs#L59-L60))\nallowing checking alignment of pools across the network is\n[reported](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/fork_aware_txpool.rs#L1288-L1292)\nusing new `TxMemPool`\n[method](https://github.com/paritytech/polkadot-sdk/blob/8a53992e2fb200b084ebf0393ad22d91314fd173/substrate/client/transaction-pool/src/fork_aware_txpool/tx_mem_pool.rs#L605-L611).\n\nfixes: #7355, #7448\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-02-13T08:25:23Z",
          "tree_id": "3dcc86374c324a490e9653161fe188675c84ce6c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5df3306a5e741bc97b21daa60f25870be897b8c"
        },
        "date": 1739438252349,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19723192,
            "range": "± 273345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19991308,
            "range": "± 122748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21796204,
            "range": "± 205894",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25838720,
            "range": "± 408724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61192105,
            "range": "± 970042",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 379081785,
            "range": "± 6569823",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2469205060,
            "range": "± 134899419",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16231620,
            "range": "± 149478",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16309816,
            "range": "± 110004",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17162561,
            "range": "± 188646",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21960755,
            "range": "± 164582",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57134753,
            "range": "± 971993",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 346917697,
            "range": "± 9694579",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2689624839,
            "range": "± 10693434",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9d14b3b5d4c3ab334299e99d0e5f6fea9c5a7e46",
          "message": "sc-informant: Print full hash when debug logging is enabled (#7554)\n\nWhen debugging stuff, it is useful to see the full hashes and not only\nthe \"short form\". This makes it easier to read logs and follow blocks.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-13T12:56:52Z",
          "tree_id": "99f351ac07d8d5b649c3c481d667c7f31f3236e0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9d14b3b5d4c3ab334299e99d0e5f6fea9c5a7e46"
        },
        "date": 1739454379012,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18114500,
            "range": "± 174121",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18408265,
            "range": "± 129179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19736746,
            "range": "± 86306",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23374224,
            "range": "± 134959",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53328850,
            "range": "± 517318",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 324798020,
            "range": "± 7782636",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2560042814,
            "range": "± 152495729",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14485580,
            "range": "± 89748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14659559,
            "range": "± 266384",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15082849,
            "range": "± 92040",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18920644,
            "range": "± 148128",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49263824,
            "range": "± 335532",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 290735966,
            "range": "± 2689044",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2379181496,
            "range": "± 7251139",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "48632512+s0me0ne-unkn0wn@users.noreply.github.com",
            "name": "s0me0ne-unkn0wn",
            "username": "s0me0ne-unkn0wn"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1866c3b4673b66a62b1eb9c8c82f2cd827cbd388",
          "message": "Shorter availability data retention period for testnets (#7353)\n\nCloses #3270\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-02-13T14:22:42Z",
          "tree_id": "21a0acc42ca1637449d95e7370dcf357dbe5699f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1866c3b4673b66a62b1eb9c8c82f2cd827cbd388"
        },
        "date": 1739459694054,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19629254,
            "range": "± 281613",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19445394,
            "range": "± 546796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21275622,
            "range": "± 285532",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25452981,
            "range": "± 808240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 62865847,
            "range": "± 1719338",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 410025262,
            "range": "± 8305944",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2841761715,
            "range": "± 65975112",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16092849,
            "range": "± 355461",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16586328,
            "range": "± 303297",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17140846,
            "range": "± 241249",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22009127,
            "range": "± 559526",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 56412041,
            "range": "± 1491464",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 354214120,
            "range": "± 6413979",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2688804930,
            "range": "± 47731726",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d11400478dddb6bb78d294ced9d688712343fde0",
          "message": "[pallet-revive] fix subxt version (#7570)\n\nCargo.lock change to subxt were rolled back \nFixing it and updating it in Cargo.toml so it does not happen again\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-13T21:53:03Z",
          "tree_id": "95f7fa673309d0b389a73c479ad3a94c2d07610d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d11400478dddb6bb78d294ced9d688712343fde0"
        },
        "date": 1739486838031,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17822203,
            "range": "± 104034",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18084474,
            "range": "± 82170",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19457641,
            "range": "± 142659",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23020824,
            "range": "± 154814",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51040726,
            "range": "± 724450",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 303371894,
            "range": "± 2855426",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2358102514,
            "range": "± 75400990",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14331597,
            "range": "± 101925",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14379691,
            "range": "± 96998",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14863044,
            "range": "± 82123",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18502812,
            "range": "± 243602",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48413729,
            "range": "± 311048",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 285320555,
            "range": "± 1821363",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2321639079,
            "range": "± 7879137",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "b44dc3a5284d6882defb88903f048e0570181642",
          "message": "pallet-revive: Add env var to allow skipping of validation for testing (#7562)\n\nWhen trying to reproduce bugs we sometimes need to deploy code that\nwouldn't pass validation. This PR adds a new environment variable\n`REVIVE_SKIP_VALIDATION` that when set will skip all validation except\nthe contract blob size limit.\n\nPlease note that this only applies to when the pallet is compiled for\n`std` and hence will never be part of on-chain.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-14T07:55:23Z",
          "tree_id": "60d33df710eb4895e01f2a9cc132d8d0334727ee",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b44dc3a5284d6882defb88903f048e0570181642"
        },
        "date": 1739523826138,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17686154,
            "range": "± 152646",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17879259,
            "range": "± 79193",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19515218,
            "range": "± 171177",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22857090,
            "range": "± 318830",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52546683,
            "range": "± 719958",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 297331555,
            "range": "± 3867336",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2305701118,
            "range": "± 28748726",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14235683,
            "range": "± 382616",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14383236,
            "range": "± 121157",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14735741,
            "range": "± 92499",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18515472,
            "range": "± 189145",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48289244,
            "range": "± 437891",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 285002811,
            "range": "± 2965820",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2346376187,
            "range": "± 22950535",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "git@kchr.de",
            "name": "Bastian Köcher",
            "username": "bkchr"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c1f4703f4d6fcf8b74c7f4b587d3a00d6adf63a0",
          "message": "frame-benchmarking: Improve macro hygiene (#7571)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-14T09:47:10Z",
          "tree_id": "fa4effc00909ba5d84b0608e35c9ec059cc2bd4e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c1f4703f4d6fcf8b74c7f4b587d3a00d6adf63a0"
        },
        "date": 1739530098205,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17212617,
            "range": "± 110085",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17380773,
            "range": "± 120713",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18999178,
            "range": "± 190168",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22618185,
            "range": "± 140697",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51201581,
            "range": "± 334047",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 305552869,
            "range": "± 5469261",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2232688815,
            "range": "± 88830714",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14048937,
            "range": "± 134904",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14205852,
            "range": "± 73072",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14768078,
            "range": "± 143153",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18757147,
            "range": "± 156137",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47516049,
            "range": "± 348131",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 282743151,
            "range": "± 4437832",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2338645343,
            "range": "± 9270033",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Florian.Franzen@gmail.com",
            "name": "Florian Franzen",
            "username": "FlorianFranzen"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9117f70f226446b2ec75db9a5052af4919b5933a",
          "message": "cumulus-client: use only valid syntax in Cargo.toml (#7455)\n\n# Description\n\nThis PR ensure that only valid syntax is uses inside the `Cargo.toml`. \n\n## Integration\n\nNot sure if worth backporting. Came across this when trying to package\n`try-runtime-cli`.\n\n## Review Notes\n\nIt should be obvious that this is not valid syntax. I am not able to add\nlabels and doubt this requires a prdoc.",
          "timestamp": "2025-02-14T10:04:55Z",
          "tree_id": "0eff889773f574747d952497bbffab4f6fbf8ca6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9117f70f226446b2ec75db9a5052af4919b5933a"
        },
        "date": 1739531851665,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17399250,
            "range": "± 115371",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17568296,
            "range": "± 115330",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19097109,
            "range": "± 112859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22758006,
            "range": "± 485950",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53060837,
            "range": "± 680709",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 309931860,
            "range": "± 2854353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2543550334,
            "range": "± 198884990",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 13970329,
            "range": "± 203344",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14151006,
            "range": "± 91330",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14696164,
            "range": "± 94154",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18855176,
            "range": "± 113250",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47429724,
            "range": "± 300436",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 278080533,
            "range": "± 2476413",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2307396693,
            "range": "± 21693070",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "20ffada06152b8926cbccfa19334fe7bbb116857",
          "message": "network/tests: Add conformance testing for litep2p and libp2p (#7361)\n\nThis PR implements conformance tests between our network backends\n(litep2p and libp2p).\n\nThe PR creates a setup for extending testing in the future, while\nimplementing the following tests:\n- connectivity check: Connect litep2p -> libp2p and libp2p -> litep2p\n- request response check: Send 32 requests from one backend to the other\n- notification check: Send 128 ping pong notifications and 128 from one\nbackend to the other\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-02-14T11:52:17Z",
          "tree_id": "bafb8b07fe11e771d25080798f9394c4c24cbbe3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/20ffada06152b8926cbccfa19334fe7bbb116857"
        },
        "date": 1739538463992,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17614428,
            "range": "± 165782",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17847164,
            "range": "± 156280",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19312118,
            "range": "± 120610",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22999615,
            "range": "± 172818",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53214276,
            "range": "± 1793933",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 332787945,
            "range": "± 6429102",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2214921268,
            "range": "± 73183424",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14427412,
            "range": "± 138233",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14546054,
            "range": "± 143973",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15017803,
            "range": "± 675565",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18886709,
            "range": "± 182262",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49514925,
            "range": "± 954618",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 286747440,
            "range": "± 2466182",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2417298481,
            "range": "± 64120742",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "60146ba5d291530880e008e6650e2cfa74c9105c",
          "message": "pallet-revive: Fix the contract size related benchmarks (#7568)\n\nPartly addresses https://github.com/paritytech/polkadot-sdk/issues/6157\n\nThe benchmarks measuring the impact of contract sizes on calling or\ninstantiating a contract were bogus because they needed to be written in\nassembly in order to tightly control the basic block size.\n\nThis fixes the benchmarks for:\n- call_with_code_per_byte\n- upload_code\n- instantiate_with_code\n\nAnd adds a new benchmark that accounts for the fact that the interpreter\nwill always compile whole basic blocks:\n- basic_block_compilation\n\nAfter this PR only the weight we assign to instructions need to be\naddressed.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>",
          "timestamp": "2025-02-14T13:16:08Z",
          "tree_id": "f31de68f1089b96d01bbc9ea3895409b158eb26b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/60146ba5d291530880e008e6650e2cfa74c9105c"
        },
        "date": 1739542264425,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17310619,
            "range": "± 120745",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17514479,
            "range": "± 83337",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18951969,
            "range": "± 79355",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22350818,
            "range": "± 912854",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50016009,
            "range": "± 316758",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 287111431,
            "range": "± 1875426",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2232930496,
            "range": "± 54922000",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 13968512,
            "range": "± 191397",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14085003,
            "range": "± 247596",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14454165,
            "range": "± 57516",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18658745,
            "range": "± 76382",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48507539,
            "range": "± 346680",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 288142737,
            "range": "± 1267803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2312897744,
            "range": "± 14677335",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c94df1bc1be2203f5bc8aa960b0b46a44d97d120",
          "message": "`txpool api`: `remove_invalid` call improved (#6661)\n\n#### Description \nCurrently the transaction which is reported as invalid by a block\nbuilder (or `removed_invalid` by other components) is silently skipped.\n\nThis PR improves this behavior. The transaction pool `report_invalid`\nfunction now accepts optional error associated with every reported\ntransaction, and also the optional block hash which provides hints how\nreported transaction shall be handled. The following API change is\nproposed:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/8be5ef3e9a18e873de78aca1b8f834fa554ce9c8/substrate/client/transaction-pool/api/src/lib.rs#L297-L318\nDepending on error, the transaction pool can decide if transaction shall\nbe removed from the view only or entirely from the pool. Invalid event\nwill be dispatched if required.\n\n\n#### Notes for reviewers\n\n- Actual logic of removing invalid txs is implented in\n[`ViewStore::report_invalid`](https://github.com/paritytech/polkadot-sdk/blob/0fad26c43a65bfb371d667278981d3c68c3ce9d6/substrate/client/transaction-pool/src/fork_aware_txpool/view_store.rs#L657-L680).\nMethod's doc explains the flow.\n- This PR changes `HashMap` to `IndexMap` in revalidation logic. This is\nto preserve the original order of transactions (mainly for purposes of\nunit tests).\n- This PR solves the problem mentioned in:\nhttps://github.com/paritytech/polkadot-sdk/issues/5477#issuecomment-2598809344\n(which can now be resolved). The invalid transactions found during\nmempool revalidation are now also removed from the `view_store`. No\ndangling invalid transaction shall be left in the pool.\n(https://github.com/paritytech/polkadot-sdk/pull/6661/commits/bfec26253219044adaf6cdb3fff542c12460ed5a)\n- The support for dropping invalid transactions reported from the views\nwas also added. This should never happen, but if for any case all views\nwill report invalid transcation (which previously was valid) the\ntransaction will be dropped from the pool\n(https://github.com/paritytech/polkadot-sdk/pull/6661/commits/48214a381438f9b78653b8995bb4e62df9da504a).\n\n\n\nfixes: #6008, #5477\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-02-14T17:30:00Z",
          "tree_id": "8ccc2e09a0dbdbbd524395d236277d529cfb92d2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c94df1bc1be2203f5bc8aa960b0b46a44d97d120"
        },
        "date": 1739557665351,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18406043,
            "range": "± 269240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18683430,
            "range": "± 362132",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19819192,
            "range": "± 173821",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23629859,
            "range": "± 180771",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52954479,
            "range": "± 697995",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 313572805,
            "range": "± 4839105",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2354634189,
            "range": "± 91636148",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15109153,
            "range": "± 157560",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15348022,
            "range": "± 82590",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15673498,
            "range": "± 274287",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19827828,
            "range": "± 330207",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50394615,
            "range": "± 704193",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 305391118,
            "range": "± 2647281",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2450036256,
            "range": "± 13826679",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8779c606a44ce4b93f2c900b1cfffa877071be23",
          "message": "Bump the ci_dependencies group across 1 directory with 6 updates (#7555)\n\nBumps the ci_dependencies group with 6 updates in the / directory:\n\n| Package | From | To |\n| --- | --- | --- |\n|\n[lycheeverse/lychee-action](https://github.com/lycheeverse/lychee-action)\n| `2.1.0` | `2.3.0` |\n| [Swatinem/rust-cache](https://github.com/swatinem/rust-cache) |\n`2.7.5` | `2.7.7` |\n|\n[peter-evans/create-pull-request](https://github.com/peter-evans/create-pull-request)\n| `7.0.5` | `7.0.6` |\n|\n[docker/setup-buildx-action](https://github.com/docker/setup-buildx-action)\n| `3.7.1` | `3.9.0` |\n|\n[aws-actions/configure-aws-credentials](https://github.com/aws-actions/configure-aws-credentials)\n| `4.0.2` | `4.1.0` |\n|\n[actions/attest-build-provenance](https://github.com/actions/attest-build-provenance)\n| `1.4.3` | `2.2.0` |\n\n\nUpdates `lycheeverse/lychee-action` from 2.1.0 to 2.3.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/lycheeverse/lychee-action/releases\">lycheeverse/lychee-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>Version 2.3.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>feat: support ARM workers by <a\nhref=\"https://github.com/LesnyRumcajs\"><code>@​LesnyRumcajs</code></a>\nin <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/273\">lycheeverse/lychee-action#273</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a\nhref=\"https://github.com/LesnyRumcajs\"><code>@​LesnyRumcajs</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/273\">lycheeverse/lychee-action#273</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/lycheeverse/lychee-action/compare/v2...v2.3.0\">https://github.com/lycheeverse/lychee-action/compare/v2...v2.3.0</a></p>\n<h2>Version 2.2.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Fix if expressions in GitHub actions by <a\nhref=\"https://github.com/YDX-2147483647\"><code>@​YDX-2147483647</code></a>\nin <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/265\">lycheeverse/lychee-action#265</a></li>\n<li>Update README.md to include continue-on-error: true in action by <a\nhref=\"https://github.com/psobolewskiPhD\"><code>@​psobolewskiPhD</code></a>\nin <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/267\">lycheeverse/lychee-action#267</a></li>\n<li>Bump default version to latest (0.18.0) by <a\nhref=\"https://github.com/trask\"><code>@​trask</code></a> in <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/269\">lycheeverse/lychee-action#269</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a\nhref=\"https://github.com/psobolewskiPhD\"><code>@​psobolewskiPhD</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/267\">lycheeverse/lychee-action#267</a></li>\n<li><a href=\"https://github.com/trask\"><code>@​trask</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/pull/269\">lycheeverse/lychee-action#269</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/lycheeverse/lychee-action/compare/v2...v2.2.0\">https://github.com/lycheeverse/lychee-action/compare/v2...v2.2.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/lycheeverse/lychee-action/commit/f613c4a64e50d792e0b31ec34bbcbba12263c6a6\"><code>f613c4a</code></a>\nfeat: support ARM workers (<a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/issues/273\">#273</a>)</li>\n<li><a\nhref=\"https://github.com/lycheeverse/lychee-action/commit/f796c8b7d468feb9b8c0a46da3fac0af6874d374\"><code>f796c8b</code></a>\nBump default version to latest (0.18.0) (<a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/issues/269\">#269</a>)</li>\n<li><a\nhref=\"https://github.com/lycheeverse/lychee-action/commit/4aa18b6ccdac05029fab067313a6a04f941e6494\"><code>4aa18b6</code></a>\nUpdate README.md to include continue-on-error: true in action (<a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/issues/267\">#267</a>)</li>\n<li><a\nhref=\"https://github.com/lycheeverse/lychee-action/commit/5cd5ba7877bce8b3973756ae3c9474ce1e50be2f\"><code>5cd5ba7</code></a>\nFix if expressions in GitHub actions (<a\nhref=\"https://redirect.github.com/lycheeverse/lychee-action/issues/265\">#265</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/lycheeverse/lychee-action/compare/f81112d0d2814ded911bd23e3beaa9dda9093915...f613c4a64e50d792e0b31ec34bbcbba12263c6a6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `Swatinem/rust-cache` from 2.7.5 to 2.7.7\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/swatinem/rust-cache/releases\">Swatinem/rust-cache's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.7.7</h2>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/Swatinem/rust-cache/compare/v2.7.6...v2.7.7\">https://github.com/Swatinem/rust-cache/compare/v2.7.6...v2.7.7</a></p>\n<h2>v2.7.6</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Updated artifact upload action to v4 by <a\nhref=\"https://github.com/guylamar2006\"><code>@​guylamar2006</code></a>\nin <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/212\">Swatinem/rust-cache#212</a></li>\n<li>Adds an option to do lookup-only of the cache by <a\nhref=\"https://github.com/danlec\"><code>@​danlec</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/217\">Swatinem/rust-cache#217</a></li>\n<li>add runner OS in cache key by <a\nhref=\"https://github.com/rnbguy\"><code>@​rnbguy</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/220\">Swatinem/rust-cache#220</a></li>\n<li>Allow opting out of caching $CARGO_HOME/bin. by <a\nhref=\"https://github.com/benjyw\"><code>@​benjyw</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/216\">Swatinem/rust-cache#216</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a\nhref=\"https://github.com/guylamar2006\"><code>@​guylamar2006</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/212\">Swatinem/rust-cache#212</a></li>\n<li><a href=\"https://github.com/danlec\"><code>@​danlec</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/217\">Swatinem/rust-cache#217</a></li>\n<li><a href=\"https://github.com/rnbguy\"><code>@​rnbguy</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/220\">Swatinem/rust-cache#220</a></li>\n<li><a href=\"https://github.com/benjyw\"><code>@​benjyw</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/216\">Swatinem/rust-cache#216</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/Swatinem/rust-cache/compare/v2.7.5...v2.7.6\">https://github.com/Swatinem/rust-cache/compare/v2.7.5...v2.7.6</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/f0deed1e0edfc6a9be95417288c0e1099b1eeec3\"><code>f0deed1</code></a>\n2.7.7</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/008623fb834cadde1d7ccee1a26dc84acb660ec3\"><code>008623f</code></a>\nalso cache <code>cargo install</code> metadata</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/720f7e45ccee46c12a7b1d7bed2ab733be9be5a1\"><code>720f7e4</code></a>\n2.7.6</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/4b1f006ad2112a11d66969e219444096a98af937\"><code>4b1f006</code></a>\nupdate dependencies, in particular <code>@actions/cache</code></li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/e8e63cdbf2788df3801e6f9a81516b2ca8391886\"><code>e8e63cd</code></a>\nAllow opting out of caching $CARGO_HOME/bin. (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/216\">#216</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/9a2e0d32122f6883cb48fad7a1ac5c49f25b7661\"><code>9a2e0d3</code></a>\nadd runner OS in cache key (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/220\">#220</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/c00f3025caeee0e9c78c18c43de11ab15fd3b486\"><code>c00f302</code></a>\nAdds an option to do lookup-only of the cache (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/217\">#217</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/68b3cb7503c78e67dae8373749990a220eb65352\"><code>68b3cb7</code></a>\nUpdated artifact upload action to v4 (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/212\">#212</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/swatinem/rust-cache/compare/v2.7.5...f0deed1e0edfc6a9be95417288c0e1099b1eeec3\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `peter-evans/create-pull-request` from 7.0.5 to 7.0.6\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/peter-evans/create-pull-request/releases\">peter-evans/create-pull-request's\nreleases</a>.</em></p>\n<blockquote>\n<h2>Create Pull Request v7.0.6</h2>\n<p>⚙️ Fixes an issue with commit signing where unicode characters in\nfile paths were not preserved.</p>\n<h2>What's Changed</h2>\n<ul>\n<li>build(deps-dev): bump <code>@​vercel/ncc</code> from 0.38.1 to\n0.38.2 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3365\">peter-evans/create-pull-request#3365</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3370\">peter-evans/create-pull-request#3370</a></li>\n<li>build(deps): bump\n<code>@​octokit/plugin-rest-endpoint-methods</code> from 13.2.4 to\n13.2.5 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3375\">peter-evans/create-pull-request#3375</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.50 to\n18.19.54 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3376\">peter-evans/create-pull-request#3376</a></li>\n<li>build(deps): bump <code>@​octokit/plugin-paginate-rest</code> from\n11.3.3 to 11.3.5 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3377\">peter-evans/create-pull-request#3377</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3388\">peter-evans/create-pull-request#3388</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.54 to\n18.19.55 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3400\">peter-evans/create-pull-request#3400</a></li>\n<li>build(deps): bump <code>@​actions/core</code> from 1.10.1 to 1.11.1\nby <a href=\"https://github.com/dependabot\"><code>@​dependabot</code></a>\nin <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3401\">peter-evans/create-pull-request#3401</a></li>\n<li>build(deps): bump\n<code>@​octokit/plugin-rest-endpoint-methods</code> from 13.2.5 to\n13.2.6 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3403\">peter-evans/create-pull-request#3403</a></li>\n<li>build(deps-dev): bump eslint-plugin-import from 2.30.0 to 2.31.0 by\n<a href=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3402\">peter-evans/create-pull-request#3402</a></li>\n<li>build(deps): bump <code>@​octokit/plugin-throttling</code> from\n9.3.1 to 9.3.2 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3404\">peter-evans/create-pull-request#3404</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3423\">peter-evans/create-pull-request#3423</a></li>\n<li>build(deps-dev): bump typescript from 5.6.2 to 5.6.3 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3441\">peter-evans/create-pull-request#3441</a></li>\n<li>build(deps): bump undici from 6.19.8 to 6.20.1 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3442\">peter-evans/create-pull-request#3442</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3451\">peter-evans/create-pull-request#3451</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.55 to\n18.19.58 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3457\">peter-evans/create-pull-request#3457</a></li>\n<li>build(deps-dev): bump <code>@​types/jest</code> from 29.5.13 to\n29.5.14 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3462\">peter-evans/create-pull-request#3462</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.58 to\n18.19.60 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3463\">peter-evans/create-pull-request#3463</a></li>\n<li>chore: don't bundle undici by <a\nhref=\"https://github.com/benmccann\"><code>@​benmccann</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3475\">peter-evans/create-pull-request#3475</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3478\">peter-evans/create-pull-request#3478</a></li>\n<li>chore: use node-fetch-native support for proxy env vars by <a\nhref=\"https://github.com/peter-evans\"><code>@​peter-evans</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3483\">peter-evans/create-pull-request#3483</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.60 to\n18.19.64 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3488\">peter-evans/create-pull-request#3488</a></li>\n<li>build(deps-dev): bump undici from 6.20.1 to 6.21.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3499\">peter-evans/create-pull-request#3499</a></li>\n<li>build(deps-dev): bump <code>@​vercel/ncc</code> from 0.38.2 to\n0.38.3 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3500\">peter-evans/create-pull-request#3500</a></li>\n<li>docs: note <code>push-to-repo</code> classic PAT\n<code>workflow</code> scope requirement by <a\nhref=\"https://github.com/scop\"><code>@​scop</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3511\">peter-evans/create-pull-request#3511</a></li>\n<li>docs: spelling fixes by <a\nhref=\"https://github.com/scop\"><code>@​scop</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3512\">peter-evans/create-pull-request#3512</a></li>\n<li>build(deps-dev): bump typescript from 5.6.3 to 5.7.2 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3516\">peter-evans/create-pull-request#3516</a></li>\n<li>build(deps-dev): bump prettier from 3.3.3 to 3.4.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3517\">peter-evans/create-pull-request#3517</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.64 to\n18.19.66 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3518\">peter-evans/create-pull-request#3518</a></li>\n<li>docs(README): clarify that an existing open PR is managed by <a\nhref=\"https://github.com/caugner\"><code>@​caugner</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3498\">peter-evans/create-pull-request#3498</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3529\">peter-evans/create-pull-request#3529</a></li>\n<li>build(deps): bump <code>@​octokit/plugin-paginate-rest</code> from\n11.3.5 to 11.3.6 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3542\">peter-evans/create-pull-request#3542</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.66 to\n18.19.67 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3543\">peter-evans/create-pull-request#3543</a></li>\n<li>build(deps-dev): bump prettier from 3.4.0 to 3.4.1 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3544\">peter-evans/create-pull-request#3544</a></li>\n<li>build(deps-dev): bump eslint-import-resolver-typescript from 3.6.3\nto 3.7.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3559\">peter-evans/create-pull-request#3559</a></li>\n<li>build(deps-dev): bump prettier from 3.4.1 to 3.4.2 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3560\">peter-evans/create-pull-request#3560</a></li>\n<li>build(deps-dev): bump <code>@​types/node</code> from 18.19.67 to\n18.19.68 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3570\">peter-evans/create-pull-request#3570</a></li>\n<li>build(deps): bump p-limit from 6.1.0 to 6.2.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3578\">peter-evans/create-pull-request#3578</a></li>\n<li>Update distribution by <a\nhref=\"https://github.com/actions-bot\"><code>@​actions-bot</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3583\">peter-evans/create-pull-request#3583</a></li>\n<li>fix: preserve unicode in filepaths when commit signing by <a\nhref=\"https://github.com/peter-evans\"><code>@​peter-evans</code></a> in\n<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3588\">peter-evans/create-pull-request#3588</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/benmccann\"><code>@​benmccann</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3475\">peter-evans/create-pull-request#3475</a></li>\n<li><a href=\"https://github.com/scop\"><code>@​scop</code></a> made their\nfirst contribution in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3511\">peter-evans/create-pull-request#3511</a></li>\n<li><a href=\"https://github.com/caugner\"><code>@​caugner</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/pull/3498\">peter-evans/create-pull-request#3498</a></li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/67ccf781d68cd99b580ae25a5c18a1cc84ffff1f\"><code>67ccf78</code></a>\nfix: preserve unicode in filepaths when commit signing (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3588\">#3588</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/bb88e27d3f9cc69c8bc689eba126096c6fe3dded\"><code>bb88e27</code></a>\nbuild: update distribution (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3583\">#3583</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/b378ed537a3374cbb7642141277ace10488f9318\"><code>b378ed5</code></a>\nbuild(deps): bump p-limit from 6.1.0 to 6.2.0 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3578\">#3578</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/fa9200e5b4f0d3fe4adc6d4a980fdb27ca333ed2\"><code>fa9200e</code></a>\nbuild(deps-dev): bump <code>@​types/node</code> from 18.19.67 to\n18.19.68 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3570\">#3570</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/16e0059bfd236716f0191bfcfa63d9ded4cf325f\"><code>16e0059</code></a>\nbuild(deps-dev): bump prettier from 3.4.1 to 3.4.2 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3560\">#3560</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/5bffd5ae80c9e3cdce3fdaba74ba437193643add\"><code>5bffd5a</code></a>\nbuild(deps-dev): bump eslint-import-resolver-typescript (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3559\">#3559</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/a22a0ddc2127a4161a9f144623d1e51be98d81aa\"><code>a22a0dd</code></a>\nbuild(deps-dev): bump prettier from 3.4.0 to 3.4.1 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3544\">#3544</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/b27ce378c8a71596550fb729c05c9a998f8ff26f\"><code>b27ce37</code></a>\nbuild(deps-dev): bump <code>@​types/node</code> from 18.19.66 to\n18.19.67 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3543\">#3543</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/4e0cc19e22f9071762b3542aa9fa90a1d682dd32\"><code>4e0cc19</code></a>\nbuild(deps): bump <code>@​octokit/plugin-paginate-rest</code> from\n11.3.5 to 11.3.6 (<a\nhref=\"https://redirect.github.com/peter-evans/create-pull-request/issues/3542\">#3542</a>)</li>\n<li><a\nhref=\"https://github.com/peter-evans/create-pull-request/commit/25b6871a4ebe4c3585f47c7a687ac6fd0ec0e32d\"><code>25b6871</code></a>\ndocs: update scopes for push-to-fork</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/peter-evans/create-pull-request/compare/5e914681df9dc83aa4e4905692ca88beb2f9e91f...67ccf781d68cd99b580ae25a5c18a1cc84ffff1f\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `docker/setup-buildx-action` from 3.7.1 to 3.9.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/docker/setup-buildx-action/releases\">docker/setup-buildx-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v3.9.0</h2>\n<ul>\n<li>Bump <code>@​docker/actions-toolkit</code> from 0.48.0 to 0.54.0 in\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/402\">docker/setup-buildx-action#402</a>\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/404\">docker/setup-buildx-action#404</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.8.0...v3.9.0\">https://github.com/docker/setup-buildx-action/compare/v3.8.0...v3.9.0</a></p>\n<h2>v3.8.0</h2>\n<ul>\n<li>Make cloud prefix optional to download buildx if driver is cloud by\n<a href=\"https://github.com/crazy-max\"><code>@​crazy-max</code></a> in\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/390\">docker/setup-buildx-action#390</a></li>\n<li>Bump <code>@​actions/core</code> from 1.10.1 to 1.11.1 in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/370\">docker/setup-buildx-action#370</a></li>\n<li>Bump <code>@​docker/actions-toolkit</code> from 0.39.0 to 0.48.0 in\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/389\">docker/setup-buildx-action#389</a></li>\n<li>Bump cross-spawn from 7.0.3 to 7.0.6 in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/382\">docker/setup-buildx-action#382</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.7.1...v3.8.0\">https://github.com/docker/setup-buildx-action/compare/v3.7.1...v3.8.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/f7ce87c1d6bead3e36075b2ce75da1f6cc28aaca\"><code>f7ce87c</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/404\">#404</a>\nfrom docker/dependabot/npm_and_yarn/docker/actions-to...</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/aa1e2a0b496d6cd3474071c7b0ab0eea5948de3a\"><code>aa1e2a0</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/673e00877621ac201ca3084ec053b85e9b65063e\"><code>673e008</code></a>\nbuild(deps): bump <code>@​docker/actions-toolkit</code> from 0.53.0 to\n0.54.0</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/ba31df4664624f17e1b1ef1c9c85ed1ca9463a6d\"><code>ba31df4</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/402\">#402</a>\nfrom docker/dependabot/npm_and_yarn/docker/actions-to...</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/5475af18ec6f58d53e9452495e8db373e6dcb469\"><code>5475af1</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/acacad903e45f670c1e2d4638f4ee5f24b03e6b6\"><code>acacad9</code></a>\nbuild(deps): bump <code>@​docker/actions-toolkit</code> from 0.48.0 to\n0.53.0</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/6a25f988bdfa969e96a38fc9f843ea31e0b5df27\"><code>6a25f98</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/396\">#396</a>\nfrom crazy-max/bake-v6</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/ca1af179f5dc207dc723446d832eb3f77d3912dc\"><code>ca1af17</code></a>\nupdate bake-action to v6</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/6524bf65af31da8d45b59e8c27de4bd072b392f5\"><code>6524bf6</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/390\">#390</a>\nfrom crazy-max/buildx-cloud-latest</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/8d5e0747fc81adde3c75a11c4ab1cd6e831c45b5\"><code>8d5e074</code></a>\nchore: update generated content</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/c47758b77c9736f4b2ef4073d4d51994fabfe349...f7ce87c1d6bead3e36075b2ce75da1f6cc28aaca\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `aws-actions/configure-aws-credentials` from 4.0.2 to 4.1.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/releases\">aws-actions/configure-aws-credentials's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v4.1.0</h2>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.3...v4.1.0\">4.1.0</a>\n(2025-02-08)</h2>\n<h3>Features</h3>\n<ul>\n<li>idempotent fetch (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1289\">#1289</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/eb70354fb423a380b6e4ab4b9f15d2ee9ffae911\">eb70354</a>)</li>\n</ul>\n<h3>Bug Fixes</h3>\n<ul>\n<li>build failure due to tests (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1283\">#1283</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/134d71efe0ecbe9ad6965f2f766c0cae63a7685f\">134d71e</a>)</li>\n<li>Dependabot autoapprove (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1284\">#1284</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/b9ee51dc600fe38c892e24f60ca26476e0e0b6de\">b9ee51d</a>)</li>\n<li>Dependabot autoapprove id-token write permission (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1285\">#1285</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/f0af89b102390dcf10ce402195d74a98f24861f3\">f0af89b</a>)</li>\n<li>typo (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1281\">#1281</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/39fd91c08ed8bf770034de4e62662503e8007d76\">39fd91c</a>)</li>\n</ul>\n<h2>v4.0.3</h2>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.2...v4.0.3\">4.0.3</a>\n(2025-01-27)</h2>\n<h3>Features</h3>\n<ul>\n<li>added release-please action config (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/0f88004d9c27e0bdbbc254b3f7c8053cb38f04d7\">0f88004</a>)</li>\n</ul>\n<h3>Bug Fixes</h3>\n<ul>\n<li>add id-token permission to automerge (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/97834a484a5ab3c40fa9e2eb40fcf8041105a573\">97834a4</a>)</li>\n<li>cpy syntax on npm package (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1195\">#1195</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/83b5a565471214aec459e234bef606339fe07111\">83b5a56</a>)</li>\n<li>force push packaged files to main (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/bfd218503eb87938c29603a551e19c6b594f5fe5\">bfd2185</a>)</li>\n</ul>\n<h3>Miscellaneous Chores</h3>\n<ul>\n<li>release 4.0.3 (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/ca00fd4d3842ad58c3c21ebfe69defa1f0e7bdc4\">ca00fd4</a>)</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/blob/main/CHANGELOG.md\">aws-actions/configure-aws-credentials's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<p>All notable changes to this project will be documented in this file.\nSee <a\nhref=\"https://github.com/conventional-changelog/standard-version\">standard-version</a>\nfor commit guidelines.</p>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.3...v4.1.0\">4.1.0</a>\n(2025-02-08)</h2>\n<h3>Features</h3>\n<ul>\n<li>idempotent fetch (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1289\">#1289</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/eb70354fb423a380b6e4ab4b9f15d2ee9ffae911\">eb70354</a>)</li>\n</ul>\n<h3>Bug Fixes</h3>\n<ul>\n<li>build failure due to tests (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1283\">#1283</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/134d71efe0ecbe9ad6965f2f766c0cae63a7685f\">134d71e</a>)</li>\n<li>Dependabot autoapprove (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1284\">#1284</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/b9ee51dc600fe38c892e24f60ca26476e0e0b6de\">b9ee51d</a>)</li>\n<li>Dependabot autoapprove id-token write permission (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1285\">#1285</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/f0af89b102390dcf10ce402195d74a98f24861f3\">f0af89b</a>)</li>\n<li>typo (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1281\">#1281</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/39fd91c08ed8bf770034de4e62662503e8007d76\">39fd91c</a>)</li>\n</ul>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.2...v4.0.3\">4.0.3</a>\n(2025-01-27)</h2>\n<h3>Features</h3>\n<ul>\n<li>added release-please action config (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/0f88004d9c27e0bdbbc254b3f7c8053cb38f04d7\">0f88004</a>)</li>\n</ul>\n<h3>Bug Fixes</h3>\n<ul>\n<li>add id-token permission to automerge (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/97834a484a5ab3c40fa9e2eb40fcf8041105a573\">97834a4</a>)</li>\n<li>cpy syntax on npm package (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1195\">#1195</a>)\n(<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/83b5a565471214aec459e234bef606339fe07111\">83b5a56</a>)</li>\n<li>force push packaged files to main (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/bfd218503eb87938c29603a551e19c6b594f5fe5\">bfd2185</a>)</li>\n</ul>\n<h3>Miscellaneous Chores</h3>\n<ul>\n<li>release 4.0.3 (<a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/ca00fd4d3842ad58c3c21ebfe69defa1f0e7bdc4\">ca00fd4</a>)</li>\n</ul>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.1...v4.0.2\">4.0.2</a>\n(2024-02-09)</h2>\n<ul>\n<li>Revert 4.0.1 to remove warning</li>\n</ul>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v4.0.0...v4.0.1\">4.0.1</a>\n(2023-10-03)</h2>\n<h3>Documentation</h3>\n<ul>\n<li>Throw a warning when customers use long-term credentials.</li>\n</ul>\n<h2><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/v3.0.2...v4.0.0\">4.0.0</a>\n(2023-09-11)</h2>\n<ul>\n<li>Upgraded runtime to <code>node20</code> from\n<code>node16</code></li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/ececac1a45f3b08a01d2dd070d28d111c5fe6722\"><code>ececac1</code></a>\nchore(main): release 4.1.0 (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1282\">#1282</a>)</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/16fec6080fdb89d4b237dee411b7bf8f3658ec97\"><code>16fec60</code></a>\nchore: Update dist</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/eb70354fb423a380b6e4ab4b9f15d2ee9ffae911\"><code>eb70354</code></a>\nfeat: idempotent fetch (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1289\">#1289</a>)</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/3478c15aa1cf2543c22efcbbd3e483d49c3a31d7\"><code>3478c15</code></a>\nchore(deps-dev): bump memfs from 4.14.0 to 4.17.0 (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1250\">#1250</a>)</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/a69d38c39d4e4ef6ebd2825ae1bf38948c4a63fa\"><code>a69d38c</code></a>\nchore: Update dist</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/6b1d0f829dbf80f581d095620da2a3d26e7f3b81\"><code>6b1d0f8</code></a>\nchore(deps-dev): bump <code>@​smithy/property-provider</code> from 3.1.8\nto 4.0.1 (<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1246\">#1246</a>)</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/f021516513c128da882cdc5b42712935bb1f89fc\"><code>f021516</code></a>\nchore: remove role session name</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/1c8dbbcc0280c0f2662d0842550c5c63ef1572a4\"><code>1c8dbbc</code></a>\nchore(deps-dev): bump <code>@​vercel/ncc</code> from 0.38.2 to 0.38.3\n(<a\nhref=\"https://redirect.github.com/aws-actions/configure-aws-credentials/issues/1204\">#1204</a>)</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/ce290d67fea24eb4c156f6207fa1d18c8ff6c891\"><code>ce290d6</code></a>\nchore: change dependabot role session name</li>\n<li><a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/commit/1780ebd97bfd07ffbef8765880395a9bfed87d09\"><code>1780ebd</code></a>\nchore: create one-off test for CAWSC</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/aws-actions/configure-aws-credentials/compare/e3dd6a429d7300a6a4c196c26e071d42e0343502...ececac1a45f3b08a01d2dd070d28d111c5fe6722\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions/attest-build-provenance` from 1.4.3 to 2.2.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions/attest-build-provenance/releases\">actions/attest-build-provenance's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.2.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump actions/attest from v2.1.0 to v2.2.0 by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/449\">actions/attest-build-provenance#449</a>\n<ul>\n<li>Includes support for now <code>subject-checksums</code> input\nparameter</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v2.1.0...v2.2.0\">https://github.com/actions/attest-build-provenance/compare/v2.1.0...v2.2.0</a></p>\n<h2>v2.1.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Update README w/ note about GH plans supporting attestations by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/414\">actions/attest-build-provenance#414</a></li>\n<li>Add <code>attestation-id</code> and <code>attestation-url</code>\noutputs by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/415\">actions/attest-build-provenance#415</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v2.0.1...v2.1.0\">https://github.com/actions/attest-build-provenance/compare/v2.0.1...v2.1.0</a></p>\n<h2>v2.0.1</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump actions/attest from 2.0.0 to 2.0.1 by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/406\">actions/attest-build-provenance#406</a>\n<ul>\n<li>Deduplicate subjects before adding to in-toto statement</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v2.0.0...v2.0.1\">https://github.com/actions/attest-build-provenance/compare/v2.0.0...v2.0.1</a></p>\n<h2>v2.0.0</h2>\n<p>The <code>attest-build-provenance</code> action now supports\nattesting multiple subjects simultaneously. When identifying multiple\nsubjects with the <code>subject-path</code> input a single attestation\nis created with references to each of the supplied subjects, rather than\ngenerating separate attestations for each artifact. This reduces the\nnumber of attestations that you need to create and manage.</p>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump cross-spawn from 7.0.3 to 7.0.6 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/319\">actions/attest-build-provenance#319</a></li>\n<li>Prepare v2.0.0 release by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/321\">actions/attest-build-provenance#321</a>\n<ul>\n<li>Bump <code>actions/attest</code> from 1.4.1 to 2.0.0 (w/\nmulti-subject attestation support)</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v1.4.4...v2.0.0\">https://github.com/actions/attest-build-provenance/compare/v1.4.4...v2.0.0</a></p>\n<h2>v1.4.4</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump predicate action from 1.1.3 to 1.1.4 by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/310\">actions/attest-build-provenance#310</a>\n<ul>\n<li>Bump <code>@​actions/core</code> from 1.10.1 to 1.11.1 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/275\">actions/attest-build-provenance#275</a></li>\n<li>Bump <code>@​actions/attest</code> from 1.4.2 to 1.5.0 by <a\nhref=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/309\">actions/attest-build-provenance#309</a>\n<ul>\n<li>Fix SLSA provenance bug related to <code>workflow_ref</code> OIDC\ntoken claims containing the &quot;@&quot; symbol in the tag name (<a\nhref=\"https://redirect.github.com/actions/toolkit/pull/1863\">actions/toolkit#1863</a>)</li>\n</ul>\n</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v1.4.3...v1.4.4\">https://github.com/actions/attest-build-provenance/compare/v1.4.3...v1.4.4</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/520d128f165991a6c774bcb264f323e3d70747f4\"><code>520d128</code></a>\nbump actions/attest from v2.1.0 to v2.2.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/449\">#449</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/5d2ced98e37711f730ee81bf4f290e59f429cea9\"><code>5d2ced9</code></a>\nAdd example of upload-artifaction integration (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/450\">#450</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/3c016c14be1000a987f5724861e76009e48945d1\"><code>3c016c1</code></a>\nbump actions/attest from v2.1.0 to v2.2.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/449\">#449</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/e06bbafba962e6346493c5fd57cfcfe0873b0474\"><code>e06bbaf</code></a>\nBump the npm-development group with 3 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/447\">#447</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/47c6e87ba15264d457c4b3aeeaca0aa4ef36cfc8\"><code>47c6e87</code></a>\nBump the npm-development group with 4 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/444\">#444</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/c083b467494a647632714fee9685ca81f12ca4d6\"><code>c083b46</code></a>\nBump the npm-development group with 2 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/438\">#438</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/1b4b366241fcfed280d0cc0db3d44132575a6a87\"><code>1b4b366</code></a>\nBump typescript-eslint in the npm-development group (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/434\">#434</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/963f8a02f24ac90336362e63ca6730cf69ad102e\"><code>963f8a0</code></a>\nBump the npm-development group with 2 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/429\">#429</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/4ecada3c132a6497cc654fcac5c8644da6815ca6\"><code>4ecada3</code></a>\nBump the npm-development group across 1 directory with 3 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/422\">#422</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/f4b7552a127d7acf1bef22d5bc9f315117aebfcd\"><code>f4b7552</code></a>\nbump eslint from 8.57.1 to 9.16.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/418\">#418</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v1.4.3...520d128f165991a6c774bcb264f323e3d70747f4\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-14T17:59:16Z",
          "tree_id": "e101977371525922363acc15ae7489a52252200b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8779c606a44ce4b93f2c900b1cfffa877071be23"
        },
        "date": 1739559197150,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17750979,
            "range": "± 166090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17906627,
            "range": "± 81569",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19499197,
            "range": "± 71190",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22831998,
            "range": "± 229532",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51672180,
            "range": "± 707724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 291240375,
            "range": "± 2225754",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2456218448,
            "range": "± 105587663",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14435943,
            "range": "± 141646",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14517998,
            "range": "± 80528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14903597,
            "range": "± 161508",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18859041,
            "range": "± 159704",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48473451,
            "range": "± 532064",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 284802016,
            "range": "± 2189933",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2337006516,
            "range": "± 17148681",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alexandre.balde@parity.io",
            "name": "Alexandre R. Baldé",
            "username": "rockbmb"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c1915afc48e8bd89fe2c44d6813a6e8b96013c29",
          "message": "Add minor improvements to `chill_other` test (#7553)\n\n# Description\n\nhttps://github.com/open-web3-stack/polkadot-ecosystem-tests/pull/174\nshowed the test for the `pallet_staking::chill_other` extrinsic could be\nmore exhaustive.\n\nThis PR adds those checks, and also a few more to another test related\nto `chill_other`,\n`pallet_staking::tests::change_of_absolute_max_nominations`.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-02-14T20:23:50Z",
          "tree_id": "e466f9789e42817101d501e97c882910994cdc83",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c1915afc48e8bd89fe2c44d6813a6e8b96013c29"
        },
        "date": 1739567735907,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17723303,
            "range": "± 121044",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18015789,
            "range": "± 80574",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19374208,
            "range": "± 133968",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23214376,
            "range": "± 489628",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52192218,
            "range": "± 2618663",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 289896785,
            "range": "± 4430164",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2301341297,
            "range": "± 82912440",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14273937,
            "range": "± 145198",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14247944,
            "range": "± 599070",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14758162,
            "range": "± 77441",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18659659,
            "range": "± 151975",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48747562,
            "range": "± 664653",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 282841583,
            "range": "± 2243260",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2359074238,
            "range": "± 10410759",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "5588131+kianenigma@users.noreply.github.com",
            "name": "Kian Paimani",
            "username": "kianenigma"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a025562b65f71dab8c2a16e027ba6efe4972818a",
          "message": "[AHM] Multi-block staking election pallet (#7282)\n\n## Multi Block Election Pallet\n\nThis PR adds the first iteration of the multi-block staking pallet. \n\nFrom this point onwards, the staking and its election provider pallets\nare being customized to work in AssetHub. While usage in solo-chains is\nstill possible, it is not longer the main focus of this pallet. For a\nsafer usage, please fork and user an older version of this pallet.\n\n---\n\n## Replaces\n\n- [x] https://github.com/paritytech/polkadot-sdk/pull/6034 \n- [x] https://github.com/paritytech/polkadot-sdk/pull/5272\n\n## Related PRs: \n\n- [x] https://github.com/paritytech/polkadot-sdk/pull/7483\n- [ ] https://github.com/paritytech/polkadot-sdk/pull/7357\n- [ ] https://github.com/paritytech/polkadot-sdk/pull/7424\n- [ ] https://github.com/paritytech/polkadot-staking-miner/pull/955\n\nThis branch can be periodically merged into\nhttps://github.com/paritytech/polkadot-sdk/pull/7358 ->\nhttps://github.com/paritytech/polkadot-sdk/pull/6996\n\n## TODOs: \n\n- [x] rebase to master \n- Benchmarking for staking critical path\n  - [x] snapshot\n  - [x] election result\n- Benchmarking for EPMB critical path\n  - [x] snapshot\n  - [x] verification\n  - [x] submission\n  - [x] unsigned submission\n  - [ ] election results fetching\n- [ ] Fix deletion weights. Either of\n  - [ ] Garbage collector + lazy removal of all paged storage items\n  - [ ] Confirm that deletion is small PoV footprint.\n- [ ] Move election prediction to be push based. @tdimitrov \n- [ ] integrity checks for bounds \n- [ ] Properly benchmark this as a part of CI -- for now I will remove\nthem as they are too slow\n- [x] add try-state to all pallets\n- [x] Staking to allow genesis dev accounts to be created internally\n- [x] Decouple miner config so @niklasad1 can work on the miner\n72841b731727e69db38f9bd616190aa8d50a56ba\n- [x] duplicate snapshot page reported by @niklasad1 \n- [ ] https://github.com/paritytech/polkadot-sdk/pull/6520 or equivalent\n-- during snapshot, `VoterList` must be locked\n- [ ] Move target snapshot to a separate block\n\n---------\n\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-14T23:47:22Z",
          "tree_id": "e232f7cf4a9ef6b2814baa6e1328e4d6ceb810c3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a025562b65f71dab8c2a16e027ba6efe4972818a"
        },
        "date": 1739579721535,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17821944,
            "range": "± 86318",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18022056,
            "range": "± 89432",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19566622,
            "range": "± 304360",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23396634,
            "range": "± 734809",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53092672,
            "range": "± 643234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 315359675,
            "range": "± 5022660",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2620828556,
            "range": "± 40253294",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14752468,
            "range": "± 160619",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14790340,
            "range": "± 113950",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15263427,
            "range": "± 130030",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19506079,
            "range": "± 165985",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50851699,
            "range": "± 500419",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 299031493,
            "range": "± 4526577",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2433377366,
            "range": "± 12623735",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jose@blockdeep.io",
            "name": "José Molina Colmenero",
            "username": "Moliholy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "c578318b8f764f62e9a94fd343b79953f7129154",
          "message": "Add chain properties to chain-spec-builder (#7368)\n\nThis PR adds support for chain properties to `chain-spec-builder`. Now\nproperties can be specified as such:\n\n```sh\n$ chain-spec-builder create -r $RUNTIME_PATH --properties tokenSymbol=DUMMY,tokenDecimals=6,isEthereum=false\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Michal Kucharczyk <1728078+michalkucharczyk@users.noreply.github.com>",
          "timestamp": "2025-02-15T11:08:44Z",
          "tree_id": "4c1c8de3985a5c662c6ebec375c3598e6c2af8ac",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c578318b8f764f62e9a94fd343b79953f7129154"
        },
        "date": 1739620718805,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19415262,
            "range": "± 297329",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20053306,
            "range": "± 392720",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21429247,
            "range": "± 480586",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 27183393,
            "range": "± 1493314",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 59271700,
            "range": "± 2442568",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 408040430,
            "range": "± 12776997",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2814187794,
            "range": "± 191217924",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14931048,
            "range": "± 243967",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15999354,
            "range": "± 116774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16667934,
            "range": "± 341939",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21546754,
            "range": "± 355150",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 58655301,
            "range": "± 1534453",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 364371079,
            "range": "± 8874271",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2817511276,
            "range": "± 40804941",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "73991674+Nathy-bajo@users.noreply.github.com",
            "name": "Nathaniel Bajo",
            "username": "Nathy-bajo"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "54deef92f86a16d413f98a6f68418d11c8142a25",
          "message": "Documentation update for weight. #7354 (#7376)\n\nresolves #7354\n\nPolkadot address: 121HJWZtD13GJQPD82oEj3gSeHqsRYm1mFgRALu4L96kfPD1\n\n---------\n\nCo-authored-by: Guillaume Thiolliere <guillaume.thiolliere@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-02-15T12:48:15Z",
          "tree_id": "3de10e9a8fc43a6f814d3dea927f1f17952982a8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/54deef92f86a16d413f98a6f68418d11c8142a25"
        },
        "date": 1739628863687,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17551905,
            "range": "± 230468",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18002798,
            "range": "± 116089",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19422880,
            "range": "± 180708",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23349319,
            "range": "± 241356",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53228189,
            "range": "± 1081260",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 330090509,
            "range": "± 3584776",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2468690475,
            "range": "± 99564322",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14513703,
            "range": "± 140482",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14673841,
            "range": "± 95257",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15130071,
            "range": "± 142490",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19723517,
            "range": "± 246803",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50762679,
            "range": "± 581112",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 293777376,
            "range": "± 4096333",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2419910079,
            "range": "± 13146306",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49699333+dependabot[bot]@users.noreply.github.com",
            "name": "dependabot[bot]",
            "username": "dependabot[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0f2024f5f33acc54d90fe209289d0195c8af9a70",
          "message": "Bump enumflags2 from 0.7.7 to 0.7.11 (#7426)\n\nBumps [enumflags2](https://github.com/meithecatte/enumflags2) from 0.7.7\nto 0.7.11.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/meithecatte/enumflags2/releases\">enumflags2's\nreleases</a>.</em></p>\n<blockquote>\n<h2>Release 0.7.10</h2>\n<ul>\n<li>Fix a case where the <code>#[bitflags]</code> macro would access the\ncrate through <code>enumflags2::...</code> instead of\n<code>::enumflags2::...</code>. This makes the generated code more\nrobust and avoids triggering the <code>unused_qualifications</code>\nlint. (<a\nhref=\"https://redirect.github.com/meithecatte/enumflags2/issues/58\">#58</a>)</li>\n<li>Rework the proc-macro to use <code>syn</code> with the\n<code>derive</code> feature (as opposed to <code>full</code>). This\nreduces the <code>cargo build</code> time for <code>enumflags2</code> by\nabout 20%.</li>\n</ul>\n<h2>Release 0.7.9</h2>\n<ul>\n<li>The <code>BitFlag</code> trait now includes convenience re-exports\nfor the constructors of <code>BitFlags</code>. This lets you do\n<code>MyFlag::from_bits</code> instead\n<code>BitFlags::&lt;MyFlag&gt;::from_bits</code> where the type of the\nflag cannot be inferred from context (thanks <a\nhref=\"https://github.com/ronnodas\"><code>@​ronnodas</code></a>).</li>\n<li>The documentation now calls out the fact that the implementation of\n<code>PartialOrd</code> may not be what you expect (reported by <a\nhref=\"https://github.com/ronnodas\"><code>@​ronnodas</code></a>).</li>\n</ul>\n<h2>Release 0.7.8</h2>\n<ul>\n<li>New API: <code>BitFlags::set</code>. Sets the value of a specific\nflag to that of the <code>bool</code> passed as argument. (thanks, <a\nhref=\"https://github.com/m4dh0rs3\"><code>@​m4dh0rs3</code></a>)</li>\n<li><code>BitFlags</code> now implements <code>PartialOrd</code> and\n<code>Ord</code>, to make it possible to use it as a key in a\n<code>BTreeMap</code>.</li>\n<li>The bounds on the implementation of <code>Hash</code> got improved,\nso that it is possible to use it in code generic over <code>T:\nBitFlag</code>.</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/cc09d89bc4ef20fbf4c8016a40e160fe47b2d042\"><code>cc09d89</code></a>\nRelease 0.7.11</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/24f03afbd0c23adaf0873a941600bd0b3b7ba302\"><code>24f03af</code></a>\nmake_bitflags: Allow omitting { } for singular flags</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/754a8de723c54c79b2a8ab6993adc59b478273b0\"><code>754a8de</code></a>\nExpand some aspects of the documentation</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/aec9558136a53a952f39b74a4a0688a31423b815\"><code>aec9558</code></a>\nUpdate ui tests for latest nightly</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/8205d5ba03ccc9ccb7407693440f8e47f8ceeeb4\"><code>8205d5b</code></a>\nRelease 0.7.10</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/1c78f097165436d043f48b9f6183501f84ff965f\"><code>1c78f09</code></a>\nRun clippy with only the declared syn features</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/561fe5eaf7ba6daeb267a41343f6def2a8b86ad7\"><code>561fe5e</code></a>\nEmit a proper error if bitflags enum is generic</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/f3bb174beb27a1d1ef28dcf03fb607a3bb7c6e55\"><code>f3bb174</code></a>\nAvoid depending on syn's <code>full</code> feature flag</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/e01808be0f151ac251121833d3225debd253ca3a\"><code>e01808b</code></a>\nAlways use absolute paths in generated proc macro code</li>\n<li><a\nhref=\"https://github.com/meithecatte/enumflags2/commit/f08cd33a18511608f4a881e53c4f4c1b951301e0\"><code>f08cd33</code></a>\nSpecify the Rust edition for the whole test package</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/meithecatte/enumflags2/compare/v0.7.7...v0.7.11\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=enumflags2&package-manager=cargo&previous-version=0.7.7&new-version=0.7.11)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-02-16T01:30:29Z",
          "tree_id": "6b36c37a53521f7ab1ba91044916764e13370e4d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0f2024f5f33acc54d90fe209289d0195c8af9a70"
        },
        "date": 1739672822718,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18318802,
            "range": "± 182693",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18479077,
            "range": "± 154310",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20260062,
            "range": "± 237922",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23959746,
            "range": "± 200761",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54529269,
            "range": "± 837494",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 344108228,
            "range": "± 16224669",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2646116600,
            "range": "± 83214326",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15172854,
            "range": "± 170732",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14975298,
            "range": "± 206369",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15506905,
            "range": "± 136434",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19493968,
            "range": "± 248752",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51280472,
            "range": "± 919637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 307572765,
            "range": "± 4508291",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2494232123,
            "range": "± 17938312",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "fedelia.cj@gmail.com",
            "name": "rainb0w-pr0mise",
            "username": "rainb0w-pr0mise"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ead8fbdfa727b707f988147dc1b92b9d46a92ce5",
          "message": "`pallet-utility: if_else` (#6321)\n\n# Utility Call Fallback\n\nThis introduces a new extrinsic: **`if_else`**\n\nWhich first attempts to dispatch the `main` call(s). If the `main`\ncall(s) fail, the `fallback` call(s) is dispatched instead. Both calls\nare executed with the same origin.\n\nIn the event of a fallback failure the whole call fails with the weights\nreturned.\n\n## Use Case\nSome use cases might involve submitting a `batch` type call in either\nmain, fallback or both.\n\nResolves #6000\n\nPolkadot Address: 1HbdqutFR8M535LpbLFT41w3j7v9ptEYGEJKmc6PKpqthZ8\n\n---------\n\nCo-authored-by: rainbow-promise <154476501+rainbow-promise@users.noreply.github.com>\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-02-17T02:36:13Z",
          "tree_id": "37d789f1d5ff2dc93d3df79ac99d60e00054297e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ead8fbdfa727b707f988147dc1b92b9d46a92ce5"
        },
        "date": 1739762728364,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18928396,
            "range": "± 288930",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19349950,
            "range": "± 221022",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20927520,
            "range": "± 133633",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25621123,
            "range": "± 240592",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63229316,
            "range": "± 675777",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 370348783,
            "range": "± 5314566",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2477401536,
            "range": "± 252365187",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15532530,
            "range": "± 153839",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15575664,
            "range": "± 125925",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16301426,
            "range": "± 177311",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20832265,
            "range": "± 162584",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 55869816,
            "range": "± 1373054",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 345771403,
            "range": "± 7553315",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2684395339,
            "range": "± 10876058",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hola@pablodorado.com",
            "name": "Pablo Andrés Dorado Suárez",
            "username": "pandres95"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "83db0474f4df9988b01c6125a49cc59aa1b90939",
          "message": "[Assets] Implement `pallet-assets-holder` (#4530)\n\nCloses #4315\n\n---------\n\nCo-authored-by: Guillaume Thiolliere <guillaume.thiolliere@parity.io>",
          "timestamp": "2025-02-17T03:51:31Z",
          "tree_id": "dff27a9cbe49e97bbc5536ece11c373c851206fa",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83db0474f4df9988b01c6125a49cc59aa1b90939"
        },
        "date": 1739767648656,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19918645,
            "range": "± 222024",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20386516,
            "range": "± 127542",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21857736,
            "range": "± 304126",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25523087,
            "range": "± 245962",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 59971269,
            "range": "± 1617722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 395657556,
            "range": "± 7806780",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2856180132,
            "range": "± 130086321",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16065114,
            "range": "± 126638",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16210493,
            "range": "± 216630",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16629874,
            "range": "± 125471",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21965678,
            "range": "± 394633",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 56864703,
            "range": "± 946375",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 375498945,
            "range": "± 6953332",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2683309672,
            "range": "± 30937992",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8cca727fbda68b9d8678573ab1ade0c3d6c9f104",
          "message": "[pallet-revive] rpc add --earliest-receipt-block (#7589)\n\nAdd a cli option to skip searching receipts for blocks older than the\nspecified limit\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-17T11:25:02Z",
          "tree_id": "fee5fde1bd6253bed22bd8441c1f7acf3cb8e4a0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8cca727fbda68b9d8678573ab1ade0c3d6c9f104"
        },
        "date": 1739795803508,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17466453,
            "range": "± 134748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17838624,
            "range": "± 95872",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19137231,
            "range": "± 191247",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22543583,
            "range": "± 251572",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51064037,
            "range": "± 433969",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 297922801,
            "range": "± 1946358",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2410482343,
            "range": "± 57037666",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14076682,
            "range": "± 64924",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14230628,
            "range": "± 92917",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14703105,
            "range": "± 81637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18522638,
            "range": "± 171009",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47864424,
            "range": "± 259090",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 281737581,
            "range": "± 1277124",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2292368482,
            "range": "± 14585018",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "daniel@olanod.com",
            "name": "Daniel Olano",
            "username": "olanod"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c078d2f41cf8ecd28ff5279fcebb22da418a14b9",
          "message": "Change pallet referenda TracksInfo::tracks to return an iterator (#2072)\n\nReturning an iterator in `TracksInfo::tracks()` instead of a static\nslice allows for more flexible implementations of `TracksInfo` that can\nuse the chain storage without compromising a lot on the\nperformance/memory penalty if we were to return an owned `Vec` instead.\n\n---------\n\nCo-authored-by: Pablo Andrés Dorado Suárez <hola@pablodorado.com>",
          "timestamp": "2025-02-17T12:18:01Z",
          "tree_id": "3d28fc6fe791ad9e1007ba5083be3815f513318b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c078d2f41cf8ecd28ff5279fcebb22da418a14b9"
        },
        "date": 1739797878272,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17762006,
            "range": "± 155612",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18007453,
            "range": "± 195796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19615023,
            "range": "± 317750",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23540016,
            "range": "± 324084",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55213964,
            "range": "± 947430",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 315474215,
            "range": "± 4948437",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2580512379,
            "range": "± 138523381",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14631214,
            "range": "± 133350",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14860182,
            "range": "± 186684",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15242219,
            "range": "± 171248",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19899268,
            "range": "± 220356",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50805305,
            "range": "± 699134",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 295791948,
            "range": "± 2891333",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2413979339,
            "range": "± 22728373",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "oliver.tale-yazdi@parity.io",
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ca91d4b58c7af8c88f47c27ccac742d9d9f5c8a7",
          "message": "[AHM] Make pallet types public (#7579)\n\nPreparation for AHM and making stuff public.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-02-17T14:27:14Z",
          "tree_id": "10fcd07c64098d39323121073c4fea7a9c248503",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ca91d4b58c7af8c88f47c27ccac742d9d9f5c8a7"
        },
        "date": 1739805645455,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18555970,
            "range": "± 126410",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18825057,
            "range": "± 151115",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20232268,
            "range": "± 134398",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23956893,
            "range": "± 184097",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52873789,
            "range": "± 807619",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 323412574,
            "range": "± 8520912",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2444211926,
            "range": "± 148037234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15132020,
            "range": "± 153776",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14990855,
            "range": "± 142442",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15403912,
            "range": "± 75852",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19755444,
            "range": "± 131441",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51397986,
            "range": "± 522773",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 309282396,
            "range": "± 2651472",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2455602211,
            "range": "± 7651424",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "09d3754319cf67aea22595823ffbcb65b7b09165",
          "message": "libp2p: Enhance logging targets for granular control  (#7494)\n\nThis PR modifies the libp2p networking-specific log targets for granular\ncontrol (e.g., just enabling trace for req-resp).\n\nPreviously, all logs were outputted to `sub-libp2p` target, flooding the\nlog messages on busy validators.\n\n### Changes\n- Discover: `sub-libp2p::discovery`\n- Notification/behaviour: `sub-libp2p::notification::behaviour`\n- Notification/handler: `sub-libp2p::notification::handler`\n- Notification/service: `sub-libp2p::notification::service`\n- Notification/upgrade: `sub-libp2p::notification::upgrade`\n- Request response: `sub-libp2p::request-response`\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2025-02-17T15:27:14Z",
          "tree_id": "e8e50c2b988b76b07636b5f257112287bda76739",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/09d3754319cf67aea22595823ffbcb65b7b09165"
        },
        "date": 1739810081693,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18868194,
            "range": "± 196413",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19059094,
            "range": "± 163414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20872169,
            "range": "± 224415",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24182334,
            "range": "± 338652",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57270049,
            "range": "± 1087066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 371412590,
            "range": "± 6237006",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2499215856,
            "range": "± 126433353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15218427,
            "range": "± 149966",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15222311,
            "range": "± 165870",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15651647,
            "range": "± 123401",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19939832,
            "range": "± 160038",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51678753,
            "range": "± 743588",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 323506705,
            "range": "± 3698218",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2555161726,
            "range": "± 23144084",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "adrian@parity.io",
            "name": "Adrian Catangiu",
            "username": "acatangiu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "430a016ced42fa9658a9b95cca7ef7af8458e007",
          "message": "integration tests: add more emulated bridge tests (#7576)\n\nAdd emulated e2e tests for following scenarios:\n\nExporting native asset to another ecosystem:\n- Sending WNDs from Penpal Westend to Penpal Rococo: PPW->AHW->AHR->PPR\n- Sending WNDs from Westend Relay to Penpal Rococo: W->AHW->AHR->PPR\n   Example: Westend Treasury funds something on Rococo Parachain.\n\nImporting native asset from another ecosystem to its native ecosystem:\n- Sending ROCs from Penpal Westend to Penpal Rococo: PPW->AHW->AHR->PPR\n- Sending ROCs from Penpal Westend to Rococo Relay: PPW->AHW->AHR->R\n   Example: Westend Parachain returns some funds to Rococo Treasury.\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-02-17T17:09:31Z",
          "tree_id": "2d06cfe92cd79c034ef623ed7dd95161afa33b97",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/430a016ced42fa9658a9b95cca7ef7af8458e007"
        },
        "date": 1739815226247,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20120619,
            "range": "± 184484",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20474900,
            "range": "± 199115",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22344716,
            "range": "± 173642",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26667805,
            "range": "± 833614",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 64715315,
            "range": "± 963445",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 407187909,
            "range": "± 6469955",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2828813340,
            "range": "± 182479346",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16262383,
            "range": "± 259361",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16116984,
            "range": "± 175517",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16900401,
            "range": "± 212387",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21560887,
            "range": "± 449700",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57878770,
            "range": "± 1372184",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 390448081,
            "range": "± 10759645",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2817555452,
            "range": "± 25233744",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "nikola.djoric@parity.io",
            "name": "nprt",
            "username": "nprt"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d61032b977b18cc962cdb23e7a3df7386e503e09",
          "message": "implement web3_clientVersion (#7580)\n\nImplements the `web3_clientVersion` method. This is a common requirement\nfor external Ethereum libraries when querying a client.\n\nFixes paritytech/contract-issues#26.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-17T17:50:46Z",
          "tree_id": "26d38d57645767a30e5bae619bb71290ee780d15",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d61032b977b18cc962cdb23e7a3df7386e503e09"
        },
        "date": 1739817869781,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21051769,
            "range": "± 889929",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22672621,
            "range": "± 1010006",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 23384514,
            "range": "± 292820",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 28686931,
            "range": "± 1077770",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 69876841,
            "range": "± 1939583",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 419778338,
            "range": "± 18285944",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2952614730,
            "range": "± 71477564",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16526479,
            "range": "± 366328",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16740444,
            "range": "± 408626",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17279006,
            "range": "± 288389",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22126467,
            "range": "± 813993",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 60413513,
            "range": "± 1540944",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 389355463,
            "range": "± 9046930",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2924856578,
            "range": "± 54128266",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "10196091+Ank4n@users.noreply.github.com",
            "name": "Ankan",
            "username": "Ank4n"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dda2cb5969985ccbf67581e18eb7c579849e27bb",
          "message": "[Staking] Bounded Slashing: Paginated Offence Processing & Slash Application (#7424)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/3610.\n\nhelps https://github.com/paritytech/polkadot-sdk/issues/6344, but need\nto migrate storage `Offences::Reports` before we can remove exposure\ndependency in RC pallets.\n\nreplaces https://github.com/paritytech/polkadot-sdk/issues/6788.\n\n## Context  \nSlashing in staking is unbounded currently, which is a major blocker\nuntil staking can move to a parachain (AH).\n\n### Current Slashing Process (Unbounded)  \n\n1. **Offence Reported**  \n- Offences include multiple validators, each with potentially large\nexposure pages.\n- Slashes are **computed immediately** and scheduled for application\nafter **28 eras**.\n\n2. **Slash Applied**  \n- All unapplied slashes are executed in **one block** at the start of\nthe **28th era**. This is an **unbounded operation**.\n\n\n### Proposed Slashing Process (Bounded)  \n\n1. **Offence Queueing**  \n   - Offences are **queued** after basic sanity checks.  \n\n2. **Paged Offence Processing (Computing Slash)**  \n   - Slashes are **computed one validator exposure page at a time**.  \n   - **Unapplied slashes** are stored in a **double map**:  \n     - **Key 1 (k1):** `EraIndex`  \n- **Key 2 (k2):** `(Validator, SlashFraction, PageIndex)` — a unique\nidentifier for each slash page\n\n3. **Paged Slash Application**  \n- Slashes are **applied one page at a time** across multiple blocks.\n- Slash application starts at the **27th era** (one era earlier than\nbefore) to ensure all slashes are applied **before stakers can unbond**\n(which starts from era 28 onwards).\n\n---\n\n## Worst-Case Block Calculation for Slash Application  \n\n### Polkadot:  \n- **1 era = 24 hours**, **1 block = 6s** → **14,400 blocks/era**  \n- On parachains (**12s blocks**) → **7,200 blocks/era**  \n\n### Kusama:  \n- **1 era = 6 hours**, **1 block = 6s** → **3,600 blocks/era**  \n- On parachains (**12s blocks**) → **1,800 blocks/era**  \n\n### Worst-Case Assumptions:  \n- **Total stakers:** 40,000 nominators, 1000 validators. (Polkadot\ncurrently has ~23k nominators and 500 validators)\n- **Max slashed:** 50% so 20k nominators, 250 validators.  \n- **Page size:** Validators with multiple page: (512 + 1)/2 = 256 ,\nValidators with single page: 1\n\n### Calculation:  \nThere might be a more accurate way to calculate this worst-case number,\nand this estimate could be significantly higher than necessary, but it\nshouldn’t exceed this value.\n\nBlocks needed: 250 + 20k/256 = ~330 blocks.\n\n##  *Potential Improvement:*  \n- Consider adding an **Offchain Worker (OCW)** task to further optimize\nslash application in future updates.\n- Dynamically batch unapplied slashes based on number of nominators in\nthe page, or process until reserved weight limit is exhausted.\n\n----\n## Summary of Changes  \n\n### Storage  \n- **New:**  \n  - `OffenceQueue` *(StorageDoubleMap)*  \n    - **K1:** Era  \n    - **K2:** Offending validator account  \n    - **V:** `OffenceRecord`  \n  - `OffenceQueueEras` *(StorageValue)*  \n    - **V:** `BoundedVec<EraIndex, BoundingDuration>`  \n  - `ProcessingOffence` *(StorageValue)*  \n    - **V:** `(Era, offending validator account, OffenceRecord)`  \n\n- **Changed:**  \n  - `UnappliedSlashes`:  \n    - **Old:** `StorageMap<K -> Era, V -> Vec<UnappliedSlash>>`  \n- **New:** `StorageDoubleMap<K1 -> Era, K2 -> (validator_acc, perbill,\npage_index), V -> UnappliedSlash>`\n\n### Events  \n- **New:**  \n  - `SlashComputed { offence_era, slash_era, offender, page }`  \n  - `SlashCancelled { slash_era, slash_key, payout }`  \n\n### Error  \n- **Changed:**  \n  - `InvalidSlashIndex` → Renamed to `InvalidSlashRecord`  \n- **Removed:**  \n  - `NotSortedAndUnique`  \n- **Added:**  \n  - `EraNotStarted`  \n\n### Call  \n- **Changed:**  \n  - `cancel_deferred_slash(era, slash_indices: Vec<u32>)`  \n    → Now takes `Vec<(validator_acc, slash_fraction, page_index)>`  \n- **New:**  \n- `apply_slash(slash_era, slash_key: (validator_acc, slash_fraction,\npage_index))`\n\n### Runtime Config  \n- `FullIdentification` is now set to a unit type (`()`) / null identity,\nreplacing the previous exposure type for all runtimes using\n`pallet_session::historical`.\n\n## TODO\n- [x] Fixed broken `CancelDeferredSlashes`.\n- [x] Ensure on_offence called only with validator account for\nidentification everywhere.\n- [ ] Ensure we never need to read full exposure.\n- [x] Tests for multi block processing and application of slash.\n- [x] Migrate UnappliedSlashes \n- [x] Bench (crude, needs proper bench as followup)\n  - [x] on_offence()\n  - [x] process_offence()\n  - [x] apply_slash()\n \n \n## Followups (tracker\n[link](https://github.com/paritytech/polkadot-sdk/issues/7596))\n- [ ] OCW task to process offence + apply slashes.\n- [ ] Minimum time for governance to cancel deferred slash.\n- [ ] Allow root or staking admin to add a custom slash.\n- [ ] Test HistoricalSession proof works fine with eras before removing\nexposure as full identity.\n- [ ] Properly bench offence processing and slashing.\n- [ ] Handle Offences::Reports migration when removing validator\nexposure as identity.\n\n---------\n\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: kianenigma <kian@parity.io>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-17T23:07:09Z",
          "tree_id": "5efd5751e1737e0a7baed1e1acf129543a30f847",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dda2cb5969985ccbf67581e18eb7c579849e27bb"
        },
        "date": 1739836768681,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18198527,
            "range": "± 167560",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18667269,
            "range": "± 247412",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20111131,
            "range": "± 115105",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24218832,
            "range": "± 476167",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55119859,
            "range": "± 813104",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 333778876,
            "range": "± 10321552",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2616691972,
            "range": "± 76786918",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14766822,
            "range": "± 150174",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14953658,
            "range": "± 100602",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15506332,
            "range": "± 85627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19715246,
            "range": "± 422178",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50958768,
            "range": "± 1159096",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 316321158,
            "range": "± 4561296",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2475695231,
            "range": "± 16280920",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "10196091+Ank4n@users.noreply.github.com",
            "name": "Ankan",
            "username": "Ank4n"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "43ea306f6307dff908551cb91099ef6268502ee0",
          "message": "[AHM] Moves disabling logic into pallet-session (#7581)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/6508.\n\n## TODO\n- [x] Migrate storage `DisabledValidators` both in pallet-session and\npallet-staking.\n- [ ] Test that disabled validator resets at era change.\n- [ ] Add always sorted try-runtime test for `DisabledValidators`.\n- [ ] More test coverage for the disabling logic.\n\n---------\n\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Guillaume Thiolliere <gui.thiolliere@gmail.com>\nCo-authored-by: kianenigma <kian@parity.io>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-18T03:25:52Z",
          "tree_id": "ed5e254fe70d039f7ef437ac47a431dbf59763d1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/43ea306f6307dff908551cb91099ef6268502ee0"
        },
        "date": 1739853174814,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18307229,
            "range": "± 239769",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18816970,
            "range": "± 209206",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20561667,
            "range": "± 191858",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24153666,
            "range": "± 345192",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 58910880,
            "range": "± 737202",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 362877619,
            "range": "± 4981129",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2803964386,
            "range": "± 230100345",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15231786,
            "range": "± 166820",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15382594,
            "range": "± 201163",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15817679,
            "range": "± 153025",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20565721,
            "range": "± 134562",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54504108,
            "range": "± 786524",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 327820430,
            "range": "± 4071570",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2577493121,
            "range": "± 23052575",
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
          "distinct": true,
          "id": "b6512be748e3dbf26d5ef56020b40bdb4772b1ab",
          "message": "Make all prdoc valid and add CI job to check prdoc with `prdoc check` (#7543)\n\nSome prdoc are invalid, `prdoc check` is failing for them. This also\nbroke usage of parity-publish.\n\nThis PR fixes the invalid prdoc, and add a ci job to check the prdoc are\nvalid. I don't think the check is unstable considering it is a simple\nyaml check, so I put the job as required.\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-02-18T07:31:13Z",
          "tree_id": "9f755e7d479183da859e585388215f6e01dd5189",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6512be748e3dbf26d5ef56020b40bdb4772b1ab"
        },
        "date": 1739866905811,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20856845,
            "range": "± 296099",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 21231460,
            "range": "± 666109",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 23574952,
            "range": "± 918800",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 27838315,
            "range": "± 740359",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 66538420,
            "range": "± 1861525",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 420985195,
            "range": "± 7478652",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2615098786,
            "range": "± 40552206",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 17157069,
            "range": "± 297052",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17402123,
            "range": "± 313006",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17755278,
            "range": "± 391735",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22597692,
            "range": "± 614552",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 62619554,
            "range": "± 1744851",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 387211898,
            "range": "± 7017821",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2976592651,
            "range": "± 66115766",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "69342343+MrishoLukamba@users.noreply.github.com",
            "name": "Mrisho Lukamba",
            "username": "MrishoLukamba"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a20c882bf3552c4c2c4ed1993ae52f8dc33a0c3",
          "message": "feat(integration test) test omni node dev mod work with dev_json file (#7511)\n\nCloses #7452 \n\nAdds new test for omni node on dev mode working correctly with\ndev_chain_spec.json\n\n@skunert\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-18T10:00:19Z",
          "tree_id": "3ced79ac588a8b9b1d514785da83bc411c1bfdab",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6a20c882bf3552c4c2c4ed1993ae52f8dc33a0c3"
        },
        "date": 1739877268592,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 21121552,
            "range": "± 430590",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 22370614,
            "range": "± 519738",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 23605927,
            "range": "± 438785",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 28564220,
            "range": "± 636260",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 65188456,
            "range": "± 3399472",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 417219426,
            "range": "± 9215997",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2692192154,
            "range": "± 131483467",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16383645,
            "range": "± 405390",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17649645,
            "range": "± 353472",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 18439765,
            "range": "± 597403",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23402818,
            "range": "± 1374955",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 60946102,
            "range": "± 1485590",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 405858315,
            "range": "± 6027179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2879924870,
            "range": "± 104588332",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "fd72d58313c297a10600037ce1bb88ec958d722e",
          "message": "[pallet-revive] move exec tests (#7590)\n\nMoving exec tests into a new file\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-02-18T10:40:15Z",
          "tree_id": "78cf5e093fb14a983806f031555e9656cb1a0ade",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd72d58313c297a10600037ce1bb88ec958d722e"
        },
        "date": 1739879355085,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18537770,
            "range": "± 206832",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18771817,
            "range": "± 268722",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21123429,
            "range": "± 233351",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25203639,
            "range": "± 386282",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57068698,
            "range": "± 1072404",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 369688887,
            "range": "± 6551531",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2644069253,
            "range": "± 20335165",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15157543,
            "range": "± 152584",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15540185,
            "range": "± 274462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15912136,
            "range": "± 166935",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20164938,
            "range": "± 281490",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53594097,
            "range": "± 764017",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 336802603,
            "range": "± 5468713",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2558187819,
            "range": "± 24136082",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "413616788014b4539cb3b039dbffc29267fb6f3c",
          "message": "[Release/CI|CD] Fix for the branch-off pipeline (#7608)\n\nThis PR contains a tiny fix for the release branch-off pipeline, so that\nnode version bump works again.",
          "timestamp": "2025-02-18T16:54:37Z",
          "tree_id": "835fe0ed64c4f72cd7d6e2cceb6c9e0006a1ccbe",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/413616788014b4539cb3b039dbffc29267fb6f3c"
        },
        "date": 1739902436149,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19229227,
            "range": "± 486931",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19602038,
            "range": "± 157591",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21386332,
            "range": "± 239337",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25315705,
            "range": "± 222186",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56568256,
            "range": "± 826894",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 347496620,
            "range": "± 4867595",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2566208712,
            "range": "± 41835437",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15418206,
            "range": "± 297920",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15304646,
            "range": "± 95280",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15731196,
            "range": "± 185777",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20768081,
            "range": "± 325462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54022462,
            "range": "± 1852341",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 324941296,
            "range": "± 3174791",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2545157219,
            "range": "± 23806799",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "5f6c8e8fd20f19de0b592ef2413e934e97c9bc68",
          "message": "HashAndNumber: Ord, Eq, PartialOrd, PartialEq implemented (#7612)\n\nThis PR adds implementation of `Ord, Eq, PartialOrd, PartialEq` traits\nfor [`HashAndNumber`\n](https://github.com/paritytech/polkadot-sdk/blob/6e645915639ee0bf682de06a0306a4baf712c1d2/substrate/primitives/blockchain/src/header_metadata.rs#L149-L154)\nstruct.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-18T17:44:51Z",
          "tree_id": "ed2d391838a25109d392ea414cf46fd2c5e1a181",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5f6c8e8fd20f19de0b592ef2413e934e97c9bc68"
        },
        "date": 1739903708200,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17886262,
            "range": "± 155500",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18289838,
            "range": "± 135059",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19818882,
            "range": "± 196693",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23105346,
            "range": "± 176856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52761916,
            "range": "± 1016726",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 338912259,
            "range": "± 9298488",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2625883546,
            "range": "± 131079634",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14422111,
            "range": "± 227305",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14308649,
            "range": "± 100160",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14947381,
            "range": "± 224813",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19576597,
            "range": "± 502767",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50114961,
            "range": "± 577201",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 297388436,
            "range": "± 5206169",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2415782459,
            "range": "± 17014196",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hrishav@parity.io",
            "name": "castillax",
            "username": "castillax"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "a48b38942f8910aeaee1fdaa7794d0416291ca4b",
          "message": "Add note for organization contributors about creating branches directly (#7611)\n\n# Description\n\n* This PR adds a note to the CONTRIBUTING.md file to inform contributors\nwho are part of the organization that they do not need to fork the\nrepository. Instead, they can create a branch directly in the repository\nto send a pull request.\n\n## Changes\n\nAdded a note under the \"What?\" section in CONTRIBUTING.md to clarify\nthat organization contributors can create branches directly in the\nrepository.",
          "timestamp": "2025-02-19T11:41:56Z",
          "tree_id": "686d343baa24fee7ca5369532fa3dbf0e015e899",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a48b38942f8910aeaee1fdaa7794d0416291ca4b"
        },
        "date": 1739968802763,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19137830,
            "range": "± 179144",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19818001,
            "range": "± 184535",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21421468,
            "range": "± 190655",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25431505,
            "range": "± 367566",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57031091,
            "range": "± 1171155",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 388758336,
            "range": "± 4614170",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2699575876,
            "range": "± 34993521",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15532884,
            "range": "± 143597",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15330796,
            "range": "± 123866",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15934874,
            "range": "± 314162",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20004601,
            "range": "± 156245",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51587735,
            "range": "± 616430",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 331754779,
            "range": "± 4946774",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2558211680,
            "range": "± 22244363",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0c258c662d2d2117063455f2807d986c2cbd7de5",
          "message": "Derive `DecodeWithMemTracking` for bridge and xcm pallets (#7620)\n\nJust deriving `DecodeWithMemTracking` for the types used by the bridge,\nsnowbridge and xcm pallets",
          "timestamp": "2025-02-19T12:34:52Z",
          "tree_id": "4dfeff8b8b249ace581fc662b8aacad88aadfc1e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0c258c662d2d2117063455f2807d986c2cbd7de5"
        },
        "date": 1739972046807,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18008220,
            "range": "± 123367",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18305452,
            "range": "± 163682",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19602234,
            "range": "± 216080",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23557507,
            "range": "± 235081",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54081465,
            "range": "± 1083662",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 336798594,
            "range": "± 2940064",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2290929453,
            "range": "± 187758123",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14591025,
            "range": "± 181525",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14856752,
            "range": "± 122418",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15365593,
            "range": "± 107115",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20121210,
            "range": "± 317042",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50730952,
            "range": "± 621709",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 306295794,
            "range": "± 2692871",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2475929147,
            "range": "± 19234717",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "1728078+michalkucharczyk@users.noreply.github.com",
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8507e70f1a72cae2a4aecd6c377c197b2fbe7005",
          "message": "`fatxpool`: event streams moved to view domain (#7545)\n\n#### Overview\n\nThis pull request refactors the transaction pool `graph` module by\nrenaming components for better clarity. The `EventHandler` trait was\nintroduced to enhance flexibility in handling transaction lifecycle\nevents. Changes include renaming `graph::Listener` to\n`graph::EventDispatcher` and moving certain functionalities from `graph`\nto `view` module in order to decouple `graph` from `view`-related\nspecifics.\n\nThis PR does not introduce changes in the logic.\n\n#### Notes for Reviewers\nAll the changes looks dense at first, but in fact following was done:\n- The `graph::Listener` was renamed to\n[`graph::EventDispatcher`](https://github.com/paritytech/polkadot-sdk/blob/515cb4042d097581ed6b4195e57b04494e385a17/substrate/client/transaction-pool/src/graph/listener.rs#L74C12-L74C27),\nto better reflect its role in dispatching transaction-related events\nfrom `ValidatedPool`. The `EventDispatcher` now utilizes the `L:\nEventHandler` generic type to handle transaction status events.\n- The new\n[`EventHandler`](https://github.com/paritytech/polkadot-sdk/blob/515cb4042d097581ed6b4195e57b04494e385a17/substrate/client/transaction-pool/src/graph/listener.rs#L34)\ntrait was introduced to handle transaction lifecycle events, improving\nimplementation flexibility and providing clearer role descriptions\nwithin the system. Introduction of this trait allowed the removal of\n`View` related entities (e.g. streams) from the `ValidatedPool`'s event\ndispatcher (previously _listener_).\n- The _dropped monitoring_ and _aggregated events_ stream\n[functionalities](https://github.com/paritytech/polkadot-sdk/blob/515cb4042d097581ed6b4195e57b04494e385a17/substrate/client/transaction-pool/src/fork_aware_txpool/view.rs#L157-L188)\nand [related\ntypes](https://github.com/paritytech/polkadot-sdk/blob/515cb4042d097581ed6b4195e57b04494e385a17/substrate/client/transaction-pool/src/fork_aware_txpool/view.rs#L112-L121)\nwere moved from `graph::listener` to the `view` module. The\n[`ViewPoolObserver`](https://github.com/paritytech/polkadot-sdk/blob/515cb4042d097581ed6b4195e57b04494e385a17/substrate/client/transaction-pool/src/fork_aware_txpool/view.rs#L128C19-L128C35),\nwhich implements `EventHandler`, now provides the implementation of\nstreams feeding.\n- Fields, arguments, and variables previously named `listener` were\nrenamed to `event_dispatcher` to align with their purpose and type\nnaming.\n- Various structs such as `Pool` and `ValidatedPool` were updated to\ninclude a generic `L: EventHandler` across the codebase.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-02-19T15:03:30Z",
          "tree_id": "6f80971806f7a7583d408ecbf4b09fc1b5317db8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8507e70f1a72cae2a4aecd6c377c197b2fbe7005"
        },
        "date": 1739981071522,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17951949,
            "range": "± 138177",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18178901,
            "range": "± 270773",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19869358,
            "range": "± 140643",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23729534,
            "range": "± 256352",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54797912,
            "range": "± 838528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 360484179,
            "range": "± 5881010",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2675637376,
            "range": "± 71303516",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15140611,
            "range": "± 104158",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15339546,
            "range": "± 130800",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16122797,
            "range": "± 190390",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19994825,
            "range": "± 110477",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50231432,
            "range": "± 658647",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 303027920,
            "range": "± 3567544",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2447847511,
            "range": "± 28801346",
            "unit": "ns/iter"
          }
        ]
      },
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
          "distinct": true,
          "id": "ecca5826937e3b2b1e6f3988b006890e088aac2d",
          "message": "ci: change gcs bucket for forklift (#7621)\n\ncc https://github.com/paritytech/ci_cd/issues/1095",
          "timestamp": "2025-02-19T17:03:11Z",
          "tree_id": "90c94215226f367b707048728a66dc63cb03659b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ecca5826937e3b2b1e6f3988b006890e088aac2d"
        },
        "date": 1739990787309,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17789515,
            "range": "± 230903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18430606,
            "range": "± 195889",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19772651,
            "range": "± 210360",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23773167,
            "range": "± 287637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55936363,
            "range": "± 1251390",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 339452731,
            "range": "± 4605790",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2778433550,
            "range": "± 88139407",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14749695,
            "range": "± 205718",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14856366,
            "range": "± 145414",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15398342,
            "range": "± 121644",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19722876,
            "range": "± 300047",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50853456,
            "range": "± 710569",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 309951685,
            "range": "± 5471822",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2490280417,
            "range": "± 34872788",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jbrown@acuity.social",
            "name": "Jonathan Brown",
            "username": "ethernomad"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "959d91870d331852e20eae85e3e0186f2223a91d",
          "message": "[pallet-broker] add extrinsic to remove a lease (#7026)\n\n# Description\n\n#6929 requests more extrinsics for \"managing the network's coretime\nallocations without needing to dabble with migration+runtime upgrade or\nset/kill storage patterns\"\n\nThis pull request implements the remove_lease() extrinsic.\n\n\n## Integration\n\nDownstream projects need to benchmark the weight for the remove_lease()\nextrinsic.\n\n## Review Notes\n\nMentorship is requested to ensure this is implemented correctly.\n\nThe lease is removed from state using the TaskId as a key. Is this\nsufficient. Does the extrinsic need to do anything else?\n\n---------\n\nCo-authored-by: Jonathan Brown <jbrown@acuity.network>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-02-19T19:00:16Z",
          "tree_id": "3a52bfc1ee684f7251776837598cbe0b7939508f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/959d91870d331852e20eae85e3e0186f2223a91d"
        },
        "date": 1739995089954,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18411829,
            "range": "± 402122",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19131749,
            "range": "± 1038629",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20580810,
            "range": "± 286987",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24482999,
            "range": "± 344973",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57413219,
            "range": "± 1275056",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 382079304,
            "range": "± 5226519",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2660532139,
            "range": "± 139591175",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15165316,
            "range": "± 139127",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15637280,
            "range": "± 123748",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16106398,
            "range": "± 265728",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20440641,
            "range": "± 239899",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53426349,
            "range": "± 918815",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 335849539,
            "range": "± 6227763",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2553241895,
            "range": "± 16682854",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jbrown@acuity.social",
            "name": "Jonathan Brown",
            "username": "ethernomad"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "bd41848788383f39410c326508b052f94afb3130",
          "message": "[pallet-broker] add extrinsic to remove an assignment (#7080)\n\n# Description\n\n#6929 requests more extrinsics for \"managing the network's coretime\nallocations without needing to dabble with migration+runtime upgrade or\nset/kill storage patterns\"\n\nThis pull request implements the remove_assignment() extrinsic.\n\n\n## Integration\n\nDownstream projects need to benchmark the weight for the\nremove_assignment() extrinsic.\n\n---------\n\nCo-authored-by: Jonathan Brown <jbrown@acuity.network>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-02-19T22:17:52Z",
          "tree_id": "c2ce87e45839f35041990ecbba07b636332b3144",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd41848788383f39410c326508b052f94afb3130"
        },
        "date": 1740006549631,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20532241,
            "range": "± 734985",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20297907,
            "range": "± 415471",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22106044,
            "range": "± 657572",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25999254,
            "range": "± 605240",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 60525214,
            "range": "± 1499264",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 412734329,
            "range": "± 9192577",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2814666369,
            "range": "± 96041938",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16094600,
            "range": "± 291565",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16047948,
            "range": "± 428020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16628329,
            "range": "± 220527",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20941307,
            "range": "± 285132",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54277662,
            "range": "± 1398064",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 345276708,
            "range": "± 7641889",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2672747709,
            "range": "± 43860151",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "donal.murray@parity.io",
            "name": "Dónal Murray",
            "username": "seadanda"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ececb4cbd68d94d1f73b6ab8e5ad8afed1cc9456",
          "message": "Add sudo pallet to coretime-westend (#6960)\n\nAdd the sudo pallet to coretime-westend, allowing use in\ndevelopment/testing. Previously the coretime-rococo runtime was used in\nsituations like this, but since Rococo is now gone this can be used\ninstead.\n\nNo sudo key is added to Westend storage with this PR, since it's likely\nthat any updates will continue to be done over XCM. If this is something\nthat is wanted the key can be set via XCM.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Maksym H <1177472+mordamax@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-19T23:48:44Z",
          "tree_id": "f4bd2fe94bb4a20383106951a6b513205c99d724",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ececb4cbd68d94d1f73b6ab8e5ad8afed1cc9456"
        },
        "date": 1740011931108,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20218010,
            "range": "± 190026",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20450744,
            "range": "± 198203",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22379282,
            "range": "± 284498",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26230307,
            "range": "± 328569",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61228659,
            "range": "± 1425275",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 399691986,
            "range": "± 16131864",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2433943160,
            "range": "± 201479124",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15832098,
            "range": "± 234172",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15669969,
            "range": "± 109161",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15987041,
            "range": "± 136457",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20661112,
            "range": "± 388462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53522258,
            "range": "± 1146146",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 336451462,
            "range": "± 6352407",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2609772489,
            "range": "± 42122165",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "e8d17cbec9b7ea739f32e2d5f7d183558ce8efc1",
          "message": "derive `DecodeWithMemTracking` for `RuntimeCall` (#7634)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/7360",
          "timestamp": "2025-02-20T09:04:39Z",
          "tree_id": "45d7d9ecc46a0db307aacc8623d371a6c814b0b8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e8d17cbec9b7ea739f32e2d5f7d183558ce8efc1"
        },
        "date": 1740046064150,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20327422,
            "range": "± 1061650",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19900092,
            "range": "± 656155",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21899395,
            "range": "± 373420",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25775973,
            "range": "± 566701",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61807129,
            "range": "± 1211248",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 346488403,
            "range": "± 4663397",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2512138534,
            "range": "± 122319625",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15959574,
            "range": "± 463627",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16429977,
            "range": "± 314677",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16900042,
            "range": "± 367997",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23613960,
            "range": "± 986052",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 60363913,
            "range": "± 2452661",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 365792255,
            "range": "± 6980393",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2906349557,
            "range": "± 84809814",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "e2d3da6128bbd6a00e2036503fc2ed3e9c3c1b80",
          "message": "Update to Rust stable 1.84.1 (#7625)\n\nRef https://github.com/paritytech/ci_cd/issues/1107\n\nWe mainly need that so that we can finally compile the `pallet_revive`\nfixtures on stable. I did my best to keep the commits focused on one\nthing to make review easier.\n\nAll the changes are needed because rustc introduced more warnings or is\nmore strict about existing ones. Most of the stuff could just be fixed\nand the commits should be pretty self explanatory. However, there are a\nfew this that are notable:\n\n## `non_local_definitions `\n\nA lot of runtimes to write `impl` blocks inside functions. This makes\nsense to reduce the amount of conditional compilation. I guess I could\nhave moved them into a module instead. But I think allowing it here\nmakes sense to avoid the code churn.\n\n## `unexpected_cfgs`\n\nThe FRAME macros emit code that references various features like `std`,\n`runtime-benchmarks` or `try-runtime`. If a create that uses those\nmacros does not have those features we get this warning. Those were\nmostly when defining a `mock` runtime. I opted for silencing the warning\nin this case rather than adding not needed features.\n\nFor the benchmarking ui tests I opted for adding the `runtime-benchmark`\nfeature to the `Cargo.toml`.\n\n## Failing UI test\n\nI am bumping the `trybuild` version and regenerating the ui tests. The\nold version seems to be incompatible. This requires us to pass\n`deny_warnings` in `CARGO_ENCODED_RUSTFLAGS` as `RUSTFLAGS` is ignored\nin the new version.\n\n## Removing toolchain file from the pallet revive fixtures\n\nThis is no longer needed since the latest stable will compile them fine\nusing the `RUSTC_BOOTSTRAP=1`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-20T10:14:16Z",
          "tree_id": "74c2650dc4ff5e22e6fed633ed2696cb5eb9bdd3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e2d3da6128bbd6a00e2036503fc2ed3e9c3c1b80"
        },
        "date": 1740051114689,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17506284,
            "range": "± 88909",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17852331,
            "range": "± 248744",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19364300,
            "range": "± 116687",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22831636,
            "range": "± 151397",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50545082,
            "range": "± 526721",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 282136316,
            "range": "± 2337726",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2145119878,
            "range": "± 77714471",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14294927,
            "range": "± 110095",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14357235,
            "range": "± 138563",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14810831,
            "range": "± 129578",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18839992,
            "range": "± 135347",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47858968,
            "range": "± 315122",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 283100791,
            "range": "± 6961833",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2235147858,
            "range": "± 13800342",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "42e9de7f4fdb191cd531b443c68cfe19886ec311",
          "message": "net/litep2p: Bring the latest compatibility fixes via v0.9.1 (#7640)\n\nThis PR updates litep2p to version 0.9.1. The yamux config is entirely\nremoved to mirror the libp2p yamux upstream version.\nWhile at it, I had to bump indexmap and URL as well. \n\n\n## [0.9.1] - 2025-01-19\n\nThis release enhances compatibility between litep2p and libp2p by using\nthe latest Yamux upstream version. Additionally, it includes various\nimprovements and fixes to boost the stability and performance of the\nWebSocket stream and the multistream-select protocol.\n\n### Changed\n\n- yamux: Switch to upstream implementation while keeping the controller\nAPI ([#320](https://github.com/paritytech/litep2p/pull/320))\n- req-resp: Replace SubstreamSet with FuturesStream\n([#321](https://github.com/paritytech/litep2p/pull/321))\n- cargo: Bring up to date multiple dependencies\n([#324](https://github.com/paritytech/litep2p/pull/324))\n- build(deps): bump hickory-proto from 0.24.1 to 0.24.3\n([#323](https://github.com/paritytech/litep2p/pull/323))\n- build(deps): bump openssl from 0.10.66 to 0.10.70\n([#322](https://github.com/paritytech/litep2p/pull/322))\n\n### Fixed\n\n- websocket/stream: Fix unexpected EOF on `Poll::Pending` state\npoisoning ([#327](https://github.com/paritytech/litep2p/pull/327))\n- websocket/stream: Avoid memory allocations on flushing\n([#325](https://github.com/paritytech/litep2p/pull/325))\n- multistream-select: Enforce `io::error` instead of empty protocols\n([#318](https://github.com/paritytech/litep2p/pull/318))\n- multistream: Do not wait for negotiation in poll_close\n([#319](https://github.com/paritytech/litep2p/pull/319))\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-02-20T13:51:42Z",
          "tree_id": "51adcdd45b526acda223f94f135f363a92dd4c79",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/42e9de7f4fdb191cd531b443c68cfe19886ec311"
        },
        "date": 1740062790373,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17670093,
            "range": "± 127464",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18099569,
            "range": "± 136746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19390337,
            "range": "± 208082",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22904415,
            "range": "± 165931",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52394775,
            "range": "± 931704",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 315373442,
            "range": "± 2533092",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2417275497,
            "range": "± 80638743",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14704850,
            "range": "± 189724",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14847028,
            "range": "± 151621",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15355897,
            "range": "± 144656",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19441298,
            "range": "± 98521",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49432473,
            "range": "± 2303866",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 286501058,
            "range": "± 2792066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2324031853,
            "range": "± 10997729",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "claravanstaden64@gmail.com",
            "name": "Clara van Staden",
            "username": "claravanstaden"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dd7562ab5618d9c2a657ead42a0f92b3496ff80f",
          "message": "Snowbridge - Ethereum Electra Upgrade Support (#7075)\n\n# Description\n\nAdds support for the upcoming Ethereum Electra upgrade, while\nmaintaining backwards compatibility for the current Deneb fork.\n\n## Integration\n\nRelayers should be updated to send updated Electra consensus data\nstructures.\n\n## Review Notes\n\nThe [Ethereum Electra hard-fork\nconsensus](https://github.com/ethereum/consensus-specs/blob/dev/specs/electra/light-client/sync-protocol.md)\nchanges affecting the Ethereum light client are mainly isolated to the\n[Generalized\nIndexes](https://github.com/protolambda/eth2.0-ssz/blob/master/specs/navigation/generalized_indices.md)\nof data structures changing. Before Electra, these values were hardcoded\nin config. For Electra, these values change and needed to the updated.\nMethods were added to return the correct g-index for the current fork\nversion.\n\nData structures used by the Ethereum client did not change in this\nhard-fork. The BeaconState container has been updated with additional\nchanges, but because the on-chain code does not reference the\nBeaconState directly (only indirectly through merkle proofs), it is not\na concern. Off-chain relayers use the BeaconState to generate proofs,\nand so the relayer code has been updated accordingly.\n\n### Companion PR for off-chain relayers\nhttps://github.com/Snowfork/snowbridge/pull/1283\n\n---------\n\nCo-authored-by: claravanstaden <Cats 4 life!>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Alistair Singh <alistair.singh7@gmail.com>\nCo-authored-by: Vincent Geddes <117534+vgeddes@users.noreply.github.com>",
          "timestamp": "2025-02-21T13:09:19Z",
          "tree_id": "dd4fa931332e83351cf60a6894efaf55f124106c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd7562ab5618d9c2a657ead42a0f92b3496ff80f"
        },
        "date": 1740147251712,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19779530,
            "range": "± 129860",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20797608,
            "range": "± 546406",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22730360,
            "range": "± 429889",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25670171,
            "range": "± 210262",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 55508701,
            "range": "± 700447",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 361973447,
            "range": "± 4509066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2432128027,
            "range": "± 122506141",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15694346,
            "range": "± 103799",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15802382,
            "range": "± 166644",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16316410,
            "range": "± 166422",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21027052,
            "range": "± 152038",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53775897,
            "range": "± 614000",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 318642250,
            "range": "± 3455265",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2511114087,
            "range": "± 12800097",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "49718502+alexggh@users.noreply.github.com",
            "name": "Alexandru Gheorghe",
            "username": "alexggh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b65c0a3d0382ee423eebf8104d84cec4277b6a07",
          "message": "remove redundant 0016-approval-voting-parallel test (#7659)\n\nThe test was flaky and was disabled here\nhttps://github.com/paritytech/polkadot-sdk/issues/6345, looked at the\nflakiness\nhttps://github.com/paritytech/polkadot-sdk/issues/6345#issuecomment-2674063608\nand it wasn't because of some bug in our production code, but because of\nthe way the test interacts with the infrastructure.\n\nSince https://github.com/paritytech/polkadot-sdk/pull/7504 this test is\nnow testing redundant things that other tests like\n0009-approval-voting-coalescing.toml and 0006-parachains-max-tranche0\nalready cover, so instead of investing trying to fix it, just remove it.\n\nFixes: https://github.com/paritytech/polkadot-sdk/issues/6345\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2025-02-21T14:42:39Z",
          "tree_id": "416939f43d7b82b5dac3947ac3020988ce543ab6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b65c0a3d0382ee423eebf8104d84cec4277b6a07"
        },
        "date": 1740152243852,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17355150,
            "range": "± 194882",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17697698,
            "range": "± 164600",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18967387,
            "range": "± 176500",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22441098,
            "range": "± 388428",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 49854819,
            "range": "± 335952",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 290658348,
            "range": "± 2451257",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2164697282,
            "range": "± 88265022",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14253129,
            "range": "± 139913",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14346069,
            "range": "± 112378",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14731478,
            "range": "± 153606",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18442614,
            "range": "± 113638",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48219182,
            "range": "± 241958",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 278977027,
            "range": "± 2242343",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2247847515,
            "range": "± 31112229",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e915cad42a5b92e76642b45cc4c5259134c8df58",
          "message": "[pallet-revive] Remove js examples (#7660)\n\nRemove JS examples, they now belongs to the evm-test-suite repo\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-21T15:49:35Z",
          "tree_id": "622f46a52ec9d543e6028f0488f4c93103113d7f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e915cad42a5b92e76642b45cc4c5259134c8df58"
        },
        "date": 1740156114862,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17306574,
            "range": "± 155391",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17539276,
            "range": "± 166700",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18988693,
            "range": "± 224327",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22589771,
            "range": "± 319428",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50936699,
            "range": "± 907537",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 298143980,
            "range": "± 3346870",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2271330098,
            "range": "± 122800669",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14494408,
            "range": "± 127714",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14633120,
            "range": "± 69732",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15011715,
            "range": "± 204075",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18638904,
            "range": "± 314914",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48335352,
            "range": "± 451353",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 277436242,
            "range": "± 1779499",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2255278568,
            "range": "± 9355907",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ayevbeosa.j@gmail.com",
            "name": "Ayevbeosa Iyamu",
            "username": "ayevbeosa"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "934c091421af4be362839996bfaa441ba59bf12b",
          "message": "xcm-builder: added logging for xcm filters/helpers/matchers/types (#2408) (#7003)\n\n# Description\n\nAdded logs in pallet-xcm to help in debugging, fixes #2408, and in\ncontinuation of #4982\n\n# Checklist\n\n- [x]\nhttps://github.com/paritytech/polkadot-sdk/blob/master/polkadot/xcm/xcm-builder/src/\n- [x]\nhttps://github.com/paritytech/polkadot-sdk/tree/master/cumulus/parachains/runtimes/assets/common/src\n- [x] runtime-defined XCM filters/converters (just [one\nexample](https://github.com/paritytech/polkadot-sdk/blob/183b55aae21e97ef39192e5a358287e2b6b7043c/cumulus/parachains/runtimes/bridge-hubs/bridge-hub-westend/src/xcm_config.rs#L284))\n\nPolkadot Address: 1Gz5aLtEu2n4jsfA6XwtZnuaRymJrDDw4kEGdNHTdxrpzrc\n\n---------\n\nCo-authored-by: Ayevbeosa Iyamu <aiyamu@vatebra.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Raymond Cheung <178801527+raymondkfcheung@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-21T17:48:03Z",
          "tree_id": "ba8115916a9b7e7273228e7e729b2a4bacb81934",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/934c091421af4be362839996bfaa441ba59bf12b"
        },
        "date": 1740163178821,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17993043,
            "range": "± 117215",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18250360,
            "range": "± 280607",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19765580,
            "range": "± 126739",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23943208,
            "range": "± 353805",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53892516,
            "range": "± 602105",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 340448110,
            "range": "± 5422278",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2341040287,
            "range": "± 185881799",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15024401,
            "range": "± 111434",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15249587,
            "range": "± 113055",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15814698,
            "range": "± 106450",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19787763,
            "range": "± 334378",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50596487,
            "range": "± 524043",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 299950470,
            "range": "± 5389647",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2406008736,
            "range": "± 28129857",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "davxy@datawok.net",
            "name": "Davide Galassi",
            "username": "davxy"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "21f6f0705e53c15aa2b8a5706b208200447774a9",
          "message": "Bandersnatch hot fix (#7670)\n\nEssentially, this locks `bandersnatch_vrfs` to a specific branch of a\nrepository I control. This is a temporary workaround to avoid issues\nlike https://github.com/paritytech/polkadot-sdk/issues/7653 until\nhttps://github.com/paritytech/polkadot-sdk/pull/7669 is ready.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/7653 \n\n@drskalman\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-02-23T14:10:33Z",
          "tree_id": "353ad3e11fb7e90512de387c16e3a1d3d020303a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/21f6f0705e53c15aa2b8a5706b208200447774a9"
        },
        "date": 1740323570871,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19560627,
            "range": "± 167688",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19991550,
            "range": "± 139820",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22034844,
            "range": "± 177951",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25672727,
            "range": "± 660275",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 58141887,
            "range": "± 1073745",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 402044574,
            "range": "± 7575189",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2900639740,
            "range": "± 110823994",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16140149,
            "range": "± 258278",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16098162,
            "range": "± 128358",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16638808,
            "range": "± 138903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 21342468,
            "range": "± 360692",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54217095,
            "range": "± 2047553",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 339857127,
            "range": "± 3738901",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2530773570,
            "range": "± 30016059",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "179002856+paritytech-cmd-bot-polkadot-sdk[bot]@users.noreply.github.com",
            "name": "paritytech-cmd-bot-polkadot-sdk[bot]",
            "username": "paritytech-cmd-bot-polkadot-sdk[bot]"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "16ed0296f7bf63e22b34e0e4b6c0bb2fc3c200f4",
          "message": "Auto-update of all weights for 2025-02-21-1740149841 (#7668)\n\nAuto-update of all weights for 2025-02-21-1740149841.\n\nSubweight results:\n- [now vs\nmaster](https://weights.tasty.limo/compare?repo=polkadot-sdk&threshold=5&path_pattern=.%2F**%2Fweights%2F**%2F*.rs%2C.%2F**%2Fweights.rs&method=asymptotic&ignore_errors=true&unit=time&old=master&new=update-weights-weekly-2025-02-21-1740149841)\n- [now vs polkadot-v1.15.6\n(2025-01-16)](https://weights.tasty.limo/compare?repo=polkadot-sdk&threshold=5&path_pattern=.%2F**%2Fweights%2F**%2F*.rs%2C.%2F**%2Fweights.rs&method=asymptotic&ignore_errors=true&unit=time&old=polkadot-v1.15.6&new=update-weights-weekly-2025-02-21-1740149841)\n- [now vs polkadot-v1.16.2\n(2024-11-14)](https://weights.tasty.limo/compare?repo=polkadot-sdk&threshold=5&path_pattern=.%2F**%2Fweights%2F**%2F*.rs%2C.%2F**%2Fweights.rs&method=asymptotic&ignore_errors=true&unit=time&old=polkadot-v1.16.2&new=update-weights-weekly-2025-02-21-1740149841)\n\nCo-authored-by: github-actions[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-24T10:13:00+01:00",
          "tree_id": "14c3cd2d71cb21a40cf97edb00be5e970b0650a6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/16ed0296f7bf63e22b34e0e4b6c0bb2fc3c200f4"
        },
        "date": 1740389414085,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17121174,
            "range": "± 119702",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17540480,
            "range": "± 105041",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18881659,
            "range": "± 167281",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22584146,
            "range": "± 464920",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50771146,
            "range": "± 741332",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 301405787,
            "range": "± 4277081",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2064945612,
            "range": "± 45056873",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14427434,
            "range": "± 76827",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14553019,
            "range": "± 52605",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15024217,
            "range": "± 167144",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18778032,
            "range": "± 278312",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48369265,
            "range": "± 406168",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 280644904,
            "range": "± 2224800",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2213714822,
            "range": "± 14674976",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32275622+seemantaggarwal@users.noreply.github.com",
            "name": "seemantaggarwal",
            "username": "seemantaggarwal"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cf52a0d960a17b4d023bb7c9351e159c60227c85",
          "message": "effort towards getting chainspecbuilder into omni-node fix 5567 (#7619)\n\nAdding chain-spec-builder as a subcommand into Polkadot omni node",
          "timestamp": "2025-02-24T11:48:11Z",
          "tree_id": "eb477c0320f902a4e9f5b5fadba40fa959927565",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cf52a0d960a17b4d023bb7c9351e159c60227c85"
        },
        "date": 1740400804151,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18160924,
            "range": "± 194963",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18240857,
            "range": "± 227623",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19680594,
            "range": "± 134025",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23324357,
            "range": "± 157436",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 51561262,
            "range": "± 417682",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 291742061,
            "range": "± 5702602",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2258766576,
            "range": "± 98479152",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14445251,
            "range": "± 106309",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14830233,
            "range": "± 156461",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15515368,
            "range": "± 98636",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19352481,
            "range": "± 172782",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49625821,
            "range": "± 476151",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 289440906,
            "range": "± 2181518",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2342788074,
            "range": "± 31721355",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "178801527+raymondkfcheung@users.noreply.github.com",
            "name": "Raymond Cheung",
            "username": "raymondkfcheung"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d189f9e771d937999db4ed17cf2c5d9e1a6a29e7",
          "message": "Enhance XCM Debugging with Log Capture in Unit Tests (#7594)\n\n# Description\n\nThis PR introduces a lightweight log-capturing mechanism for XCM unit\ntests, simplifying debugging by enabling structured log assertions. It\npartially addresses #6119 and #6125, offering an optional way to verify\nlogs in tests while remaining unobtrusive in normal execution.\n\n# Key Changes\n\n* [x] Introduces a log capture utility in `sp_tracing`.\n* [x] Adds XCM test examples demonstrating how and when to use log\ncapturing.\n\n# Review Notes:\n\n* The log capture mechanism is opt-in and does not affect existing tests\nunless explicitly used.\n* The implementation is minimal and does not add complexity to existing\ntest setups.\n* It provides a structured alternative to\n[`sp_tracing::init_for_tests()`](https://paritytech.github.io/polkadot-sdk/master/sp_tracing/fn.init_for_tests.html)\nfor log verification in automated tests.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-24T14:47:32Z",
          "tree_id": "b73f98e33533378e7f473f6178dc9ec8c99ff190",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d189f9e771d937999db4ed17cf2c5d9e1a6a29e7"
        },
        "date": 1740411694615,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17120500,
            "range": "± 107842",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17446298,
            "range": "± 97525",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18845422,
            "range": "± 83831",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22795044,
            "range": "± 762906",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50815747,
            "range": "± 424543",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 286099435,
            "range": "± 4690915",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2236187248,
            "range": "± 62095121",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14908872,
            "range": "± 664640",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14428807,
            "range": "± 116539",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14982361,
            "range": "± 348397",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18738917,
            "range": "± 173157",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47965462,
            "range": "± 450688",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 276254640,
            "range": "± 2200179",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2219707745,
            "range": "± 33082145",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cdc033945741d79ebc87a630558b4fc507a51df6",
          "message": "[pallet-revive] tracing should wrap around call stack execution (#7676)\n\nFix tracing should wrap around the entire call stack execution\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-24T16:56:21Z",
          "tree_id": "71da43008b21cdf21d6b1e2c9c81e61493d2d579",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cdc033945741d79ebc87a630558b4fc507a51df6"
        },
        "date": 1740420652955,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17461607,
            "range": "± 120352",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17565845,
            "range": "± 339832",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19169846,
            "range": "± 127076",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22664140,
            "range": "± 113312",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50285152,
            "range": "± 280743",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 301545803,
            "range": "± 2765932",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2234491601,
            "range": "± 81775945",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14412391,
            "range": "± 98986",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14540544,
            "range": "± 158726",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15108705,
            "range": "± 347492",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18731426,
            "range": "± 133514",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 48077070,
            "range": "± 219373",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 280358078,
            "range": "± 4119757",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2289689594,
            "range": "± 25678194",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "rohit.sarpotdar@parity.io",
            "name": "Rohit Sarpotdar",
            "username": "rosarp"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4a400dc1866f11707331fb6408df1055d0f42a70",
          "message": "updated substrate-relay to v1.8.0 (#7697)\n\nremoved run-relay references removed in PR #7549",
          "timestamp": "2025-02-25T06:44:06Z",
          "tree_id": "d1641ec80314a47328f32781d06b9ace847e26f7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4a400dc1866f11707331fb6408df1055d0f42a70"
        },
        "date": 1740468785526,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19492793,
            "range": "± 265551",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20111373,
            "range": "± 401149",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21582589,
            "range": "± 483151",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26121281,
            "range": "± 270071",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 63382878,
            "range": "± 1434864",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 445449765,
            "range": "± 7125555",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2905134674,
            "range": "± 204102686",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16709669,
            "range": "± 266258",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16498331,
            "range": "± 224224",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17497475,
            "range": "± 264757",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22679341,
            "range": "± 312071",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 58117325,
            "range": "± 546089",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 375543338,
            "range": "± 6321936",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2723697653,
            "range": "± 60738569",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "vedhavyas.singareddi@gmail.com",
            "name": "CrabGopher",
            "username": "vedhavyas"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dd005c48e44053a84a5a5a7e63444064be49bf6e",
          "message": "Expose extension weights (#7637)\n\n# Description\nSeems like SubstrateWeights that used T::DBWeights was not public unlike\nthe frame_system's Call weights. PR just made those weights public\n\n## Integration\nInstead of using `()` impl which used RockDB weights, ExtensionWeights\ncan be used to to use the provided DBWeights to System config",
          "timestamp": "2025-02-25T11:06:07Z",
          "tree_id": "8cdce3e24be0f8728cd12863af168346e0e7a398",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd005c48e44053a84a5a5a7e63444064be49bf6e"
        },
        "date": 1740484582517,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20132639,
            "range": "± 225837",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20378303,
            "range": "± 260004",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22516042,
            "range": "± 425130",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25897522,
            "range": "± 311312",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56811748,
            "range": "± 800310",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 362253895,
            "range": "± 13033776",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2856230372,
            "range": "± 68778583",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16188246,
            "range": "± 243927",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16755553,
            "range": "± 198602",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17324683,
            "range": "± 257198",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23225373,
            "range": "± 297584",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57981842,
            "range": "± 1034334",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 375847098,
            "range": "± 6916593",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2754329783,
            "range": "± 90272033",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "giuseppe.re@parity.io",
            "name": "Giuseppe Re",
            "username": "re-gius"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3dc3a11cd68762c2e5feb0beba0b61f448c4fc92",
          "message": "Add Runtime Api version to metadata (#7607)\n\nThe runtime API implemented version is not explicitly shown in metadata,\nso here we add it to improve developer experience.\nWe need to bump `frame-metadata` and `merkleized-metadata` to allow this\nnew feature.\n\nThis closes #7352 .\n\n_Refactor_: also changing all the occurrences of `ViewFunctionMethod` to\njust `ViewFunction` for metadata types.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-25T13:29:22Z",
          "tree_id": "0ca0ec20c0c8001402b5b57e995c4dce59ff5f21",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3dc3a11cd68762c2e5feb0beba0b61f448c4fc92"
        },
        "date": 1740493296814,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18210042,
            "range": "± 99493",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18613840,
            "range": "± 236049",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19970590,
            "range": "± 115537",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23527301,
            "range": "± 169544",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52688129,
            "range": "± 362859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 321303116,
            "range": "± 4460241",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2234058384,
            "range": "± 136182130",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14907193,
            "range": "± 147859",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15313498,
            "range": "± 161128",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15647651,
            "range": "± 95496",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19767787,
            "range": "± 144341",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50300695,
            "range": "± 202340",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 304117113,
            "range": "± 2368988",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2432947020,
            "range": "± 26034464",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "37865735+clangenb@users.noreply.github.com",
            "name": "clangenb",
            "username": "clangenb"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "51170495d0a8902bc20e650b17639773698cb0b1",
          "message": "remove leftovers of the contracts-rococo parachain (#7638)\n\nThere were already previous efforts to remove the contracts-rococo\nchain, see #5471, which was done as a response to this comment\nhttps://github.com/paritytech/polkadot-sdk/pull/5288#discussion_r1711157476.\n\nThis PR intends to fix the parts that were overlooked back then, and\nremove all traces of contracts-rococo as it is intended to be replaced\nby a new testnet including pallet-revive.",
          "timestamp": "2025-02-26T08:46:50Z",
          "tree_id": "414d1562c7533ba6f60dda15e1ec2ba86a11c2c3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/51170495d0a8902bc20e650b17639773698cb0b1"
        },
        "date": 1740562764750,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18437828,
            "range": "± 236810",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18438155,
            "range": "± 172026",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20113958,
            "range": "± 255771",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23929952,
            "range": "± 188075",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 56720152,
            "range": "± 952295",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 349811927,
            "range": "± 8189988",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2583622344,
            "range": "± 117765831",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15146445,
            "range": "± 225086",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15660852,
            "range": "± 191969",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16091296,
            "range": "± 563907",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20199308,
            "range": "± 227200",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53088850,
            "range": "± 679161",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 314225322,
            "range": "± 5036730",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2476743528,
            "range": "± 28055187",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "37865735+clangenb@users.noreply.github.com",
            "name": "clangenb",
            "username": "clangenb"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cbc9b90130fb346aad657fa5b08b66bdeeac01f1",
          "message": "add genesis presets for glutton westend (#7481)\n\nExtracted from #7473.\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/5704.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-02-26T11:46:58Z",
          "tree_id": "e9c10a2274889da6bb31d328910e0894bdce5acc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cbc9b90130fb346aad657fa5b08b66bdeeac01f1"
        },
        "date": 1740574954514,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17372775,
            "range": "± 137266",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17748822,
            "range": "± 180266",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19308750,
            "range": "± 360046",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22705534,
            "range": "± 721961",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53545517,
            "range": "± 1002142",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 307519886,
            "range": "± 4196321",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2575358988,
            "range": "± 47242286",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14842719,
            "range": "± 180557",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14877891,
            "range": "± 162376",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15453920,
            "range": "± 388475",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19432317,
            "range": "± 147359",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50145911,
            "range": "± 682873",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 288350618,
            "range": "± 2912520",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2359520689,
            "range": "± 43571918",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ludovic.domingues96@gmail.com",
            "name": "Ludovic_Domingues",
            "username": "Krayt78"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "0c0d8929712ca7e306a001bcc214c81886dda386",
          "message": "Migrating polkadot-runtime-parachains paras_inherent benchmarking to V2 (#6606)\n\n# Description\n\nMigrating polkadot-runtime-parachains paras_inherent benchmarking to the\nnew benchmarking syntax v2.\nThis is part of #6202\n\n---------\n\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>",
          "timestamp": "2025-02-26T15:23:47Z",
          "tree_id": "5a1da67a53d306e41dbead723fb45a720f62a445",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0c0d8929712ca7e306a001bcc214c81886dda386"
        },
        "date": 1740587343961,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17444076,
            "range": "± 105564",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17832896,
            "range": "± 188857",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19245737,
            "range": "± 153319",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23031650,
            "range": "± 362632",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 53163153,
            "range": "± 1866626",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 312058892,
            "range": "± 2397628",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2416730098,
            "range": "± 156344585",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14721442,
            "range": "± 123234",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14998875,
            "range": "± 153170",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15236410,
            "range": "± 67584",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19301482,
            "range": "± 198023",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50864849,
            "range": "± 2425975",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 291500159,
            "range": "± 2881238",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2337307576,
            "range": "± 10138454",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "pgherveou@gmail.com",
            "name": "PG Herveou",
            "username": "pgherveou"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "86019edba891fee6b5c008f0f5d0b5d782e42d2d",
          "message": "[pallet-revive] ecrecover (#7652)\n\nAdd ECrecover 0x1 precompile and remove the unstable equivalent host\nfunction.\n\n- depend on https://github.com/paritytech/polkadot-sdk/pull/7676\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-02-26T15:32:49Z",
          "tree_id": "d3998ba7bd1ddf4e07c5bf47b0588945a26bf579",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/86019edba891fee6b5c008f0f5d0b5d782e42d2d"
        },
        "date": 1740588523370,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17496590,
            "range": "± 155761",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17818579,
            "range": "± 81978",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19276997,
            "range": "± 86770",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23180355,
            "range": "± 286964",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52436723,
            "range": "± 1143824",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 290367357,
            "range": "± 1887111",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2275063558,
            "range": "± 114085903",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14704580,
            "range": "± 374985",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14918818,
            "range": "± 175472",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15499366,
            "range": "± 215058",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19027594,
            "range": "± 254212",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49452489,
            "range": "± 438528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 282497956,
            "range": "± 2818187",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2288726266,
            "range": "± 30739116",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "cyrill@parity.io",
            "name": "xermicus",
            "username": "xermicus"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c29e72a8628835e34deb6aa7db9a78a2e4eabcee",
          "message": "[pallet-revive] allow delegate calls to non-contract accounts (#7729)\n\nThis PR changes the behavior of delegate calls when the callee is not a\ncontract account: Instead of returning a `CodeNotFound` error, this is\nallowed and the caller observes a successful call with empty output.\n\nThe change makes for example the following contract behave the same as\non EVM:\n\n```Solidity\ncontract DelegateCall {\n    function delegateToLibrary() external returns (bool) {\n        address testAddress = 0x0000000000000000000000000000000000000000;\n        (bool success, ) = testAddress.delegatecall(\n            abi.encodeWithSignature(\"test()\")\n        );\n        return success;\n    }\n}\n```\n\nCloses https://github.com/paritytech/revive/issues/235\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-26T16:39:41Z",
          "tree_id": "82e9c6e2a8ed749fce0e768036f08e185315f965",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c29e72a8628835e34deb6aa7db9a78a2e4eabcee"
        },
        "date": 1740591294123,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17201735,
            "range": "± 220609",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17568017,
            "range": "± 82570",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 18819341,
            "range": "± 137287",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22500349,
            "range": "± 221681",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50259456,
            "range": "± 504657",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 284953487,
            "range": "± 3575196",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2262343317,
            "range": "± 58889723",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14325401,
            "range": "± 99245",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14462661,
            "range": "± 116689",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15076213,
            "range": "± 118437",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18698467,
            "range": "± 72691",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47953704,
            "range": "± 555952",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 280613208,
            "range": "± 10572705",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2261756781,
            "range": "± 12972838",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "10196091+Ank4n@users.noreply.github.com",
            "name": "Ankan",
            "username": "Ank4n"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f7e98b40cab7475898c99ea48809635ac069af3a",
          "message": "[Nomination Pool] Make staking restrictions configurable (#7685)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/5742\n\nNeed to be backported to stable2503 release.\n\nWith the migration of staking accounts to [fungible\ncurrency](https://github.com/paritytech/polkadot-sdk/pull/5501), we can\nnow allow pool users to stake directly and vice versa. This update\nintroduces a configurable filter mechanism to determine which accounts\ncan join a nomination pool.\n\n## Example Usage  \n\n### 1. Allow any account to join a pool  \nTo permit all accounts to join a nomination pool, use the `Nothing`\nfilter:\n\n```rust\nimpl pallet_nomination_pools::Config for Runtime {\n    ...\n    type Filter = Nothing;\n}\n```\n\n### 2. Restrict direct stakers from joining a pool\n\nTo prevent direct stakers from joining a nomination pool, use\n`pallet_staking::AllStakers`:\n```rust\nimpl pallet_nomination_pools::Config for Runtime {\n    ...\n    type Filter = pallet_staking::AllStakers<Runtime>;\n}\n```\n\n### 3. Define a custom filter\nFor more granular control, you can define a custom filter:\n```rust\nstruct MyCustomFilter<T: Config>(core::marker::PhantomData<T>);\n\nimpl<T: Config> Contains<T::AccountId> for MyCustomFilter<T> {\n    fn contains(account: &T::AccountId) -> bool {\n        todo!(\"Implement custom logic. Return `false` to allow the account to join a pool.\")\n    }\n}\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-02-26T21:12:57+01:00",
          "tree_id": "b9f372d3c4c7c4168149e548615d0a37f2342acd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f7e98b40cab7475898c99ea48809635ac069af3a"
        },
        "date": 1740601856204,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18553121,
            "range": "± 146685",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19009860,
            "range": "± 182512",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 20790472,
            "range": "± 275020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 25226215,
            "range": "± 315934",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57049693,
            "range": "± 1572956",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 374672039,
            "range": "± 5433670",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2685583156,
            "range": "± 109825302",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15844268,
            "range": "± 173378",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 16079371,
            "range": "± 91368",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16823439,
            "range": "± 173809",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20922352,
            "range": "± 286655",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 54228064,
            "range": "± 2924525",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 341420825,
            "range": "± 6736696",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2666830300,
            "range": "± 72203458",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "69342343+MrishoLukamba@users.noreply.github.com",
            "name": "Mrisho Lukamba",
            "username": "MrishoLukamba"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e64b53c2fff2daceafc453caeeee79756372a9e2",
          "message": "feat(collator) add export pov on slot base collator (#7585)\n\nCloses #7573\n\n@skunert  @bkchr\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-02-27T08:12:09Z",
          "tree_id": "bfe3134446073d0c9ce4856d34263a4e9074c964",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e64b53c2fff2daceafc453caeeee79756372a9e2"
        },
        "date": 1740647021274,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17290113,
            "range": "± 179024",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17476852,
            "range": "± 118836",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19003631,
            "range": "± 118361",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22938232,
            "range": "± 203504",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52089881,
            "range": "± 812787",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 319638084,
            "range": "± 4400290",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2355888861,
            "range": "± 52902254",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14635227,
            "range": "± 155443",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14701000,
            "range": "± 125406",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15129664,
            "range": "± 167913",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19015665,
            "range": "± 138746",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50094859,
            "range": "± 415511",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 285224872,
            "range": "± 3621991",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2275837026,
            "range": "± 18895081",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e3e3f4814e183ea1a174e11232776374b66f1e26",
          "message": "notifications/tests: Check compatiblity between litep2p and libp2p  (#7484)\n\nThis PR ensures compatibility in terms of expectations between the\nlibp2p and litep2p network backends at the notification protocol level.\n\nThe libp2p node is tested with the `Notification` behavior that contains\nthe protocol controller, while litep2p is tested at the lowest level API\n(without substrate shim layers).\n\n## Notification Behavior\n\n(I) Libp2p protocol controller will eagerly reopen a closed substream,\neven if it is the one that closed it:\n- When a node (libp2p or litep2p) closes the substream with **libp2p**,\nthe **libp2p** controller will reopen the substream\n- When **libp2p** closes the substream with a node (either litep2p with\nno controller or libp2p), the **libp2p** controller will reopen the\nsubstream\n- However in this case, libp2p was the one closing the substream\nsignaling it is no longer interested in communicating with the other\nside\n\n(II) Notifications are lost and not reported to the higher level in the\nfollowing scenario:\n- T0: Node A opens a substream with Node B\n- T1: Node A closes the substream or the connection with Node B\n- T2: Node B sends a notification to Node A => *notification is lost*\nand never reported\n- T3: Node B detects the closed substream or connection\n\n\n## Testing\n\nThis PR effectively checks:\n- connectivity at the notification level\n- litep2p rejecting libp2p substream and keep-alive mechanism\nfunctionality\n- libp2p disconnecting libp2p and connection re-establishment (and all\nthe other permutations)\n- idling of connections with active substreams and keep-alive mechanism\nis not enforced\n\n\nPrior work:\n- https://github.com/paritytech/polkadot-sdk/pull/7361\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2025-02-27T09:53:01Z",
          "tree_id": "f930cb75f7dc768c45a71fbf13c28916d9d12766",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e3e3f4814e183ea1a174e11232776374b66f1e26"
        },
        "date": 1740652887988,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20871365,
            "range": "± 191145",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 21329458,
            "range": "± 186662",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 23129198,
            "range": "± 304833",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 27392639,
            "range": "± 330928",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 65635399,
            "range": "± 2183034",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 451601676,
            "range": "± 6272999",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 3182986013,
            "range": "± 18647867",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16997605,
            "range": "± 129351",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17159355,
            "range": "± 212078",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17699020,
            "range": "± 253941",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22630895,
            "range": "± 274088",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 57151960,
            "range": "± 1075904",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 381771334,
            "range": "± 4308637",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2729068224,
            "range": "± 28898256",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "egor@parity.io",
            "name": "Egor_P",
            "username": "EgorPopelyaev"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "84b3ae9be5fdd52f99506c34a5f0e640023e3f04",
          "message": "[Backport] Version bumps form stable2412-2 (#7744)\n\nThis PR backports version bumps and prdocs reorg from the latest stable\nbranch back to master",
          "timestamp": "2025-02-27T12:18:33Z",
          "tree_id": "45ceb92c160fae3f5a1a412b49c89b4125f64546",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/84b3ae9be5fdd52f99506c34a5f0e640023e3f04"
        },
        "date": 1740662866014,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 20092531,
            "range": "± 731365",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20847041,
            "range": "± 592541",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22246072,
            "range": "± 825020",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 26469431,
            "range": "± 534658",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 61647547,
            "range": "± 1539304",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 386835804,
            "range": "± 14795159",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2692398820,
            "range": "± 55472835",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16765249,
            "range": "± 366215",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 18263410,
            "range": "± 444861",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17762606,
            "range": "± 247854",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 22725486,
            "range": "± 397204",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 61397185,
            "range": "± 3118101",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 402110916,
            "range": "± 9505621",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2615142447,
            "range": "± 238651469",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "lumosksm@gmail.com",
            "name": "huntbounty",
            "username": "huntbounty"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d734e79e17976fd7cabb7e5aa7179acb57d9846b",
          "message": "Add README.md to umbrella (#7600)\n\nResolves #7536\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-02-27T14:59:34Z",
          "tree_id": "1cec29bc19cf8a25a07aa2a48d1128fe20380e27",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d734e79e17976fd7cabb7e5aa7179acb57d9846b"
        },
        "date": 1740671322631,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19609087,
            "range": "± 247573",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 20448361,
            "range": "± 246056",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 22253768,
            "range": "± 547260",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 29517718,
            "range": "± 1028717",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 70207360,
            "range": "± 3763319",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 427671536,
            "range": "± 5736180",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2929613917,
            "range": "± 112777348",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 16520015,
            "range": "± 235066",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 17314602,
            "range": "± 223319",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 17759592,
            "range": "± 182134",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 23195499,
            "range": "± 651507",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 61847904,
            "range": "± 1477098",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 372121047,
            "range": "± 3593557",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2824241777,
            "range": "± 59133571",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ub2262000@gmail.com",
            "name": "Utkarsh Bhardwaj",
            "username": "UtkarshBhardwaj007"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cc83fba12f2b34415229240d47f4f6a7ea83b194",
          "message": "[AHM] Poke deposits: Multisig pallet (#7700)\n\n# Description\n\n* This PR adds a new extrinsic `poke_deposit` to `pallet-multisig`. This\nextrinsic will be used to re-adjust the deposits made in the pallet to\ncreate a multisig operation after AHM.\n* Part of #5591 \n\n## Review Notes\n\n* Added a new extrinsic `poke_deposit` in `pallet-multisig`.\n* Added a new event `DepositPoked` to be emitted upon a successful call\nof the extrinsic.\n* Although the immediate use of the extrinsic will be to give back some\nof the deposit after the AH-migration, the extrinsic is written such\nthat it can work if the deposit decreases or increases (both).\n* The call to the extrinsic would be `free` if an actual adjustment is\nmade to the deposit and `paid` otherwise.\n* Added tests to test all scenarios.\n\n## TO-DOs\n* [x] Add Benchmark\n* [x] Run CI cmd bot to benchmark\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Giuseppe Re <giuseppe.re@parity.io>",
          "timestamp": "2025-02-27T16:50:26Z",
          "tree_id": "d67c9c0e2d5360bb8c512a53324cf709c8ee6e25",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cc83fba12f2b34415229240d47f4f6a7ea83b194"
        },
        "date": 1740678004924,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 18979666,
            "range": "± 186796",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19338287,
            "range": "± 183893",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21053857,
            "range": "± 220529",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24871096,
            "range": "± 327429",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54232882,
            "range": "± 487424",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 330694947,
            "range": "± 5711130",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2432652164,
            "range": "± 15937057",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15348212,
            "range": "± 244372",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15710632,
            "range": "± 86686",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16092363,
            "range": "± 180192",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20183674,
            "range": "± 403892",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 51778628,
            "range": "± 591055",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 316799550,
            "range": "± 3946423",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2485206246,
            "range": "± 31503029",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "178801527+raymondkfcheung@users.noreply.github.com",
            "name": "Raymond Cheung",
            "username": "raymondkfcheung"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a3d10b3a8d9ae1162cd18779ec34c551c27d5d9",
          "message": "Simplify event assertion with predicate-based check (#7734)\n\nA follow-up PR to simplify event assertions by introducing\n`contains_event`, allowing event checks without needing exact field\nmatches. This reduces redundancy and makes tests more flexible.\n\nPartially addresses #6119 by providing an alternative way to assert\nevents.\n\nReference: [PR #7594 -\nDiscussion](https://github.com/paritytech/polkadot-sdk/pull/7594#discussion_r1965566349)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-02-27T23:32:51Z",
          "tree_id": "da2e7fe7bcf7480253d5dd33f8e610bc5d7f7708",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6a3d10b3a8d9ae1162cd18779ec34c551c27d5d9"
        },
        "date": 1740702067269,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17687022,
            "range": "± 205512",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18005732,
            "range": "± 155939",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19736445,
            "range": "± 161342",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23640322,
            "range": "± 192667",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54225811,
            "range": "± 1022692",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 322620871,
            "range": "± 6332800",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2467313928,
            "range": "± 161371812",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14886893,
            "range": "± 128400",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15011018,
            "range": "± 169102",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15731411,
            "range": "± 127033",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19616032,
            "range": "± 245153",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50925839,
            "range": "± 1092617",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 292689570,
            "range": "± 2479981",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2369941822,
            "range": "± 21441594",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "serban@parity.io",
            "name": "Serban Iorga",
            "username": "serban300"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c11b1f8526d704140f4329e68348c0b0965285de",
          "message": "Derive `DecodeWithMemTracking` for `Block` (#7655)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/7360\n\nThis PR adds `DecodeWithMemTracking` as a trait bound for `Header`,\n`Block` and `TransactionExtension` and\nderives it for all the types that implement these traits in\n`polkadot-sdk`.",
          "timestamp": "2025-02-28T08:03:55Z",
          "tree_id": "5f42c79126e3554089f5e29edd355ab7f763492e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c11b1f8526d704140f4329e68348c0b0965285de"
        },
        "date": 1740732804320,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17682306,
            "range": "± 112776",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17967914,
            "range": "± 122825",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19362085,
            "range": "± 201921",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23400246,
            "range": "± 250501",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 52578963,
            "range": "± 550847",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 329350544,
            "range": "± 4287339",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2439321775,
            "range": "± 85631352",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14564868,
            "range": "± 355267",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14611141,
            "range": "± 138882",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15110943,
            "range": "± 139540",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 19332889,
            "range": "± 152856",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 49269202,
            "range": "± 630268",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 290670350,
            "range": "± 4249462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2316200076,
            "range": "± 24398720",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alex.theissen@me.com",
            "name": "Alexander Theißen",
            "username": "athei"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4087e2d96313539db8721aa5946f6ca0bc7a67b2",
          "message": "pallet_revive: Change address derivation to use hashing (#7662)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/6723\n\n## Motivation\n\nInternal auditors recommended to not truncate Polkadot Addresses when\nderiving Ethereum addresses from it. Reasoning is that they are raw\npublic keys where truncating could lead to collisions when weaknesses in\nthose curves are discovered in the future. Additionally, some pallets\ngenerate account addresses in a way where only the suffix we were\ntruncating contains any entropy. The changes in this PR act as a safe\nguard against those two points.\n\n## Changes made\n\nWe change the `to_address` function to first hash the AccountId32 and\nthen use trailing 20 bytes as `AccountId20`. If the `AccountId32` ends\nwith 12x 0xEE we keep our current behaviour of just truncating those\ntrailing bytes.\n\n## Security Discussion\n\nThis will allow us to still recover the original `AccountId20` because\nthose are constructed by just adding those 12 bytes. Please note that\ngenerating an ed25519 key pair where the trailing 12 bytes are 0xEE is\ntheoretically possible as 96bits is not a huge search space. However,\nthis cannot be used as an attack vector. It will merely allow this\naddress to interact with `pallet_revive` without registering as the\nfallback account is the same as the actual address. The ultimate vanity\naddress. In practice, this is not relevant since the 0xEE addresses are\nnot valid public keys for sr25519 which is used almost everywhere.\n\ntl:dr: We keep truncating in case of an Ethereum address derived account\nid. This is safe as those are already derived via keccak. In every other\ncase where we have to assume that the account id might be a public key.\nTherefore we first hash and then take the trailing bytes.\n\n## Do we need a Migration for Westend\n\nNo. We changed the name of the mapping. This means the runtime will not\ntry to read the old data. Ethereum keys are unaffected by this change.\nWe just advise people to re-register their AccountId32 in case they need\nto use it as it is a very small circle of users (just 3 addresses\nregistered). This will not cause disturbance on Westend.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-02-28T13:17:16Z",
          "tree_id": "917b8c6a08bd8aad4e5cf98df9440e900063f28e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4087e2d96313539db8721aa5946f6ca0bc7a67b2"
        },
        "date": 1740751474910,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17200514,
            "range": "± 111501",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 17584171,
            "range": "± 82327",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19158315,
            "range": "± 166591",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 22900213,
            "range": "± 105462",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 50591160,
            "range": "± 1192276",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 297218261,
            "range": "± 3102669",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2293782156,
            "range": "± 32861185",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 14299722,
            "range": "± 137750",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 14411506,
            "range": "± 93411",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 14739176,
            "range": "± 149260",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 18578696,
            "range": "± 223065",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 47859239,
            "range": "± 1029632",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 280050749,
            "range": "± 2139251",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2226264265,
            "range": "± 35471792",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "skunert49@gmail.com",
            "name": "Sebastian Kunert",
            "username": "skunert"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9adb8d28ab1f6744f1fb26db41f42361ac1254a0",
          "message": "Remove leftovers of leftovers of contracts-rococo (#7750)\n\nFollow-up of https://github.com/paritytech/polkadot-sdk/pull/7638, which\nattempted to remove contracts-rococo.\n\nBut there were some leftover weight files still chilling in the repo.",
          "timestamp": "2025-02-28T15:09:37Z",
          "tree_id": "491208a14ac9d7f53407dcf3111a392fce965e59",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9adb8d28ab1f6744f1fb26db41f42361ac1254a0"
        },
        "date": 1740758145455,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 19178791,
            "range": "± 256211",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 19435559,
            "range": "± 346509",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 21161932,
            "range": "± 324387",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 24946734,
            "range": "± 566823",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 57307024,
            "range": "± 1198841",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 359978947,
            "range": "± 9193436",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2676560605,
            "range": "± 57932961",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15733676,
            "range": "± 106648",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15955342,
            "range": "± 194182",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 16398955,
            "range": "± 103867",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20709594,
            "range": "± 320387",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 53372948,
            "range": "± 712852",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 333025076,
            "range": "± 6093872",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2499324268,
            "range": "± 18684191",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "60601340+lexnv@users.noreply.github.com",
            "name": "Alexandru Vasile",
            "username": "lexnv"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1bc6ca606438a65c927f14be3f36634ca0e58e8f",
          "message": "notifications/libp2p: Terminate the outbound notification substream on `std::io::Errors` (#7724)\n\nThis PR handles a case where we called the `poll_next` on an outbound\nsubstream notification to check if the stream is closed. It is entirely\npossible that the `poll_next` would return an `io::error`, for example\nend of file.\n\nThis PR ensures that we make the distinction between unexpected incoming\ndata, and error originated from `poll_next`.\n\nWhile at it, the bulk of the PR change propagates the PeerID from the\nnetwork behavior, through the notification handler, to the notification\noutbound stream for logging purposes.\n\ncc @paritytech/networking \n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/7722\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-02-28T17:05:16Z",
          "tree_id": "2f62c029b2e9a2c776165b90f05bb49fecad5a2c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1bc6ca606438a65c927f14be3f36634ca0e58e8f"
        },
        "date": 1740765384846,
        "tool": "cargo",
        "benches": [
          {
            "name": "request_response_protocol/libp2p/serially/64B",
            "value": 17570739,
            "range": "± 194954",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/512B",
            "value": 18142756,
            "range": "± 222528",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/4KB",
            "value": 19632342,
            "range": "± 150580",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/64KB",
            "value": 23752481,
            "range": "± 186348",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/256KB",
            "value": 54088336,
            "range": "± 708176",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/2MB",
            "value": 327724939,
            "range": "± 3443457",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/libp2p/serially/16MB",
            "value": 2517063125,
            "range": "± 82445486",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64B",
            "value": 15049555,
            "range": "± 131386",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/512B",
            "value": 15117840,
            "range": "± 122206",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/4KB",
            "value": 15607622,
            "range": "± 99597",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/64KB",
            "value": 20006619,
            "range": "± 120319",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/256KB",
            "value": 50419285,
            "range": "± 669015",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/2MB",
            "value": 295332665,
            "range": "± 6240569",
            "unit": "ns/iter"
          },
          {
            "name": "request_response_protocol/litep2p/serially/16MB",
            "value": 2406081800,
            "range": "± 18816434",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}