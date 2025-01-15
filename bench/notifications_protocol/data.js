window.BENCHMARK_DATA = {
  "lastUpdate": 1736950847532,
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
        "date": 1735914603946,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3790714,
            "range": "± 49805",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 280423,
            "range": "± 8535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3740076,
            "range": "± 63535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 378203,
            "range": "± 9030",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4538192,
            "range": "± 68828",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 810119,
            "range": "± 20666",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9301175,
            "range": "± 183160",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4309488,
            "range": "± 119004",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2828183,
            "range": "± 52096",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1481785,
            "range": "± 14669",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2909037,
            "range": "± 44675",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1559170,
            "range": "± 14384",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3472338,
            "range": "± 45398",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1852563,
            "range": "± 45379",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7372091,
            "range": "± 112889",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4680947,
            "range": "± 98921",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4101459,
            "range": "± 76049",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3410050,
            "range": "± 48784",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 31271719,
            "range": "± 489301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27258045,
            "range": "± 377716",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 243457716,
            "range": "± 2102365",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 254837166,
            "range": "± 13296301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3267873024,
            "range": "± 8798185",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1985384616,
            "range": "± 27913000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3924722,
            "range": "± 116922",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3884115,
            "range": "± 127652",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 35751783,
            "range": "± 733203",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 35967372,
            "range": "± 1067528",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 325261952,
            "range": "± 3199301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 348435466,
            "range": "± 9604483",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3800460071,
            "range": "± 31244354",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3059649471,
            "range": "± 39515314",
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
        "date": 1735947354355,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3944990,
            "range": "± 92447",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 277780,
            "range": "± 9957",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4037729,
            "range": "± 103709",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 365733,
            "range": "± 10243",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4647091,
            "range": "± 146536",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 820305,
            "range": "± 24687",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9461549,
            "range": "± 312455",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4496076,
            "range": "± 175532",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2841015,
            "range": "± 48307",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1479968,
            "range": "± 23352",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2966684,
            "range": "± 56924",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1542928,
            "range": "± 20895",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3627783,
            "range": "± 124814",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1876753,
            "range": "± 42325",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 8291595,
            "range": "± 382313",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4918821,
            "range": "± 300327",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4258076,
            "range": "± 125864",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3631554,
            "range": "± 174637",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32304698,
            "range": "± 1233620",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26866931,
            "range": "± 684322",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 243511242,
            "range": "± 3138339",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 250816924,
            "range": "± 13088899",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3283341160,
            "range": "± 28213906",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1989251853,
            "range": "± 21133350",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4152213,
            "range": "± 144076",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4115112,
            "range": "± 195587",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36991460,
            "range": "± 1452566",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37182421,
            "range": "± 1897911",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 350617389,
            "range": "± 10046434",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 383970463,
            "range": "± 14911332",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4186809329,
            "range": "± 77725562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3503044971,
            "range": "± 58740262",
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
        "date": 1735960696275,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3893421,
            "range": "± 71915",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 283137,
            "range": "± 10232",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3741320,
            "range": "± 66768",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 373672,
            "range": "± 8416",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4576450,
            "range": "± 99153",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 829460,
            "range": "± 21715",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9442735,
            "range": "± 189960",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4345652,
            "range": "± 106535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2903042,
            "range": "± 40565",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1452544,
            "range": "± 18426",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2955019,
            "range": "± 44517",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1509819,
            "range": "± 23918",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3530483,
            "range": "± 65683",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1818739,
            "range": "± 31399",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7521213,
            "range": "± 223106",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4713694,
            "range": "± 148859",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4247828,
            "range": "± 187120",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3583987,
            "range": "± 136822",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32427198,
            "range": "± 1137804",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27429801,
            "range": "± 717428",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 265004638,
            "range": "± 12957562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 264684418,
            "range": "± 10329420",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3302716996,
            "range": "± 30018213",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1998115263,
            "range": "± 23777235",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4176462,
            "range": "± 177166",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4238774,
            "range": "± 214441",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 37132306,
            "range": "± 1472061",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 36735156,
            "range": "± 1421857",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 329545744,
            "range": "± 5446563",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 369957532,
            "range": "± 10906933",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4075679310,
            "range": "± 25664431",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3198182712,
            "range": "± 51410875",
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
          "id": "63c73bf6db1c8982ad3f2310a40799c5987f8900",
          "message": "Implement cumulus StorageWeightReclaim as wrapping transaction extension + frame system ReclaimWeight (#6140)\n\n(rebasing of https://github.com/paritytech/polkadot-sdk/pull/5234)\n\n## Issues:\n\n* Transaction extensions have weights and refund weight. So the\nreclaiming of unused weight must happen last in the transaction\nextension pipeline. Currently it is inside `CheckWeight`.\n* cumulus storage weight reclaim transaction extension misses the proof\nsize of logic happening prior to itself.\n\n## Done:\n\n* a new storage `ExtrinsicWeightReclaimed` in frame-system. Any logic\nwhich attempts to do some reclaim must use this storage to avoid double\nreclaim.\n* a new function `reclaim_weight` in frame-system pallet: info and post\ninfo in arguments, read the already reclaimed weight, calculate the new\nunused weight from info and post info. do the more accurate reclaim if\nhigher.\n* `CheckWeight` is unchanged and still reclaim the weight in post\ndispatch\n* `ReclaimWeight` is a new transaction extension in frame system. For\nsolo chains it must be used last in the transactino extension pipeline.\nIt does the final most accurate reclaim\n* `StorageWeightReclaim` is moved from cumulus primitives into its own\npallet (in order to define benchmark) and is changed into a wrapping\ntransaction extension.\nIt does the recording of proof size and does the reclaim using this\nrecording and the info and post info. So parachains don't need to use\n`ReclaimWeight`. But also if they use it, there is no bug.\n\n    ```rust\n  /// The TransactionExtension to the basic transaction logic.\npub type TxExtension =\ncumulus_pallet_weight_reclaim::StorageWeightReclaim<\n         Runtime,\n         (\n                 frame_system::CheckNonZeroSender<Runtime>,\n                 frame_system::CheckSpecVersion<Runtime>,\n                 frame_system::CheckTxVersion<Runtime>,\n                 frame_system::CheckGenesis<Runtime>,\n                 frame_system::CheckEra<Runtime>,\n                 frame_system::CheckNonce<Runtime>,\n                 frame_system::CheckWeight<Runtime>,\npallet_transaction_payment::ChargeTransactionPayment<Runtime>,\n                 BridgeRejectObsoleteHeadersAndMessages,\n\n(bridge_to_rococo_config::OnBridgeHubWestendRefundBridgeHubRococoMessages,),\nframe_metadata_hash_extension::CheckMetadataHash<Runtime>,\n         ),\n  >;\n  ```\n\n---------\n\nCo-authored-by: GitHub Action <action@github.com>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-05T03:25:52Z",
          "tree_id": "92e44a9c9c2bddb2fc5b835eb4da173f2b8a4077",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/63c73bf6db1c8982ad3f2310a40799c5987f8900"
        },
        "date": 1736051991523,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 4108024,
            "range": "± 122936",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 289418,
            "range": "± 17464",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4127093,
            "range": "± 193037",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 404407,
            "range": "± 26311",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4966927,
            "range": "± 200976",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 886258,
            "range": "± 41049",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 10478120,
            "range": "± 613029",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4599790,
            "range": "± 139915",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2975430,
            "range": "± 53431",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1480696,
            "range": "± 21041",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3042417,
            "range": "± 57241",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1543709,
            "range": "± 24600",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3635623,
            "range": "± 91894",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1877262,
            "range": "± 21798",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 8061633,
            "range": "± 272383",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4840501,
            "range": "± 142892",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4435848,
            "range": "± 112657",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3611870,
            "range": "± 147599",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32656201,
            "range": "± 787173",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27933054,
            "range": "± 553586",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 248180590,
            "range": "± 1996286",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 253508839,
            "range": "± 12809213",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3309586616,
            "range": "± 26173213",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2018726758,
            "range": "± 23807635",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4182050,
            "range": "± 125135",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4055555,
            "range": "± 112954",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 38089348,
            "range": "± 1752581",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37661385,
            "range": "± 1507463",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 355075054,
            "range": "± 12976759",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 363756456,
            "range": "± 12879202",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4054913258,
            "range": "± 48878190",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3125492903,
            "range": "± 30161622",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "taozui472@gmail.com",
            "name": "taozui472",
            "username": "taozui472"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6eca7647dc99dd0e78aacb740ba931e99e6ba71f",
          "message": "chore: delete repeat words (#7034)\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-01-06T08:44:06Z",
          "tree_id": "abf7ac90d5af0ebd540aee8c38f228c769112504",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6eca7647dc99dd0e78aacb740ba931e99e6ba71f"
        },
        "date": 1736157291167,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3905970,
            "range": "± 66056",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 280654,
            "range": "± 6764",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3690004,
            "range": "± 59087",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 371504,
            "range": "± 9190",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4509512,
            "range": "± 81374",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 816039,
            "range": "± 21283",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9503289,
            "range": "± 152039",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4359837,
            "range": "± 145205",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2884607,
            "range": "± 43739",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1493017,
            "range": "± 24703",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3043151,
            "range": "± 38467",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1552171,
            "range": "± 21059",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3484634,
            "range": "± 45449",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1843064,
            "range": "± 26427",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7523725,
            "range": "± 104130",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4717841,
            "range": "± 97705",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4163308,
            "range": "± 99133",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3487337,
            "range": "± 93978",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 31933932,
            "range": "± 923860",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27611852,
            "range": "± 433782",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 260946370,
            "range": "± 11408175",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 265278331,
            "range": "± 13340064",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3279333357,
            "range": "± 14396138",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2008927945,
            "range": "± 34921018",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4034122,
            "range": "± 138013",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4015369,
            "range": "± 167241",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36834549,
            "range": "± 676804",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37169170,
            "range": "± 1401140",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 331173815,
            "range": "± 6501652",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 361279707,
            "range": "± 14281528",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3953471688,
            "range": "± 37577238",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3149646681,
            "range": "± 37444649",
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
          "id": "ffa90d0f2b9b4438e2f0fa3d4d532923d7ba978f",
          "message": "fix chunk fetching network compatibility zombienet test (#6988)\n\nFix this zombienet test\n\nIt was failing because in\nhttps://github.com/paritytech/polkadot-sdk/pull/6452 I enabled the v2\nreceipts for testnet genesis,\nso the collators started sending v2 receipts with zeroed collator\nsignatures to old validators that were still checking those signatures\n(which lead to disputes, since new validators considered the candidates\nvalid).\n\nThe fix is to also use an old image for collators, so that we don't\ncreate v2 receipts.\n\nWe cannot remove this test yet because collators also perform chunk\nrecovery, so until all collators are upgraded, we need to maintain this\ncompatibility with the old protocol version (which is also why\nsystematic recovery was not yet enabled)",
          "timestamp": "2025-01-06T09:57:29Z",
          "tree_id": "09edfb7ad810cd2c4e6d7ce014ae8e109aae45ec",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ffa90d0f2b9b4438e2f0fa3d4d532923d7ba978f"
        },
        "date": 1736162191502,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3808890,
            "range": "± 110244",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 275815,
            "range": "± 9926",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3717387,
            "range": "± 93585",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 358515,
            "range": "± 9556",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4418636,
            "range": "± 90393",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 812835,
            "range": "± 20234",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9290663,
            "range": "± 211434",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4281558,
            "range": "± 128695",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2801598,
            "range": "± 52864",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1444506,
            "range": "± 19230",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2860477,
            "range": "± 45106",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1502810,
            "range": "± 15012",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3470659,
            "range": "± 65499",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1835416,
            "range": "± 35099",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7392263,
            "range": "± 269149",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4706682,
            "range": "± 178533",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4190980,
            "range": "± 137119",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3438698,
            "range": "± 112496",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 30226480,
            "range": "± 925005",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26639687,
            "range": "± 586670",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 239068548,
            "range": "± 2660569",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 247436794,
            "range": "± 11669233",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3221729474,
            "range": "± 16760086",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1962686755,
            "range": "± 27345165",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3859106,
            "range": "± 163503",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3925331,
            "range": "± 149674",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36373204,
            "range": "± 854067",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 35938730,
            "range": "± 1230967",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 324243368,
            "range": "± 12216152",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 357438764,
            "range": "± 11866322",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3803651710,
            "range": "± 41524888",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3011533976,
            "range": "± 42638794",
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
          "id": "1dcff3df39b85fa43c7ca1dafe10f802cd812234",
          "message": "Avoid incomplete block import pipeline with full verifying import queue (#7050)\n\n## Problem\nIn the parachain template we use the [fully verifying import queue\n\n](https://github.com/paritytech/polkadot-sdk/blob/3d9eddbeb262277c79f2b93b9efb5af95a3a35a8/cumulus/client/consensus/aura/src/equivocation_import_queue.rs#L224-L224)\nwhich does extra equivocation checks.\n\nHowever, when we import a warp synced block with state, we don't set a\nfork choice, leading to an incomplete block import pipeline and error\nhere:\nhttps://github.com/paritytech/polkadot-sdk/blob/3d9eddbeb262277c79f2b93b9efb5af95a3a35a8/substrate/client/service/src/client/client.rs#L488-L488\n\nThis renders warp sync useless for chains using this import queue.\n\n## Fix\nThe fix is to always import a block with state as best block, as we\nalready do in the normal Aura Verifier.\nIn a follow up we should also take another look into unifying the usage\nof the different import queues.\n\nfixes https://github.com/paritytech/project-mythical/issues/256\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-06T13:09:06Z",
          "tree_id": "7963451289ed3c1109e365c661371697a9d65d03",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1dcff3df39b85fa43c7ca1dafe10f802cd812234"
        },
        "date": 1736173070494,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3876549,
            "range": "± 53021",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 279488,
            "range": "± 10096",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3761606,
            "range": "± 92494",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 368404,
            "range": "± 10616",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4494115,
            "range": "± 67068",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 811600,
            "range": "± 17447",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9211622,
            "range": "± 219915",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4277603,
            "range": "± 107807",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2835708,
            "range": "± 35283",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1445679,
            "range": "± 15290",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2903256,
            "range": "± 25615",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1516331,
            "range": "± 16810",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3410323,
            "range": "± 48356",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1800265,
            "range": "± 56556",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7254335,
            "range": "± 142824",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4640605,
            "range": "± 95251",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4071122,
            "range": "± 75040",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3348239,
            "range": "± 67348",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 30710471,
            "range": "± 780883",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26787528,
            "range": "± 385730",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 242816757,
            "range": "± 2769338",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 249817438,
            "range": "± 11476578",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3291345392,
            "range": "± 13796513",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2012043213,
            "range": "± 36155286",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4053165,
            "range": "± 108721",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3839772,
            "range": "± 89516",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 34850659,
            "range": "± 1083269",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 33707496,
            "range": "± 1087444",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 321362859,
            "range": "± 5783083",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 352770778,
            "range": "± 11541321",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3794627028,
            "range": "± 19540581",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3036323015,
            "range": "± 18669648",
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
          "id": "568231a9a85d94954c002532a0f4351a3bb59e83",
          "message": "[core-fellowship] Add permissionless import_member (#7030)\n\nChanges:\n- Add call `import_member` to the core-fellowship pallet.\n- Move common logic between `import` and `import_member` into\n`do_import`.\n\n## `import_member`\n\nCan be used to induct an arbitrary collective member and is callable by\nany signed origin. Pays no fees upon success.\nThis is useful in the case that members did not induct themselves and\nare idling on their rank.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-06T13:52:07Z",
          "tree_id": "d1e2d74ae5b93d0e9604eeeaca75c9b9d82569a9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/568231a9a85d94954c002532a0f4351a3bb59e83"
        },
        "date": 1736178094735,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 4090792,
            "range": "± 81701",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 309648,
            "range": "± 16803",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4051604,
            "range": "± 154752",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 397445,
            "range": "± 14795",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4710195,
            "range": "± 121127",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 847450,
            "range": "± 42654",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9968301,
            "range": "± 342537",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4586250,
            "range": "± 174715",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 3319024,
            "range": "± 220213",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1571194,
            "range": "± 45141",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3326287,
            "range": "± 181630",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1563890,
            "range": "± 59133",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3602093,
            "range": "± 101809",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1892136,
            "range": "± 78813",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7836004,
            "range": "± 233502",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4871388,
            "range": "± 326632",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4407396,
            "range": "± 223825",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3901972,
            "range": "± 178398",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 34640600,
            "range": "± 1344764",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 29580566,
            "range": "± 917696",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 271870468,
            "range": "± 11949353",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 275158562,
            "range": "± 13354818",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3296898431,
            "range": "± 39312735",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2023906261,
            "range": "± 29906929",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4217090,
            "range": "± 293165",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4367271,
            "range": "± 235475",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 38466227,
            "range": "± 1184383",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37935309,
            "range": "± 1408587",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 332026838,
            "range": "± 3695866",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 351125623,
            "range": "± 14905931",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3890996261,
            "range": "± 83933683",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3192131201,
            "range": "± 77317458",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "3776356370@qq.com",
            "name": "jasmy",
            "username": "jasmyhigh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6b6c70b0165b2c38e239eb740a7561e9ed4570de",
          "message": "Fix typos (#7027)\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-01-06T19:16:08Z",
          "tree_id": "abcb5e8f549fd7a7da9d45937584e1de89eb613e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b6c70b0165b2c38e239eb740a7561e9ed4570de"
        },
        "date": 1736195398788,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3847911,
            "range": "± 53851",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 276348,
            "range": "± 12031",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3957493,
            "range": "± 64996",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 363743,
            "range": "± 11154",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4435763,
            "range": "± 109846",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 803285,
            "range": "± 19889",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9298377,
            "range": "± 208041",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4275453,
            "range": "± 100321",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2797003,
            "range": "± 35222",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1453386,
            "range": "± 15340",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2911013,
            "range": "± 29789",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1511974,
            "range": "± 20330",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3436176,
            "range": "± 44581",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1814989,
            "range": "± 22538",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7372065,
            "range": "± 122139",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4669273,
            "range": "± 111088",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4150749,
            "range": "± 81888",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3415643,
            "range": "± 60105",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 30887069,
            "range": "± 454642",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26965721,
            "range": "± 419006",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 242738308,
            "range": "± 2192165",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 242909294,
            "range": "± 8823581",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3265657179,
            "range": "± 11414487",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1984962670,
            "range": "± 31965301",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3871853,
            "range": "± 111766",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3882269,
            "range": "± 98805",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36013315,
            "range": "± 887604",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 36790137,
            "range": "± 1084654",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 326358817,
            "range": "± 3388483",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 355689200,
            "range": "± 10957478",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3803327515,
            "range": "± 22924736",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3031814082,
            "range": "± 22366508",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "conr2d@proton.me",
            "name": "Jeeyong Um",
            "username": "conr2d"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c139739868eddbda495d642219a57602f63c18f5",
          "message": "Remove usage of `sp-std` from Substrate (#7043)\n\n# Description\n\nThis PR removes usage of deprecated `sp-std` from Substrate. (following\nPR of #5010)\n\n## Integration\n\nThis PR doesn't remove re-exported `sp_std` from any crates yet, so\ndownstream projects using re-exported `sp_std` will not be affected.\n\n## Review Notes\n\nThe existing code using `sp-std` is refactored to use `alloc` and `core`\ndirectly. The key-value maps are instantiated from a vector of tuples\ndirectly instead of using `sp_std::map!` macro.\n\n`sp_std::Writer` is a helper type to use `Vec<u8>` with\n`core::fmt::Write` trait. This PR copied it into `sp-runtime`, because\nall crates using `sp_std::Writer` (including `sp-runtime` itself,\n`frame-support`, etc.) depend on `sp-runtime`.\n\nIf this PR is merged, I would write following PRs to remove remaining\nusage of `sp-std` from `bridges` and `cumulus`.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Guillaume Thiolliere <guillaume.thiolliere@parity.io>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-07T07:57:06Z",
          "tree_id": "e2af4afb74389012a6222e82ffced1d704f0788c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c139739868eddbda495d642219a57602f63c18f5"
        },
        "date": 1736241087132,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3963590,
            "range": "± 61072",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 284794,
            "range": "± 7616",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4046422,
            "range": "± 62562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 370042,
            "range": "± 8308",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4554038,
            "range": "± 124522",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 814169,
            "range": "± 23344",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9508392,
            "range": "± 162898",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4336056,
            "range": "± 122545",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2877284,
            "range": "± 40936",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1456618,
            "range": "± 12303",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2983712,
            "range": "± 39240",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1532912,
            "range": "± 26583",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3465626,
            "range": "± 51560",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1835075,
            "range": "± 32065",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7325516,
            "range": "± 195263",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4589776,
            "range": "± 162702",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4197768,
            "range": "± 128165",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3552716,
            "range": "± 113889",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 31055406,
            "range": "± 1106470",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26502711,
            "range": "± 502921",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 241161965,
            "range": "± 2108721",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 250922118,
            "range": "± 10864242",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3276828729,
            "range": "± 16616981",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2010581182,
            "range": "± 23209947",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4056225,
            "range": "± 115775",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3991707,
            "range": "± 154188",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36434782,
            "range": "± 1072184",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 35824166,
            "range": "± 1191894",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 329431593,
            "range": "± 6194212",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 358208520,
            "range": "± 13100818",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4005180256,
            "range": "± 129601846",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3179180880,
            "range": "± 70336373",
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
          "id": "1059be75c36634dff26a9b8711447a0c66926582",
          "message": "workflows: add debug input for sync templates act (#7057)\n\n# Description\n\nIntroduce a workflow `debug` input for `misc-sync-templates.yml` and use\nit instead of the `runner.debug` context variable, which is set to '1'\nwhen `ACTIONS_RUNNER_DEBUG` env/secret is set\n(https://docs.github.com/en/actions/monitoring-and-troubleshooting-workflows/troubleshooting-workflows/enabling-debug-logging#enabling-runner-diagnostic-logging).\nThis is useful for controlling when to show debug prints.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nUsing `runner.debug` requires setting the `ACTIONS_RUNNER_DEBUG` env\nvariable, but setting it to false/true is doable through an input, or by\nimporting a variable from the github env file (which requires a code\nchange). This input alone can replace the entire `runner.debug` +\n`ACTIONS_RUNNER_DEBUG` setup, which simplifies debug printing, but it\ndoesn't look as standard as `runner.debug`. I don't think it is a big\ndeal overall, for this action alone, but happy to account for other\nopinions.\n\nNote: setting the `ACTIONS_RUNNER_DEBUG` whenever we want in a separate\nbranch wouldn't be useful because we can not run the\n`misc-sync-templates.yml` action from other branch than `master` (due to\nbranch protection rules), so we need to expose this input to be\ncontrollable from `master`.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-01-07T09:14:13Z",
          "tree_id": "ffe876cd067ff686e941927f9a3d09c764674c94",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1059be75c36634dff26a9b8711447a0c66926582"
        },
        "date": 1736245494659,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3802828,
            "range": "± 76359",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 271146,
            "range": "± 10030",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3710292,
            "range": "± 118090",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 363559,
            "range": "± 11357",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4533071,
            "range": "± 156926",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 819563,
            "range": "± 32978",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9234844,
            "range": "± 236620",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4281461,
            "range": "± 121224",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2777976,
            "range": "± 42453",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1444982,
            "range": "± 15869",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2899807,
            "range": "± 49593",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1499538,
            "range": "± 39792",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3480172,
            "range": "± 81247",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1834153,
            "range": "± 42844",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7363363,
            "range": "± 251934",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4608326,
            "range": "± 144656",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4125386,
            "range": "± 134468",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3423154,
            "range": "± 124524",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 29528043,
            "range": "± 1012155",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 26103942,
            "range": "± 520606",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 237035184,
            "range": "± 1952751",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 264947173,
            "range": "± 13683456",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3244402572,
            "range": "± 11313293",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1974747359,
            "range": "± 22381332",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4127381,
            "range": "± 160081",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3902517,
            "range": "± 177610",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 34581168,
            "range": "± 1399202",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 34990870,
            "range": "± 1289348",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 322722717,
            "range": "± 8767909",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 355540252,
            "range": "± 12364191",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3785452325,
            "range": "± 28004573",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3060501125,
            "range": "± 27335187",
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
          "id": "baa3bcc60ddab6a700a713e241ad6599feb046dd",
          "message": "Fix defensive! macro to be used in umbrella crates (#7069)\n\nPR for #7054 \n\nReplaced frame_support with $crate from @gui1117 's suggestion to fix\nthe dependency issue\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2025-01-07T13:28:28Z",
          "tree_id": "5d4e70f7cd0c9f24448bd2fd2fff6e915f1e2493",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/baa3bcc60ddab6a700a713e241ad6599feb046dd"
        },
        "date": 1736262582958,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3874516,
            "range": "± 69326",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 278108,
            "range": "± 7072",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3948313,
            "range": "± 80437",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 375878,
            "range": "± 12963",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4485957,
            "range": "± 125089",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 811811,
            "range": "± 25042",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9373535,
            "range": "± 183775",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4384405,
            "range": "± 154492",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2943651,
            "range": "± 81061",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1471068,
            "range": "± 28766",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3001380,
            "range": "± 110513",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1535702,
            "range": "± 31890",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3461374,
            "range": "± 77944",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1854525,
            "range": "± 32793",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7853338,
            "range": "± 267802",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4879917,
            "range": "± 207413",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4552091,
            "range": "± 193097",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3759916,
            "range": "± 164414",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32599780,
            "range": "± 840543",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 28515656,
            "range": "± 955139",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 249838344,
            "range": "± 3072058",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 268609528,
            "range": "± 13382869",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3311063987,
            "range": "± 19896959",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2001033042,
            "range": "± 8195330",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4284618,
            "range": "± 251976",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4266244,
            "range": "± 218578",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36987523,
            "range": "± 1180116",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37557168,
            "range": "± 1410702",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 339157777,
            "range": "± 5902198",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 370631611,
            "range": "± 11737920",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4035602653,
            "range": "± 59010754",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3263368275,
            "range": "± 50627176",
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
          "id": "a5780527041e39268fc8b05b0f3d098cde204883",
          "message": "release: unset SKIP_WASM_BUILD (#7074)\n\n# Description\n\nSeems like I added `SKIP_WASM_BUILD=1` 💀 for arch64 binaries, which\nresults in various errors like:\nhttps://github.com/paritytech/polkadot-sdk/issues/6966. This PR unsets\nthe variable.\n\nCloses #6966.\n\n## Integration\n\nPeople who found workarounds as in #6966 can consume the fixed binaries\nagain.\n\n## Review Notes\n\nI introduced SKIP_WASM_BUILD=1 for some reason for aarch64 (probably to\nspeed up testing) and forgot to remove it. It slipped through and\ninterfered with `stable2412` release artifacts. Needs backporting to\n`stable2412` and then rebuilding/overwriting the aarch64 artifacts.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-01-07T15:25:16Z",
          "tree_id": "893a3df0d4e6361dbfdaf01ae5d38d5d4c987ee4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a5780527041e39268fc8b05b0f3d098cde204883"
        },
        "date": 1736268520754,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3858754,
            "range": "± 58786",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 279890,
            "range": "± 8998",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3751212,
            "range": "± 74582",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 371236,
            "range": "± 9057",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4544672,
            "range": "± 101807",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 828343,
            "range": "± 25551",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9600049,
            "range": "± 247602",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4385913,
            "range": "± 169766",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2881374,
            "range": "± 43686",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1465212,
            "range": "± 10702",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3016014,
            "range": "± 63672",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1543856,
            "range": "± 23994",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3550801,
            "range": "± 87230",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1863856,
            "range": "± 93640",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7803012,
            "range": "± 250452",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4834639,
            "range": "± 189713",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4407904,
            "range": "± 179752",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3702689,
            "range": "± 153893",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 33892316,
            "range": "± 1082717",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27014245,
            "range": "± 873282",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 243997884,
            "range": "± 2690903",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 253208256,
            "range": "± 11727197",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3274560097,
            "range": "± 17451774",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1989348512,
            "range": "± 12458750",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4112669,
            "range": "± 229871",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4140383,
            "range": "± 223642",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 36511321,
            "range": "± 1194242",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37089446,
            "range": "± 1187270",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 334458908,
            "range": "± 6893505",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 358201942,
            "range": "± 11638018",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3997357658,
            "range": "± 64559063",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3112011289,
            "range": "± 63644348",
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
          "id": "645878a27115db52e5d63115699b4bbb89034067",
          "message": "adding warning when using default substrateWeight in production (#7046)\n\nPR for #3581 \nAdded a cfg to show a deprecated warning message when using std\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-01-07T17:17:10Z",
          "tree_id": "975fb5e3dc7c97a7455b793258611ed4568d9131",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/645878a27115db52e5d63115699b4bbb89034067"
        },
        "date": 1736274405467,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3971611,
            "range": "± 60169",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 281864,
            "range": "± 7136",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4111796,
            "range": "± 79086",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 380091,
            "range": "± 8346",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4593554,
            "range": "± 68219",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 825306,
            "range": "± 47347",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9571115,
            "range": "± 161794",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4362975,
            "range": "± 108326",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2942083,
            "range": "± 39564",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1486628,
            "range": "± 18994",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 3088958,
            "range": "± 87832",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1553166,
            "range": "± 13546",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3650516,
            "range": "± 63991",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1874004,
            "range": "± 26653",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7622723,
            "range": "± 140835",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4747776,
            "range": "± 98099",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4227940,
            "range": "± 83229",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3558831,
            "range": "± 67563",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 32799002,
            "range": "± 484221",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27801934,
            "range": "± 434177",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 248441770,
            "range": "± 3705206",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 263097055,
            "range": "± 14144011",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3285820798,
            "range": "± 18754926",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2006116244,
            "range": "± 28591536",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3903296,
            "range": "± 106187",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4018563,
            "range": "± 146090",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 37795081,
            "range": "± 939173",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 37160495,
            "range": "± 1138845",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 341121148,
            "range": "± 10594422",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 363211542,
            "range": "± 11338803",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3864558624,
            "range": "± 46413934",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3039702934,
            "range": "± 21738715",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "alistair.singh7@gmail.com",
            "name": "Alistair Singh",
            "username": "alistair-singh"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4059282fc7b6ec965cc22a9a0df5920a4f3a4101",
          "message": "Snowbridge: Support bridging native ETH (#6855)\n\nChanges:\n1. Use the 0x0000000000000000000000000000000000000000 token address as\nNative ETH.\n2. Convert it to/from `{ parents: 2, interior:\nX1(GlobalConsensus(Ethereum{chain_id: 1})) }` when encountered.\n\nOnchain changes:\nThis will require a governance request to register native ETH (with the\nabove location) in the foreign assets pallet and make it sufficient.\n\nRelated solidity changes:\nhttps://github.com/Snowfork/snowbridge/pull/1354\n\nTODO:\n- [x] Emulated Tests\n\n---------\n\nCo-authored-by: Vincent Geddes <117534+vgeddes@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-01-07T21:23:45Z",
          "tree_id": "3f15f3f4ba924ca1b7785e747d2d5ebca3574d75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4059282fc7b6ec965cc22a9a0df5920a4f3a4101"
        },
        "date": 1736289282454,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3773954,
            "range": "± 45480",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 270671,
            "range": "± 18823",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3883825,
            "range": "± 64909",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 360745,
            "range": "± 9451",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4352012,
            "range": "± 42486",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 800764,
            "range": "± 24813",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9081430,
            "range": "± 155709",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4235362,
            "range": "± 110503",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2773601,
            "range": "± 26123",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1432732,
            "range": "± 12891",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2848062,
            "range": "± 30938",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1501859,
            "range": "± 30900",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3330977,
            "range": "± 33365",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1789380,
            "range": "± 16536",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7129040,
            "range": "± 119363",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4621847,
            "range": "± 92414",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 3982131,
            "range": "± 80913",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3248476,
            "range": "± 50814",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 28966955,
            "range": "± 863263",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 25762491,
            "range": "± 357664",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 258935285,
            "range": "± 12059739",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 256700144,
            "range": "± 12331903",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3207325726,
            "range": "± 9006280",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1959223714,
            "range": "± 25182359",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3813931,
            "range": "± 115772",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3722577,
            "range": "± 94857",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 34756077,
            "range": "± 682765",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 34320641,
            "range": "± 995836",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 312152149,
            "range": "± 5709760",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 345985398,
            "range": "± 10391811",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3806987692,
            "range": "± 41860745",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3044001204,
            "range": "± 34217172",
            "unit": "ns/iter"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "wenmujia@gmail.com",
            "name": "wmjae",
            "username": "wmjae"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cdf107de700388a52a17b2fb852c98420c78278e",
          "message": "fix typo (#7096)\n\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>",
          "timestamp": "2025-01-09T11:51:38Z",
          "tree_id": "3a346971e1d34265114adf78bf999eaa0b5b158d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cdf107de700388a52a17b2fb852c98420c78278e"
        },
        "date": 1736427857552,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3879280,
            "range": "± 67235",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 276356,
            "range": "± 7411",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 4000964,
            "range": "± 70169",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 368043,
            "range": "± 7500",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4547597,
            "range": "± 92221",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 821382,
            "range": "± 19862",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9388421,
            "range": "± 178727",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4462378,
            "range": "± 150082",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2906832,
            "range": "± 47976",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1470546,
            "range": "± 22673",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2982762,
            "range": "± 37500",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1536729,
            "range": "± 16692",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3623290,
            "range": "± 89352",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1883776,
            "range": "± 31581",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7757829,
            "range": "± 220533",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4855076,
            "range": "± 189332",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 4405210,
            "range": "± 196050",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3748726,
            "range": "± 139968",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 33408568,
            "range": "± 1032657",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 27686700,
            "range": "± 564304",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 249794381,
            "range": "± 2978159",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 259000543,
            "range": "± 13683403",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3351220991,
            "range": "± 11133285",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 2023812442,
            "range": "± 26464424",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 4507529,
            "range": "± 217057",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 4408204,
            "range": "± 196764",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 38347372,
            "range": "± 1321158",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 39009581,
            "range": "± 1567948",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 346782242,
            "range": "± 11684804",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 381454528,
            "range": "± 13605353",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 4160775890,
            "range": "± 41479648",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 3312227861,
            "range": "± 38626756",
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
          "id": "2f179585229880a596ab3b8b04a4be6c7db15efa",
          "message": "Migrating salary pallet to use umbrella crate (#7048)\n\n# Description\n\nMigrating salary pallet to use umbrella crate. It is a follow-up from\nhttps://github.com/paritytech/polkadot-sdk/pull/7025\nWhy did I create this new branch? \nI did this, so that the unnecessary cargo fmt changes from the previous\nbranch are discarded and hence opened this new PR.\n\n\n\n## Review Notes\n\nThis PR migrates pallet-salary to use the umbrella crate.\n\nAdded change: Explanation requested for why `TestExternalities` was\nreplaced by `TestState` as testing_prelude already includes it\n`pub use sp_io::TestExternalities as TestState;`\n\n\nI have also modified the defensive! macro to be compatible with umbrella\ncrate as it was being used in the salary pallet",
          "timestamp": "2025-01-09T14:48:59Z",
          "tree_id": "80ad6cace63f730e02467bda8a02b408a11f6a18",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f179585229880a596ab3b8b04a4be6c7db15efa"
        },
        "date": 1736438387181,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64B",
            "value": 3746328,
            "range": "± 50011",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64B",
            "value": 266946,
            "range": "± 7138",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/512B",
            "value": 3581032,
            "range": "± 40118",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/512B",
            "value": 353675,
            "range": "± 8229",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/4KB",
            "value": 4278872,
            "range": "± 79823",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/4KB",
            "value": 791619,
            "range": "± 34238",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/serially/64KB",
            "value": 9017530,
            "range": "± 136760",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/libp2p/with_backpressure/64KB",
            "value": 4157611,
            "range": "± 70996",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64B",
            "value": 2724857,
            "range": "± 21775",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64B",
            "value": 1418880,
            "range": "± 14934",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/512B",
            "value": 2816469,
            "range": "± 65367",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/512B",
            "value": 1469661,
            "range": "± 15379",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/4KB",
            "value": 3283013,
            "range": "± 33593",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/4KB",
            "value": 1760945,
            "range": "± 17765",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/serially/64KB",
            "value": 7016864,
            "range": "± 75063",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/small_payload/litep2p/with_backpressure/64KB",
            "value": 4525404,
            "range": "± 71329",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/256KB",
            "value": 3943814,
            "range": "± 69308",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/256KB",
            "value": 3173205,
            "range": "± 34195",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/2MB",
            "value": 28028511,
            "range": "± 245216",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/2MB",
            "value": 25329999,
            "range": "± 300385",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/16MB",
            "value": 231582439,
            "range": "± 1768326",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/16MB",
            "value": 244223103,
            "range": "± 23151160",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/serially/128MB",
            "value": 3210023167,
            "range": "± 11298390",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/libp2p/with_backpressure/128MB",
            "value": 1962457493,
            "range": "± 22235950",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/256KB",
            "value": 3732539,
            "range": "± 105806",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/256KB",
            "value": 3653157,
            "range": "± 97229",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/2MB",
            "value": 33526466,
            "range": "± 732809",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/2MB",
            "value": 32781117,
            "range": "± 1076919",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/16MB",
            "value": 304154455,
            "range": "± 8170112",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/16MB",
            "value": 340633565,
            "range": "± 11750076",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/serially/128MB",
            "value": 3644978398,
            "range": "± 10144677",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/large_payload/litep2p/with_backpressure/128MB",
            "value": 2907009122,
            "range": "± 11191583",
            "unit": "ns/iter"
          }
        ]
      }
    ],
    "notifications_protocol": [
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
        "date": 1736449887214,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3916300,
            "range": "± 57754",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 283693,
            "range": "± 6562",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4003301,
            "range": "± 35743",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 366026,
            "range": "± 3913",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4724657,
            "range": "± 53664",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 835324,
            "range": "± 20276",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9633809,
            "range": "± 57526",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4512822,
            "range": "± 70469",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 42344498,
            "range": "± 388186",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36003389,
            "range": "± 312116",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 337147226,
            "range": "± 2607089",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 276402224,
            "range": "± 2220892",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2481566683,
            "range": "± 10727894",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2747053851,
            "range": "± 75963977",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2894391,
            "range": "± 37626",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1473478,
            "range": "± 8135",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2989574,
            "range": "± 32068",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1545840,
            "range": "± 29535",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3525168,
            "range": "± 24478",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1854878,
            "range": "± 18338",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7546160,
            "range": "± 58380",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4764178,
            "range": "± 54826",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 39565327,
            "range": "± 497901",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 38070475,
            "range": "± 418339",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 374627042,
            "range": "± 3003944",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 432991245,
            "range": "± 7068816",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3352563084,
            "range": "± 19260000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3727272264,
            "range": "± 74032644",
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
        "date": 1736478653735,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3785082,
            "range": "± 30766",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 278670,
            "range": "± 3772",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3863373,
            "range": "± 25614",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 357720,
            "range": "± 7479",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4637444,
            "range": "± 43625",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 826093,
            "range": "± 8227",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9524602,
            "range": "± 207704",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4466046,
            "range": "± 41512",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41131784,
            "range": "± 460295",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36342742,
            "range": "± 430185",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 340356980,
            "range": "± 5667950",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 267097769,
            "range": "± 1221957",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2400086727,
            "range": "± 10230591",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2531966857,
            "range": "± 222159352",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2836970,
            "range": "± 15922",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1465755,
            "range": "± 4714",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2949665,
            "range": "± 21477",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1547295,
            "range": "± 16287",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3376120,
            "range": "± 13909",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1826528,
            "range": "± 11738",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7347416,
            "range": "± 84314",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4665120,
            "range": "± 44519",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 38427468,
            "range": "± 356990",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 36773068,
            "range": "± 686590",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 351498438,
            "range": "± 3261252",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 410714529,
            "range": "± 7095047",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3176017676,
            "range": "± 16156027",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3530794982,
            "range": "± 58367052",
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
        "date": 1736594621203,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3777912,
            "range": "± 41955",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 278483,
            "range": "± 6752",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3903445,
            "range": "± 41102",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 357833,
            "range": "± 2926",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4654831,
            "range": "± 45626",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 821936,
            "range": "± 9611",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9623963,
            "range": "± 114007",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4688239,
            "range": "± 93981",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 42871373,
            "range": "± 639785",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36800224,
            "range": "± 536026",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 340617095,
            "range": "± 2779158",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 282491378,
            "range": "± 2432804",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2445503995,
            "range": "± 10094502",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2648461022,
            "range": "± 69472381",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2866970,
            "range": "± 18888",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1497736,
            "range": "± 6924",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2987006,
            "range": "± 46028",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1537401,
            "range": "± 9781",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3487528,
            "range": "± 49041",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1868171,
            "range": "± 12893",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7505754,
            "range": "± 91872",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4646896,
            "range": "± 21833",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40973995,
            "range": "± 616592",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 38726724,
            "range": "± 1074630",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 363568750,
            "range": "± 4193204",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 391030553,
            "range": "± 19672552",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3264920051,
            "range": "± 20627455",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3584889904,
            "range": "± 51205389",
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
        "date": 1736775152465,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3788422,
            "range": "± 43329",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 274065,
            "range": "± 2392",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3905402,
            "range": "± 32611",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 356337,
            "range": "± 5312",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4550270,
            "range": "± 45624",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 814899,
            "range": "± 9212",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9385030,
            "range": "± 65390",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4426854,
            "range": "± 54914",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 40669559,
            "range": "± 1189532",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35189263,
            "range": "± 553040",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 321680072,
            "range": "± 2736674",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 268884974,
            "range": "± 1198140",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2408236629,
            "range": "± 8798457",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2573485851,
            "range": "± 111008013",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2773971,
            "range": "± 15740",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1444141,
            "range": "± 10543",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2875450,
            "range": "± 18210",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1510887,
            "range": "± 12370",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3361692,
            "range": "± 18941",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1805270,
            "range": "± 8374",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7310480,
            "range": "± 78677",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4656035,
            "range": "± 43242",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 38281469,
            "range": "± 369278",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 36499492,
            "range": "± 564993",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 354958906,
            "range": "± 2396803",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 384781835,
            "range": "± 5316347",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3193235041,
            "range": "± 19041708",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3585031363,
            "range": "± 51348732",
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
        "date": 1736784063662,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3914493,
            "range": "± 61714",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 278163,
            "range": "± 2388",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3970971,
            "range": "± 19791",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 360002,
            "range": "± 2853",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4637518,
            "range": "± 52517",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 821699,
            "range": "± 13360",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9496174,
            "range": "± 137079",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4471868,
            "range": "± 46559",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 40865860,
            "range": "± 546530",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 34809056,
            "range": "± 485831",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 318946176,
            "range": "± 1682352",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 272147733,
            "range": "± 1354031",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2425309343,
            "range": "± 17954470",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2491571101,
            "range": "± 54253669",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2793668,
            "range": "± 19046",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1451941,
            "range": "± 5218",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2910049,
            "range": "± 15039",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1513224,
            "range": "± 6703",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3409809,
            "range": "± 27989",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1828645,
            "range": "± 12198",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7366001,
            "range": "± 125380",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4700843,
            "range": "± 62399",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 39937122,
            "range": "± 537998",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 38393368,
            "range": "± 992753",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 357171014,
            "range": "± 4447318",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 419904164,
            "range": "± 8638884",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3339569999,
            "range": "± 61671096",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3576125308,
            "range": "± 100016903",
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
        "date": 1736793198012,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3734911,
            "range": "± 31342",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 274936,
            "range": "± 2016",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3876095,
            "range": "± 29009",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 355482,
            "range": "± 2689",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4548339,
            "range": "± 26648",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 822370,
            "range": "± 5805",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9402587,
            "range": "± 118327",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4469250,
            "range": "± 59395",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41289052,
            "range": "± 699709",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35854881,
            "range": "± 249541",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 332496844,
            "range": "± 2581891",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 282012526,
            "range": "± 2955561",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2456053953,
            "range": "± 14679658",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2614989431,
            "range": "± 83732455",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2777261,
            "range": "± 12137",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1441013,
            "range": "± 3835",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2854816,
            "range": "± 17374",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1504493,
            "range": "± 5385",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3396667,
            "range": "± 32761",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1816543,
            "range": "± 10746",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7485032,
            "range": "± 129628",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4651820,
            "range": "± 36115",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40557397,
            "range": "± 954001",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 38498746,
            "range": "± 946873",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 367330810,
            "range": "± 4796012",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 422829357,
            "range": "± 7633587",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3329300563,
            "range": "± 17091682",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3754738650,
            "range": "± 73573634",
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
        "date": 1736847200709,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3871753,
            "range": "± 48785",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 275027,
            "range": "± 2977",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3998102,
            "range": "± 33586",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 355855,
            "range": "± 3879",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4660396,
            "range": "± 42504",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 808024,
            "range": "± 11508",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9552954,
            "range": "± 174720",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4411878,
            "range": "± 107959",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41157289,
            "range": "± 470381",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35039037,
            "range": "± 445404",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 315341389,
            "range": "± 2670638",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 267898761,
            "range": "± 2648146",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2386868361,
            "range": "± 7503806",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2582841862,
            "range": "± 57979993",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2842648,
            "range": "± 17498",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1442775,
            "range": "± 9272",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2892428,
            "range": "± 28574",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1497197,
            "range": "± 6370",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3439682,
            "range": "± 26722",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1800000,
            "range": "± 10739",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7416363,
            "range": "± 105421",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4598205,
            "range": "± 75000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 38912023,
            "range": "± 806770",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 37646309,
            "range": "± 1079662",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 351779870,
            "range": "± 3251384",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 409732430,
            "range": "± 6642406",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3251790150,
            "range": "± 37814858",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3632578924,
            "range": "± 44997241",
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
        "date": 1736865023944,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4038927,
            "range": "± 62217",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 294604,
            "range": "± 4362",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4173081,
            "range": "± 29138",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 377610,
            "range": "± 6949",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4965777,
            "range": "± 78848",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 863414,
            "range": "± 6139",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 10155264,
            "range": "± 142177",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4711848,
            "range": "± 187779",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 45200542,
            "range": "± 1078302",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 37469983,
            "range": "± 399459",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 347112151,
            "range": "± 1882762",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 285141556,
            "range": "± 1518194",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2528555591,
            "range": "± 21963166",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2761085238,
            "range": "± 72124785",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 3027302,
            "range": "± 28799",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1497281,
            "range": "± 7779",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 3113076,
            "range": "± 42853",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1556170,
            "range": "± 11326",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3728366,
            "range": "± 61527",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1909315,
            "range": "± 17921",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7936613,
            "range": "± 93051",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4959558,
            "range": "± 48236",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 42449242,
            "range": "± 854797",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 40549332,
            "range": "± 773211",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 393213675,
            "range": "± 4802109",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 457749119,
            "range": "± 4710554",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3523023289,
            "range": "± 49905393",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3857670926,
            "range": "± 59902907",
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
        "date": 1736866672784,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4174471,
            "range": "± 38928",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 296888,
            "range": "± 5716",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4250518,
            "range": "± 36044",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 371113,
            "range": "± 3569",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 5010433,
            "range": "± 55406",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 884387,
            "range": "± 13178",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 10038446,
            "range": "± 186127",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4652014,
            "range": "± 62875",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 43414561,
            "range": "± 584895",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36576338,
            "range": "± 408270",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 341958747,
            "range": "± 3074440",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 278175048,
            "range": "± 1404879",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2704768794,
            "range": "± 46465899",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2658162185,
            "range": "± 39296412",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2878692,
            "range": "± 41351",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1468428,
            "range": "± 12703",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2929185,
            "range": "± 62495",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1514063,
            "range": "± 5726",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3522224,
            "range": "± 43209",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1859933,
            "range": "± 29434",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7980716,
            "range": "± 149825",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 5022007,
            "range": "± 44779",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 44715796,
            "range": "± 840204",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 44100661,
            "range": "± 1209925",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 430780227,
            "range": "± 10369054",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 507325621,
            "range": "± 13053933",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3747868103,
            "range": "± 106311594",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3873528121,
            "range": "± 114776326",
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
        "date": 1736869526636,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4232036,
            "range": "± 73802",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 308274,
            "range": "± 7909",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4331431,
            "range": "± 36098",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 389100,
            "range": "± 4402",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 5070129,
            "range": "± 80934",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 914525,
            "range": "± 24697",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 10548211,
            "range": "± 125160",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4892260,
            "range": "± 161365",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 50358941,
            "range": "± 1171008",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 38217720,
            "range": "± 508905",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 363457606,
            "range": "± 1991306",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 292274530,
            "range": "± 3238906",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2586770841,
            "range": "± 8478961",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2745712758,
            "range": "± 38865908",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 3124126,
            "range": "± 36784",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1528706,
            "range": "± 13490",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 3315580,
            "range": "± 27791",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1615740,
            "range": "± 10794",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3919305,
            "range": "± 23896",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1960575,
            "range": "± 12378",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 8229694,
            "range": "± 234017",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 5054233,
            "range": "± 61603",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 42955837,
            "range": "± 448451",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 44402531,
            "range": "± 1096881",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 436417352,
            "range": "± 8608039",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 482390048,
            "range": "± 13887428",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3762777194,
            "range": "± 101235227",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3838172579,
            "range": "± 83878706",
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
        "date": 1736877676812,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4015816,
            "range": "± 61287",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 281659,
            "range": "± 3107",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4074094,
            "range": "± 52880",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 363933,
            "range": "± 5437",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4837073,
            "range": "± 43703",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 843672,
            "range": "± 12941",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9947825,
            "range": "± 186008",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4705275,
            "range": "± 262306",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 44183324,
            "range": "± 357631",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 37020356,
            "range": "± 377470",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 338000807,
            "range": "± 1709426",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 278742096,
            "range": "± 2886690",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2423116651,
            "range": "± 11889586",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2430628565,
            "range": "± 279933504",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2857450,
            "range": "± 16400",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1472957,
            "range": "± 7385",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2981769,
            "range": "± 26971",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1539909,
            "range": "± 7383",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3580638,
            "range": "± 32763",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1878296,
            "range": "± 17340",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 8165085,
            "range": "± 121796",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 5026547,
            "range": "± 105017",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 43953709,
            "range": "± 689016",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 42824378,
            "range": "± 933113",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 405598081,
            "range": "± 7412349",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 449542201,
            "range": "± 5896897",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3660587969,
            "range": "± 63141382",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3945801505,
            "range": "± 47906396",
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
        "date": 1736880372737,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4016625,
            "range": "± 40764",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 282999,
            "range": "± 4125",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4134841,
            "range": "± 27863",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 365232,
            "range": "± 3368",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4822952,
            "range": "± 30716",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 835976,
            "range": "± 10224",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9861727,
            "range": "± 63988",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4525443,
            "range": "± 33302",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 43071614,
            "range": "± 384803",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35928671,
            "range": "± 402072",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 334118929,
            "range": "± 5139951",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 275147659,
            "range": "± 1855929",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2428400611,
            "range": "± 8917765",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2669118833,
            "range": "± 39228540",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2875864,
            "range": "± 46878",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1472197,
            "range": "± 15172",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 3006159,
            "range": "± 17616",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1549248,
            "range": "± 8191",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3422340,
            "range": "± 37209",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1817379,
            "range": "± 21656",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7347512,
            "range": "± 80908",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4701345,
            "range": "± 38383",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 41755688,
            "range": "± 388206",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 39903805,
            "range": "± 887285",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 369455499,
            "range": "± 1381953",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 407079631,
            "range": "± 6965634",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3273634133,
            "range": "± 14660973",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3583290801,
            "range": "± 87903276",
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
        "date": 1736887755915,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3833416,
            "range": "± 33871",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 278954,
            "range": "± 5236",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3930345,
            "range": "± 48097",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 352851,
            "range": "± 8782",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4647497,
            "range": "± 49160",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 824195,
            "range": "± 6443",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9499072,
            "range": "± 119437",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4433691,
            "range": "± 54592",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41103699,
            "range": "± 355922",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35352595,
            "range": "± 390252",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 320169924,
            "range": "± 1902824",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 273435476,
            "range": "± 1678245",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2441079639,
            "range": "± 12322871",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2183838695,
            "range": "± 176497893",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2778597,
            "range": "± 22292",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1431994,
            "range": "± 9607",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2897054,
            "range": "± 19594",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1497712,
            "range": "± 8799",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3374378,
            "range": "± 21904",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1798444,
            "range": "± 8147",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7341660,
            "range": "± 93788",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4673742,
            "range": "± 55825",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40130216,
            "range": "± 695638",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 39073287,
            "range": "± 627795",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 367017865,
            "range": "± 4573380",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 417550682,
            "range": "± 8419759",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3375268134,
            "range": "± 31363478",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3619228326,
            "range": "± 113985699",
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
        "date": 1736889212699,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4180671,
            "range": "± 62200",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 294153,
            "range": "± 10331",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4206131,
            "range": "± 50305",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 378220,
            "range": "± 3182",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 5044520,
            "range": "± 60556",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 873283,
            "range": "± 8736",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 10134223,
            "range": "± 124915",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4630219,
            "range": "± 50024",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 44602717,
            "range": "± 660741",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36285380,
            "range": "± 335697",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 348610703,
            "range": "± 4252385",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 283662692,
            "range": "± 2309433",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2540649530,
            "range": "± 8856026",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2818058246,
            "range": "± 54683294",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 3092209,
            "range": "± 48834",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1491452,
            "range": "± 8009",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 3152361,
            "range": "± 27259",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1576144,
            "range": "± 10145",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3753041,
            "range": "± 52866",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1909707,
            "range": "± 19579",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7865499,
            "range": "± 61341",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4896906,
            "range": "± 127982",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40824947,
            "range": "± 536637",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 39102417,
            "range": "± 803881",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 386535154,
            "range": "± 6079732",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 395275803,
            "range": "± 8435450",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3351445151,
            "range": "± 9099317",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3627295078,
            "range": "± 52204781",
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
        "date": 1736897805194,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3739527,
            "range": "± 149872",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 271938,
            "range": "± 3825",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3869995,
            "range": "± 11081",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 347067,
            "range": "± 2848",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4485205,
            "range": "± 32817",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 809929,
            "range": "± 6147",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9258310,
            "range": "± 48000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4353926,
            "range": "± 34280",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 39315338,
            "range": "± 297069",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 34057944,
            "range": "± 204011",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 307540679,
            "range": "± 3071287",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 262840948,
            "range": "± 2035588",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2334678432,
            "range": "± 15785465",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2406341560,
            "range": "± 99512312",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2732180,
            "range": "± 22727",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1422190,
            "range": "± 9896",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2829860,
            "range": "± 10076",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1475203,
            "range": "± 12036",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3318892,
            "range": "± 14266",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1783233,
            "range": "± 13266",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7237214,
            "range": "± 69991",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4587630,
            "range": "± 171445",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 38092580,
            "range": "± 492217",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 36146238,
            "range": "± 372777",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 344931493,
            "range": "± 1958705",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 414667443,
            "range": "± 4475969",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3274453814,
            "range": "± 32133129",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3541528741,
            "range": "± 60309364",
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
        "date": 1736935742386,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 4007734,
            "range": "± 32740",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 290769,
            "range": "± 3874",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4090512,
            "range": "± 44967",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 363278,
            "range": "± 6778",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4790900,
            "range": "± 23175",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 838748,
            "range": "± 9030",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9954977,
            "range": "± 36375",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4560481,
            "range": "± 66638",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 42220129,
            "range": "± 644697",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 36037608,
            "range": "± 195625",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 339908197,
            "range": "± 3214521",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 278115930,
            "range": "± 908362",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2472258639,
            "range": "± 15707592",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2688877557,
            "range": "± 117709611",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2944167,
            "range": "± 38125",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1495605,
            "range": "± 9094",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 3085727,
            "range": "± 15698",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1558794,
            "range": "± 7338",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3694359,
            "range": "± 25425",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1891656,
            "range": "± 9865",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7741601,
            "range": "± 75625",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4853241,
            "range": "± 39941",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40224143,
            "range": "± 512258",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 38792039,
            "range": "± 712816",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 378266059,
            "range": "± 3465971",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 409746059,
            "range": "± 5514226",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3379600303,
            "range": "± 11133795",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3661297627,
            "range": "± 69608676",
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
          "id": "f798111afc15f464a772cd7ed37910cc6208b713",
          "message": "Fix reversed error message in DispatchInfo (#7170)\n\nFix error message in `DispatchInfo` where post-dispatch and pre-dispatch\nweight was reversed.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-01-15T10:08:49Z",
          "tree_id": "609eaa12121e35ad653cd4c11d114adb537eb683",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f798111afc15f464a772cd7ed37910cc6208b713"
        },
        "date": 1736939179337,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3806774,
            "range": "± 26047",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 273609,
            "range": "± 3969",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3927061,
            "range": "± 24566",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 354810,
            "range": "± 2892",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4607200,
            "range": "± 20306",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 798739,
            "range": "± 5548",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9105546,
            "range": "± 75931",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4243508,
            "range": "± 48000",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 38914541,
            "range": "± 303881",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 33123939,
            "range": "± 369291",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 296417293,
            "range": "± 2036700",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 261993454,
            "range": "± 1580985",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2336801276,
            "range": "± 14214383",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2127946047,
            "range": "± 15494203",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2758413,
            "range": "± 11994",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1437835,
            "range": "± 3366",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2843344,
            "range": "± 14141",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1490152,
            "range": "± 5839",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3347714,
            "range": "± 10492",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1777432,
            "range": "± 7349",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7221351,
            "range": "± 59574",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4585072,
            "range": "± 33151",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 37649829,
            "range": "± 291324",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 35936675,
            "range": "± 307022",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 338102452,
            "range": "± 3521794",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 381402557,
            "range": "± 8480626",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3039921053,
            "range": "± 19422349",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3453719962,
            "range": "± 69851318",
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
        "date": 1736944075341,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3821816,
            "range": "± 27412",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 278010,
            "range": "± 2173",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 3975701,
            "range": "± 26673",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 364665,
            "range": "± 3849",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4655931,
            "range": "± 125711",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 828138,
            "range": "± 9690",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9515735,
            "range": "± 70862",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4509752,
            "range": "± 32896",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41512992,
            "range": "± 574256",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35934065,
            "range": "± 884833",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 336979257,
            "range": "± 5076129",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 275985743,
            "range": "± 2396478",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2428726428,
            "range": "± 7895245",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2630879526,
            "range": "± 128050513",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2791132,
            "range": "± 12660",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1457870,
            "range": "± 10483",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2938541,
            "range": "± 72255",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1516066,
            "range": "± 6569",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3443669,
            "range": "± 18736",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1839359,
            "range": "± 20490",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7431429,
            "range": "± 73070",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4733240,
            "range": "± 36957",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 40758859,
            "range": "± 1360243",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 39482669,
            "range": "± 662902",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 371270150,
            "range": "± 3098292",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 399610890,
            "range": "± 7073138",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3332974880,
            "range": "± 20790905",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3611302171,
            "range": "± 45364107",
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
        "date": 1736950830271,
        "tool": "cargo",
        "benches": [
          {
            "name": "notifications_protocol/libp2p/serially/64B",
            "value": 3895044,
            "range": "± 28674",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64B",
            "value": 283685,
            "range": "± 4153",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/512B",
            "value": 4008113,
            "range": "± 35459",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/512B",
            "value": 364561,
            "range": "± 15785",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/4KB",
            "value": 4615393,
            "range": "± 26687",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/4KB",
            "value": 825392,
            "range": "± 8044",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/64KB",
            "value": 9552068,
            "range": "± 91252",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/64KB",
            "value": 4463801,
            "range": "± 35871",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/256KB",
            "value": 41053461,
            "range": "± 559801",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/256KB",
            "value": 35493376,
            "range": "± 249622",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/2MB",
            "value": 330191334,
            "range": "± 3351492",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/2MB",
            "value": 275178902,
            "range": "± 2156541",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/serially/16MB",
            "value": 2443739069,
            "range": "± 4822358",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/libp2p/with_backpressure/16MB",
            "value": 2668549433,
            "range": "± 269185369",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64B",
            "value": 2845986,
            "range": "± 28221",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64B",
            "value": 1471427,
            "range": "± 6503",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/512B",
            "value": 2908156,
            "range": "± 21381",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/512B",
            "value": 1512563,
            "range": "± 10376",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/4KB",
            "value": 3451150,
            "range": "± 29668",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/4KB",
            "value": 1835512,
            "range": "± 9584",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/64KB",
            "value": 7341448,
            "range": "± 71510",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/64KB",
            "value": 4672384,
            "range": "± 42391",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/256KB",
            "value": 39173703,
            "range": "± 364161",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/256KB",
            "value": 37773083,
            "range": "± 491831",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/2MB",
            "value": 361334177,
            "range": "± 2461557",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/2MB",
            "value": 410316641,
            "range": "± 11044254",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/serially/16MB",
            "value": 3432271446,
            "range": "± 48610130",
            "unit": "ns/iter"
          },
          {
            "name": "notifications_protocol/litep2p/with_backpressure/16MB",
            "value": 3588250847,
            "range": "± 70150226",
            "unit": "ns/iter"
          }
        ]
      }
    ]
  }
}