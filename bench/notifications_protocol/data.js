window.BENCHMARK_DATA = {
  "lastUpdate": 1736178111886,
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
      }
    ]
  }
}