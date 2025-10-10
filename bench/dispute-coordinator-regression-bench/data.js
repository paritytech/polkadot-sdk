window.BENCHMARK_DATA = {
  "lastUpdate": 1760120117737,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "dispute-coordinator-regression-bench": [
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
          "id": "83b0409093f811acb412b07ac7219b7ad1a514ff",
          "message": "[subsystem-bench] Add Dispute Coordinator subsystem benchmark (#8828)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/8811\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-03T12:22:23Z",
          "tree_id": "7dedca9f4f5317f038bb7713852df1f21eeee806",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83b0409093f811acb412b07ac7219b7ad1a514ff"
        },
        "date": 1751549436117,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005595405729999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008679936599999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026281824699999996,
            "unit": "seconds"
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
          "id": "3bd01b9c89dbef0f57a3c0fb7f600fbb5befff65",
          "message": "[Release|CI/CD] Fix syncing in the release flow (#9092)\n\nThis PR adds a fix for the release pipelines. The sync flow needs a\nsecrete to be passed when it is called from another flow and syncing\nbetween release org and the main repo is needed.\nMissing secrets were added to the appropriate flows.",
          "timestamp": "2025-07-03T15:06:37Z",
          "tree_id": "806f5adc03322aa929b1b29440cb9212f69c9fe8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3bd01b9c89dbef0f57a3c0fb7f600fbb5befff65"
        },
        "date": 1751559377721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005582663829999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026697256099999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008752567599999988,
            "unit": "seconds"
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
          "distinct": false,
          "id": "f1ba2a1c7206c70ad66168859c90ab4e4327aab6",
          "message": "Optimize buffered offence storage and prevent unbounded growth in staking-async ah-client pallet (#9049)\n\n## 🤔 Why\nThis addresses potential memory issues and improves efficiency of\noffence handling during buffered operating mode (see\nhttps://github.com/paritytech-secops/srlabs_findings/issues/525)\n\n\n## 🔑 Key changes\n\n- Prevents duplicate offences for the same offender in the same session\nby keeping only the highest slash fraction\n- Introduces `BufferedOffence` struct with optional reporter and slash\nfraction fields\n- Restructures buffered offences storage from `Vec<(SessionIndex,\nVec<Offence>)>` to nested `BTreeMap<SessionIndex, BTreeMap<AccountId,\nBufferedOffence>>`\n- Adds `MaxOffenceBatchSize` configuration parameter for batching\ncontrol\n- Processes offences in batches with configurable size limits, sending\nonly first session's offences per block\n- Implements proper benchmarking infrastructure for\n`process_buffered_offences` function\n- Adds WeightInfo trait with benchmarked weights for batch processing in\n`on_initialize` hook\n\n## ✍️ Co-authors\n@Ank4n \n@sigurpol\n\n---------\n\nCo-authored-by: Paolo La Camera <paolo@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-04T09:02:33Z",
          "tree_id": "410487862394418dd87119db2954a36e4de0c43c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f1ba2a1c7206c70ad66168859c90ab4e4327aab6"
        },
        "date": 1751623985007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002641694280000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871780210999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005657479960000001,
            "unit": "seconds"
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
          "distinct": false,
          "id": "22714211e4f558abbabae28fc2e8f2c971143638",
          "message": "[AHM] Derive DecodeWithMemTracking and pub fields (#9067)\n\n- Derive `DecodeWithMemTracking` on structs\n- Make some fields public\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-04T10:36:12Z",
          "tree_id": "0dd0655d92d837e407ee908f523b783ecccc626a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/22714211e4f558abbabae28fc2e8f2c971143638"
        },
        "date": 1751629886195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005486065759999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008570165919999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267932138,
            "unit": "seconds"
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
          "id": "252649fc0105efc8b32b2e1a3649bd6d09f8bd53",
          "message": "add benchmark for prune-era (#9056)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-04T18:25:54Z",
          "tree_id": "c4480f0f14cd79f70f4a2733fab6a6d0c4c81f6b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/252649fc0105efc8b32b2e1a3649bd6d09f8bd53"
        },
        "date": 1751657691195,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005649167919999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008880581469999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026971257299999987,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "james@jsdw.me",
            "name": "James Wilson",
            "username": "jsdw"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "771c9988e2a636a150d97c10e3122af8068d1687",
          "message": "Bump CI to Rustc 1.88 to support 2024 edition crates (#8592)\n\nAs one example, this allows us to use the latest version of Subxt: 0.42.\nAlso if-let chains :)\n\nMain changes:\n- Update CI image\n- Remove `forklift` from Build step in\n`check-revive-stable-uapi-polkavm`; it seemed to [cause an\nerror](https://github.com/paritytech/polkadot-sdk/actions/runs/16004536662/job/45148002314?pr=8592).\nPerhaps we can open an issue for this to fix/try again after this\nmerges.\n- Bump `polkavm` deps to 0.26 to avoid [this\nerror](https://github.com/paritytech/polkadot-sdk/actions/runs/16004991577/job/45150325849?pr=8592#step:5:1967)\n(thanks @koute!)\n- Add `result_large_err` clippy to avoid a bunch of clippy warnings\nabout a 176 byte error (again, we could fix this later more properly).\n- Clippy fixes (mainly inlining args into `format!`s where possible),\nremove one `#[no_mangle]` on a `#[panic_hook]` and a few other misc\nautomatic fixes.\n- `#[allow(clippy::useless_conversion)]` in frame macro to avoid the\ngenerated `.map(Into::into).map_err(Into::into)` code causing an issue\nwhen not necessary (it is sometimes; depends on the return type in\npallet calls)\n- UI test updates\n\nAs a side note, I haven't added a `prdoc` since I'm not making any\nbreaking changes (despite touching a bunch of pallets), just clippy/fmt\ntype things. Please comment if this isn't ok!\n\nAlso, thankyou @bkchr for the wasmtime update PR which fixed a blocker\nhere!\n\n---------\n\nCo-authored-by: Evgeny Snitko <evgeny@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-04T21:54:27Z",
          "tree_id": "bbce6a530538cfc5d3328f5239b16d133890b86d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/771c9988e2a636a150d97c10e3122af8068d1687"
        },
        "date": 1751670346956,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008583395449999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051193470899999925,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025815619300000006,
            "unit": "seconds"
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
          "id": "436b4935b52562f79a83b6ecadeac7dcbc1c2367",
          "message": "`polkadot-omni-node`: pass timestamp inherent data for block import (#9102)\n\n# Description\n\nThis should allow aura runtimes to check timestamp inherent data to\nsync/import blocks that include timestamp inherent data.\n\nCloses #8907 \n\n## Integration\n\nRuntime developers can check timestamp inherent data while using\n`polkadot-omni-node-lib`/`polkadot-omni-node`/`polkadot-parachain`\nbinaries. This change is backwards compatible and doesn't require\nruntimes to check the timestamp inherent, but they are able to do it now\nif needed.\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-06T09:32:11Z",
          "tree_id": "239ba865d190c48c06af7d1fa35ceb411cc31cea",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/436b4935b52562f79a83b6ecadeac7dcbc1c2367"
        },
        "date": 1751798589854,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00855703834999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002733640860000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005003881119999989,
            "unit": "seconds"
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
          "id": "cb12563ae4e532876c29b67be9a7f5d06fdc9fc3",
          "message": "Replace `assert_para_throughput` with `assert_finalized_para_throughput` (#9117)\n\nThere is no need to have two functions which are essentially doing the\nsame. It is also better to oberserve the finalized blocks, which also\nsimplifies the code. So, this pull request is replacing the\n`assert_para_throughput` with `assert_finalized_para_throughput`. It\nalso replaces any usage of `assert_finalized_para_throughput` with\n`assert_para_throughput`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-08T16:04:23Z",
          "tree_id": "faed545176a9de8b004b29e5ee7e4b5c2ccecef6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cb12563ae4e532876c29b67be9a7f5d06fdc9fc3"
        },
        "date": 1751995024154,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026695474100000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00859867911999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005122107889999993,
            "unit": "seconds"
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
          "id": "88fc41c9cf5e46277b7cab53a72c650b75377d25",
          "message": "make 0002-parachains-disputes a bit more robust (#9074)\n\nThere is inherently a race between the time we snapshot\nfinality_lag/disputes_finality_lag metrics and if the dispute/approvals\nfinished, so sometimes the test was failing because it was reporting 1\nwhich is in no way a problem, so let's make it a bit more robust by\nsimply waiting more time to reach 0.\n\nFixes: https://github.com/paritytech/polkadot-sdk/issues/8941.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2025-07-08T16:10:51Z",
          "tree_id": "8a90317b0febd3a60f76b56d7a854edcf7a4085d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88fc41c9cf5e46277b7cab53a72c650b75377d25"
        },
        "date": 1751997109460,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026244691599999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005114807139999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560092539999998,
            "unit": "seconds"
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
          "distinct": true,
          "id": "4d5e95217831fb75942d8153a22f6864858c1d71",
          "message": "XCM precompile: don't support older xcm versions (#9126)\n\nThe latest XCM version is 5. A lot of parachains are still running V3 or\nV4 which is why we haven't removed them, but the XCM precompile is new\nand should only have to deal with versions 5 and onwards. No need to\nkeep dragging 3 and 4 in contracts.",
          "timestamp": "2025-07-08T17:27:43Z",
          "tree_id": "2944a79e52968a0f54da0a246a07867b8f95dffe",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4d5e95217831fb75942d8153a22f6864858c1d71"
        },
        "date": 1752000039848,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005085985199999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263165981,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00852913286999999,
            "unit": "seconds"
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
          "id": "9b7c20a2a187e57433c055592609e35af0258bbc",
          "message": "Fix seal_call benchmark (#9112)\n\nFix seal_call benchmark, ensure that the benchmarked block actually\nsucceed\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-08T18:30:43Z",
          "tree_id": "a5d64f5c7d1bffccf857ee5ff83a6f6b305f5ee0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b7c20a2a187e57433c055592609e35af0258bbc"
        },
        "date": 1752004430350,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026429404299999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008568671429999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005159262569999995,
            "unit": "seconds"
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
          "distinct": true,
          "id": "ba2a8dc536db30397c332a2aa2cd9f9863027093",
          "message": "XCM precompile: small cleanup (#9135)\n\nFollow-up to\nhttps://github.com/paritytech/polkadot-sdk/pull/9125#discussion_r2192896809",
          "timestamp": "2025-07-08T19:47:45Z",
          "tree_id": "e7aeb64bf7cbd7d415bc142f30193c7d6ec3f579",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba2a8dc536db30397c332a2aa2cd9f9863027093"
        },
        "date": 1752008673216,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00520881492999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026551542999999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008714952019999993,
            "unit": "seconds"
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
          "id": "cc972542e0df0266cde2ead4cfac3b1558c860af",
          "message": "pallet bounties v2 benchmark (#8952)\n\ncloses #8649\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-08T21:47:29Z",
          "tree_id": "92ea303bb8df02e5752f9903f5541e35918ac3a9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cc972542e0df0266cde2ead4cfac3b1558c860af"
        },
        "date": 1752015675272,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026522110800000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008721413299999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005168960659999988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Sajjon@users.noreply.github.com",
            "name": "Alexander Cyon",
            "username": "Sajjon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ab0dcd62887ea3c5e50cfb5b1b01beb09d0ec92",
          "message": "Add `para_ids` Runtime API (#9055)\n\nImplementation of https://github.com/paritytech/polkadot-sdk/issues/9053\n\n---------\n\nCo-authored-by: alindima <alin@parity.io>",
          "timestamp": "2025-07-09T07:17:25Z",
          "tree_id": "efefbe78f8e545dae503496bbc822b03e32d1e13",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ab0dcd62887ea3c5e50cfb5b1b01beb09d0ec92"
        },
        "date": 1752049594274,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002608908810000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008476387969999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005002263799999994,
            "unit": "seconds"
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
          "id": "cd39c26a4da04693b07b3ed752ea239f452795ea",
          "message": "[Release|CI/CD] Move runtimes build to a separate pipeline and let it trigger the next flow (#9118)\n\nThis PR incudes the following changes:\n- Cut the runtimes build from the Create Draft flow into a standalone\npipeline\n- Add a trigger to the Build Runtimes pipeline that will be starting the\nCreate Draft flow automatically when the runtimes are built\nsuccessfully.\n\nCloses: https://github.com/paritytech/devops/issues/3827 and partially:\nhttps://github.com/paritytech/devops/issues/3828",
          "timestamp": "2025-07-09T08:40:25Z",
          "tree_id": "69aff4dc6192fec945b7a0b030222c92ac453a33",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cd39c26a4da04693b07b3ed752ea239f452795ea"
        },
        "date": 1752054592670,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00271226005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005194933789999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008831572839999986,
            "unit": "seconds"
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
          "id": "83afbeeb906131755fdcea3b891ea1883c4d17d0",
          "message": "Expose more constants for pallet-xcm (#9139)\n\nLet's expose more constants, similar as `AdvertisedXcmVersion`.\n\n\n![image](https://github.com/user-attachments/assets/5ddc265f-546b-45a0-8235-3f53c3108823)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-09T12:29:35Z",
          "tree_id": "6fb2c4c504887609989d96ab44ba1a1afbe03294",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83afbeeb906131755fdcea3b891ea1883c4d17d0"
        },
        "date": 1752068758017,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005127152969999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260139055,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00855473461999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7305f96aa8fc68b7249587c21f5fa2d4520c54cd",
          "message": "CI - zombienet cumulus tests zombienet sdk (#8954)\n\n### This PR includes the following changes:\n\n- Migrates Zombienet Cumulus tests to `zombienet-sdk`\n- Re-enables the tests, with the following exceptions (to be addressed\nseparately):\n  - `zombienet-cumulus-0002-pov_recovery` - #8985 \n- `zombienet-cumulus-0006-rpc_collator_builds_blocks` - root cause the\nsame as #8985\n  - `zombienet-cumulus-0009-elastic_scaling_pov_recovery` – #8999\n- `zombienet-cumulus-0010-elastic_scaling_multiple_block_per_slot` –\n#9018\n- Adds the following tests to CI:\n  - `zombienet-cumulus-0011-dht-bootnodes`\n  - `zombienet-cumulus-0012-parachain_extrinsic_gets_finalized`\n  - `zombienet-cumulus-0013-elastic_scaling_slot_based_rp_offset`\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Anthony Lazam <xlzm.tech@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-07-09T16:01:41Z",
          "tree_id": "7b46e0ac8c2ed95e791c472fb7a82ebbc6a32685",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7305f96aa8fc68b7249587c21f5fa2d4520c54cd"
        },
        "date": 1752081449064,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004915220189999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00252144691,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008389578499999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "409587adfb4cc5e28e28272e768361afdbea2191",
          "message": "Enable parachain-templates zombienet tests (#9131)\n\nThis PR includes the following changes:\n- Refactor Parachain Templates workflow to run tests individually\n- Enables Zombienet Parachain Templates tests in CI\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Anthony Lazam <xlzm.tech@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-07-10T06:33:27Z",
          "tree_id": "36c66069301310187811ad4f0537df4b18e2050f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/409587adfb4cc5e28e28272e768361afdbea2191"
        },
        "date": 1752133208346,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005032093999999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025824143899999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008484359889999996,
            "unit": "seconds"
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
          "distinct": false,
          "id": "12ddb5a71ddd744e48bbf49a4cc0b44c5381747e",
          "message": "bitfield_distribution: fix subsystem clogged at begining of a session (#9094)\n\n`handle_peer_view_change` gets called on NewGossipTopology with the\nexisting view of the peer to cover for the case when the topology might\narrive late, but in that case in the view will contain old blocks from\nprevious session, so since the X/Y neighbour change because of the\ntopology change you end up sending a lot of messages for blocks before\nthe session changed.\n\nFix it by checking the send message only for relay chains that are in\nthe same session as the current topology.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-10T10:00:44Z",
          "tree_id": "0adae7550a477fef6b79346b2a017a665b321042",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12ddb5a71ddd744e48bbf49a4cc0b44c5381747e"
        },
        "date": 1752145985591,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005142384619999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026690400299999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008612664559999986,
            "unit": "seconds"
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
          "id": "466149d0eac8e608a6e6b6db8cda98a555b6c7e8",
          "message": "Replace `log` with `tracing` on XCM-related modules (#8732)\n\nThis PR replaces `log` with `tracing` instrumentation on XCM-related\nmodules to significantly improve debugging capabilities for XCM\nexecution flows.\n\nContinues #8724 and partially addresses #6119 by providing structured\nlogging throughout XCM components, making it easier to diagnose\nexecution failures, fee calculation errors, and routing issues.\n\n## Key Features\n\n- **Consistent targets**: All components use predictable `xcm::*` log\ntargets\n- **Structured fields**: Uses `?variable` syntax for automatic Debug\nformatting\n- **Zero runtime impact**: No behavioural changes, only observability\nimprovements",
          "timestamp": "2025-07-10T12:54:12Z",
          "tree_id": "363cb00f3cfd55c0e8a1f74f8964ebc2e32b0156",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/466149d0eac8e608a6e6b6db8cda98a555b6c7e8"
        },
        "date": 1752156645834,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004946151739999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008473425829999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002641511270000001,
            "unit": "seconds"
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
          "id": "62a9808172832e13ca2ae02c1888491ee74b03fb",
          "message": "`fatxpool`: debug levels adjusted (#9159)\n\nThis PR removes redundant debug message and lowers the info about\ntimeout in `ready_at`.\n\nRelated: #9151\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-10T13:42:24Z",
          "tree_id": "cbedb9094437416e71f65e6fc550c42db2cc5e48",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/62a9808172832e13ca2ae02c1888491ee74b03fb"
        },
        "date": 1752159160895,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008731686799999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0052292882599999915,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00266668266,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "psykyodai@gmail.com",
            "name": "psykyo-dai(精神 大)",
            "username": "PsyKyodai"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "874a8dbdd9cbc7fdbfffc4c307f6f21974650a55",
          "message": "Add BlockNumberProvider to PureCreated Event (#9107)\n\n[AHM] [Proxy] Add creation block number to PureCreated event\n\nCloses #9066 \n\n## Problem\nAfter AHM, killing pure proxies requires the relay chain block height at\ncreation time. This information is non-trivial to obtain since the proxy\npallet lives on Asset Hub while the block height refers to Relay Chain.\n\n## Solution\nAdd `at: BlockNumberFor<T>` field to `Event::PureCreated` to include the\ncreation block height. This is populated using the `BlockNumberProvider`\nat creation time.\n\n## Changes\n1. Added `at` field to `Event::PureCreated` containing current block\nnumber\n2. Modified tests and benchmarks to reflect new event structure\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-10T15:19:15Z",
          "tree_id": "e16c795118f66c71b0a031259521c3beef122083",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/874a8dbdd9cbc7fdbfffc4c307f6f21974650a55"
        },
        "date": 1752165442081,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008586072849999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026598708800000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005140858249999995,
            "unit": "seconds"
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
          "id": "d4e4773ea531db55149191693f038e65d64f8107",
          "message": "use correct era planning config in westend-asset-hub (#9152)\n\ntiny mistake of the past, will use the automatic type rather than\nhard-coding it.",
          "timestamp": "2025-07-10T21:44:42Z",
          "tree_id": "325fb85d58fc53b2a8bd2826058c53e9398eb817",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d4e4773ea531db55149191693f038e65d64f8107"
        },
        "date": 1752188048792,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026505049499999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008641344849999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005308614759999992,
            "unit": "seconds"
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
          "id": "540941cc654ece30dcd5dfed3cbc93828cd25b81",
          "message": "Improve `pr_8860.prdoc` (#9171)\n\nImproved PR doc for https://github.com/paritytech/polkadot-sdk/pull/8860\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-11T10:53:15Z",
          "tree_id": "8b1fbfcc7a1599623446a446914cc1e37a981b75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/540941cc654ece30dcd5dfed3cbc93828cd25b81"
        },
        "date": 1752235480495,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005092631319999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008540824899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026675525299999993,
            "unit": "seconds"
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
          "id": "7058819a45ed5b74cedd6d21698f1c2ac2445d6b",
          "message": "add block hashes to the randomness used by hashmaps and friends in validation context (#9127)\n\nhttps://github.com/paritytech/polkadot-sdk/pull/8606\nhttps://github.com/paritytech/trie/pull/221 replaced the usage of\nBTreeMap with HashMaps in validation context. The keys are already\nderived with a cryptographic hash function from user data, so users\nshould not be able to manipulate it.\n\nTo be on safe side this PR also modifies the TrieCache, TrieRecorder and\nMemoryDB to use a hasher that on top of the default generated randomness\nalso adds randomness generated from the hash of the relaychain and that\nof the parachain blocks, which is not something users can control or\nguess ahead of time.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-11T15:34:56Z",
          "tree_id": "6b0e66c2eaa94537bb1ed602b345585455da88be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7058819a45ed5b74cedd6d21698f1c2ac2445d6b"
        },
        "date": 1752252959245,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00268550198,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560138529999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005147522609999995,
            "unit": "seconds"
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
          "id": "064512ed11042b34ca7330b93e39aa864219d475",
          "message": "pallet-bags-list: Emit `ScoreUpdated` event only if it has changed (#9166)\n\nquick follow-up to https://github.com/paritytech/polkadot-sdk/pull/8684,\nensuring all blocks don't have x events when the feature is enabled (as\nit is now in WAH)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-12T16:17:44Z",
          "tree_id": "bcdf93b2b053f979c59ad0094670fadf95855c33",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/064512ed11042b34ca7330b93e39aa864219d475"
        },
        "date": 1752341163429,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005091624569999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263388819,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008532146189999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jesse.chejieh@gmail.com",
            "name": "Doordashcon",
            "username": "Doordashcon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9339acc7e4eb58498fe7a4c412dfb9f8e75ae72a",
          "message": "Add Missing Events for Balances Pallet (#7250)\n\nAttempts to resolve #6974\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-13T00:04:30+02:00",
          "tree_id": "c5a5b6fa875bb790a7f98206b6d220ac1a957b32",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9339acc7e4eb58498fe7a4c412dfb9f8e75ae72a"
        },
        "date": 1752359944023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005217768139999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008592346529999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025697406199999993,
            "unit": "seconds"
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
          "id": "fb0d310e07438caafcc2dda4d502eba040ecf06c",
          "message": "emit sparse debug info in unoptimized builds (#8646)\n\nSee\n[here](https://kobzol.github.io/rust/rustc/2025/05/20/disable-debuginfo-to-improve-rust-compile-times.html)\nfor more details.\n\nI found that on my host, this reduces `cargo build` (after `cargo\nclean`) from 19m 35s to 17m 50s, or about 10%.\n\nThanks @pgherveou\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-13T22:45:18Z",
          "tree_id": "6fa4ad83ce7581d17e6bfc24fc886cf3fe8b40d7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fb0d310e07438caafcc2dda4d502eba040ecf06c"
        },
        "date": 1752450977544,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005105971109999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026844928300000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008627228169999984,
            "unit": "seconds"
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
          "id": "e98c88e297f58fa0a28b85bc8eee68fcf5cdaec3",
          "message": "feat(binary-merkle-tree): add `merkle_root_raw` and `merkle_proof_raw` methods (#9105)\n\n# Description\n\nResolves [#9103](https://github.com/paritytech/polkadot-sdk/issues/9103)\n\nAdded `merkle_root_raw` and `merkle_proof_raw` methods, which allow\ndevelopers to avoid double hashing when working with sequences like\n`Vec<H256>`, where `H256` is already hash of some message.\n\n## Integration\n\nThere were no breaking changes.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-14T06:42:30Z",
          "tree_id": "0c3604f400a15e405af3ecb3b31b480883e07235",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e98c88e297f58fa0a28b85bc8eee68fcf5cdaec3"
        },
        "date": 1752480237515,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008463795659999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026059141200000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004986761559999993,
            "unit": "seconds"
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
          "id": "f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40",
          "message": "Deduplicate client-side inherents checking logic (#9175)\n\nStumbled upon this while working on other issue\n(https://github.com/paritytech/polkadot-sdk/pull/7902). I thought I\nmight need to change the `CheckInherentsResult` and this deduplication\nwould have made everything easier. Probably changing\n`CheckInherentsResult` won't be needed in the end, but even so it would\nbe nice to reduce the duplication.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-14T08:22:53Z",
          "tree_id": "bfca803819835b7f3000ebe25955951078a64f09",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40"
        },
        "date": 1752486629323,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008531452779999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005127110599999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026487071699999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "evgeny@parity.io",
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8f4d80071a4f478a4540aa8ab63dc1a1b26a8187",
          "message": "Update forklift to 0.14.1 (#9163)\n\ncc https://github.com/paritytech/polkadot-sdk/issues/9123\n\ncc https://github.com/paritytech/devops/issues/4151\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: alvicsam <alvicsam@gmail.com>",
          "timestamp": "2025-07-14T10:59:34Z",
          "tree_id": "ed66147a2d1d0f7bcd93cfeaa94fba29aacdfe07",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8f4d80071a4f478a4540aa8ab63dc1a1b26a8187"
        },
        "date": 1752495770437,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026515603000000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00872908804999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005118706869999995,
            "unit": "seconds"
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
          "id": "641cca3841e7599380d66c14e12ebbe248c739e9",
          "message": "Bump the ci_dependencies group across 1 directory with 5 updates (#9017)\n\nBumps the ci_dependencies group with 5 updates in the / directory:\n\n| Package | From | To |\n| --- | --- | --- |\n| [Swatinem/rust-cache](https://github.com/swatinem/rust-cache) |\n`2.7.8` | `2.8.0` |\n|\n[actions-rust-lang/setup-rust-toolchain](https://github.com/actions-rust-lang/setup-rust-toolchain)\n| `1.12.0` | `1.13.0` |\n|\n[stefanzweifel/git-auto-commit-action](https://github.com/stefanzweifel/git-auto-commit-action)\n| `5` | `6` |\n|\n[docker/setup-buildx-action](https://github.com/docker/setup-buildx-action)\n| `3.10.0` | `3.11.1` |\n|\n[actions/attest-build-provenance](https://github.com/actions/attest-build-provenance)\n| `2.3.0` | `2.4.0` |\n\n\nUpdates `Swatinem/rust-cache` from 2.7.8 to 2.8.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/swatinem/rust-cache/releases\">Swatinem/rust-cache's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.8.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Add cache-workspace-crates feature by <a\nhref=\"https://github.com/jbransen\"><code>@​jbransen</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/246\">Swatinem/rust-cache#246</a></li>\n<li>Feat: support warpbuild cache provider by <a\nhref=\"https://github.com/stegaBOB\"><code>@​stegaBOB</code></a> in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/247\">Swatinem/rust-cache#247</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/jbransen\"><code>@​jbransen</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/246\">Swatinem/rust-cache#246</a></li>\n<li><a href=\"https://github.com/stegaBOB\"><code>@​stegaBOB</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/Swatinem/rust-cache/pull/247\">Swatinem/rust-cache#247</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/Swatinem/rust-cache/compare/v2.7.8...v2.8.0\">https://github.com/Swatinem/rust-cache/compare/v2.7.8...v2.8.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/Swatinem/rust-cache/blob/master/CHANGELOG.md\">Swatinem/rust-cache's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<h2>2.8.0</h2>\n<ul>\n<li>Add support for <code>warpbuild</code> cache provider</li>\n<li>Add new <code>cache-workspace-crates</code> feature</li>\n</ul>\n<h2>2.7.8</h2>\n<ul>\n<li>Include CPU arch in the cache key</li>\n</ul>\n<h2>2.7.7</h2>\n<ul>\n<li>Also cache <code>cargo install</code> metadata</li>\n</ul>\n<h2>2.7.6</h2>\n<ul>\n<li>Allow opting out of caching $CARGO_HOME/bin</li>\n<li>Add runner OS in cache key</li>\n<li>Adds an option to do lookup-only of the cache</li>\n</ul>\n<h2>2.7.5</h2>\n<ul>\n<li>Support Cargo.lock format cargo-lock v4</li>\n<li>Only run macOsWorkaround() on macOS</li>\n</ul>\n<h2>2.7.3</h2>\n<ul>\n<li>Work around upstream problem that causes cache saving to hang for\nminutes.</li>\n</ul>\n<h2>2.7.2</h2>\n<ul>\n<li>Only key by <code>Cargo.toml</code> and <code>Cargo.lock</code>\nfiles of workspace members.</li>\n</ul>\n<h2>2.7.1</h2>\n<ul>\n<li>Update toml parser to fix parsing errors.</li>\n</ul>\n<h2>2.7.0</h2>\n<ul>\n<li>Properly cache <code>trybuild</code> tests.</li>\n</ul>\n<h2>2.6.2</h2>\n<ul>\n<li>Fix <code>toml</code> parsing.</li>\n</ul>\n<h2>2.6.1</h2>\n<ul>\n<li>Fix hash contributions of\n<code>Cargo.lock</code>/<code>Cargo.toml</code> files.</li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/98c8021b550208e191a6a3145459bfc9fb29c4c0\"><code>98c8021</code></a>\n2.8.0</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/14d3bc39c43eec8ca2cd08dd0805a32ee0cb3666\"><code>14d3bc3</code></a>\nupdate Changelog</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/52ea1434f87f7081841d430fb7b1235754488e51\"><code>52ea143</code></a>\nsupport warpbuild cache provider (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/247\">#247</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/eaa85be6b1bfdc6616fd14d8916fc5aa0435e435\"><code>eaa85be</code></a>\nAdd cache-workspace-crates feature (<a\nhref=\"https://redirect.github.com/swatinem/rust-cache/issues/246\">#246</a>)</li>\n<li><a\nhref=\"https://github.com/Swatinem/rust-cache/commit/901019c0f83889e6f8eaa395f97093151c05c4b0\"><code>901019c</code></a>\nUpdate the test lockfiles</li>\n<li>See full diff in <a\nhref=\"https://github.com/swatinem/rust-cache/compare/9d47c6ad4b02e050fd481d890b2ea34778fd09d6...98c8021b550208e191a6a3145459bfc9fb29c4c0\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions-rust-lang/setup-rust-toolchain` from 1.12.0 to 1.13.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/releases\">actions-rust-lang/setup-rust-toolchain's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.13.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>feat: support cache-provider by <a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a> in <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/pull/65\">actions-rust-lang/setup-rust-toolchain#65</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/pull/65\">actions-rust-lang/setup-rust-toolchain#65</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.12.0...v1.13.0\">https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.12.0...v1.13.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/blob/main/CHANGELOG.md\">actions-rust-lang/setup-rust-toolchain's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<p>All notable changes to this project will be documented in this\nfile.</p>\n<p>The format is based on <a\nhref=\"https://keepachangelog.com/en/1.0.0/\">Keep a Changelog</a>,\nand this project adheres to <a\nhref=\"https://semver.org/spec/v2.0.0.html\">Semantic Versioning</a>.</p>\n<h2>[Unreleased]</h2>\n<h2>[1.13.0] - 2025-06-16</h2>\n<ul>\n<li>Add new parameter <code>cache-provider</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-provider</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/65\">#65</a>\nby <a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a>)</li>\n</ul>\n<h2>[1.12.0] - 2025-04-23</h2>\n<ul>\n<li>Add support for installing rustup on Windows (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/58\">#58</a>\nby <a href=\"https://github.com/maennchen\"><code>@​maennchen</code></a>)\nThis adds support for using Rust on the GitHub provided Windows ARM\nrunners.</li>\n</ul>\n<h2>[1.11.0] - 2025-02-24</h2>\n<ul>\n<li>Add new parameter <code>cache-bin</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-bin</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/51\">#51</a>\nby <a\nhref=\"https://github.com/enkhjile\"><code>@​enkhjile</code></a>)</li>\n<li>Add new parameter <code>cache-shared-key</code> that is propagated\nto <code>Swatinem/rust-cache</code> as <code>shared-key</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/52\">#52</a>\nby <a\nhref=\"https://github.com/skanehira\"><code>@​skanehira</code></a>)</li>\n</ul>\n<h2>[1.10.1] - 2024-10-01</h2>\n<ul>\n<li>Fix problem matcher for rustfmt output.\nThe format has changed since <a\nhref=\"https://redirect.github.com/rust-lang/rustfmt/pull/5971\">rust-lang/rustfmt#5971</a>\nand now follows the form &quot;filename:line&quot;.\nThanks to <a\nhref=\"https://github.com/0xcypher02\"><code>@​0xcypher02</code></a> for\npointing out the problem.</li>\n</ul>\n<h2>[1.10.0] - 2024-09-23</h2>\n<ul>\n<li>Add new parameter <code>cache-directories</code> that is propagated\nto <code>Swatinem/rust-cache</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/44\">#44</a>\nby <a\nhref=\"https://github.com/pranc1ngpegasus\"><code>@​pranc1ngpegasus</code></a>)</li>\n<li>Add new parameter <code>cache-key</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>key</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/41\">#41</a>\nby <a\nhref=\"https://github.com/iainlane\"><code>@​iainlane</code></a>)</li>\n<li>Make rustup toolchain installation more robust in light of planned\nchanges <a\nhref=\"https://redirect.github.com/rust-lang/rustup/issues/3635\">rust-lang/rustup#3635</a>\nand <a\nhref=\"https://redirect.github.com/rust-lang/rustup/pull/3985\">rust-lang/rustup#3985</a></li>\n<li>Allow installing multiple Rust toolchains by specifying multiple\nversions in the <code>toolchain</code> input parameter.</li>\n<li>Configure the <code>rustup override</code> behavior via the new\n<code>override</code> input. (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/38\">#38</a>)</li>\n</ul>\n<h2>[1.9.0] - 2024-06-08</h2>\n<ul>\n<li>Add extra argument <code>cache-on-failure</code> and forward it to\n<code>Swatinem/rust-cache</code>. (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/39\">#39</a>\nby <a\nhref=\"https://github.com/samuelhnrq\"><code>@​samuelhnrq</code></a>)<br\n/>\nSet the default the value to true.\nThis will result in more caching than previously.\nThis helps when large dependencies are compiled only for testing to\nfail.</li>\n</ul>\n<h2>[1.8.0] - 2024-01-13</h2>\n<ul>\n<li>Allow specifying subdirectories for cache.</li>\n<li>Fix toolchain file overriding.</li>\n</ul>\n<h2>[1.7.0] - 2024-01-11</h2>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/fb51252c7ba57d633bc668f941da052e410add48\"><code>fb51252</code></a>\nUpdate CHANGELOG.md</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/33b85c358d935f8a72fcfe469bdb7d9f78182141\"><code>33b85c3</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/65\">#65</a>\nfrom mindrunner/main</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/82947d77a9ec18480f3f187b0102cac015771477\"><code>82947d7</code></a>\nfeat: support cache-provider</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/9d7e65c320fdb52dcd45ffaa68deb6c02c8754d9...fb51252c7ba57d633bc668f941da052e410add48\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `stefanzweifel/git-auto-commit-action` from 5 to 6\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/releases\">stefanzweifel/git-auto-commit-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v6.0.0</h2>\n<h2>Added</h2>\n<ul>\n<li>Throw error early if repository is in a detached state (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/357\">#357</a>)</li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>Fix PAT instructions with Dependabot (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/376\">#376</a>)\n<a\nhref=\"https://github.com/@Dreamsorcerer\"><code>@​Dreamsorcerer</code></a></li>\n</ul>\n<h2>Removed</h2>\n<ul>\n<li>Remove support for <code>create_branch</code>,\n<code>skip_checkout</code>, <code>skip_Fetch</code> (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/314\">#314</a>)</li>\n</ul>\n<h2>v5.2.0</h2>\n<h2>Added</h2>\n<ul>\n<li>Add <code>create_git_tag_only</code> option to skip commiting and\nalways create a git-tag. (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/364\">#364</a>)\n<a href=\"https://github.com/@zMynxx\"><code>@​zMynxx</code></a></li>\n<li>Add Test for <code>create_git_tag_only</code> feature (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/367\">#367</a>)\n<a\nhref=\"https://github.com/@stefanzweifel\"><code>@​stefanzweifel</code></a></li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>docs: Update README.md per <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/354\">#354</a>\n(<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/361\">#361</a>)\n<a href=\"https://github.com/@rasa\"><code>@​rasa</code></a></li>\n</ul>\n<h2>v5.1.0</h2>\n<h2>Changed</h2>\n<ul>\n<li>Include <code>github.actor_id</code> in default\n<code>commit_author</code> (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/354\">#354</a>)\n<a\nhref=\"https://github.com/@parkerbxyz\"><code>@​parkerbxyz</code></a></li>\n</ul>\n<h2>Fixed</h2>\n<ul>\n<li>docs(README): fix broken protected branch docs link (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/346\">#346</a>)\n<a href=\"https://github.com/@scarf005\"><code>@​scarf005</code></a></li>\n<li>Update README.md (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/343\">#343</a>)\n<a href=\"https://github.com/@Kludex\"><code>@​Kludex</code></a></li>\n</ul>\n<h2>Dependency Updates</h2>\n<ul>\n<li>Bump bats from 1.11.0 to 1.11.1 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/353\">#353</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 6 to 7 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/342\">#342</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 5 to 6 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/335\">#335</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2>v5.0.1</h2>\n<h2>Fixed</h2>\n<ul>\n<li>Fail if attempting to execute git commands in a directory that is\nnot a git-repo. (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/326\">#326</a>)\n<a\nhref=\"https://github.com/@ccomendant\"><code>@​ccomendant</code></a></li>\n</ul>\n<h2>Dependency Updates</h2>\n<ul>\n<li>Bump bats from 1.10.0 to 1.11.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/325\">#325</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump release-drafter/release-drafter from 5 to 6 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/319\">#319</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2>Misc</h2>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/blob/master/CHANGELOG.md\">stefanzweifel/git-auto-commit-action's\nchangelog</a>.</em></p>\n<blockquote>\n<h2><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v4.16.0...v5.0.0\">v5.0.0</a>\n- 2023-10-06</h2>\n<p>New major release that bumps the default runtime to Node 20. There\nare no other breaking changes.</p>\n<h3>Changed</h3>\n<ul>\n<li>Update node version to node20 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/300\">#300</a>)\n<a\nhref=\"https://github.com/@ryudaitakai\"><code>@​ryudaitakai</code></a></li>\n<li>Add _log and _set_github_output functions (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/273\">#273</a>)\n<a\nhref=\"https://github.com/@stefanzweifel\"><code>@​stefanzweifel</code></a></li>\n</ul>\n<h3>Fixed</h3>\n<ul>\n<li>Seems like there is an extra space (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/288\">#288</a>)\n<a\nhref=\"https://github.com/@pedroamador\"><code>@​pedroamador</code></a></li>\n<li>Fix git-auto-commit.yml (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/277\">#277</a>)\n<a\nhref=\"https://github.com/@zcong1993\"><code>@​zcong1993</code></a></li>\n</ul>\n<h3>Dependency Updates</h3>\n<ul>\n<li>Bump actions/checkout from 3 to 4 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/302\">#302</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump bats from 1.9.0 to 1.10.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/293\">#293</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump github/super-linter from 4 to 5 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/289\">#289</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n<li>Bump bats from 1.8.2 to 1.9.0 (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/282\">#282</a>)\n<a\nhref=\"https://github.com/@dependabot\"><code>@​dependabot</code></a></li>\n</ul>\n<h2><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v4.15.4...v4.16.0\">v4.16.0</a>\n- 2022-12-02</h2>\n<h3>Changed</h3>\n<ul>\n<li>Don't commit files when only LF/CRLF changes (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/265\">#265</a>)\n<a href=\"https://github.com/@ZeroRin\"><code>@​ZeroRin</code></a></li>\n<li>Update default email address of github-actions[bot] (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/264\">#264</a>)\n<a href=\"https://github.com/@Teko012\"><code>@​Teko012</code></a></li>\n</ul>\n<h3>Fixed</h3>\n<ul>\n<li>Fix link and text for workflow limitation (<a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/pull/263\">#263</a>)\n<a href=\"https://github.com/@Teko012\"><code>@​Teko012</code></a></li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/778341af668090896ca464160c2def5d1d1a3eb0\"><code>778341a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/379\">#379</a>\nfrom stefanzweifel/disable-detached-state-check</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/33b203d92a47ab2370a88ce03d9825cdb52cc98c\"><code>33b203d</code></a>\nDisable Check if Repo is in Detached State</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/a82d80a75f85e7feb8d2777704c545af1c7affd9\"><code>a82d80a</code></a>\nUpdate CHANGELOG</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/3cc016cfc892e0844046da36fc68da4e525e081f\"><code>3cc016c</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/375\">#375</a>\nfrom stefanzweifel/v6-next</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/ddb7ae415961225797e0234a7018a30ba1e66bb3\"><code>ddb7ae4</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/376\">#376</a>\nfrom Dreamsorcerer/patch-1</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/b001e5f0ff05d7297c0101f4b44e861799e417dd\"><code>b001e5f</code></a>\nApply suggestions from code review</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/6494dc61d3e663a9f5166a099d9736ceefc5a3aa\"><code>6494dc6</code></a>\nFix PAT instructions with Dependabot</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/76180511d9f2354bb712ec6338ce79d4f2061bfe\"><code>7618051</code></a>\nAdd deprecated inputs to fix unbound variable issue</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/ae114628ea78fd141aa4fa7730f70c984b29c391\"><code>ae11462</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/stefanzweifel/git-auto-commit-action/issues/371\">#371</a>\nfrom stefanzweifel/dependabot/npm_and_yarn/bats-1.12.0</li>\n<li><a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/commit/3058f91afb4f03b73d38f33c35023fb22cf546b8\"><code>3058f91</code></a>\nBump bats from 1.11.1 to 1.12.0</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/stefanzweifel/git-auto-commit-action/compare/v5...v6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `docker/setup-buildx-action` from 3.10.0 to 3.11.1\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/docker/setup-buildx-action/releases\">docker/setup-buildx-action's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v3.11.1</h2>\n<ul>\n<li>Fix <code>keep-state</code> not being respected by <a\nhref=\"https://github.com/crazy-max\"><code>@​crazy-max</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/429\">docker/setup-buildx-action#429</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.11.0...v3.11.1\">https://github.com/docker/setup-buildx-action/compare/v3.11.0...v3.11.1</a></p>\n<h2>v3.11.0</h2>\n<ul>\n<li>Keep BuildKit state support by <a\nhref=\"https://github.com/crazy-max\"><code>@​crazy-max</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/427\">docker/setup-buildx-action#427</a></li>\n<li>Remove aliases created when installing by default by <a\nhref=\"https://github.com/hashhar\"><code>@​hashhar</code></a> in <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/139\">docker/setup-buildx-action#139</a></li>\n<li>Bump <code>@​docker/actions-toolkit</code> from 0.56.0 to 0.62.1 in\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/422\">docker/setup-buildx-action#422</a>\n<a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/pull/425\">docker/setup-buildx-action#425</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/v3.10.0...v3.11.0\">https://github.com/docker/setup-buildx-action/compare/v3.10.0...v3.11.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/e468171a9de216ec08956ac3ada2f0791b6bd435\"><code>e468171</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/429\">#429</a>\nfrom crazy-max/fix-keep-state</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/a3e7502fd02828f4a26a8294ad2621a6c2204952\"><code>a3e7502</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/b145473295476dbef957d01d109fe7810b511c95\"><code>b145473</code></a>\nfix keep-state not being respected</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/18ce135bb5112fa8ce4ed6c17ab05699d7f3a5e0\"><code>18ce135</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/425\">#425</a>\nfrom docker/dependabot/npm_and_yarn/docker/actions-to...</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/0e198e93af3b40a76583e851660b876e62b3a155\"><code>0e198e9</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/05f3f3ac108784e8fb56815c12fbfcf2d0ed660f\"><code>05f3f3a</code></a>\nbuild(deps): bump <code>@​docker/actions-toolkit</code> from 0.61.0 to\n0.62.1</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/622913496df23a5293cfb3418e5836ee4dd28f3a\"><code>6229134</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/docker/setup-buildx-action/issues/427\">#427</a>\nfrom crazy-max/keep-state</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/c6f6a0702519e6c47b71b117b24c0c1c130fdf32\"><code>c6f6a07</code></a>\nchore: update generated content</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/6c5e29d8485c56f3f8d1cb2197b657959dd6e032\"><code>6c5e29d</code></a>\nskip builder creation if one already exists with the same name</li>\n<li><a\nhref=\"https://github.com/docker/setup-buildx-action/commit/548b2977492e10f459d0f0df8bee7ce3c5937792\"><code>548b297</code></a>\nci: keep-state check</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/docker/setup-buildx-action/compare/b5ca514318bd6ebac0fb2aedd5d36ec1b5c232a2...e468171a9de216ec08956ac3ada2f0791b6bd435\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions/attest-build-provenance` from 2.3.0 to 2.4.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions/attest-build-provenance/releases\">actions/attest-build-provenance's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v2.4.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump undici from 5.28.5 to 5.29.0 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a> in <a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/633\">actions/attest-build-provenance#633</a></li>\n<li>Bump actions/attest from 2.3.0 to <a\nhref=\"https://github.com/actions/attest/releases/tag/v2.4.0\">2.4.0</a>\nby <a href=\"https://github.com/bdehamer\"><code>@​bdehamer</code></a> in\n<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/pull/654\">actions/attest-build-provenance#654</a>\n<ul>\n<li>Includes support for the new well-known summary file which will\naccumulate paths to all attestations generated in a given workflow\nrun</li>\n</ul>\n</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/v2.3.0...v2.4.0\">https://github.com/actions/attest-build-provenance/compare/v2.3.0...v2.4.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/e8998f949152b193b063cb0ec769d69d929409be\"><code>e8998f9</code></a>\nbump actions/attest from 2.3.0 to 2.4.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/654\">#654</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/11c67f22cd5a3968528de1f8de4bb4487ee5306e\"><code>11c67f2</code></a>\nBump the npm-development group across 1 directory with 6 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/649\">#649</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/39cb715ce0ddd23df1f705e863f642bfb72dfb2b\"><code>39cb715</code></a>\nBump the npm-development group across 1 directory with 7 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/641\">#641</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/7d91c4030d8fdc376f87f022d8ca01fe8bf07fcd\"><code>7d91c40</code></a>\nBump undici from 5.28.5 to 5.29.0 (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/633\">#633</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/d848170917c12653fb344e617a79614f36d13e00\"><code>d848170</code></a>\nBump super-linter/super-linter in the actions-minor group (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/640\">#640</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/0ca36ea29fc5b46379679e3d2a9ce33a62c57e04\"><code>0ca36ea</code></a>\nBump the npm-development group with 7 updates (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/582\">#582</a>)</li>\n<li><a\nhref=\"https://github.com/actions/attest-build-provenance/commit/d82e7cd0c70d3e7b2615badc4d8824ca0b098d86\"><code>d82e7cd</code></a>\noffboard from eslint in superlinter (<a\nhref=\"https://redirect.github.com/actions/attest-build-provenance/issues/618\">#618</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions/attest-build-provenance/compare/db473fddc028af60658334401dc6fa3ffd8669fd...e8998f949152b193b063cb0ec769d69d929409be\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-07-14T14:37:10Z",
          "tree_id": "b1fc202864e0d0bdd4231a00f446f544f2eb6992",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/641cca3841e7599380d66c14e12ebbe248c739e9"
        },
        "date": 1752508312883,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260669658,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008522426739999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005078379879999991,
            "unit": "seconds"
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
          "distinct": true,
          "id": "efa765b6d9fbab59dd9bab944f99b40a157d0d64",
          "message": "Pallet XCM - `transfer_assets` pre-ahm patch (#9137)\n\nAddresses https://github.com/paritytech/polkadot-sdk/issues/9054\n\n`transfer_assets` automatically figures out the reserve for a\ncross-chain transfer\nbased on on-chain configurations like `IsReserve` and the asset ids.\nThe Asset Hub Migration (AHM) will make it unable to return the correct\nreserve for\nthe network native asset (DOT, KSM, WND, PAS) since its reserve will\nmove from the\nRelay Chain to the Asset Hub.\n\nBefore the migration, it'll be disabled to do network native reserve\ntransfers\nvia `transfer_assets`. After the migration, once everything is\nconfigured properly,\nit'll be patched to use the correct reserve.\n\n## TODO\n\n- [x] Patch\n- [x] Tests\n- [x] PRDoc",
          "timestamp": "2025-07-14T19:28:37Z",
          "tree_id": "13bb2dff7ac3a2f86d6b38b9817c02e34410e467",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efa765b6d9fbab59dd9bab944f99b40a157d0d64"
        },
        "date": 1752525554130,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008708443639999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265841113,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005321601519999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ismailov.m.h@gmail.com",
            "name": "muharem",
            "username": "muharem"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ec6e6843b847de92be649073317fa729898d0e1b",
          "message": "Asset Hub Westend: nfts block provider is RC (#9141)\n\nAsset Hub Westend: nfts block provider is Relay Chain.\n\nnfts pallet uses the blocks to define `mint.start_block` and\n`mint.end_block` for collections. therefor the RC is a better choice\nhere since its more time accurate.\n\nthis does not requires a migration since there is no single collection\nwith the start and end block set.\n\nit would be nice to deploy this change asap to let clients test this\nbefore it hit production on Kusama and Polkadot.",
          "timestamp": "2025-07-14T20:25:23Z",
          "tree_id": "06fb447dd706c0508dbf8c601711dc3d56a98d56",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ec6e6843b847de92be649073317fa729898d0e1b"
        },
        "date": 1752528743675,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005144160799999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264743916,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008559953459999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32168055+antkve@users.noreply.github.com",
            "name": "Anthony Kveder",
            "username": "antkve"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "999b4fa90ea0073a193662b1162b5f1f25f3beb6",
          "message": "Fixed westend asset hub ID (#9191)\n\nAddresses #9190 by adding cumulus_primitives_core::GetParachainInfo impl\nto the AHW runtime.\n\n---------\n\nCo-authored-by: Karol Kokoszka <karol@parity.io>",
          "timestamp": "2025-07-15T10:47:05Z",
          "tree_id": "db38b696266f561a16f37e601e33a516a9086616",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/999b4fa90ea0073a193662b1162b5f1f25f3beb6"
        },
        "date": 1752580513301,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005073641599999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560284299999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026340730199999997,
            "unit": "seconds"
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
          "id": "607a1b24b7902a657426ce2412e316a57b61894b",
          "message": "`apply_authorized_force_set_current_code` does not need to consume the whole block (#9202)\n\nThere is no need that this dispatchable consumes the full block as this\nis just writing the given value to storage. On a chain this is done,\nbecause the runtime changes and thus, a lot of stuff potentially\nchanges. In the case of the relay chain only on parachain changes and\nnot the relay chain runtime itself.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-15T11:47:13Z",
          "tree_id": "275460b2842ffe07aa0ea2d00e95f080163d9b74",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/607a1b24b7902a657426ce2412e316a57b61894b"
        },
        "date": 1752584733291,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026354900100000007,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008632280229999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005237780869999995,
            "unit": "seconds"
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
          "id": "495d5a24c8078a0da1eb5e0fe8742a09f1f1bd5c",
          "message": "fix(minimal): pre-seal a first block to trigger maintain (#9207)\n\n# Description\n\nAfter making fork aware txpool the default, instant seal of minimal node\nstopped working as expected because any transaction sent to it got stuck\nin mempool , since there are no active views to include the tx in.\n\nTo overcome this we can create a first view by pre-sealing a first empty\nblock, which triggers the `maintain` phase and view building logic. This\nis compatible with single-state tx pool too.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-16T06:51:33Z",
          "tree_id": "7fac401304e67f49b7999d7b132e8d00bd241316",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/495d5a24c8078a0da1eb5e0fe8742a09f1f1bd5c"
        },
        "date": 1752652753636,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260627661,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008597162699999978,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005186324109999997,
            "unit": "seconds"
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
          "id": "40e1a2a7c99c67fe5201145e473c87e1aea4bf05",
          "message": "Allow create backport branches to unstable by A4-backport-unstable* tag (#9167)\n\nIn this\n[PR](https://github.com/paritytech/polkadot-sdk/pull/9139#issuecomment-3052828167),\nI added the `A4-backport-unstable2507` label, but no backport branch was\ncreated for `unstable2507`.\n\nWas this intentional or just an oversight or did I miss anything in the\nrelease channel?\nHow do we do backports to unstable2507? If manually, just close this.\n\ncc: @EgorPopelyaev - this PR is just a blind draft (not sure if it\nworks), probably more needs to be fixed and properly tested. If we\nreally need this, could you please take it over the finish line? If not,\njust close it :)\n\nCo-authored-by: Egor_P <egor@parity.io>",
          "timestamp": "2025-07-16T16:23:51Z",
          "tree_id": "42f9ddd2a1ed680cc694879a7f84761b03ea1e9c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/40e1a2a7c99c67fe5201145e473c87e1aea4bf05"
        },
        "date": 1752687144561,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008535617119999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00259539007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005169772949999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6ecd83761b4fcac8c2c02ee05e8ec7213bacbc30",
          "message": "feat(pallet-xcm): Add supported_version to pallet-xcm genesis config (#9225)\n\nRelates to: https://github.com/polkadot-fellows/runtimes/issues/544\nCloses https://github.com/paritytech/polkadot-sdk/issues/9075\n\nAdds a `supported_version` field to the pallet-xcm genesis config. Which\nallows specifying versioned locations at genesis.",
          "timestamp": "2025-07-17T08:34:46Z",
          "tree_id": "4c37cf3bac9a7da5b7949782b000a08141fc207b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ecd83761b4fcac8c2c02ee05e8ec7213bacbc30"
        },
        "date": 1752745545781,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027113571899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008648413839999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005089740139999995,
            "unit": "seconds"
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
          "id": "86d2a410ca95643e589f270808a6fa57de41369f",
          "message": "Don't use labels for branch names creation in the backport bot (#9243)",
          "timestamp": "2025-07-17T10:25:22Z",
          "tree_id": "438b192e6b4d0c685f1c5920b5081911a4e5283b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/86d2a410ca95643e589f270808a6fa57de41369f"
        },
        "date": 1752753205743,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008647039529999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002618373079999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005179710219999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6b17df5ae96f7970109ec3934c7d288f05baa23b",
          "message": "Remove unused deps (#9235)\n\n# Description\n\nRemove unused deps using `cargo udeps`\n\nPart of: #6906\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-07-17T14:18:53Z",
          "tree_id": "20c94cc5015d6ff1c010a46fd69c90c70442033b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6b17df5ae96f7970109ec3934c7d288f05baa23b"
        },
        "date": 1752767045610,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008528037719999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005030579369999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264220842,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "32168055+antkve@users.noreply.github.com",
            "name": "Anthony Kveder",
            "username": "antkve"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0ae5c5bbd96a600aed81358339be2f16bade4a81",
          "message": "Fixed genesis config presets for bridge tests (#9185)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/9116\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Karol Kokoszka <karol@parity.io>",
          "timestamp": "2025-07-17T16:04:44Z",
          "tree_id": "19d750bba6685e132f90c471118eb1342e943c9f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0ae5c5bbd96a600aed81358339be2f16bade4a81"
        },
        "date": 1752775967882,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026003321699999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008560757039999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005194371439999989,
            "unit": "seconds"
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
          "id": "b9fd81b1d511c1b82d44129ae6e3592620508d85",
          "message": "Remove `subwasmlib` (#9252)\n\nThis removes `subwasmlib` and replaces it with some custom code to fetch\nthe metadata. Main point of this change is the removal of some external\ndependency.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/9203\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-18T11:26:43Z",
          "tree_id": "8158e2688a0180b1512385a845c592f5319d7f2d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b9fd81b1d511c1b82d44129ae6e3592620508d85"
        },
        "date": 1752842282023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526041989999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005162670399999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263012882,
            "unit": "seconds"
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
          "id": "8b21416986049b26bf99e61bf1c43b7347ed564f",
          "message": "zombienet, make logs for para works (#9230)\n\nFix for correctly display the logs (urls) for paras.",
          "timestamp": "2025-07-18T15:25:49Z",
          "tree_id": "2bf2a30b19851859a8c1db0ac7e145031cf773ed",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8b21416986049b26bf99e61bf1c43b7347ed564f"
        },
        "date": 1752856888443,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005145037809999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526873949999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026067719700000004,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e007db09171dd5248f5d8663a56be679b92fdbe7",
          "message": "feat(cumulus): Adds support for additional relay state keys in parachain validation data inherent (#9262)\n\nAdds the possibility for parachain clients to collect additional relay\nstate keys into the validation data inherent.\n\nWith this change, other consensus engines can collect additional relay\nkeys into the parachain inherent data:\n```rs\nlet paras_inherent_data = ParachainInherentDataProvider::create_at(\n  relay_parent,\n  relay_client,\n  validation_data,\n  para_id,\n  vec![\n     relay_well_known_keys::EPOCH_INDEX.to_vec() // <----- Example\n  ],\n)\n.await;\n```",
          "timestamp": "2025-07-18T21:26:30Z",
          "tree_id": "12ecd4a047e3074ed0ff7953b85e24443d9a7332",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e007db09171dd5248f5d8663a56be679b92fdbe7"
        },
        "date": 1752878226694,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261525499,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00841458589999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004945185839999989,
            "unit": "seconds"
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
          "id": "0d2106685025a0dad542b0980aab763ce0352755",
          "message": "Allow locking to bump consumer without limits (#9176)\n\nLocking is a system-level operation, and can only increment the consumer\nlimit at most once. Therefore, it should use\n`inc_consumer_without_limits`. This behavior is optional, and is only\nused in the call path of `LockableCurrency`. Reserves, Holds and Freezes\n(and other operations like transfer etc.) have the ability to return\n`DispatchResult` and don't need this bypass. This is demonstrated in the\nunit tests added.\n\nBeyond this, this PR: \n\n* uses the correct way to get the account data in tests\n* adds an `Unexpected` event instead of a silent `debug_assert!`. \n* Adds `try_state` checks for correctness of `account.frozen` invariant.\n\n---------\n\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-20T08:51:04Z",
          "tree_id": "f4835fef77bc77f12a7b25e5789a72edb66a8110",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d2106685025a0dad542b0980aab763ce0352755"
        },
        "date": 1753005685826,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005183158079999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00856138804999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026465043899999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b17f06bf06dbee585bbd8dc6d070c5edf56916e1",
          "message": "babe: keep stateless verification in `Verifier`, move everything else to the import queue (#9147)\n\nWe agreed to split https://github.com/paritytech/polkadot-sdk/pull/8446\ninto two PRs: one for BABE (this one) and one for AURA. This is the\neasier one.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-20T16:43:16Z",
          "tree_id": "c968ceb147b12e27e9ff5063f8c4303d14b3aeb9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b17f06bf06dbee585bbd8dc6d070c5edf56916e1"
        },
        "date": 1753034282957,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008536643269999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026549024399999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051494235899999935,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "443c2ffa03ee20e8244fa4b52ec3c62750d55ca6",
          "message": "Rewrite validator disabling test with zombienet-sdk (#9128)\n\nFixes #9085\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-21T09:08:32Z",
          "tree_id": "60764da01881d68bd585187a0cf3d60596bfbc12",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/443c2ffa03ee20e8244fa4b52ec3c62750d55ca6"
        },
        "date": 1753093279804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026315498100000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051757640699999965,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008546156399999985,
            "unit": "seconds"
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
          "id": "c7f9908c2eeb1be70e57819537058beb53664446",
          "message": "gossip-support: make low connectivity message an error (#9264)\n\nAll is not well when a validator is not properly connected, e.g: of\nthings that might happen:\n- Finality might be slightly delay because validator will be no-show\nbecause they can't retrieve PoVs to validate approval work:\nhttps://github.com/paritytech/polkadot-sdk/issues/8915.\n- When they author blocks they won't back things because gossiping of\nbacking statements happen using the grid topology:, e.g blocks authored\nby validators with a low number of peers:\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc-polkadot.helixstreet.io#/explorer/query/26931262\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Frpc-polkadot.helixstreet.io#/explorer/query/26931260\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot.api.onfinality.io%2Fpublic-ws#/explorer/query/26931334\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931314\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931292\n\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fpolkadot-public-rpc.blockops.network%2Fws#/explorer/query/26931447\n\n\nThe problem is seen in `polkadot_parachain_peer_count` metrics, but it\nseems people are not monitoring that well enough, so let's make it more\nvisible nodes with low connectivity are not working in good conditions.\n\nI also reduced the threshold to 85%, so that we don't trigger this error\nto eagerly.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-21T09:57:39Z",
          "tree_id": "472c2b031140ce823b7947201ff39347eaf6dbee",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c7f9908c2eeb1be70e57819537058beb53664446"
        },
        "date": 1753096166257,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008698310099999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005238696959999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026194913499999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "161e7f4d8b9b6908694d0ccc9bd0ef4a1674e860",
          "message": "Rewrite old disputes test with zombienet-sdk (#9257)\n\nFixes: #9256\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-21T13:46:01Z",
          "tree_id": "24c826bb9c1c557cd72bf46b1b000eea42cc3c0f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/161e7f4d8b9b6908694d0ccc9bd0ef4a1674e860"
        },
        "date": 1753109706142,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00853376491999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026080278799999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005051998489999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b4b019e4db0ef47b0952638388eba4958e1c4004",
          "message": "Zombienet CI improvements (#9172)\n\n## 🔄 Zombienet CI Refactor: Matrix-Based Workflows\n\nThis PR refactors the Zombienet CI workflows to use a **matrix-based\napproach**, resulting in:\n\n- ✅ **Easier test maintenance** – easily add or remove tests without\nduplicating workflow logic.\n- 🩹 **Improved flaky test handling** – flaky tests are excluded by\ndefault but can be explicitly included by pattern.\n- 🔍 **Pattern-based test selection** – run only tests matching a name\npattern, ideal for debugging.\n\n---\n\n## 🗂️ Structure Changes\n\n- **Test definitions** are now stored in `.github/zombienet-tests/`.\n- Each workflow (`Cumulus`, `Substrate`, `Polkadot`, `Parachain\nTemplate`) has its own YAML file with test configurations.\n\n---\n\n## 🧰 Added Scripts\n\n### `.github/scripts/parse-zombienet-tests.py`\n- Parses test definitions and generates a GitHub Actions matrix.\n- Filters out flaky tests by default.\n- If a `test_pattern` is provided, matching tests are **included even if\nflaky**.\n\n### `.github/scripts/dispatch-zombienet-workflow.sh`\n- Triggers a Zombienet workflow multiple times, optionally filtered by\ntest name pattern.\n- Stores results in a **CSV file** for analysis.\n- Useful for debugging flaky tests or stress-testing specific workflows.\n- Intended to be run from the local machine.\n\n---------\n\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: Javier Viola <javier@parity.io>",
          "timestamp": "2025-07-21T16:28:18Z",
          "tree_id": "b6ee4c0f3e3cb8b9bd8a8cadc045014f1ac0fd77",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b4b019e4db0ef47b0952638388eba4958e1c4004"
        },
        "date": 1753119635777,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005184043969999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026784164,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008702161579999989,
            "unit": "seconds"
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
          "id": "2f8d2a2b875a3f08aae5dd82c09ffe65932d2e62",
          "message": "consensus/grandpa: Fix high number of peer disconnects with invalid justification (#9015)\n\nA grandpa race-casse has been identified in the versi-net stack around\nauthority set changes, which leads to the following:\n\n- T0 / Node A: Completes round (15)\n- T1 / Node A: Applies new authority set change and increments the SetID\n(from 0 to 1)\n- T2 / Node B: Sends Precommit for round (15) with SetID (0) -- previous\nset ID\n- T3 / Node B: Applies new authority set change and increments the SetID\n(1)\n\nIn this scenario, Node B is not aware at the moment of sending\njustifications that the Set ID has changed.\nThe downstream effect is that Node A will not be able to verify the\nsignature of justifications, since a different SetID is taken into\naccount. This will cascade through the sync engine, where the Node B is\nwrongfully banned and disconnected.\n\nThis PR aims to fix the edge-case by making the grandpa resilient to\nverifying prior setIDs for signatures.\nWhen the signature of the grandpa justification fails to decode, the\nprior SetID is also verified. If the prior SetID produces a valid\nsignature, then the outdated justification error is propagated through\nthe code (ie `SignatureResult::OutdatedSet`).\n\nThe sync engine will handle the outdated justifications as invalid, but\nwithout banning the peer. This leads to increased stability of the\nnetwork during authority changes, which caused frequent disconnects to\nversi-net in the past.\n\n### Review Notes\n- Main changes that verify prior SetId on failures are placed in\n[check_message_signature_with_buffer](https://github.com/paritytech/polkadot-sdk/pull/9015/files#diff-359d7a46ea285177e5d86979f62f0f04baabf65d595c61bfe44b6fc01af70d89R458-R501)\n- Sync engine no longer disconnects outdated justifications in\n[process_service_command](https://github.com/paritytech/polkadot-sdk/pull/9015/files#diff-9ab3391aa82ee2b2868ece610100f84502edcf40638dba9ed6953b6e572dfba5R678-R703)\n\n### Testing Done\n- Deployed the PR to versi-net with 40 validators\n- Prior we have noticed 10/40 validators disconnecting every 15-20\nminutes, leading to instability\n- Over past 24h the issue has been mitigated:\nhttps://grafana.teleport.parity.io/goto/FPNWlmsHR?orgId=1\n- Note: bootnodes 0 and 1 are currently running outdated versions that\ndo not incorporate this SetID verification improvement\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/8872\nCloses: https://github.com/paritytech/polkadot-sdk/issues/1147\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>",
          "timestamp": "2025-07-22T12:08:36Z",
          "tree_id": "e83cda247a4ac590cf45c24390e0736eea169d4c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f8d2a2b875a3f08aae5dd82c09ffe65932d2e62"
        },
        "date": 1753190879648,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025613894599999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004870168669999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008396031089999995,
            "unit": "seconds"
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
          "distinct": true,
          "id": "db5e645422ccf952018a3c466a33fef477858602",
          "message": "network: Upgrade litep2p to v0.10.0 (#9287)\n\n## litep2p v0.10.0\n\nThis release adds the ability to use system DNS resolver and change\nKademlia DNS memory store capacity. It also fixes the Bitswap protocol\nimplementation and correctly handles the dropped notification substreams\nby unregistering them from the protocol list.\n\n### Added\n\n- kad: Expose memory store configuration\n([#407](https://github.com/paritytech/litep2p/pull/407))\n- transport: Allow changing DNS resolver config\n([#384](https://github.com/paritytech/litep2p/pull/384))\n\n### Fixed\n\n- notification: Unregister dropped protocols\n([#391](https://github.com/paritytech/litep2p/pull/391))\n- bitswap: Fix protocol implementation\n([#402](https://github.com/paritytech/litep2p/pull/402))\n- transport-manager: stricter supported multiaddress check\n([#403](https://github.com/paritytech/litep2p/pull/403))\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-22T14:24:10Z",
          "tree_id": "a01eacbdd376755eea81cbd3e34a8279b13055c8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/db5e645422ccf952018a3c466a33fef477858602"
        },
        "date": 1753198888028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008526110029999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005134003279999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002580963229999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6a951f77bf0cbdb4bbb07783aac8a45bfb38351a",
          "message": "Dedup dependencies between dependencies and dev-dependencies (#9233)\n\n# Description\n\nDeduplicate some dependencies between `dependencies` and\n`dev-dependencies` sections\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-22T22:17:02+02:00",
          "tree_id": "8cb1aa69bfd7b4adc90c07e3af54a8f5ef858e5b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6a951f77bf0cbdb4bbb07783aac8a45bfb38351a"
        },
        "date": 1753217601791,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005121498869999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026405267999999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008573786009999983,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e2802be4f32f55006abd3a40fc1808d997eaa4e1",
          "message": "fix: skip verifying imported blocks (#9280)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/9277. Still WIP\ntesting\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-22T21:08:01Z",
          "tree_id": "81087a8a688af80d3a3c027177554189be9e4050",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e2802be4f32f55006abd3a40fc1808d997eaa4e1"
        },
        "date": 1753222801759,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005010651049999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008487558749999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002620140880000001,
            "unit": "seconds"
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
          "distinct": false,
          "id": "a34de56236e741a081aeb9f7af5094d781e0ac9d",
          "message": "[Staking Async] Saturating accrue era reward points (#9186)\n\nReplaces regular addition with saturating addition when accumulating era\nreward points in `pallet-staking-async` to prevent potential overflow.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-23T07:59:42Z",
          "tree_id": "17724309115bdf377ef4cbb9701eb49cfe7146f6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a34de56236e741a081aeb9f7af5094d781e0ac9d"
        },
        "date": 1753261860342,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008485585319999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262507239,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005108467979999992,
            "unit": "seconds"
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
          "id": "9428742a2994c4fb2b2de8d4bfc36deeca01e19d",
          "message": "Replace `log` with `tracing` on `pallet-bridge-grandpa` (#9294)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-grandpa` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-23T16:44:52Z",
          "tree_id": "531de75c002557fd8ed854570af158ed405f2c2f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9428742a2994c4fb2b2de8d4bfc36deeca01e19d"
        },
        "date": 1753293439922,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871029359999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005195045979999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263738035,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "karol@parity.io",
            "name": "Karol Kokoszka",
            "username": "karolk91"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "069b7b56118fd65ecdbc6fea6c4dd1ffbf586d67",
          "message": "Fix subsume_assets incorrectly merging two AssetsInHolding (#9179)\n\n`subsume_assets` fails to correctly subsume two instances of\n`AssetsInHolding` under certain conditions which can result in loss of\nfunds (as assets are overriden rather than summed together)\n\nEg. consider following test:\n```\n\t#[test]\n\tfn subsume_assets_different_length_holdings() {\n\t\tlet mut t1 = AssetsInHolding::new();\n\t\tt1.subsume(CFP(400));\n\n\t\tlet mut t2 = AssetsInHolding::new();\n\t\tt2.subsume(CF(100));\n\t\tt2.subsume(CFP(100));\n\n\t\tt1.subsume_assets(t2);\n```\n\ncurrent result (without this PR change):\n```\n\t\tlet mut iter = t1.into_assets_iter();\n\t\tassert_eq!(Some(CF(100)), iter.next());\n\t\tassert_eq!(Some(CFP(100)), iter.next());\n```\n\nexpected result:\n```\n\t\tlet mut iter = t1.into_assets_iter();\n\t\tassert_eq!(Some(CF(100)), iter.next());\n\t\tassert_eq!(Some(CFP(500)), iter.next());\n```\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-07-24T05:51:09Z",
          "tree_id": "a2e8b99e5afdb2a058e9db3d8566040f491d5955",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/069b7b56118fd65ecdbc6fea6c4dd1ffbf586d67"
        },
        "date": 1753340645750,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008624006969999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026637322000000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005221194950000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "4211399+ordian@users.noreply.github.com",
            "name": "ordian",
            "username": "ordian"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e5e3941b5fb480a27e37f28fd437962dd029ec96",
          "message": "yap-runtime: fixes for `GetParachainInfo` (#9312)\n\nThis fixes the YAP parachain runtimes in case you encounter a panic in\nthe collator similar to\nhttps://github.com/paritytech/zombienet/issues/2050:\n```\nFailed to retrieve the parachain id\n```\n(which we do have zombienet-sdk tests for\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/client/transaction-pool/tests/zombienet/yap_test.rs))\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-24T13:03:01Z",
          "tree_id": "d908f5b48bf7d1a16c929565a909eeb7371482b8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5e3941b5fb480a27e37f28fd437962dd029ec96"
        },
        "date": 1753366352004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005268503699999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00867873461999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026467916899999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "40807189+AlexandruCihodaru@users.noreply.github.com",
            "name": "Alexandru Cihodaru",
            "username": "AlexandruCihodaru"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "bb39b4ecea005157687ed61c6ca4775f2264494f",
          "message": "RecentDisputes/ActiveDisputes use BTreeMap instead of Vec (#9309)\n\nFixes #782\n\n---------\n\nSigned-off-by: Alexandru Cihodaru <alexandru.cihodaru@parity.io>",
          "timestamp": "2025-07-24T15:30:53Z",
          "tree_id": "f92f583f62b60e6c2c65e160ad06d8fce52e9c06",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bb39b4ecea005157687ed61c6ca4775f2264494f"
        },
        "date": 1753375260085,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008601914719999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005142760259999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026224171099999997,
            "unit": "seconds"
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
          "distinct": true,
          "id": "5b4ce9cbf1a46c843e391768ae179ac377aab951",
          "message": "network/litep2p: Switch to system DNS resolver (#9321)\n\nSwitch to system DNS resolver instead of 8.8.8.8 that litep2p uses by\ndefault. This enables full administrator control of what upstream DNS\nservers to use, including resolution of local names using custom DNS\nservers.\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/9298.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-25T09:05:15Z",
          "tree_id": "0487343c65a8af24281068c6c4aa4dcbfe0dab75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5b4ce9cbf1a46c843e391768ae179ac377aab951"
        },
        "date": 1753438570743,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008515756469999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00510302597999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261330908,
            "unit": "seconds"
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
          "id": "e11b1dcecc10f0fe3dab8785e6c52f243f82030b",
          "message": "[Backport] Regular version bumps and prdoc reordering from the stable2506 release branch back to master (#9320)\n\nThis PR backports:\n- NODE_VERSION bumps\n- spec_version bumps\n- prdoc reordering\nfrom the release branch back to master\n\n---------\n\nCo-authored-by: ParityReleases <release-team@parity.io>",
          "timestamp": "2025-07-25T10:39:37Z",
          "tree_id": "754e61e7d28611ce0028d2067705a17bf7d8e6ae",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e11b1dcecc10f0fe3dab8785e6c52f243f82030b"
        },
        "date": 1753444634118,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008571068779999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004993273269999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002675667429999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "50408393+TDemeco@users.noreply.github.com",
            "name": "Tobi Demeco",
            "username": "TDemeco"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "33a43cf48b0dc78fad1212ceb15b64b81fb8e761",
          "message": "fix: :bug: use `MaxKeys` from `pallet-im-online`'s Config trait instead of hardcoded one in benchmarks (#9325)\n\nThis PR is a simple fix for issue #9324, by making the benchmarks of\n`pallet-im-online` linear up to `pallet_im_online::Config::MaxKeys`\ninstead of the hardcoded constant `MAX_KEYS = 1000`.\n\nThis should allow any runtime that uses `pallet-im-online` with less\nthan 1000 max keys to be able to benchmark the pallet correctly.",
          "timestamp": "2025-07-25T11:25:12Z",
          "tree_id": "162909f5bbb972eef3d874f961d45df3fd0315c3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33a43cf48b0dc78fad1212ceb15b64b81fb8e761"
        },
        "date": 1753446950071,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026286785399999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008580544459999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005081402369999996,
            "unit": "seconds"
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
          "id": "73b44193c8e66acd699f04265027289d030f6c66",
          "message": "frame_system: Whitelist storage items and do not double kill! (#9335)\n\nThis pull requests adds some storage values to the whitelisted storage\nitem list, because they are written in every block. Also it stops double\nkilling `InherentsApplied`. It is killed in `on_finalize`, so there is\nno need to do it again in `on_initialize`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-25T14:45:45Z",
          "tree_id": "250c3b45b5be25c30e0286f5dab152b98fee7eef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73b44193c8e66acd699f04265027289d030f6c66"
        },
        "date": 1753459074924,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026509467199999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005055511939999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008539114469999989,
            "unit": "seconds"
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
          "id": "7ef027551fd1290c42581a85052b643bffc9cbe4",
          "message": "`fatxpool`: avoid premature revalidation of transactions (#9189)\n\nThis PR improves handling of the following scenario:\n```\nsend tx1: transfer to fund new  X account \n# wait for tx1 in block event (let's assume it happens at block N) \nsend tx2: spend from X account\n```\n\nBefore this PR `tx2` could be invalidated (and most likely was) when\n`block N-k` was finalized, because transactions are checked for being\ninvalid on finalized block. (The `X account` does not yet exists for any\nblock before `block N`).\n\nAfter this commit transactions will be revalidated on finalized blocks\nonly if their height is greater then height of the block at which\ntransactions was originally submitted.\n\nNote: There are no guarantees that `tx2` will be actually included, it\nstill may happen that it will be dropped under some circumstances. This\nonly reduces likelihood of dropping transaction.\n\n\nNote for reviewers:\nThe fix is to simply initialize\n[`validated_at`](https://github.com/paritytech/polkadot-sdk/blob/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40/substrate/client/transaction-pool/src/fork_aware_txpool/tx_mem_pool.rs#L98-L99)\nfield of `TxInMemPool` which is used to\n[select](https://github.com/paritytech/polkadot-sdk/blob/f8a1fe64c29b1ddcb5824bbb3bf327f528f18d40/substrate/client/transaction-pool/src/fork_aware_txpool/tx_mem_pool.rs#L583-L586)\ntransactions for mempool revalidation on finalized block.\n\nFixes: #9150\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-07-26T09:38:10Z",
          "tree_id": "9290436971f50c36e2f18f32158c7ff376adf03a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ef027551fd1290c42581a85052b643bffc9cbe4"
        },
        "date": 1753526929103,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005298488929999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026702950300000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008772860789999977,
            "unit": "seconds"
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
          "id": "edc8a7f95405b929318bb40867a3caca1bca9565",
          "message": "Replace `log` with `tracing` on `pallet-bridge-messages` (#9308)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-messages` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-28T08:02:29Z",
          "tree_id": "7bd256fb1ed72a826f529fda260828a26b18a940",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/edc8a7f95405b929318bb40867a3caca1bca9565"
        },
        "date": 1753694128689,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008605633209999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026268005800000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005136259489999987,
            "unit": "seconds"
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
          "id": "bb4130369ed03ec130ba41ea4cf33cbc97a98c2f",
          "message": "Replace `log` with `tracing` on `bridge-runtime-common` (#9288)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`bridge-runtime-common` by providing structured logging.\n\nPartially addresses #9211\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrii <ndk@parity.io>",
          "timestamp": "2025-07-28T11:54:43Z",
          "tree_id": "646c8459d9769ef4cfaf313adf64bccd2294d254",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bb4130369ed03ec130ba41ea4cf33cbc97a98c2f"
        },
        "date": 1753707913127,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008538299879999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002612974020000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00505242120999999,
            "unit": "seconds"
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
          "id": "492f66cdfcb2da0dfc8ce66b8b32e8801ea14fe9",
          "message": "include poll_index in voted and vote removed event (#8840)\n\ncloses #8785\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-28T13:10:54Z",
          "tree_id": "f964a6f7afdb1acbb1ccef2fee733840f65de696",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/492f66cdfcb2da0dfc8ce66b8b32e8801ea14fe9"
        },
        "date": 1753712971043,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008574292499999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264460166,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005102661489999995,
            "unit": "seconds"
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
          "distinct": true,
          "id": "a15d066faac70676101854cfa9b55f00f61e865a",
          "message": "network/kad: Increase memory store capacity for providers (#9315)\n\nIncrease Kademlia memory store capacity for DHT content providers (used\nby parachain DHT-based bootnodes) and reduce provider republish interval\n& TTL. This is needed to support testnets with 1-minute fast runtime and\nup to 13 parachains.\n\nParameters set:\n- 10000 provider keys per node\n- 10h provider record TTL\n- 3.5h provider republish interval\n\nCloses https://github.com/paritytech/litep2p/issues/405.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-28T15:24:53Z",
          "tree_id": "51754d7b2f1622572ee764f0d16ad5ca318154ee",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a15d066faac70676101854cfa9b55f00f61e865a"
        },
        "date": 1753721061459,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005135600069999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025990491500000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008712407089999985,
            "unit": "seconds"
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
          "id": "ff6a5f7ec52525bfada191fd04cf6e8c34d2b78e",
          "message": "Replace `log` with `tracing` on `pallet-bridge-parachains` (#9318)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-parachains` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-28T16:12:47Z",
          "tree_id": "bf341a229cf46b75766555d16d51600cbf931095",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff6a5f7ec52525bfada191fd04cf6e8c34d2b78e"
        },
        "date": 1753723377424,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002671135479999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008559249359999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005040635599999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "140437456+hamidmuslih@users.noreply.github.com",
            "name": "Hamid Muslih",
            "username": "hamidmuslih"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "ba5ed25153d37d273040925d1336620a34044fbd",
          "message": "update the builder dockerfile (#9310)\n\n# Description\n\nUpdates the base image in the Polkadot builder Dockerfile\n\nCloses #9306\n## Integration\n\nNot applicable - this PR has no downstream integration impacts as it\nonly affects the local build environment\n\n## Review Notes\n\nThis PR updates the builder base image version in\n`polkadot_builder.Dockerfile`.\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-07-28T23:31:56+02:00",
          "tree_id": "b96e77db1d8bcdf9b9df5163aa72d74c87993ed2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba5ed25153d37d273040925d1336620a34044fbd"
        },
        "date": 1753740412079,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004869029299999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008463655959999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026308021399999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "82968568+miloskriz@users.noreply.github.com",
            "name": "Milos Kriz",
            "username": "miloskriz"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d55dc56df31a9f4fdd59ca7ca06f2a8b00ad808b",
          "message": "Maintenance of bootnodes for `westend` and related chains (#9353)\n\n# Description\n\nPlease consider this Pull Request to remove the bootnodes provided by\nGatotech to the following relaychain and systemchains:\n\n- `westend`\n  - `asset-hub-westend`\n  - `bridge-hub-westend`\n  - `collectives-westend`\n  - `coretime-westend`\n  - `people-westend`\n\nThis removal responds to the discontinuation of support by the\nInfrastructure Builders' Programme of Westend in favour of enhanced\nsupport to the Paseo testnet.\n\nAfter this PR is merged, we will proceed to decommission the relevant\nnodes..\n\nMany thanks!!\n\nBest regards\n\n**_Milos_**\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-07-29T12:39:54Z",
          "tree_id": "5e96d7613fc4bd03f97b864271e24e4e0bc984db",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d55dc56df31a9f4fdd59ca7ca06f2a8b00ad808b"
        },
        "date": 1753797238724,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026583770800000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008787148869999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00521899523999999,
            "unit": "seconds"
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
          "id": "835ee4782522319cf5d8de9e35a20e56ead143b7",
          "message": "cumulus zombienet: Send transactions as immortal (#9362)\n\nDeep inside subxt the default period for a transaction is set to 32\nblocks. When you have some chain that is building blocks every 500ms,\nthis may leads to issues that manifest as invalid transaction\nsignatures. To protect the poor developers of endless debugging sessions\nwe now send transactions as immortal.",
          "timestamp": "2025-07-29T19:28:17Z",
          "tree_id": "7bf288f567d290b92248d562ce63c1d26211e02b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/835ee4782522319cf5d8de9e35a20e56ead143b7"
        },
        "date": 1753823016079,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0051729114400000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00863501575999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025792348500000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "karol@parity.io",
            "name": "Karol Kokoszka",
            "username": "karolk91"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "a64eb1fb02d4012948cba024fca2f27d94732e52",
          "message": "Remove whitespaces added by macros due to token re-parsing (#9354)\n\nRelates to: https://github.com/paritytech/polkadot-sdk/issues/9336,\nhttps://github.com/paritytech/polkadot-sdk/pull/7321\n\nThis PR aims to normalize result of `stringify` in scenarios when used\ninside nested macros to stringify token streams for benchmarking\nframework. Different versions of rust can include, or not, \"space\"\ncharacters around tokens like `<`,`>`,`::` so we are just removing\nadditional spaces.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-07-30T05:46:16Z",
          "tree_id": "b85a3b83c7dfcdd03e82495f9156048789f905e2",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a64eb1fb02d4012948cba024fca2f27d94732e52"
        },
        "date": 1753859133113,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0028171183100000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008914146899999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005404896939999998,
            "unit": "seconds"
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
          "id": "d753869cbb5a6db66ac0dc88c2acb407301eaa01",
          "message": "Fix definition of held balance (#9347)\n\n## Changes\n- Updated the `Held Balance` definition to reflect the current behavior.\nThe previous explanation was accurate when staking used locks (which\nwere part of the free balance), but since [staking now uses\nholds](https://github.com/paritytech/polkadot-sdk/pull/5501), the old\ndefinition is misleading.\nThis issue was originally pointed out by @michalisFr\n[here](https://github.com/w3f/polkadot-wiki/pull/6793#discussion_r2231472702).\n- Fixed a broken reference in the deprecated doc for `ExposureOf`, which\nwas (ironically) pointing to a non-existent type named `ExistenceOf`.\nThis slipped in during our [mega async staking\nPR](https://github.com/paritytech/polkadot-sdk/pull/8127).",
          "timestamp": "2025-07-30T12:06:36Z",
          "tree_id": "233781385e6bdbed9c58e4af8c5b98876f525d62",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d753869cbb5a6db66ac0dc88c2acb407301eaa01"
        },
        "date": 1753882253014,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026555206300000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005170200819999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008575080259999989,
            "unit": "seconds"
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
          "id": "e08d8f0173db4394e0f99fff91a42d86e8d6062b",
          "message": "[Staking/AHM] Properly report weight of rc -> ah xcm back to the calls (#9380)\n\nWhich will consequently make the XCM/MQ code path aware of the weights,\nwhich was previously not the case.\n\nAdditionally, adds an event for when an era is pruned.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Paolo La Camera <paolo@parity.io>",
          "timestamp": "2025-07-30T14:51:41Z",
          "tree_id": "37ee2ae118ac7b9b9196dd78e4445a0f9a15c469",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e08d8f0173db4394e0f99fff91a42d86e8d6062b"
        },
        "date": 1753891804401,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008631030899999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005054633029999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026336694300000004,
            "unit": "seconds"
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
          "id": "5633cf5f16d226428a5e87a9a17b7415bcaaec6d",
          "message": "Ignore trie nodes while recording a proof (#8172)\n\nThis pull requests implements support for ignoring trie nodes while\nrecording a proof. It directly includes the feature into\n`basic-authorship` to later make use of it in Cumulus for multi-block\nPoVs.\n\nThe idea behind this is when you have multiple blocks per PoV that trie\nnodes accessed or produced by a block before (in the same `PoV`), are\nnot required to be added to the storage proof again. So, all the blocks\nin one `PoV` basically share the same storage proof. This also impacts\nthings like storage weight reclaim, because ignored trie node do not\ncontribute a to the storage proof size (similar to when this would\nhappen in the same block).\n\n# Example \n\nLet's say block `A` access key `X` and block `B` accesses key `X` again.\nAs `A` already has read it, we know that it is part of the storage proof\nand thus, don't need to add it again to the storage proof when building\n`B`. The same applies for storage values produced by an earlier block\n(in the same PoV). These storage values are an output of the execution\nand thus, don't need to be added to the storage proof :)\n\n\nDepends on https://github.com/paritytech/polkadot-sdk/pull/6137. Base\nbranch will be changed when this got merged.\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/6495\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Michal Kucharczyk <1728078+michalkucharczyk@users.noreply.github.com>",
          "timestamp": "2025-07-31T09:59:56Z",
          "tree_id": "431ac4005d7655af6fd7d76f89ff6162d34b87b1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5633cf5f16d226428a5e87a9a17b7415bcaaec6d"
        },
        "date": 1753960589454,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005123743669999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026385790100000007,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008515434699999987,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "karol@parity.io",
            "name": "Karol Kokoszka",
            "username": "karolk91"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d9f451a6b94ab2cf39371ee5192130379eb6e199",
          "message": "XCMv5 asset exchange test scenarios (#9195)\n\nRelates to: #9093\nRequires: #9179\n\nThis PR introduces emulated test scenarios:\n\n#### [Scenario 1]\n(Penpal -> AH -> Penpal) to showcase usage of remote `Transact` to swap\nassets remotely on AssetHub while also making use of\n`add_authorized_alias`, to transact as Sender on remote side (instead of\nSenders sovereign account).\n\n1. Prepare sovereign accounts funds, create pools, prepare aliasing\nrules\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Alias into sender account and exchange WNDs for USDT using `Transact`\nwith `swap_tokens_for_exact_tokens` call inside\n4. Send USDT and leftover WND back to Penpal\n\n#### [Scenario 2]\n(Penpal -> AH -> Penpal) to showcase usage of remote `Transact` to swap\nassets remotely on AssetHub.\n\n1. Prepare sovereign accounts funds, create pools, prepare aliasing\nrules\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Exchange WNDs for USDT using `Transact` with\n`swap_tokens_for_exact_tokens` call inside\n4. Send USDT and leftover WND back to Penpal\n\n#### [Scenario 3]\n(Penpal -> AH -> Penpal) to showcase same as above but this time using\n`ExchangeAsset` XCM instruction instead of `Transact`:\n\n1. Prepare sovereign accounts funds, create pools\n2. Send WND from Penpal to AssetHub (AH being remote reserve for WND)\n3. Exchange WNDs for USDT using `ExchangeAsset`\n4. Send USDT and leftover WND back to Penpal\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-07-31T10:42:16Z",
          "tree_id": "c39005e9c21f11c1f9743d3a32b8454e91d41b37",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d9f451a6b94ab2cf39371ee5192130379eb6e199"
        },
        "date": 1753963500826,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005233778669999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263545215,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00874518134999999,
            "unit": "seconds"
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
          "id": "fec2a9129a9e0238891c4102bb78b06e450e8e14",
          "message": "Replace `log` with `tracing` on `pallet-bridge-beefy` (#9378)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-beefy` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-07-31T12:36:33Z",
          "tree_id": "2acc237d85439092d66685c471f132ee381fad74",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fec2a9129a9e0238891c4102bb78b06e450e8e14"
        },
        "date": 1753969642282,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025956951700000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008509747089999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005020687039999994,
            "unit": "seconds"
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
          "id": "177b03958c766fe053f28424ee6f6748644bb794",
          "message": "[AHM] Make stuff public and derive (#9384)\n\nMake some stuff public and derive traits. Also removes one silently\ntruncating constructor from ParaId.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-31T13:29:50Z",
          "tree_id": "f7914d1e9b38929282d65a0b7e256c4cea538a61",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/177b03958c766fe053f28424ee6f6748644bb794"
        },
        "date": 1753973357753,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026792441300000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005139147339999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008668094609999979,
            "unit": "seconds"
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
          "id": "7304295748b1d85eb9fc2b598eba43d9f7971f22",
          "message": "[AHM] Staking async e2e zn and papi tests (#8802)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/8766\n\nThis PR mainly adds a setup based on PAPI to automate our e2e tests for\nstaking async. Most of the new code is in\n`frame/staking-async/runtimes/papi-tests`. There is `README`, and a\n`Justfile` there that should contain all the info you would need.\n\nBest way to get started is:\n\n```\njust setup\nbun test tests/unsigned-dev.test.ts\n```\n\nTests are written in Typescript, and monitro the underlying ZN process\nfor a specific sequence of events. An example of how to write tests is\n[here](https://github.com/paritytech/polkadot-sdk/pull/8802/files#diff-4b44e03288aeaf5ec576ae0094c7a7ae28689dfcc5b317a28478767b345991db).\n\nAll other changes are very insubstantial. \n\n### Why this setup? \n\n* Staking async e2e tests are long running, and doing multiple scenarios\nmanually is hard. Expressing them as a sequence of events is much\neasier.\n* For all scenarios, we need to monitor both the onchain weight, and the\noffchain weight/PoV recorded by the collator (therefore our only option\nis ZN). The setup reports both. For example, the logs look like this:\n\n```\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 14\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [1,10]\ninfo:    Primary event passed\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 13\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [2,10]\ninfo:    Primary event passed\nverbose: Next expected event: Observe(Para, MultiBlockElectionVerifier, Verified, no dataCheck, no byBlock), remaining events: 12\nverbose: [Para#56][⛓ 52ms / 2,119 kb][✍️ hd=0.22, xt=3.94, st=6.54, sum=10.70, cmp=9.61, time=1ms] Processing event: MultiBlockElectionVerifier Verified [3,10]\n```\n\n`⛓` indicates the onchain weights and `✍️` the collator PoV date\n(header, extrinsic, storage, sum of all, and all compressed,\nrespectively). The above lines are an example of code paths where the\nonchain weight happens to over-estimate by a lot. This setup helps us\neasily find and optimize all.\n\n---------\n\nCo-authored-by: Tsvetomir Dimitrov <tsvetomir@parity.io>\nCo-authored-by: Paolo La Camera <paolo@parity.io>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexandre R. Baldé <alexandre.balde@parity.io>",
          "timestamp": "2025-07-31T17:51:37Z",
          "tree_id": "49a98e39596f07d10155e85247e4ef3dd13af3be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7304295748b1d85eb9fc2b598eba43d9f7971f22"
        },
        "date": 1753988593316,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008612936139999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026488008099999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005134206549999994,
            "unit": "seconds"
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
          "id": "13bc266c3f3cb337a36998cfdc5940ca559051c9",
          "message": "Upgrade wasmtime (#8714)\n\nThis upgrades wasmtime to the latest version and also fixes backtraces\nfor `debug` builds.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-07-31T21:51:41Z",
          "tree_id": "26b7b22e5e91ce9e7e84a9a186f5d4d94abb898c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/13bc266c3f3cb337a36998cfdc5940ca559051c9"
        },
        "date": 1754004072029,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008547402349999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005096510979999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026075796000000007,
            "unit": "seconds"
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
          "id": "aa010dc3286063cd1f3522fc988ba472dede345b",
          "message": "Revert \"fix(minimal): pre-seal a first block to trigger maintain (#92… (#9423)\n\n# Description\n\nThis PR reverts #9207 after @michalkucharczyk's proper fix in #9338.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A",
          "timestamp": "2025-08-03T21:07:52Z",
          "tree_id": "757aca1f0e6e6b970edfe82ae3a36dc65718d58e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa010dc3286063cd1f3522fc988ba472dede345b"
        },
        "date": 1754259581399,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026343631699999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008646266469999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051919826099999945,
            "unit": "seconds"
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
          "id": "a48307b7f0c40225aa8b6fcfecf7cedb6a41d6c2",
          "message": "CoreIndexMismatch: Include more information in the error (#9396)",
          "timestamp": "2025-08-04T07:10:18Z",
          "tree_id": "f5f43827128516e54f503afddfa6f54f7c2175dd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a48307b7f0c40225aa8b6fcfecf7cedb6a41d6c2"
        },
        "date": 1754296306141,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00867922318999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026238306399999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00514372894,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jesse.chejieh@gmail.com",
            "name": "Doordashcon",
            "username": "Doordashcon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "33bdd634d6ae0eb43c6660ba3ab6be6ed3668789",
          "message": "Westend Secretary Program (#9024)\n\n## Westend Secretary Program\n\nThis PR includes the Secretary program and end-to-end validation of\nXCM-based salary payments for the Westend runtime, ensuring consistency\nbetween implementations.\n\n### Key Changes\n1. Integrated Secretary configuration into Westend runtime\n- Added `SecretaryCollective` and `SecretarySalary` pallets to the\nruntime.\n   - Triggers salary payment through XCM\n   - Verifies successful:\n     - XCM message transmission\n     - Asset transfer execution\n     - Message queue processing\n\n### Context from Runtime PRs\n- Based on [Secretary Program\nimplementation](https://github.com/polkadot-fellows/runtimes/pull/347)\n- Follows patterns established in [Fellowship salary\ntests](https://github.com/paritytech/polkadot-sdk/blob/master/cumulus/parachains/integration-tests/emulated/tests/collectives/collectives-westend/src/tests/fellowship_salary.rs)\n- Addresses feedback from original implementation:\n  - Simplified polling mechanism using `NoOpPoll`\n  - Maintained consistent salary structure (6666 USDT for rank 1)\n  - Kept same XCM payment configuration",
          "timestamp": "2025-08-04T09:08:22Z",
          "tree_id": "28efe6737ecde815a68064c4a9dbf4bf6903a5ac",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33bdd634d6ae0eb43c6660ba3ab6be6ed3668789"
        },
        "date": 1754303248054,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008637548689999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263316841,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005060671869999993,
            "unit": "seconds"
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
          "id": "53e30e5c60bdef92ae46f2f9b6d29a4d113e7419",
          "message": "Collator Protocol: Be more informative why a collation wasn't advertised (#9419)\n\nThis prints more information on why a collation wasn't advertised. In\nthis exact case it checks if the collation wasn't advertised because of\na session change. This is mainly some debugging help.",
          "timestamp": "2025-08-04T10:26:18Z",
          "tree_id": "9d7c051e4ae3a47c43b46c29c568fdd9227cd1c4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/53e30e5c60bdef92ae46f2f9b6d29a4d113e7419"
        },
        "date": 1754307580553,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005050755729999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026496740900000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008563689969999993,
            "unit": "seconds"
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
          "id": "224eab75d3a05e7c7a85baa5e044858d0f104d4a",
          "message": "Replace `log` with `tracing` on `bp-runtime` (#9401)\n\nThis PR replaces `log` with `tracing` instrumentation on `bp-runtime` by\nproviding structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-08-04T13:10:17Z",
          "tree_id": "0adad46d8710fc31ea650371fbfe51296ca7184f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/224eab75d3a05e7c7a85baa5e044858d0f104d4a"
        },
        "date": 1754317889306,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008758490709999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005308165079999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002646625570000001,
            "unit": "seconds"
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
          "id": "0140f9934cd553e5a36c623c75d371f6a5108774",
          "message": "cumulus tests: Improve prefix of the relay chain node (#9420)\n\nInstead of using the name of the node, we should use `Relaychain` as\ndone by normal nodes. This makes it easier to read the logs.",
          "timestamp": "2025-08-04T15:17:30Z",
          "tree_id": "fec233df11d8df3202be325ba52901bf5ba98c45",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0140f9934cd553e5a36c623c75d371f6a5108774"
        },
        "date": 1754324927121,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00853023757999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005033950639999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026221914300000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hetterich.charles@gmail.com",
            "name": "Charles",
            "username": "charlesHetterich"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "bcee8dc5ba1205628b22e1ec499988d393b7e293",
          "message": "Added `substrate-node` and `eth-rpc` binaries into release workflow (#9393)\n\nAdds a total of 4 new jobs to `Release - Build node release candidate`\nCI workflow\n- 2 for releasing `substrate-node` binaries for linux/mac\n- 2 for releasing `eth-rpc` binaries for linux/mac\n\nCLOSES: #9386\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>",
          "timestamp": "2025-08-04T18:21:20Z",
          "tree_id": "286bfd6aa3f48745d03e2fc1c6a297d361b19c80",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bcee8dc5ba1205628b22e1ec499988d393b7e293"
        },
        "date": 1754336092773,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005105538319999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008606340929999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025249205200000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22696121+sekisamu@users.noreply.github.com",
            "name": "sekiseki",
            "username": "sekisamu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "59fb2e7482d471a7ec4e8d3b30499497efa7b34c",
          "message": "Fixes dust balance handling for pallet revive (#9357)\n\nfix issue: https://github.com/paritytech/contract-issues/issues/141\n\nCorrects the condition for minting a new currency unit when transferring\ndust. The condition was incorrectly checking\n`to_info.dust.saturating_add(dust) >= plank` which could lead to\nunexpected minting behavior. It now correctly checks if `to_info.dust >=\nplank` before minting.",
          "timestamp": "2025-08-04T19:36:58Z",
          "tree_id": "1f82f1472637c5b251995d04a7f494c144c5daf4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59fb2e7482d471a7ec4e8d3b30499497efa7b34c"
        },
        "date": 1754341053973,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008665357929999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050287778799999894,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026385609900000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0f59afba36c8affac3b7fd41b0518fd9b81cefef",
          "message": "staking-async/papi-tests: fix justfile to run in CI (#9411)\n\nMake sure to run `just killall` step in a bash shell, otherwise while\ntrying to kill a non existing process e.g.\n```bash\nkillall:\n  pkill -f zombienet || true\n```\n\nwe would get the following issue while running in a container:\n\n```bash\nRun just setup\n  just setup\n  shell: sh -e {0}\n  env:\n    IMAGE: docker.io/paritytech/ci-unified:bullseye-1.85.0-2025-01-28-v202504231537\n    RUST_INFO: rustup show && cargo --version && rustup +nightly show && cargo +nightly --version\n    CACHE_ON_FAILURE: true\n    CARGO_INCREMENTAL: 0\n🧹 Killing any existing zombienet or chain processes...\npkill -f zombienet || true\nerror: Recipe `killall` was terminated on line 124 by signal 15\nerror: Recipe `setup` failed with exit code 143\nError: Process completed with exit code 143.\n```\n\nRunning the just step within a bash shell, ensure that the error is\nproperly handled and propagated without terminating the just script.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-05T08:24:56Z",
          "tree_id": "d7576a5133a9265d2155b6a6babad37d766607fd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0f59afba36c8affac3b7fd41b0518fd9b81cefef"
        },
        "date": 1754386627380,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00860610840999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005021919349999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265242727,
            "unit": "seconds"
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
          "id": "56f683fabfc5a4554d4b9208e91103624264407c",
          "message": "do not trigger zombienet workflows on 'labeled' (#9427)\n\nAt some point of the stabilization process we added the 'labeled' to the\nlist of events that trigger the zombienet workflows. This is not needed\nanymore and also is causing failures because the _artifacts_ could be\nexpired\n([example](https://github.com/paritytech/polkadot-sdk/actions/runs/16529272288/job/46752021278?pr=9286#step:6:127)).\n\nThx!",
          "timestamp": "2025-08-05T15:10:46Z",
          "tree_id": "445f4b598ca4a41c328ca69c595b652faa137169",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/56f683fabfc5a4554d4b9208e91103624264407c"
        },
        "date": 1754411060761,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025591779099999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050356993999999934,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008448646149999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "xlchen1291@gmail.com",
            "name": "Xiliang Chen",
            "username": "xlc"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa",
          "message": "pallet-timestamp is dev dependency of pallet-xcm (#9435)\n\nit is only used in mock\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-06T07:18:50Z",
          "tree_id": "18684bbb92e38d18676ffa52b4c6665a77cc1449",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa"
        },
        "date": 1754469096658,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008601640039999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026121301,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005157548269999997,
            "unit": "seconds"
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
          "id": "43da422003fa5a5e66faee0bf9919a8a32bb630f",
          "message": "CumulusDigestItem: Add function to fetch the relay block identifier (#9432)\n\nThis simplifies code paths that are fetching these information from a\nparachain header.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-06T19:08:19Z",
          "tree_id": "bcc9d3db53b62f7a85fe3db2e791b1da77787ae7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/43da422003fa5a5e66faee0bf9919a8a32bb630f"
        },
        "date": 1754511525496,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005212407019999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008700687759999983,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002698745,
            "unit": "seconds"
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
          "id": "8468c3e5944ab5efdcce886e275f4cec1cdc9057",
          "message": "pr_8838.prdoc: oversight fix: major -> minor (#9440)\n\nThis fixes the\n[pr_8838.prdoc](https://github.com/paritytech/polkadot-sdk/blob/cf5a24ecc5802ecf78d943f9723b6f4ccdc0ddfa/prdoc/pr_8838.prdoc#L7).\nI somehow forgotten to fix this\n[here](https://github.com/paritytech/polkadot-sdk/pull/8838#discussion_r2152760564).",
          "timestamp": "2025-08-07T08:12:57Z",
          "tree_id": "98f2c1f36fd55a3668a7e72b4b7efc1fdd109707",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8468c3e5944ab5efdcce886e275f4cec1cdc9057"
        },
        "date": 1754558788301,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025801787400000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008537892599999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005129196109999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "beb9030b249cc078b3955232074a8495e7e0302a",
          "message": "[pallet-revive] Implement basic `System` pre-compile, move `seal_hash_blake2_256` into it (#9441)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nJust the `hash_blake2_256` in this PR, to gauge if you're fine with this\nsetup.\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-08-07T11:20:46Z",
          "tree_id": "61e2c4a158c8f86612d14047e993f1858f2b86fd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/beb9030b249cc078b3955232074a8495e7e0302a"
        },
        "date": 1754570556691,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008649509869999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005096052659999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026123898199999994,
            "unit": "seconds"
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
          "distinct": true,
          "id": "0d765ce37b258640a6eeb575f6bff76d6a7b7c46",
          "message": "pallet-xcm: fix authorized_alias benchmarks (#9445)\n\nDepending on runtime configuration of ED and storage deposits, the old\nbenchmark code did not set up enough funds to cover authorized aliases\nstorage deposits.\n\nFix it by adding more funds as part of benchmark setup.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Karol Kokoszka <karol.k91@gmail.com>",
          "timestamp": "2025-08-07T19:28:09Z",
          "tree_id": "143f16d946fef64897bb51a08e29cbc6ccc42185",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d765ce37b258640a6eeb575f6bff76d6a7b7c46"
        },
        "date": 1754599247855,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005211298289999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00867143288999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026565984000000006,
            "unit": "seconds"
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
          "id": "c40b36c3a7c208f9a6837b80812473af3d9ba7f7",
          "message": "Cleanup staking try states + fix min bonds (#9415)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-08T09:25:18Z",
          "tree_id": "6f1e8993c14fa59502c2a975c3bbe1d6d991ad61",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c40b36c3a7c208f9a6837b80812473af3d9ba7f7"
        },
        "date": 1754650312547,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005185368899999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026787099000000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008681026779999984,
            "unit": "seconds"
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
          "id": "0034d178fff88a0fd87cf0ec1d8f122ae0011d78",
          "message": "[CI] add timeout to allow alloy to process the logs (#9459)\n\nCI fix to give time to process zombienet's logs.\n\ncc https://github.com/paritytech/devops/issues/4229",
          "timestamp": "2025-08-11T18:05:05Z",
          "tree_id": "6cc0759361e6f4b043a6d1d53ef60866f5f68f67",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0034d178fff88a0fd87cf0ec1d8f122ae0011d78"
        },
        "date": 1754939832458,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263800379,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005133243719999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008626922539999986,
            "unit": "seconds"
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
          "id": "9fe9950f2173981209dcbe1b6d640764090d9f36",
          "message": "Minor Snowbridge test fixes (#9463)\n\nThe Polkadot runtimes repo block size is too small to test all Ethereum\nclient extrinsics in a single block. This PR runs to the next block\nbefore attempting more test extriniscs. Once this PR has been released,\nthe following code can be removed from the fellows runtime repo:\nhttps://github.com/polkadot-fellows/runtimes/blob/main/system-parachains/bridge-hubs/bridge-hub-polkadot/tests/snowbridge.rs#L234-L370\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-08-12T07:06:26Z",
          "tree_id": "c5e7264aa47b9ad38029d67eae255579d1a026c6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9fe9950f2173981209dcbe1b6d640764090d9f36"
        },
        "date": 1754988067976,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027865009600000013,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00901572283999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005396325759999992,
            "unit": "seconds"
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
          "id": "2db5e16bf2b497e8ef877d3d7e79b3fcdcab5f82",
          "message": "Bridges - relax trait bound from Codec to Encode (#9470)\n\n### Problem\n\nWhile bumping the parity-bridges-common repo to the latest polkadot-sdk\nmaster, we encountered a new compilation issue:\n```\nerror[E0277]: the trait bound `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>: Decode` is not satisfied\n   --> relay-clients/client-rococo/src/lib.rs:95:3\n    |\n95  |         bp_polkadot_core::UncheckedExtrinsic<Self::Call, bp_rococo::TransactionExtension>;\n    |         ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `parity_scale_codec::Decode` is not implemented for `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>`\n    |\n    = help: the trait `parity_scale_codec::Decode` is implemented for `sp_runtime::generic::UncheckedExtrinsic<Address, Call, Signature, Extension, MAX_CALL_SIZE>`\n    = note: required for `<Rococo as ChainWithTransactions>::SignedTransaction` to implement `parity_scale_codec::Codec`\nnote: required by a bound in `relay_substrate_client::ChainWithTransactions::SignedTransaction`\n   --> ~/.cargo/git/checkouts/polkadot-sdk-dee0edd6eefa0594/c40b36c/bridges/relays/client-substrate/src/chain.rs:227:42\n    |\n227 |     type SignedTransaction: Clone + Debug + Codec + Send + 'static;\n    |                                             ^^^^^ required by this bound in `ChainWithTransactions::SignedTransaction`\n```\n\nI added the test simulating the same compilation error here in the\npolkadot-sdk:\n```\ncargo test -p relay-substrate-client\n\nerror[E0277]: the trait bound `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, ..., ..., ..., 16777216>: Decode` is not satisfied\n   --> bridges/relays/client-substrate/src/test_chain.rs:92:27\n    |\n92  |       type SignedTransaction = bp_polkadot_core::UncheckedExtrinsic<\n    |  ______________________________^\n93  | |         TestRuntimeCall,\n94  | |         bp_polkadot_core::SuffixedCommonTransactionExtension<(\n95  | |             bp_runtime::extensions::BridgeRejectObsoleteHeadersAndMessages,\n96  | |             bp_runtime::extensions::RefundBridgedParachainMessagesSchema,\n97  | |         )>,\n98  | |     >;\n    | |_____^ the trait `parity_scale_codec::Decode` is not implemented for `UncheckedExtrinsic<MultiAddress<AccountId32, ()>, EncodedOrDecodedCall<...>, ..., ..., 16777216>`\n    |\n    = help: the trait `parity_scale_codec::Decode` is implemented for `UncheckedExtrinsic<Address, Call, Signature, Extension, MAX_CALL_SIZE>`\n    = note: required for `<TestChain as ChainWithTransactions>::SignedTransaction` to implement `parity_scale_codec::Codec`\nnote: required by a bound in `ChainWithTransactions::SignedTransaction`\n   --> bridges/relays/client-substrate/src/chain.rs:227:42\n    |\n227 |     type SignedTransaction: Clone + Debug + Codec + Send + 'static;\n    |                                             ^^^^^ required by this bound in `ChainWithTransactions::SignedTransaction`\n    = note: the full name for the type has been written to '/home/bkontur/cargo-remote-builds-aaa/4049172861662423200/target/debug/deps/relay_substrate_client-3bc9e3563aed810c.long-type-11484417815568207698.txt'\n```\n\n### Solution?\n\nAfter some investigation, this compilation issue stared with\nhttps://github.com/paritytech/polkadot-sdk/pull/8234, and relaxing the\n`type SignedTransaction` constraint resolves the issue. Any other\nsolution?\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-12T10:11:16Z",
          "tree_id": "99b4240a7c41440c005fff1d1b4f9a5a6537f9b0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2db5e16bf2b497e8ef877d3d7e79b3fcdcab5f82"
        },
        "date": 1754997755076,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026099018800000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008533682069999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005075402909999995,
            "unit": "seconds"
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
          "id": "5bb3afcd733a18744a09c1df840c3813623fb0ab",
          "message": "check-semver: enable the support for edition 2024 (#9473)\n\n# Description\n\n`check-semver` job fails since some time in the majority of the PRs, and\nthe issue has been tracked down to an indirect dependency which is based\non edition 2024 rust, which can not compile successfully with the\ncurrent nightly.\n\n## Integration\n\nN/A\n\n## Review Notes\n\n- Updated parity-publish:\nhttps://github.com/paritytech/parity-publish/pull/58\n- This PR completes the circle and makes check-semver functional again\n- Tested already these changes here:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/16909276800/workflow\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>",
          "timestamp": "2025-08-13T07:37:51Z",
          "tree_id": "623c90f2a9e3da06be3694e350eafc34b23a00b3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5bb3afcd733a18744a09c1df840c3813623fb0ab"
        },
        "date": 1755074913618,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002652146679999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008571812769999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004935259389999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "29a0c4a746a61a690df06822c52266ef69bf6b64",
          "message": "store headers and justifications during warp sync (#9424)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/2738.\n\nStill need to add tests for this - but I think the easiest way might be\nafter the zombienet tests are converted to Rust, in the warp sync test\nmaybe we can just request the headers (and justifications?) from\nJSON-RPC? Though I'm not sure there is an API for the justifications.\nBut in any case we can in theory make a P2P justifications request as\nwell and the node should be able to respond. Let me know if anybody has\nsome better ideas.\n\n---------\n\nSigned-off-by: sistemd <enntheprogrammer@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Iulian Barbu <14218860+iulianbarbu@users.noreply.github.com>",
          "timestamp": "2025-08-13T14:50:47Z",
          "tree_id": "b2b0080997291071c910a50b9deac8ed488c035f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/29a0c4a746a61a690df06822c52266ef69bf6b64"
        },
        "date": 1755100931870,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00515054584999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00862464901999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026485671299999996,
            "unit": "seconds"
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
          "id": "c8e7a682f5961dd812fde30f9d909b86f16cd54f",
          "message": "cargo: Use rust-yamux version 0.13.6 (#9479)\n\nThis PR updates the litep2p' rust-yamux crate to version 0.13.6.\n\nThis version solves the following issue:\n\n```\n0: sp_panic_handler::set::{{closure}}\n1: std::panicking::rust_panic_with_hook\n2: std::panicking::begin_panic_handler::{{closure}}\n3: std::sys::backtrace::__rust_end_short_backtrace\n4: rust_begin_unwind\n5: core::panicking::panic_fmt\n6: core::slice::index::slice_start_index_len_fail::do_panic::runtime\n7: core::slice::index::slice_start_index_len_fail\n8: <yamux::frame::io::Io as futures_sink::Sink<yamux::frame::Frame<()>>>::poll_ready\n9: yamux::connection::Connection::poll_next_inbound\n10: litep2p::transport::websocket::connection::WebSocketConnection::start::{{closure}}\n11: <litep2p::transport::websocket::WebSocketTransport as litep2p::transport::Transport>::accept::{{closure}}\n12: <tracing_futures::Instrumented as core::future::future::Future>::poll\n13: tokio::runtime::task::raw::poll\n14: tokio::runtime::scheduler::multi_thread::worker::Context::run_task\n15: tokio::runtime::scheduler::multi_thread::worker::run\n16: tokio::runtime::task::raw::poll\n17: std::sys::backtrace::__rust_begin_short_backtrace\n18: core::ops::function::FnOnce::call_once{{vtable.shim}}\n19: std::sys::pal::unix::thread::Thread::new::thread_start\n20: start_thread\nat /build/glibc-FcRMwW/glibc-2.31/nptl/pthread_create.c:477:8\n21: clone\nat /build/glibc-FcRMwW/glibc-2.31/misc/../sysdeps/unix/sysv/linux/x86_64/clone.S:95\n```\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/9169\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-14T08:25:37Z",
          "tree_id": "7a997ec7458a771c3d93301b5c35483cbe458228",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c8e7a682f5961dd812fde30f9d909b86f16cd54f"
        },
        "date": 1755166121546,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008563362489999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026181287600000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005072127819999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "45178695+pkhry@users.noreply.github.com",
            "name": "Pavlo Khrystenko",
            "username": "pkhry"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "fd417de617f303b84a0cac1972cf5d7090000d2f",
          "message": "[pallet-revive] expose `exec::Key` (#9482)\n\n# Description\n\nThis is a fix for the fact that `exec::Key` is exposed from within\n`pallet_revive::tracing::Tracing` interface, but not from the crate\nitself making custom tracers effectively unimplementable outside said\ncrate.\n\nIn my case it's useful for implementing custom tracers for integration\nwith `foundry`\n\n## Integration\n\nRequires no downstream changes\n\n## Review Notes\n\nThis is a fix for the fact that `exec::Key` is exposed from within\n`pallet_revive::tracing::Tracing` interface, but not from the crate\nitself making custom tracers effectively unimplementable outside said\ncrate.\n\nsee here for one of the methods: [`exec::Key` exposed to the\nimplementor, despite not being exported by the\n`crate`](https://github.com/paritytech/polkadot-sdk/blob/pkhry/expose_key_pallet_revive/substrate/frame/revive/src/tracing.rs#L68)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-14T12:06:16Z",
          "tree_id": "989ae132623b6eea42407bc96833a8495c8fef21",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd417de617f303b84a0cac1972cf5d7090000d2f"
        },
        "date": 1755177963515,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027101600699999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008863073949999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00521436059,
            "unit": "seconds"
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
          "id": "e117602f60bf3a0debe6843c94278275e3912a40",
          "message": "Remove free balance check in `prepare_unlock` (#9489)\n\nThe free balance check during unlocking is unnecessary since a lock can\ncover both free and reserved balances. Removing it allows locks to be\ncleared even if part of the locked funds is reserved or already slashed.",
          "timestamp": "2025-08-15T06:33:29Z",
          "tree_id": "44a9e3e9bbdefd13644497674a8362005a86ec67",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e117602f60bf3a0debe6843c94278275e3912a40"
        },
        "date": 1755243905730,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026895273399999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005207769489999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008697579319999984,
            "unit": "seconds"
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
          "id": "ce5ecdd483440557d4d49f55818ed517bdf64940",
          "message": "`fatxpool`: buckets for event-timings metrics adjusted (#9495)\n\nThis PR adjusts the buckets for transactions' event-timings metrics as\nrequested in #9158 for reliability dashboard.\nMetrics were initially introduced in #7355. \n\nfixes: #9158\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-15T14:34:38Z",
          "tree_id": "f9d6c205d02c8530896693bc1f48d6cd90aff2e6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ce5ecdd483440557d4d49f55818ed517bdf64940"
        },
        "date": 1755272724056,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005170542569999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008587653409999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00263213877,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "8744f5e5ee65786e3254cf45c4de54606917effd",
          "message": "[pallet-revive] Move `to_account_id` host function to `System` pre-compile (#9455)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: xermicus <bigcyrill@hotmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-08-18T09:05:56Z",
          "tree_id": "bdf0796810802ecf2bc56112c470efa5a7f72319",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8744f5e5ee65786e3254cf45c4de54606917effd"
        },
        "date": 1755513467096,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002637127670000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008712955419999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005235125019999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "bruno.devic@parity.io",
            "name": "BDevParity",
            "username": "BDevParity"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9899378386f540055b292bcfaf66b98ef2dbe774",
          "message": "[Release|CI/CD] Create pipeline with build runtimes, publish release draft and build RC all in 1 pipeline (#9437)\n\nThis PR incudes the following changes:\n\n- Creates single pipeline containing build RC, build runtimes and\npublish release candidate.\nCloses: https://github.com/paritytech/devops/issues/3828\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-08-18T13:35:55Z",
          "tree_id": "95593c69c5db5e1c3aabd5589581dbe238272b25",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9899378386f540055b292bcfaf66b98ef2dbe774"
        },
        "date": 1755528323637,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005187174989999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026449259499999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008669008949999984,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7ede4fd048f8a99e62ef31050aa2e167e99d54b9",
          "message": "[pallet-revive] Move `blake2_128` host function to `System` pre-compile (#9454)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nI'm splitting some of the host function migrations into separate PRs, as\nthere are sometimes refactorings involved and this should make reviewing\neasier.\n\ncc @athei @pgherveou",
          "timestamp": "2025-08-18T18:40:41Z",
          "tree_id": "a1ee8d9a9483f19af3f1faf1905bef318dc29a79",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ede4fd048f8a99e62ef31050aa2e167e99d54b9"
        },
        "date": 1755546798512,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005112155139999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026036496500000013,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008551858059999994,
            "unit": "seconds"
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
          "id": "7f1949d86d179d82d647a749c34a02b71492f5ff",
          "message": "Paras: Clean up `AuthorizedCodeHash` when offboarding (#9514)\n\nThis PR updates the `Paras` pallet to clear entries in\n`AuthorizedCodeHash` as part of the offboarding process.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-19T15:45:15Z",
          "tree_id": "a974eaac440aa21cf92978876f2af93910cbcd41",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f1949d86d179d82d647a749c34a02b71492f5ff"
        },
        "date": 1755622804062,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261966599,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008589039879999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051881322199999955,
            "unit": "seconds"
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
          "id": "4619e9b6e805d132f4307752270044028273dd11",
          "message": "[revive] move existing files to prepare evm backend introduction (#9501)\n\n- Move exisiting files in pallet-revive to accomodate the upcoming EVM\nbackend\n- Add solc/resolc compilation feature for fixtures\n- Add `fn is_pvm` to later distinguish between pvm / evm bytecode\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-20T05:46:03Z",
          "tree_id": "65fbf1c1d0f6b7c75874c4ea1dd241f439b4c5bd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4619e9b6e805d132f4307752270044028273dd11"
        },
        "date": 1755673572038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027029405,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008631158949999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005138139849999996,
            "unit": "seconds"
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
          "id": "2b56efc4d5be1a4d47b94b496193a764b8a6488b",
          "message": "[CI/CD] Fix build binary flow (#9526)\n\nThis PR fixes build-binary flow, that is used to build a binary for the\ntesting purposes from any branch. The issue was that there were too many\ninput args for the build script.",
          "timestamp": "2025-08-20T08:54:05Z",
          "tree_id": "49dc83fb4cac9dc3541ffb08932745c28fd5880f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2b56efc4d5be1a4d47b94b496193a764b8a6488b"
        },
        "date": 1755685206876,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027178649500000014,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00893629731999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005339076139999995,
            "unit": "seconds"
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
          "id": "349fe9b9111d74b5e4bc3002136b211b3444f28b",
          "message": "fix: add missing crates bumps and upgrade parity-publish (#9488)\n\n# Description\n\nAdds a few crate bumps associated to PRs which missed to bump them, and\nupdates parity-publish version across the board to 0.10.6 (to support\nrustc 1.88).\n\nAdditionally, makes it so that parity-publish-check-compile runs first\non all unreleased prdocs to bump associated crates, and only after\nmoving those to an `unreleased` directory, runs on the current PR's\nprdoc. This is so that we first create a \"local release\" based on the\nunreleased prdocs, and then we follow with a \"patch\" release based on\nthe previous local release, considering only the prdoc pushed with the\ncurrent PR. If the workflow fails at the end it means current PR missed\ncertain bumps. If we don't do the plan/apply twice we risk to miss bumps\ndue to all prdocs being considered (current PR's prdoc + unreleased\nones) when running parity-publish plan/apply, which might result in a\nset of crate bumps which are sufficient, but once some unreleased prdocs\nwill be moved to a stable prodoc directory, because they will be part of\na stable release, then the ones left will not be enough from a bump\nperspective (e.g. like it happened in #9320). That's why it is important\nto check every PR that adds a prdoc whether it is self-sufficient from a\ncrates bumping perspective.\n\nIf no prdoc is provided, the parity-publish does not need to be taken\ninto consideration, but it should also pass nonetheless.\n\n## Integration\n\nN/A\n\n## Review Notes\n\nThere seems to a be a corner case parity-publish can not easily catch.\nAll bumps below are a manifestation of it. More details below:\n\n* #8714 - a major bump is necessary for `sp-wasm-interface` - context\nhere:\nhttps://github.com/paritytech/polkadot-sdk/pull/8714#discussion_r2273355186\n* `sp-keystore` was bumped during 2506 in #6010 , and the relevant prdoc\ngot moved to stable2506 dir in #9320. This moved prdoc coexisted\nalongside other unreleased prdocs, and covered a needed patch bump for\n`sp-keystore`, that is not easily visible, and also required for crates\npublishing IIUC:\n1. `sp-io` is major bumped because its direct dependency,\n`sp-state-machine`, was major bumped.\n2. `sp-io` has a direct dependency on `sp-core` (minor bumped), and\n`sp-keystore` (not touched, not bumped by now)\n3. `sp-io` fails to compile because it pulls same types from different\n`sp-core` versions (it implements `Keystore` trait from `sp-keystore`\nwith methods signatures referencing types from `sp-core 38.0.0` by using\nthe `sp-core 0.38.1` - unreleased yet - types, which confuses rustc).\n* `sp-rpc` needs a bump too due to pulling `sp-core 38.0.0`, like\n`sp-keystore`, and it is an indirect dependency of `polkadot-cli`, which\nhas also a direct dependency on unreleased `sp-core 38.1.0`, so again,\nif we don't bump `sp-rpc` (historically it has been bumped only with\nmajor, but I think we can go with patch on this one), `polkadot-cli`\ncan't compile.\n* `sc-storage-monitor` is in a similar situation as\n`sp-rpc`/`sp-keystore` - `polkadot-cli` depends on `sc-storage-monitor`\n(which is not bumped, and has a dependency on `sp-core 38.0.0`), but it\nalso depends on `sp-core 38.1.0`. And yet again, something is used in\n`polkadot-cli` from the two different `sp-core` versions, which confuses\nrustc.\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-08-20T11:47:46Z",
          "tree_id": "3b32afd34a525f52faa8db9906d3ff42149a8474",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/349fe9b9111d74b5e4bc3002136b211b3444f28b"
        },
        "date": 1755695006083,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0049793718199999965,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026689797200000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008521806289999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hetterich.charles@gmail.com",
            "name": "Charles",
            "username": "charlesHetterich"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "386b3abb72283c2c0efacd0fd2975163b333bce6",
          "message": "Added `subkey` to CI release process (#9466)\n\n- Added 2 jobs to `Release - Build node release candidate` CI workflow\nfor linux/mac subkey binaries\n- Added 2 jobs to `RC Build` CI workflow to upload linux/mac `subkey`\nartifacts to S3\n- updated `release_lib.sh` to reflect new S3 artifacts\n\nCLOSES: #9465\n\n---------\n\nCo-authored-by: EgorPopelyaev <egor@parity.io>",
          "timestamp": "2025-08-20T16:20:41Z",
          "tree_id": "b37735b9bf06f64f0094cfa01debf9393f601c1d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/386b3abb72283c2c0efacd0fd2975163b333bce6"
        },
        "date": 1755711364480,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026707993300000006,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008609825189999979,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051522955899999945,
            "unit": "seconds"
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
          "id": "56d3c42cf4b8b650ae416db0482ad56eb64938c9",
          "message": "EPMB/Signed: Make invulnerables non-eject-able (#9511)\n\nFollow-up to https://github.com/paritytech/polkadot-sdk/pull/8877 and\naudits: Make it such that invulnerable accounts cannot be ejected from\nthe election signed queue altogether.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2025-08-21T08:22:32Z",
          "tree_id": "2d9f9eabd9dddec904ce178c9d3486ceddf00d59",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/56d3c42cf4b8b650ae416db0482ad56eb64938c9"
        },
        "date": 1755768972632,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00259508205,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005002684619999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008450085419999991,
            "unit": "seconds"
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
          "distinct": false,
          "id": "1e4af2353ea9dcb9ad0afc1ce63b03df68108ecf",
          "message": "Replace `log` with `tracing` on `pallet-bridge-relayers` (#9381)\n\nThis PR replaces `log` with `tracing` instrumentation on\n`pallet-bridge-relayers` by providing structured logging.\n\nPartially addresses #9211",
          "timestamp": "2025-08-21T10:16:26Z",
          "tree_id": "227e8c7bb7b7adb486125c662468b3c2894feb3b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1e4af2353ea9dcb9ad0afc1ce63b03df68108ecf"
        },
        "date": 1755775892336,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00857276897999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005090415769999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027136283300000013,
            "unit": "seconds"
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
          "id": "9969e1e81c94f2153412d647d92ecad8db3ccbf8",
          "message": "[Backport] Version bumps and prdoc reordering from stable2506-1 (#9529)\n\nThis PR backport regular version bumps and prdocs reordering from the\nstable2506 branch back to master",
          "timestamp": "2025-08-21T14:46:57Z",
          "tree_id": "1c02f70053ccdced9c6f2f6a599c00d6076584ef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9969e1e81c94f2153412d647d92ecad8db3ccbf8"
        },
        "date": 1755793390852,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261616011,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008538731129999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005075817549999998,
            "unit": "seconds"
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
          "id": "2660bf5f04736beef5c7002ffb5a5856e9420d1a",
          "message": "`polkadot-omni-node`: fixes and changes related to `GetParachainInfo` (#9201)\n\n# Description\n\n- log::info! the error of accessing `GetParachainInfo::parachain_id()`\nruntime api if any, before reading the `para_id` from the chain\nspecification (relevant for debugging).\n- removes comments/deprecation notices throughout the code that\nintroduce `para-id` flag removal (from chain-spec-builder and support\nfor parsing it from chain specifications)\n\nCloses #9217 \n\n## Integration\n\nN/A\n\n## Review Notes\n\nN/A\n\n---------\n\nSigned-off-by: Iulian Barbu <iulian.barbu@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <mail@skunert.dev>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2025-08-22T12:59:14Z",
          "tree_id": "5b83afefc295e7e302e0f1ae5f368c6b316f7ab8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2660bf5f04736beef5c7002ffb5a5856e9420d1a"
        },
        "date": 1755872026349,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027669229499999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008831722099999979,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005237363689999996,
            "unit": "seconds"
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
          "id": "13320f333c00619165c406fdfcb28b6056b543df",
          "message": "align eth-rpc response with geth (#9177)\n\n- Update some serde encoding for eth-rpc to match serialization behavior\nof Geth\n- Add support for serializing / deserializing EIP7702 tx types\n- Disable transaction type we don't support yet in\ntry_ino_unchecked_extrinsics\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-22T14:18:13Z",
          "tree_id": "c37739e4310b85426b09807e257c5ce83e309bb4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/13320f333c00619165c406fdfcb28b6056b543df"
        },
        "date": 1755876913801,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00266481784,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008687426819999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005171359449999992,
            "unit": "seconds"
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
          "id": "2cb8f12822346d3772855be8a9caa25abad21e33",
          "message": "[XCMP] `take_first_concatenated_xcm()` improvements (#9539)\n\nThis PR:\n- improves `take_first_concatenated_xcm()` avoiding the XCM re-encoding\n- makes the benchmarks for `take_first_concatenated_xcm()` more\ngranular, accounting for the number of bytes of the message\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-25T07:57:56Z",
          "tree_id": "c88ecd500c2bea0dfa451ff9a9fa2e26e5364afb",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2cb8f12822346d3772855be8a9caa25abad21e33"
        },
        "date": 1756113555704,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0051421041999999955,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264109233,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008589589109999985,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ismailov.m.h@gmail.com",
            "name": "muharem",
            "username": "muharem"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1a512570552119a49a8ecb2abfb7021954c4422d",
          "message": "Society pallet supports non-consecutive block provider (#9497)\n\nSociety pallet supports non-consecutive block provider\n\nSociety pallet correctly handles situations where `on_initialize` is\ninvoked with block numbers that:\n- increase but are not strictly consecutive (e.g., jump from 5 → 10), or\n- are repeated (e.g., multiple blocks are built at the same Relay Chain\nparent block, all reporting the same BlockNumberProvider value).\n\nThis situation may occur when the BlockNumberProvider is not local - for\nexample, on a parachain using the Relay Chain block number provider.\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-08-25T11:04:31Z",
          "tree_id": "2d5738b63692aa5d082a7286608ad0a8ba3f9bdc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1a512570552119a49a8ecb2abfb7021954c4422d"
        },
        "date": 1756124852007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008596276549999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005156241139999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026511979999999987,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "evgeny@parity.io",
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3f9231dc75346c65826e70112ecc1a3a507e187f",
          "message": "tests-linux-stable cattery wf (#9041)\n\ncc https://github.com/paritytech/devops/issues/3875\n\n---------\n\nCo-authored-by: alvicsam <alvicsam@gmail.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-08-26T14:52:31Z",
          "tree_id": "d5e89c8b65bc6930c3c9dfefbc7aedf231e212b6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3f9231dc75346c65826e70112ecc1a3a507e187f"
        },
        "date": 1756224646952,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025754759999999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005060706459999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00847520664999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dcd9cacf40b282ea1fb9870e29e0eec8fbfd1c88",
          "message": "track authorities from aura digests (#9272)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/9064.\n\nTracks AURA authorities in a `ForkTree`. The fork tree is updated\nwhenever there is an authorities change log in the digest. If the fork\ntree doesn't contain the authorities, they are fetched for the runtime\n(should only happen at startup, or if something weird is going on with\nforks maybe).\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-26T17:14:45Z",
          "tree_id": "92913a06102ac8246dd730837e5c8598925b9de5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dcd9cacf40b282ea1fb9870e29e0eec8fbfd1c88"
        },
        "date": 1756233043909,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002630948750000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005059338579999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008461669909999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "vrom911@gmail.com",
            "name": "Veronika Romashkina",
            "username": "vrom911"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0c51d2e259d1742a809d11fecbeb663033726846",
          "message": "Improve omni-node installation docs (#9555)\n\n# Description\n\nWhile following the `polkadot-omni-node` installation section\ninstructions [here](https://crates.io/crates/polkadot-omni-node), I\nfound that it could be improved a bit.\n\nThe `<stable_release_tag>` should be replaced with the release tag, but\nthere is no mention of how to get that tag fast.\nI added this information as a note in addition to the existing line.\n\nCo-authored-by: Raymond Cheung <178801527+raymondkfcheung@users.noreply.github.com>",
          "timestamp": "2025-08-27T02:24:11Z",
          "tree_id": "71a0907d4ce3e8cd70743ddc7c830028a1e0bdd3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0c51d2e259d1742a809d11fecbeb663033726846"
        },
        "date": 1756265925761,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0050568395999999955,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008486870399999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264166529,
            "unit": "seconds"
          }
        ]
      },
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
          "distinct": true,
          "id": "930d4ca1b82fa52681f9607360a690506b277b54",
          "message": "Fix regression benchmarks (#9044)\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-08-27T12:54:27Z",
          "tree_id": "d15d13bb0172be70338d2f687eb2985e65f8e78c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/930d4ca1b82fa52681f9607360a690506b277b54"
        },
        "date": 1756304259145,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026435064999999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008546927909999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005041544189999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "hs+github@haikoschol.com",
            "name": "Haiko Schol",
            "username": "haikoschol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "b7b7f0c50f6ce8bad7a7a3a10139e53714740b4e",
          "message": "Cumulus: Remove `--relay-chain-light-client` (#9446)\n\n# Description\n\nThis PR removes the experimental flag `--relay-chain-light-client` from\ncumulus and as a consequence, smoldot and smoldot-light as workspace\ndependencies.\n\nCloses #9013 \n\n## Integration\n\nSince this PR changes the public API of\n[cumulus-relay-chain-rpc-interface](https://crates.io/crates/cumulus-relay-chain-rpc-interface),\nit affects node developers and the PR should include a prdoc file. Since\nthe crate is not v1 yet, I reckon prdoc should include `bump: minor`.",
          "timestamp": "2025-08-27T13:55:11Z",
          "tree_id": "337ee44b8294a43c32b5e779850b81585ab362b1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b7b7f0c50f6ce8bad7a7a3a10139e53714740b4e"
        },
        "date": 1756307429349,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005301035349999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00271739355,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008810461939999983,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "3dfbdf4a454f35238500779e503e1ec32ba7fc63",
          "message": "Parachains runtime: properly filter backed candidate votes (#9564)\n\nThe `filter_backed_statements_from_disabled_validators` function does\nnot properly map indices in the validator group to indices in the\nvalidity votes vec. This PR fixes that.\n\nTODO: \n- [x] add more tests\n- [x] PRDoc\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-27T19:52:12Z",
          "tree_id": "52879c2b0806e93ea47828178932bd130f45092c",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3dfbdf4a454f35238500779e503e1ec32ba7fc63"
        },
        "date": 1756328633990,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00859684265999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265650239,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005214589819999992,
            "unit": "seconds"
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
          "distinct": true,
          "id": "c1a31e3505c0c4e01b9d2daad5f4d19b220345ec",
          "message": "Disable reserve_asset_transfer for DOT (#9544)\n\n- [x] Add check to `do_reserve_asset_transfer`\n- [x] Modify existing tests\n- [ ] Add new tests if needed\n\n---------\n\nCo-authored-by: Karol Kokoszka <karol@parity.io>",
          "timestamp": "2025-08-28T21:31:06Z",
          "tree_id": "1e246b2a94ebc81065a2bdcf7b7fcc4f3851c436",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c1a31e3505c0c4e01b9d2daad5f4d19b220345ec"
        },
        "date": 1756421286406,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008533148469999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026012786699999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005039181619999996,
            "unit": "seconds"
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
          "id": "f87d061a195ff40d7e91b00c8c1e40a75140c2cb",
          "message": "Snowbridge Westend runtime config cleanup (#9547)\n\nMinor cleanup to match Polkadot runtime config.",
          "timestamp": "2025-08-29T08:34:30Z",
          "tree_id": "1e8fe29aa15813b490bbc10bab20d60c7198d865",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f87d061a195ff40d7e91b00c8c1e40a75140c2cb"
        },
        "date": 1756460771305,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027465320299999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00917544285999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005346878929999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "ismailov.m.h@gmail.com",
            "name": "muharem",
            "username": "muharem"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "27519874677950b3cb8a3aea4116bbdfcbb69a22",
          "message": "Society pallet: Make fields of storage-persisted types public (#9604)\n\nSociety pallet: Make fields of storage-persisted types public.\n\nFields of types persisted in storage have been made public.",
          "timestamp": "2025-08-29T13:21:51Z",
          "tree_id": "961e5ef109e6333061079cc1c61094ba014c326e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/27519874677950b3cb8a3aea4116bbdfcbb69a22"
        },
        "date": 1756478096236,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0028113817199999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005173883769999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008699103189999993,
            "unit": "seconds"
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
          "id": "33819101a1a465d31303b9e97d55b24e0d6902a3",
          "message": "Fix `check_hrmp_message_metadata()` (#9602)\n\nWe need to update `maybe_prev_msg_metadata` inside\n`check_hrmp_message_metadata()`\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-08-29T14:33:15Z",
          "tree_id": "c8fbda75bdd1620aaead47bebd7a70da1a6ca53f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33819101a1a465d31303b9e97d55b24e0d6902a3"
        },
        "date": 1756482372626,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005114362339999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026532432099999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008639690849999982,
            "unit": "seconds"
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
          "id": "40cd38db9e3a758e80af28ba2aa9f6420173ee65",
          "message": "[revive] revm backend (#9285)\n\n# EVM initial support  for pallet-revive\n\nInitial EVM support via the REVM crate to create a dual-VM system that\ncan execute both PolkaVM and EVM\n\n- Added `AllowEVMBytecode: Get<bool>` to the config to enable/disable\nEVM call and instantiation\n- CodeInfo has been updated to add the type of bytecode (EVM / PVM).\n`migration/v2.rs` takes care of migrating existing storages\n- The CodeUploadDeposit is not held by a pallet account instead of being\nheld on the uploader, It's automatically refunded when the refcount\ndrops to 0 and the code is removed.\n- The basic flow of uploading an EVM contract and running it should work\n- instructions are copied and adapted from REVM they should be ignored\nin this PR and reviewed in follow-up PR\n(**reviewers** please ignore\n`substrate/frame/revive/src/vm/evm/instructions/*` for now)\n\n## Implementation Guidelines\n\n### Basic Instruction Structure\nA basic instruction looks like this:\n\n```rust\npub fn coinbase<'ext, E: Ext>(context: Context<'_, 'ext, E>) {\n\tgas_legacy!(context.interpreter, revm_gas::BASE);\n\tpush!(context.interpreter, context.host.beneficiary().into_word().into());\n}\n```\n\n### Required Changes for REVM Instructions\n\nAll instructions have been copied from `REVM` and updated with generic\ntypes for pallet-revive. Two main changes are required:\n\n#### 1. Gas Handling\nReplace REVM gas calls with existing benchmarks where available:\n\n```diff\n- gas_legacy!(context.interpreter, revm_gas::BASE);\n+ gas!(context.interpreter, RuntimeCosts::BlockAuthor);\n```\n\n#### 2. Context Access\nReplace `context.host` calls with `context.extend` (set to `&mut Ext`):\n\n```diff\n- push!(context.interpreter, context.host.beneficiary().into_word().into());\n+ let coinbase: Address = context.interpreter.extend.block_author().unwrap_or_default().0.into();\n+ push!(context.interpreter, coinbase.into_word().into());\n```\n\n### Gas Benchmarking Notes\n- For cases without existing benchmarks (e.g arithmetic, bitwise) , we\nwill keep `gas_legacy!`\n- The u64 gas value are multiplied by a base cost benchmarked by\n`evm_opcode`\n\n- ### Important Rules\n- All calls to `context.host` should be removed (initialized to default\nvalues)\n- All calls to `context.interpreter.gas` should be removed (except\n`gas.memory` handled by `resize_memory!` macro)\n- See `block_number` implementation as a reference example\n\nThe following instructions in src/vm/evm/instructions/** need to be\nupdated\n\n### Basic Instructions\n\nWe probably don't need to touch these implementations here, they use the\ngas_legacy! macro to charge a low gas value that will be scaled with our\ngas_to_weight benchmark. The only thing needed here are tests that\nexercise these instructions\n\n<details>\n\n#### Arithmetic Instructions\n\n- [ ] **add**\n- [ ] **mul**\n- [ ] **sub**\n- [ ] **div**\n- [ ] **sdiv**\n- [ ] **rem**\n- [ ] **smod**\n- [ ] **addmod**\n- [ ] **mulmod**\n- [ ] **exp**\n- [ ] **signextend**\n\n#### Bitwise Instructions\n\n- [ ] **lt**\n- [ ] **gt**\n- [ ] **slt**\n- [ ] **sgt**\n- [ ] **eq**\n- [ ] **iszero**\n- [ ] **bitand**\n- [ ] **bitor**\n- [ ] **bitxor**\n- [ ] **not**\n- [ ] **byte**\n- [ ] **shl**\n- [ ] **shr**\n- [ ] **sar**\n- [ ] **clz**\n\n#### Control Flow Instructions\n\n- [ ] **jump**\n- [ ] **jumpi**\n- [ ] **jumpdest**\n- [ ] **pc**\n- [ ] **stop**\n- [ ] **ret**\n- [ ] **revert**\n- [ ] **invalid**\n\n### Memory Instructions\n- [ ] **mload**\n- [ ] **mstore**\n- [ ] **mstore8**\n- [ ] **msize**\n- [ ] **mcopy**\n\n#### Stack Instructions\n- [ ] **pop**\n- [ ] **push0**\n- [ ] **push**\n- [ ] **dup**\n- [ ] **swap**\n\n</details>\n\n### Sys calls instructions\n\nThese instructions should be updated from using gas_legacy! to gas! with\nthe appropriate RuntimeCost, the returned value need to be pulled from\nour `&mut Ext` ctx.interpreter.extend instead of the host or input\ncontext value\n\n<details>\n\n#### Block Info Instructions\n\n- [x] **block_number**\n- [ ] **coinbase**\n- [ ] **timestamp**\n- [ ] **difficulty**\n- [ ] **gaslimit**\n- [ ] **chainid**\n- [ ] **basefee**\n- [ ] **blob_basefee**\n\n#### Host Instructions\n\n- [ ] **balance**\n- [ ] **extcodesize**\n- [ ] **extcodecopy**\n- [ ] **extcodehash**\n- [ ] **blockhash**\n- [ ] **sload**\n- [ ] **sstore**\n- [ ] **tload**\n- [ ] **tstore**\n- [ ] **log**\n- [ ] **selfdestruct**\n- [ ] **selfbalance**\n\n#### System Instructions\n- [ ] **keccak256**\n- [ ] **address**\n- [ ] **caller**\n- [ ] **callvalue**\n- [ ] **calldataload**\n- [ ] **calldatasize**\n- [ ] **calldatacopy**\n- [ ] **codesize**\n- [ ] **codecopy**\n- [ ] **returndatasize**\n- [ ] **returndatacopy**\n- [ ] **gas**\n\n#### Transaction Info Instructions\n- [ ] **origin**\n- [ ] **gasprice**\n- [ ] **blob_hash**\n\n</details>\n\n### Contract Instructions\n\nThese instructions should be updated,, that's where I expect the most\ncode change in the instruction implementation.\nSee how it's done in vm/pvm module, the final result should look pretty\nsimilar to what we are doing there with the addition of custom gas_limit\ncalculation that works with our gas model.\n\nsee also example code here https://github.com/paritytech/revm_example\n\n<details>\n\n- [ ] **create**\n- [ ] **create**\n- [ ] **call**\n- [ ] **call_code**\n- [ ] **delegate_call**\n- [ ] **static_call**\n\n</details>\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: xermicus <cyrill@parity.io>\nCo-authored-by: 0xRVE <robertvaneerdewijk@gmail.com>\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>",
          "timestamp": "2025-09-01T09:50:02Z",
          "tree_id": "d9b8743fb3951843e2e35b32fbb2ebaedd43cbef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/40cd38db9e3a758e80af28ba2aa9f6420173ee65"
        },
        "date": 1756724436120,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027473636199999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008932647299999983,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005382658569999991,
            "unit": "seconds"
          }
        ]
      },
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
          "distinct": true,
          "id": "44416758c410cad2c7c2adee09c18f99b1f92d02",
          "message": "[pallet-revive] Expose `AccountInfo` and `ContractInfo` in the public interface (#9606)\n\n# Description\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/9553\nSee https://github.com/paritytech/foundry-polkadot/issues/276\n\nExposes revive types to use in foundry-polkadot project.\n\n## Integration\n\nShould not affect downstream projects.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-01T11:45:58Z",
          "tree_id": "28f5c5c2c78d69f2bc31a51ad855fe79a9d5a4a0",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/44416758c410cad2c7c2adee09c18f99b1f92d02"
        },
        "date": 1756731906983,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008667930759999984,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002675982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005317243989999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "s.miasojed@gmail.com",
            "name": "Sebastian Miasojed",
            "username": "smiasojed"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f9ef0f34358d1f593776201b2728f2817120b424",
          "message": "[pallet-revive] EVM backend: Implement tx, block system and call stack instructions (#9414)\n\nThis PR is part of the road to EVM.\n- Implement call and create frames, allowing to call and instantiate\nother contracts.\n- Implement support for tx info, block info, system and contract\nopcodes.\n- The `InstructionResult` <-> `ExecError` conversion functions.\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: pgherveou <pgherveou@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: xermicus <cyrill@parity.io>\nCo-authored-by: 0xRVE <robertvaneerdewijk@gmail.com>\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: Cyrill Leutwiler <bigcyrill@hotmail.com>",
          "timestamp": "2025-09-01T17:06:49Z",
          "tree_id": "5f299b1a945c67b32282e425f5dba3d2933e6c15",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f9ef0f34358d1f593776201b2728f2817120b424"
        },
        "date": 1756750750551,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027845077100000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008967587819999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005302270829999999,
            "unit": "seconds"
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
          "id": "c5b4afcaea03367ff56062834fbe258489e74fa1",
          "message": "[pallet-revive] Update genesis config (#9557)\n\nUpdate pallet-revive Genesis config\nMake it possible to define accounts (contracts or EOA) that we want to\nsetup at Genesis\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nSigned-off-by: xermicus <cyrill@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: xermicus <cyrill@parity.io>\nCo-authored-by: 0xRVE <robertvaneerdewijk@gmail.com>\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>",
          "timestamp": "2025-09-02T07:40:25Z",
          "tree_id": "6df31dbaa2e64663ba4d7c118e8872f85e26a68a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c5b4afcaea03367ff56062834fbe258489e74fa1"
        },
        "date": 1756803438515,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026218687600000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008600505969999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050286760599999895,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7753112a1b6aae323af71e8904fbab02fdc73c22",
          "message": "Call SingleBlockMigrations from frame_system::Config on try_on_runtime_upgrade (#9451)\n\nRecently, when moving the single block migrations from\n`frame_executive::Executive` to `SingleBlockMigrations` in\n`frame_system::Config`, I noticed that `try_runtime_upgrade` was\nignoring the `SingleBlockMigrations` defined in frame_system. More\ncontext at https://github.com/polkadot-fellows/runtimes/pull/844\n\nBased on PR https://github.com/paritytech/polkadot-sdk/pull/1781 and\n[PRDoc](https://github.com/paritytech/polkadot-sdk/blob/beb9030b249cc078b3955232074a8495e7e0302a/prdoc/1.9.0/pr_1781.prdoc#L29),\nthe new way for providing the single block migrations should be through\n`SingleBlockMigrations` in `frame_system::Config`. Providing them from\n`frame_executive::Executive` is still supported, but from what I\nunderstood is or will be deprecated.\n\n> `SingleBlockMigrations` this is the new way of configuring migrations\nthat run in a single block. Previously they were defined as last generic\nargument of Executive. This shift is brings all central configuration\nabout migrations closer into view of the developer (migrations that are\nconfigured in Executive will still work for now but is deprecated).\n\n## Follow-up Changes\nWill try to open a pull request tomorrow for deprecating the use of\n`OnRuntimeUpgrade` in `frame_executive::Executive`.",
          "timestamp": "2025-09-02T10:47:13Z",
          "tree_id": "e461504342bb3fa2c0f5be604e7139194938f873",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7753112a1b6aae323af71e8904fbab02fdc73c22"
        },
        "date": 1756814386821,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002626955919999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00863575184999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005191717539999991,
            "unit": "seconds"
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
          "id": "12b5b37b8cd1cef19d01679dc70b933d0d80ba68",
          "message": "bump zombienet-sdk and subxt versions (#9587)\n\nReplace https://github.com/paritytech/polkadot-sdk/pull/9506\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2025-09-02T16:31:43+02:00",
          "tree_id": "8485bbe198b710fdce8c03ca9792b14f10af7cde",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12b5b37b8cd1cef19d01679dc70b933d0d80ba68"
        },
        "date": 1756825593344,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0051516472099999945,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026947802200000008,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008627373869999982,
            "unit": "seconds"
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
          "id": "6f9236b9b6827150a366f5c7b1e4e9cd523594e0",
          "message": "basic-authorship: end_reason improved (#9550)\n\nThe `end_reason` reported in block authoring can be\n[misleading](https://github.com/paritytech/polkadot-sdk/issues/9188#issuecomment-3070697415)\nwhen resource limits are hit. The basic authorship module tries\nadditional transactions after hitting limits, and if it runs out of\ntransactions or time during this extended trial phase, it reports the\ninaccurate reason.\n\nMisleading scenarios are:\n- Scenario 1: Resource limit masked by `NoMoreTransactions`\n1. Block hits weight/size limit -> should report\n`HitBlockWeightLimit`/`HitBlockSizeLimit`\n      2. Code tries up to `MAX_SKIPPED_TRANSACTIONS`,\n      3. If still before soft deadline, continues trying transactions,\n      4. Transaction pool runs out during this extended trial phase,\n5. Reason reported is: `NoMoreTransactions`, while the reality is that\nblock was _resource-constrained_, not _mp-transactions-constrained_.\n- Scenario 2: Resource limit masked by `HitDeadline`\n1. Block hits weight/size limit, (let's assume 100k fat transactions in\nthe pool)\n2. Code keeps trying transactions that can't fit due to weight\nconstraints\n      3. Deadline is reached while trying pool transactions\n4. Reason reported is: `HitDeadline`, while the reality is that block\nwas _resource-constrained_, not _time-constrained_.\n\nThis PR proposes to change the actual `end_reason` to be more accurate.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-02T19:28:10Z",
          "tree_id": "0edd6bb34ea90352d1cd8ca0e8fcbd980aa2e476",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6f9236b9b6827150a366f5c7b1e4e9cd523594e0"
        },
        "date": 1756846162489,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026375205400000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00519482856999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008714793319999987,
            "unit": "seconds"
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
          "id": "b8a05717efc3f1b730321554ba603c1bfe71a4cb",
          "message": "Improve Penpal with async backing (#9293)",
          "timestamp": "2025-09-02T21:35:13Z",
          "tree_id": "02ce070e5eaea90e01fe60f36dc2b24a2a9f6b28",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b8a05717efc3f1b730321554ba603c1bfe71a4cb"
        },
        "date": 1756853536118,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005045563899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262450306,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008532729369999987,
            "unit": "seconds"
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
          "id": "3f8534ee18967c1169176d29944b257537a7cbad",
          "message": "ci: try experimental runners (#9618)\n\ncc https://github.com/paritytech/devops/issues/3875",
          "timestamp": "2025-09-03T07:53:07Z",
          "tree_id": "24fd87249822204aa03b8c70cca28ec71b1beff5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3f8534ee18967c1169176d29944b257537a7cbad"
        },
        "date": 1756891931376,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026102610500000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008596710449999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005088720019999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "enntheprogrammer@gmail.com",
            "name": "sistemd",
            "username": "sistemd"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "75173f8c55e7f2d83c545397700576b58bcd92e5",
          "message": "fix: parachain informant (#9581)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/9559.\n\nThe parachain informant was logging information for all parachains, not\njust ours. This PR fixes that by filtering the events by parachain ID.\n\nI tried adding a zombienet test for this but there isn't really a good\nway to do it. So I ended up only testing manually with zombienet, by\ncreating a network of two parachains and adding some extra logging to\nensure that the events are now being filtered out correctly.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-03T10:49:02Z",
          "tree_id": "35a2d6e8bcf1927302d16a9fd799a492610fa67d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/75173f8c55e7f2d83c545397700576b58bcd92e5"
        },
        "date": 1756901013202,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005292480809999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027444798600000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008884965099999977,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Sajjon@users.noreply.github.com",
            "name": "Alexander Cyon",
            "username": "Sajjon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "63958c454643ddafdde8be17af5334aa95954550",
          "message": "move released primitives and APIs out of staging (#9443)\n\nSolves https://github.com/paritytech/polkadot-sdk/issues/9400\n\nNo logic change, only moves types from\n`polkadot/primitives/src/vstaging` into `polkadot/primitives/src/v9`\n(renamed from `v8` to `v9`).\n\n---------\n\nCo-authored-by: Alexander Cyon <alex.cyon@parity.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Dmitry Sinyavin <dmitry.sinyavin@parity.io>\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>",
          "timestamp": "2025-09-03T20:30:44Z",
          "tree_id": "e657de1eac98014fd24bc497703ad0c8e5c9d974",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/63958c454643ddafdde8be17af5334aa95954550"
        },
        "date": 1756936031606,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0050210471999999964,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008488733189999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261409081,
            "unit": "seconds"
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
          "id": "f9efa67cf05d0a2404605c391ac3858c3c9bf6b8",
          "message": "Account for PoV size when enqueing XCMP message (#9641)\n\nRelated to https://github.com/paritytech/polkadot-sdk/pull/9630 , but\nadjusting the benchmark\n\nUsing `#[benchmark(pov_mode = Measured)]` for the\n`enqueue_empty_xcmp_message_at` benchmark.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-04T13:04:52Z",
          "tree_id": "2f6bde86497d3978a82f49bd9cd87396f44f6a05",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f9efa67cf05d0a2404605c391ac3858c3c9bf6b8"
        },
        "date": 1756995634862,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026884264,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008784454309999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005302906709999996,
            "unit": "seconds"
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
          "id": "fa417f9fde23634d6157b928ddfe9d0b19299d57",
          "message": "Update `kvdb-rocksdb` to `v0.20.0` (#9644)\n\nRelated to https://github.com/paritytech/parity-common/issues/932\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-04T20:05:17Z",
          "tree_id": "1cac64dbbcde0f916daa42d8cede3e716dae0a72",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fa417f9fde23634d6157b928ddfe9d0b19299d57"
        },
        "date": 1757021238569,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005195809549999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008629009099999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002654404889999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "5314442a060f53391c5d8d1ece4937332dfa9fc9",
          "message": "staking-async: implement lazy era pruning extrinsic (#9632)\n\nMove era pruning from automatic unbounded deletions to a permissionless\nlazy pruning system.\n\nFix https://github.com/paritytech-secops/srlabs_findings/issues/528.\n\n\n## Changes:\n- Add `prune_era_step extrinsic` for permissionless era maintenance\n- Add `PruningStep` enum and `EraPruningState` storage for tracking\nprogress\n- Implement `do_prune_era_step()` with item/weight-based deletion limits\n- Remove automatic pruning to prevent DoS from unbounded operations\n- Add `MaxPruningItems` Runtime parameter for safe incremental deletions\n- Return `Pays::No` when work is done to incentivize regular maintenance\n- Add `EraNotPrunable` error for proper validation\n- Update benchmarking to test new extrinsic-based approach\n- Update tests to account for manual pruning instead of automatic\ncleanup\n\nThe new system processes era pruning across multiple blocks using a\nstate machine pattern, ensuring storage operations remain bounded and\npreventing potential DoS attacks from large era cleanup operations.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-05T08:29:51Z",
          "tree_id": "54de2b3e6dd6844c873e56f81a6335492bd4fd3a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5314442a060f53391c5d8d1ece4937332dfa9fc9"
        },
        "date": 1757065568862,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005108399689999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008572927689999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002649939860000001,
            "unit": "seconds"
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
          "id": "4acb964059a218be9bac954b4e3803b78b5526bf",
          "message": "Forward `CoreInfo` via an digest to the runtime (#9002)\n\nBefore this pull request we had this rather inflexible `SelectCore` type\nin `parachain-system`. It was just taking the last byte of the block\nnumber as the core selector. This resulted in issues like #8893. While\nit was not totally static, it was very complicated to forward the needed\ninformation to the runtime. In the case of running with block bundling\n(500ms blocks), multiple blocks are actually validated on the same core.\nFinding out the selector and offset without having access to the claim\nqueue is rather hard. The claim queue could be forwarded to the runtime,\nbut it would waste POV size as we would need to include the entire claim\nqueue of all parachains.\n\nThis pull request solves the problem by moving the entire core selection\nto the collator side. From there the information is passed via a\n`PreRuntime` digest to the runtime. The `CoreInfo` contains the\n`selector`, `claim_queue_offset` and `number_of_cores`. Doing this on\nthe collator side is fine as long as we don't have slot durations that\nare lower than the relay chain slot duration. As we have agreed to\nalways have equal or bigger slot durations on parachains, there should\nbe no problem with this change.\n\nDownstream users need to remove the `SelectCore` type from the\n`parachain_system::Config`:\n```diff\n- type SelectCore = ...;\n+\n```\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/8893\nhttps://github.com/paritytech/polkadot-sdk/issues/8906\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2025-09-05T15:26:49Z",
          "tree_id": "edd4ea3b225223f31c9ab6550f848bf6d9254b3e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4acb964059a218be9bac954b4e3803b78b5526bf"
        },
        "date": 1757090410140,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026135060699999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005069274199999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008580861539999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c8c7dba4030fba2d7504c47c40d654c9954fe3d7",
          "message": "staking-async: prevent manual application of cancelled slashes (#9659)\n\nFix security vulnerability where the permissionless `apply_slash`\nextrinsic could be used to manually apply slashes that governance had\ncancelled via `cancel_deferred_slash`.\n\nRelated issue:\nhttps://github.com/paritytech-secops/srlabs_findings/issues/563\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-07T06:12:34Z",
          "tree_id": "d8fe36db80cd2964ec9577b6d61bc29340fdc5fc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c8c7dba4030fba2d7504c47c40d654c9954fe3d7"
        },
        "date": 1757230380401,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026519693,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008649197579999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051090713699999905,
            "unit": "seconds"
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
          "id": "a93a489a3c072eb010a700c4b5033ba4fda1e9cc",
          "message": "basic-authorship: Improve inherent logging (#9664)\n\nThis PR aims to improve the inherent logging situation a bit. After the\nrecent incident it was unnecessary painful to figure out what exactly\nhappened. The logs should now be a bit more clear.\n\n- We get how many inherents where provided by the runtime\n- We get the names of the data items\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Rodrigo Quelhas <22591718+RomarQ@users.noreply.github.com>",
          "timestamp": "2025-09-08T08:12:54Z",
          "tree_id": "8095002d2ba128cb750293735dce9759a2d875ba",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a93a489a3c072eb010a700c4b5033ba4fda1e9cc"
        },
        "date": 1757323854793,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00866026708999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002626662610000001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005189583019999993,
            "unit": "seconds"
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
          "id": "644f14fc86a4bd4ca2edec922c5e617103fcf387",
          "message": "zombienet test with timeout (#9168)\n\nI added timeout for async operation in the statement store zombienet\ntest.\n\n@lrubasze\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-08T10:46:46Z",
          "tree_id": "47a2773f5740a1cb9294a782df960cc139aa113a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/644f14fc86a4bd4ca2edec922c5e617103fcf387"
        },
        "date": 1757332932678,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008955117769999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005326488239999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027034482100000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "acac0127168dac1d603e4d996cb210ceeddeb5de",
          "message": "[pallet-revive] EVM backend: implement various missing opcodes (#9385)\n\n* [x] system (other PR, no tests)\n* [x] block_info (other PR)\n* [x] contract (other PR)\n* [x] tx_info (other PR)\n* [x] arithmetic\n* [x] bitwise\n* [x] i256 (these are not opcodes so will not test)\n* [x] host (except `log()`)\n* [x] memory\n* [x] control (except `pc()`)\n* [x] macros (these are not opcodes so will not test)\n* [x] utility (these are not opcodes so will not test)\n* [x] stack\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: pgherveou <pgherveou@gmail.com>\nCo-authored-by: Sebastian Miasojed <sebastian.miasojed@parity.io>\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Sebastian Miasojed <s.miasojed@gmail.com>\nCo-authored-by: xermicus <cyrill@parity.io>\nCo-authored-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: Alexander Cyon <Sajjon@users.noreply.github.com>\nCo-authored-by: Alexander Cyon <alex.cyon@parity.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Dmitry Sinyavin <dmitry.sinyavin@parity.io>\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>\nCo-authored-by: Serban Iorga <serban@parity.io>",
          "timestamp": "2025-09-08T15:00:45Z",
          "tree_id": "51fe767884f5f0db6533148e553a06efa236a772",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/acac0127168dac1d603e4d996cb210ceeddeb5de"
        },
        "date": 1757348180407,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008805108469999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026229994700000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0052443190899999935,
            "unit": "seconds"
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
          "id": "4231722827e525fc7c794f05f712bb158fa43efb",
          "message": "[backport] Regular version bumps from the stable2506-2 (#9676)\n\nThis PR backport regular version bumps from the stable release branch\n`stabl2506` back to `master`",
          "timestamp": "2025-09-09T05:50:49Z",
          "tree_id": "12e66ffcd337a3e943322fb324c2fd1039b5cb13",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4231722827e525fc7c794f05f712bb158fa43efb"
        },
        "date": 1757401889626,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0050572096599999904,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026128258200000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008564688119999995,
            "unit": "seconds"
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
          "id": "e6166154ef71ac37434515630e1a9e268eff43f2",
          "message": "[Release|CI/CD] Fix macos build in release pipeline (#9682)\n\nCurrent release flow that prepares binaries for the RC fails on the\nbuild for the macos. Due to missing `llvm` library on the runner.\nThis PR fixes this issue\n\nCloses: https://github.com/paritytech/release-engineering/issues/271\n\n---------\n\nCo-authored-by: Bruno Devic <bruno.devic@parity.io>",
          "timestamp": "2025-09-09T10:54:03Z",
          "tree_id": "a19b489a02405c641f95f7846085ab5294c35c0d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6166154ef71ac37434515630e1a9e268eff43f2"
        },
        "date": 1757419767431,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005084047119999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008608352749999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026875303400000004,
            "unit": "seconds"
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
          "id": "ae7177e0d2f99879cb0d91a589ac7f202d39e192",
          "message": "ci: update forklift in ci image (#9684)\n\ncc https://github.com/paritytech/devops/issues/3875",
          "timestamp": "2025-09-09T13:13:44Z",
          "tree_id": "391c83adce9ac0a105cee4738b52dcd62c50acfa",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ae7177e0d2f99879cb0d91a589ac7f202d39e192"
        },
        "date": 1757429120975,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008668393689999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005206891859999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262719769,
            "unit": "seconds"
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
          "id": "6060499f9a807406e13449561b0fc603d9aaeedc",
          "message": "[pallet-revive] fix GAS_PRICE (#9679)\n\nCurrently submitting a transactio to the dev-node or kitchensink will\ntrigger an error when you try to submit a transaction trhough cast (or\nanything using alloy) as the block gas limit on these runtime is greater\nthan u64::max.\n\nThis bump the GAS_PRICE to fix this issue, this will eventually be\nsuperseeded by the new gas model\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-09-10T10:22:50Z",
          "tree_id": "829d84997fde6b92f6d6fc17397ba67c00137f08",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6060499f9a807406e13449561b0fc603d9aaeedc"
        },
        "date": 1757504785659,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026264377199999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871048166999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005200673999999994,
            "unit": "seconds"
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
          "id": "05672500f639411e10a122d95baafec537603294",
          "message": "[Release|CI/CD] Fix release flows (#9700)\n\nThis PR contains few fixes for the release flows:\n- delete debug lines\n- added installation of the `solc` and `resolc` for the`\npolkadot-omni-node` macos build\n- fixed destination repo for the release draft creation\n- notification about the draft release waits now till all the\npublication jobs are done",
          "timestamp": "2025-09-10T13:36:41Z",
          "tree_id": "1dd31d208ce072db1a9710124dd23dc8a4c180e6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/05672500f639411e10a122d95baafec537603294"
        },
        "date": 1757516086441,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025994244099999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008580308509999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005127448699999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7af791f7594c61a97c31841c681666461e300563",
          "message": "Update elastic scaling documentation (#9677)\n\nCloses https://github.com/paritytech/polkadot-sdk/pull/9677 \n\nAdd docs and remove MVP.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Cyon <Sajjon@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Alin Dima <alin@parity.io>",
          "timestamp": "2025-09-10T16:53:52Z",
          "tree_id": "2fab67ef4a232a7d5c9b37b1fc956511afee6064",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7af791f7594c61a97c31841c681666461e300563"
        },
        "date": 1757527612318,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008729502499999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005209065309999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026479311600000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "e99f93b24b1d8a568db46791463e03b46c2c0c7f",
          "message": "Cumulus: adjust authorship duration (#9703)\n\nFor elastic scaling usecases with more than 3 cores we need to ensure\nblock authorship ends before the next block is supposed to be built.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-10T19:17:22Z",
          "tree_id": "b99a1653d67454022234437f908705f2c468d44a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e99f93b24b1d8a568db46791463e03b46c2c0c7f"
        },
        "date": 1757536182028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00268103435,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008671967849999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005197657649999994,
            "unit": "seconds"
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
          "id": "2b4fbe61c8609804c72157eaccd99ef440d1cde6",
          "message": "DB: Ensure that when we revert blocks, we actually delete all their data (#9691)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-10T20:41:42Z",
          "tree_id": "60a6de7c99f92c999fbcdac0f37c9dad715b9778",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2b4fbe61c8609804c72157eaccd99ef440d1cde6"
        },
        "date": 1757541254524,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008695687099999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026387901499999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005246050329999991,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "11329616+Klapeyron@users.noreply.github.com",
            "name": "Klapeyron",
            "username": "Klapeyron"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1d4e9ec206ef417724c23c82ac94de5d24599173",
          "message": "Extend AppSignature trait with Signature (#9645)\n\n[sp_application_crypto::AppPublic](https://docs.rs/sp-application-crypto/latest/sp_application_crypto/trait.AppPublic.html)\nrequires\n[sp_core::crypto::Public](https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Public.html):\n```rust\n/// Application-specific public key.\npub trait AppPublic: AppCrypto + Public + Debug + MaybeHash + Codec {\n\t/// The wrapped type which is just a plain instance of `Public`.\n\ttype Generic: IsWrappedBy<Self> + Public + Debug + MaybeHash + Codec;\n}\n```\n\nbut it looks like similar requirement is missing for\n[sp_application_crypto::AppSignature](https://docs.rs/sp-application-crypto/latest/sp_application_crypto/trait.AppSignature.html)\nand\n[sp_core::crypto::Signature](https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Signature.html):\n\n```rust\n/// Application-specific signature.\npub trait AppSignature: AppCrypto + Eq + PartialEq + Debug + Clone {\n\t/// The wrapped type which is just a plain instance of `Signature`.\n\ttype Generic: IsWrappedBy<Self> + Eq + PartialEq + Debug;\n}\n```\n\nThis PR extends\n[sp_application_crypto::AppSignature](https://docs.rs/sp-application-crypto/latest/sp_application_crypto/trait.AppSignature.html)\ntrait with\n[sp_core::crypto::Signature](https://paritytech.github.io/polkadot-sdk/master/sp_core/crypto/trait.Signature.html).",
          "timestamp": "2025-09-11T09:45:57Z",
          "tree_id": "fd7030168a8d877153d8839e5d5337e3001fdc16",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1d4e9ec206ef417724c23c82ac94de5d24599173"
        },
        "date": 1757590871121,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002753890449999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.009019737739999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005377935449999988,
            "unit": "seconds"
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
          "id": "32cc5d6163781a077c4bdb2cafdf1a538127ebd5",
          "message": "[XCMP] Add support for receiving double encoded XCMs (#9588)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/8308\n\nThis PR adds support for receiving double encoded XCMs via XCMP.\n\n## Description\n\nRight now parachains pass XCM messages between them through XCMP pages\nthat use the `XcmpMessageFormat::ConcatenatedVersionedXcm` format. These\npages contain concatenated encoded `VersionedXcm`s and on the receiving\nside, in order to split the page into individual messages, we need to\nfirst decode them and then re-encode and forward them to the\n`pallet-messages-queue`. This adds extra overhead ([about 2.5\nmicroseconds + some cost per\nbyte](https://github.com/paritytech/polkadot-sdk/blob/3dfbdf4a454f35238500779e503e1ec32ba7fc63/cumulus/parachains/runtimes/assets/asset-hub-rococo/src/weights/cumulus_pallet_xcmp_queue.rs#L199-L208)).\n\nThis PR adds a new (`XcmpMessageFormat::ConcatenatedOpaqueVersionedXcm`)\nformat that will be used for pages with double-encoded XCMs. This makes\nthe decoding much easier and almost free, improving the XCMP bandwidth.\n\n## Rollout\n\nAn easy approach here is to consider that all parachains that support\nXCMv6 also have this upgrade and to use\n`XcmpMessageFormat::ConcatenatedOpaqueVersionedXcm` when sending\nmessages to such a parachain.\n\nThere are other better approaches, but they would be harder to\nimplement. For example:\n- another approach would be for each parachain to expose a list of\nsupported features and to check if\n`XcmpMessageFormat::ConcatenatedOpaqueVersionedXcm` is supported when\nsending messages to a connected parachain.\n- or we could advertise this through signals somehow\n\nStill thinking of other simpler approaches. We could also probably do it\nmanually for each XCMP channel.\n\nFor the moment it's important to add the support for receiving\n`XcmpMessageFormat::ConcatenatedOpaqueVersionedXcm` and to let it\npropagate to as many parachains as possible as they update the runtime.\nAfter that we'll have to come out with a rollout strategy.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-09-11T13:45:29Z",
          "tree_id": "56843a82de8590620d8f9b9bbb9677c9651939f4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/32cc5d6163781a077c4bdb2cafdf1a538127ebd5"
        },
        "date": 1757602751023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008726623479999982,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026502050599999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005168547479999991,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "35698397+drskalman@users.noreply.github.com",
            "name": "drskalman",
            "username": "drskalman"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "dae4b9cf572920848910b520d3cefe83d34692f3",
          "message": "Proof of possession alongside signing on owner (#9471)\n\n# Description\n  \nWhen signing on a new session key the signer must also use the session\nkey to sign on the authority signer key to prove that it is not faking\nthe ownership of someone's else key to mount a front runner attack. On\nthe other hand for aggregatable crypto schemes, the signer should proof\nthe ownership of the private key by signing a specific statement in a\nseparate domain than one is used for usual signing to prevent rogue key\nattack. This means that those scheme needs to submit two signature as\nproof in contrast to non-aggregatble schemes. It is also possible that\nin future some crypto scheme requires the key submitter to prove other\nfact before accepting its submission.\n\nThis PR introduce a new customize type ProofOfPossession for Pairs (in\naddition to Public and Signature) to represent these proof. Currently\n`ProofOfPossession = Signature` for `ecdsa, ed25519 and sr25519` while\n`ProofOfPossession = Signature | Signature` for bls381 and\n`ProofOfPossession = ecdsa:Signature | bls381:Signature |\nbls381:Signature` for `ecdsa_bls381` paired_key scheme.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Davide Galassi <davxy@datawok.net>",
          "timestamp": "2025-09-11T19:58:10Z",
          "tree_id": "597cc7d36c3d338a58f4b577b6a2e2d78d7bdf1f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dae4b9cf572920848910b520d3cefe83d34692f3"
        },
        "date": 1757627442465,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026904307199999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00524138211999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008819237639999988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "117115317+lrubasze@users.noreply.github.com",
            "name": "Lukasz Rubaszewski",
            "username": "lrubasze"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "32142045e09e9e0e822d47f064372acb35d14c84",
          "message": "ci: reenable zombienet pov_recovery and rpc_collator_builds_block tests (#9695)\n\nSince https://github.com/paritytech/zombienet-sdk/issues/371 has been\nsolved\nReenable:\n- `zombienet-cumulus-0002-pov_recovery` -\nhttps://github.com/paritytech/polkadot-sdk/issues/8985\n- `zombienet-cumulus-0006-rpc_collator_builds_blocks` -\nhttps://github.com/paritytech/polkadot-sdk/issues/9154\n\nAdditionally allow to use regex patterns when dispatching zombienet\ntests manually:\neg. \n```\n.github/scripts/dispatch-zombienet-workflow.sh \\\n  -w zombienet_cumulus.yml \\\n  -b \"lrubasze/reenable-some-zombienet-ci-tests\" \\\n  -p \"0002-pov_recovery|0006-rpc_collator_builds_blocks\"\n```",
          "timestamp": "2025-09-12T07:52:18Z",
          "tree_id": "664d06488a8c42dcfbc2b6150ab44b0f4b960cc6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/32142045e09e9e0e822d47f064372acb35d14c84"
        },
        "date": 1757668685919,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00880422321999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00266232205,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005242644959999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "f82d684c4a4a4430316c6d892213ba6aff91cf7b",
          "message": "staking-async: handle uninitialized state in try-runtime checks (#9721)\n\nHandle the case where `ActiveEra` is `None` (uninitialized staking\nstate) in the try-state checks.\nThis fixes `try-runtime` failures when deploying `staking-async` for the\nfirst time on chains without existing staking.",
          "timestamp": "2025-09-12T09:02:38Z",
          "tree_id": "f5eeab765f487db20d09d878cb03fbb7ce9ad7be",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f82d684c4a4a4430316c6d892213ba6aff91cf7b"
        },
        "date": 1757672496148,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005166451129999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264641478,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008669741069999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "6c06057ae819724f89ff1ef9f060592b6c96e5d3",
          "message": "Deprecate `OnRuntimeUpgrade` parameter in `frame_executive::Executive` (#9638)\n\nFollow-up of https://github.com/paritytech/polkadot-sdk/pull/9451\n\nBased on PR https://github.com/paritytech/polkadot-sdk/pull/1781 and\n[PRDoc](https://github.com/paritytech/polkadot-sdk/blob/beb9030b249cc078b3955232074a8495e7e0302a/prdoc/1.9.0/pr_1781.prdoc#L29),\nthe new way for providing the single block migrations should be through\n`SingleBlockMigrations` in `frame_system::Config`. Providing them from\n`frame_executive::Executive` is still supported, but is deprecated.\n\n> `SingleBlockMigrations` this is the new way of configuring migrations\nthat run in a single block. Previously they were defined as last generic\nargument of Executive. This shift is brings all central configuration\nabout migrations closer into view of the developer (migrations that are\nconfigured in Executive will still work for now but is deprecated).\n\n`Executive` docs will look like:\n\n<img width=\"800\" alt=\"image\"\nsrc=\"https://github.com/user-attachments/assets/6f285c26-5c61-4350-a41b-aebc6b856601\"\n/>\n\nCompanion PR in https://github.com/polkadot-fellows/runtimes/pull/844",
          "timestamp": "2025-09-12T09:42:24Z",
          "tree_id": "59e562a5cf13de084e691cb12c3ea92120a99189",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6c06057ae819724f89ff1ef9f060592b6c96e5d3"
        },
        "date": 1757675724713,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026529542099999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008644015489999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005146801829999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "136b4cb5f52515ec8086ab4466226d48e8ba220b",
          "message": "Remove deprecated collator-related code in cumulus (#9662)\n\nRemoves collator-related code in cumulus, which has been deprecated for\na long time.\n\nRemoves an old test, which was adapted in\nhttps://github.com/paritytech/cumulus/pull/480 and duplicated by\nhttps://github.com/paritytech/polkadot-sdk/blob/acac0127168dac1d603e4d996cb210ceeddeb5de/substrate/client/block-builder/src/lib.rs#L389-L415\n\n## PoV Recovery Test Updates\n\nUpdates the PoV recovery test\n(`cumulus/zombienet/zombienet-sdk/tests/zombie_ci/pov_recovery.rs`) to\nuse a more realistic consensus mechanism:\n\n### Changes Made\n- **Removed**: `--use-null-consensus` flag from test configuration\n\n### Rationale\n\n**Previous behavior** (with null consensus):\n- Nodes operated without real block production\n- PoV recovery mechanisms triggered more frequently\n- Created artificial test conditions that don't reflect production\nscenarios\n\n**New behavior** (with actual consensus):\n- Nodes produce blocks normally but don't announce them to peers\n- PoV recovery occurs at a more realistic frequency\n- Better simulates real-world network conditions where blocks may be\nmissed\n\n### Impact\n\nThis change makes the test **more representative** of actual network\nconditions while maintaining the core functionality being tested.\n\n## Follow-up\nRemove the following lines:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/4acb964059a218be9bac954b4e3803b78b5526bf/cumulus/pallets/parachain-system/src/lib.rs#L993-L994\n\n## Review notes\n\nI recommend enabling `Hide whitespace` option when reviewing the\nchanges:\n\n<img width=\"300\" alt=\"image\"\nsrc=\"https://github.com/user-attachments/assets/41f137af-c0b9-435e-af1e-84e51cbdfa23\"\n/>",
          "timestamp": "2025-09-12T19:11:41Z",
          "tree_id": "c16dce985562719e48e322f8393011c361d8572d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/136b4cb5f52515ec8086ab4466226d48e8ba220b"
        },
        "date": 1757708794770,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008539138149999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005053543979999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025977184999999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "jesse.chejieh@gmail.com",
            "name": "Doordashcon",
            "username": "Doordashcon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1cbf4eed97a87ae4c1aef6176c80761c49f60e6f",
          "message": "Simulate `rank_to_votes` in `pallet-ranked-collective` benchmark. (#9731)\n\nresolves #9730\n\n---------\n\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-09-15T08:33:06Z",
          "tree_id": "730280b04c0fddba8c040d6fb5df823addb18688",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1cbf4eed97a87ae4c1aef6176c80761c49f60e6f"
        },
        "date": 1757929600785,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008525519599999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050322243099999945,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025930019799999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "61b566ac14054aff4859b38094716aa2b5e63caf",
          "message": "added trace logging in EVM interpreter loop (#9561)\n\nAdded trace logging for each instruction to evm::run function.\nsolves https://github.com/paritytech/polkadot-sdk/issues/9575\n\n---------\n\nSigned-off-by: xermicus <cyrill@parity.io>\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: pgherveou <pgherveou@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: xermicus <cyrill@parity.io>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: Cyrill Leutwiler <bigcyrill@hotmail.com>",
          "timestamp": "2025-09-15T09:02:43Z",
          "tree_id": "0cf02dce839110cd1525b28ad8c7a6a63abbe8b4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/61b566ac14054aff4859b38094716aa2b5e63caf"
        },
        "date": 1757931880823,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004982684209999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002612795469999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008504438039999996,
            "unit": "seconds"
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
          "id": "9075dee80e78810c6de1b94afcb147c55a4546a2",
          "message": "[pallet-revive] fix CodeInfo owner (#9744)\n\nFix CodeInfo owner, it should always be set to the origin of the\ntransaction\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-16T08:46:08Z",
          "tree_id": "3b78613dc97191bc7b1080dc44d38685bfdb6a1f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9075dee80e78810c6de1b94afcb147c55a4546a2"
        },
        "date": 1758017128160,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262241329,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005114015269999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008533056459999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "6e4111b005fb53bc94b419c36d295274fffade97",
          "message": "staking-async: handle uninitialized state in try-state checks (#9747)\n\n- Add early return in do_try_state when pallet is uninitialized\n- Add test for empty state validation\n\nFollowup of #9721 .\nOnce backported to `2507` and crate is published, should unlock\nhttps://github.com/polkadot-fellows/runtimes/pull/904\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-16T12:04:24Z",
          "tree_id": "50384005abaa1c513d8e5e350cdf4d3eb7d9c9d3",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6e4111b005fb53bc94b419c36d295274fffade97"
        },
        "date": 1758028714749,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008663889539999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005168952489999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026731529700000003,
            "unit": "seconds"
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
          "id": "3d29a17bdc53aa7dcd75376cb01b3ef524271f99",
          "message": "Avoid double counting PoV size when enqueing XCMP message (#9745)\n\nRelated to #9641\n\nAvoid double counting PoV size when enqueing XCMP message\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrii <ndk@parity.io>",
          "timestamp": "2025-09-16T14:10:36Z",
          "tree_id": "29a8a2c3ef43fff18a7dbd1d2a64141c4640c4ad",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3d29a17bdc53aa7dcd75376cb01b3ef524271f99"
        },
        "date": 1758036229518,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005005800589999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025860852099999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00848797963,
            "unit": "seconds"
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
          "id": "631eb8b90f7756a68391eceaf6d3d63b7c697a23",
          "message": "`frame-support`: Move all macros from `lib.rs` to a new file (#9742)\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-16T15:04:54Z",
          "tree_id": "8a7fe15e0953ca39cb8d256a16d9ea9b9ddbfdaa",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/631eb8b90f7756a68391eceaf6d3d63b7c697a23"
        },
        "date": 1758039881160,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00894864335999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027367973599999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005371510619999993,
            "unit": "seconds"
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
          "id": "a554c92f4aeb908b95c71051cd98e7ba55f2297c",
          "message": "revive-rpc: use generic RpcClient instead of ReconnectingRpcClient (#9701)\n\nThis will enable more flexible usage of the revive RPC as a library.\n\nNeeded so that we can reuse it with an in-memory RPC client for\nanvil-polkadot:\nhttps://github.com/paritytech/foundry-polkadot/issues/238",
          "timestamp": "2025-09-16T17:47:47Z",
          "tree_id": "2c97b0a83c916dba480734b06543f33434c05272",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a554c92f4aeb908b95c71051cd98e7ba55f2297c"
        },
        "date": 1758049476749,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002620097539999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005050727219999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008544327569999989,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4331b282ecc6b6e911eadb322c600cbea2c4541a",
          "message": "[pallet-revive] Migrate various getters to `System` pre-compile (#9517)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nMigrates:\n* `own_code_hash`\n* `caller_is_origin`\n* `caller_is_root`\n* `weight_left`\n* `minimum_balance`\n\nThere are some minor other fixes in there (removing leftovers from\ndeprecating chain extensions, stabilizing `block_hash` in overlooked\ncrates, etc.).\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-09-17T00:09:01Z",
          "tree_id": "a6c0491b75fecea82e7ea14bf44adc568502f847",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4331b282ecc6b6e911eadb322c600cbea2c4541a"
        },
        "date": 1758072487490,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005161172709999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261264704,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008676851539999988,
            "unit": "seconds"
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
          "id": "127c0780e180ab9bdbc8c6f85fe2b20d64b5c094",
          "message": "Fix Aura authorities tracker bug (#9753)\n\nCurrently, the Aura authorities tracker uses the block pre-hash to\nimport the authorities, but the post-hash to fetch them. That results in\nblock verification failures. A scenario to reproduce the bug is as\nfollows:\n\n* Start a parachain with a single-collator fixed-authority Aura;\n* Upgrade the parachain runtime to include `session` and\n`collator-selection` pallets;\n* Register the collator keys as session keys, then add the collator to\ninvulnerables;\n* Start a second collator, rotate its keys, register them as session\nkeys, and add the second collator to invulnerables;\n* When the second collator is trying to import the block where it's\nenacted as the second Aura authority, it fails the block verification\nand does not import or produce any blocks anymore.\n\nThis PR changes the behavior to importing the authorities using the\nblock post-hash, which fixes the bug.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-17T10:49:04Z",
          "tree_id": "f6c91c1ca8237f7ad1b92cacd579d0c117d2576b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/127c0780e180ab9bdbc8c6f85fe2b20d64b5c094"
        },
        "date": 1758110484979,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00892506277999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005496750639999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027340042499999994,
            "unit": "seconds"
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
          "id": "d5bc25b57c300d0477ceb2d53cbbc2e6734da933",
          "message": "ci: switch tests to new runners (#9757)\n\nPR switches test-linux-stable to new runners.\n\ncc https://github.com/paritytech/devops/issues/3875",
          "timestamp": "2025-09-17T15:16:53Z",
          "tree_id": "998eff5e49918d1ba88285293224c20087010e72",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5bc25b57c300d0477ceb2d53cbbc2e6734da933"
        },
        "date": 1758126355573,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264359641,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008675804009999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00517213885999999,
            "unit": "seconds"
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
          "distinct": false,
          "id": "978b35ebdcb5b02acce935a55a0dd5ef5798220e",
          "message": "Improve README files (#9760)\n\nThis PR makes it easier for first-time builders to just copy-and-paste\nand fix typos.\n\nRelates to\nhttps://github.com/paritytech/polkadot-sdk-minimal-template/pull/25",
          "timestamp": "2025-09-17T20:01:25Z",
          "tree_id": "6ca78419a38b39ac40fdfb0f81044dafdc22f977",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/978b35ebdcb5b02acce935a55a0dd5ef5798220e"
        },
        "date": 1758143737851,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008633296649999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050227305399999965,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002604929569999999,
            "unit": "seconds"
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
          "id": "e7f36ab82934a7142f3ebd7f8b5566f12f85339b",
          "message": "[pallet-revive] fix salt endianness  (#9771)\n\nfix <https://github.com/paritytech/polkadot-sdk/issues/9769>\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-18T13:17:02Z",
          "tree_id": "06cdab641c5e7d06aab25b71ec5c2053c03f2622",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e7f36ab82934a7142f3ebd7f8b5566f12f85339b"
        },
        "date": 1758205544764,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027197234500000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005335297869999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.009029794869999993,
            "unit": "seconds"
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
          "id": "ac4a011580bbba60a99afd618f645330380f0fa9",
          "message": "pallet_revive: add account_id and new_balance_with_dust runtime APIs  (#9683)\n\nNeeded for https://github.com/paritytech/foundry-polkadot/issues/240",
          "timestamp": "2025-09-18T13:51:57Z",
          "tree_id": "4729b50f039e96fc95229c9f24579d15bbe80b31",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ac4a011580bbba60a99afd618f645330380f0fa9"
        },
        "date": 1758209619532,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260825768,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008600571839999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005161683399999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "fea33a9dd5e7133c95c5e0496b8e9fd9e215c855",
          "message": "EPMB: ensure to have enough funds for benchmarking (#9772)\n\nFix `pallet_election_provider_multi_block_signed::register_eject`\nbenchmark failing on KAHM due to `funded_account()` function not\nproviding enough balance to cover the required deposits. See for example\n[here](https://github.com/polkadot-fellows/runtimes/actions/runs/17765363309/job/50487393309?pr=856).\n\nThe fix ensures that benchmark accounts have sufficient funds to cover\nthe worst-case deposit scenario (registration + all pages submission)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-18T15:59:16Z",
          "tree_id": "6635fdd80d0b5b15555c2153da6a9decc5e101d5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fea33a9dd5e7133c95c5e0496b8e9fd9e215c855"
        },
        "date": 1758215716960,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002604492919999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008602161589999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00510051620999999,
            "unit": "seconds"
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
          "id": "83c744990c5fdffb4e24464a4e569b190ebc41d6",
          "message": "ci: pin all actions version (#9776)\n\nPR pins hash versions for all actions\n\ncc https://github.com/paritytech/devops/issues/4319",
          "timestamp": "2025-09-19T08:34:57Z",
          "tree_id": "4df334d77589310708b9afb520197faa98699fbe",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/83c744990c5fdffb4e24464a4e569b190ebc41d6"
        },
        "date": 1758275173626,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027368553100000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008817795049999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005227643579999991,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7c2642df6079b4e73d51fc62c41269cb5c288af2",
          "message": "Use `total balance (free + reserved)` when performing liquidity checks for a new reserve (#8108)\n\n# Description\n\nSolves: https://github.com/paritytech/polkadot-sdk/issues/8099\n\nBased on the documentation and existing code, the usable balance is\ncomputed with the following formula:\n\n```rs\n// If Fortitude == Polite \nlet usable_balance = free - max(frozen - reserved, existential balance)\n```\n\n### The problem:\n\nIf an account's `free balance` is lower than `frozen balance`, no\nreserves will be allowed even though the `usable balance` is enough to\ncover the reserve, resulting in a `LiquidityRestrictions` error, which\nshould not happen.\n\n### Visual example of how `usable/spendable` balance works:  \n```bash\n|__total__________________________________|\n|__on_hold__|_____________free____________|\n|__________frozen___________|\n|__on_hold__|__ed__|\n            |__untouchable__|__spendable__|\n```\n\n## Integration\n\nNo action is required, the changes only change existing code, it does\nnot add or change any API.\n\n## Review Notes\n\nFrom my understanding, the function `ensure_can_withdraw` is incorrect,\nand instead of checking that the new `free` balance is higher or equal\nto the `frozen` balance, it should make sure the `new free` balance is\nhigher or equal to the `usable` balance.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2025-09-19T12:17:42Z",
          "tree_id": "9a637c36dc231e47e6541f3451b8ecc64eefb79f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7c2642df6079b4e73d51fc62c41269cb5c288af2"
        },
        "date": 1758289012348,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008708909209999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005330646869999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00268557604,
            "unit": "seconds"
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
          "distinct": false,
          "id": "8dd73c42799119a877aa1ad150da372ef209295f",
          "message": "ci: disable cache for parity-publish actions (#9788)\n\nActions that use parity-publish don't need cache in PR since they only\ninstall the crate. PR disables saving caches for those actions in PR,\nthey'll only consume it from master.\n\ncc https://github.com/paritytech/devops/issues/4317",
          "timestamp": "2025-09-19T14:30:57Z",
          "tree_id": "a11ff1a5fb3b24288a9059d4ae2b24e6756493f4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8dd73c42799119a877aa1ad150da372ef209295f"
        },
        "date": 1758296416331,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005225591859999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00260026609,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008628767689999985,
            "unit": "seconds"
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
          "id": "277a26585b3d7e38668d5068d2f10ef39051f5d6",
          "message": "revive-fixtures: Provide an env variable to disable compilation (#9791)\n\nRight now `pallet-revive-fixtures` is always trying to build the\nfixtures. It requires `solc` and other stuff for compilation. If you are\nnot requiring the fixtures, because you for example only run `cargo\ncheck`, this pull request introduces `SKIP_PALLET_REVIVE_FIXTURES`. When\nthe environment variable is set, the compilation of the fixtures is\nskipped. It will set the fixtures to `None` and they will panic at\nruntime.",
          "timestamp": "2025-09-21T20:14:43Z",
          "tree_id": "e407f4c6511f33e46c069dda76a27764fb319064",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/277a26585b3d7e38668d5068d2f10ef39051f5d6"
        },
        "date": 1758490145574,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005201969369999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025866543400000002,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00865172106999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "36680e6d4e2eea9d552930b247c67d817c48045a",
          "message": "EIP-3607 added check to make sure a contract account cannot transfer funds as an EOA account (#9717)\n\nfixes https://github.com/paritytech/polkadot-sdk/issues/9570\n\n---------\n\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-09-22T10:41:37Z",
          "tree_id": "330a8ca3062285a07970f0d17271f2b69790a7f6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/36680e6d4e2eea9d552930b247c67d817c48045a"
        },
        "date": 1758542262179,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008674655549999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00261473498,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005150568299999991,
            "unit": "seconds"
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
          "id": "d97bed091052726f6d1930031ccde53b82ed3c00",
          "message": "Limit the number of signals per XCMP page (#9781)\n\nRight now we have only 2 XCMP signals: `SuspendChannel` and\n`ResumeChannel` and we can write at most 1 per page.\n\nLet's also add a limit when reading the signals in a page. Even if now 1\nis enough, since in the future we might add more signals, let's have a\nlimit of 3 per page.\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-22T11:33:39Z",
          "tree_id": "c27568598af0505447235dd267fede5622faaa1d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d97bed091052726f6d1930031ccde53b82ed3c00"
        },
        "date": 1758545013661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008588030729999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005140290549999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002573907060000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "f8fc34052efe427fbfbfc835d8b7fedc6cfca567",
          "message": "Added pallet-root-offences to Westend RC runtime (#9799)\n\nNeeded to let us test a manual slash on Westend relay-chain and see what\nhappens in terms of UI, indexers etc on the revamped [PJS's staking\nasync\npage](https://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fwestend-asset-hub-rpc.polkadot.io#/staking-async)\n🍿\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-22T12:20:09Z",
          "tree_id": "f6bca1982f09a29e480775565a4b295e9834c5b8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8fc34052efe427fbfbfc835d8b7fedc6cfca567"
        },
        "date": 1758547831396,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008655896879999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005225493629999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002625369729999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "ef10d5e743475cc8dab36520d4e19c2e924be40a",
          "message": "Improve inbound_queue::BenchmarkHelper to add more flexibility (#9627)\n\n# Description\n\nImprove the usage of the `inbound_queue::BenchmarkHelper` to decouple\nthe mocks from the benchmark.\nThis change will enable any user to benchmark custom messages since now\nit's harcoded to the register_token_message only\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-22T13:56:42Z",
          "tree_id": "03eaa43c58461bc4a3314b431c7f3dd03366335b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ef10d5e743475cc8dab36520d4e19c2e924be40a"
        },
        "date": 1758554022429,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008667895779999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005216792989999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267111725,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "evgeny@parity.io",
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "7a776bf70efb9f04c6784969dc079476c279656a",
          "message": "ci-unified image update (#9800)\n\nci-unified v202509220255, updated forklift to 0.14.3\npossible [AWS Deadlock\n#23](https://github.com/paritytech/forklift/issues/23) fix",
          "timestamp": "2025-09-22T17:40:53Z",
          "tree_id": "9f775cb83f3cd14a3dbac9424632da185610b445",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7a776bf70efb9f04c6784969dc079476c279656a"
        },
        "date": 1758566794656,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264501857,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008631207429999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005177921119999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "paolo@parity.io",
            "name": "Paolo La Camera",
            "username": "sigurpol"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "4431e51b0dc638f6bd185c5664dfe49c69d9a8bb",
          "message": "EPMB: fix benchmark funding for exponential deposit growth (#9787)\n\nFixes funding issues in benchmarks that were failing on Asset Hub Kusama\nwith \"Funds are unavailable\" errors.\n\n\n\nTwo root causes exist:  \n- The `funded_account()` function calculated deposits based on the\ncurrent queue state, but `GeometricDepositBase` leads to exponential\ngrowth: `deposit = base * (1 + increase_factor)^{queue_len}`.\n-  We did not account for transaction fees.  \n\nSolution:  \n- Calculate deposits using the worst-case scenario with the maximum\nqueue size (`T::MaxSubmissions::get()`) to ensure sufficient funding,\nregardless of changes in queue state during benchmark execution.\n- Estimate total transaction fees as 1% of the minimum balance\nmultiplied by the number of operations.\n\n\nThis should provide a more robust fix than\nhttps://github.com/paritytech/polkadot-sdk/pull/9772 and allow to fix\nEPMB on KAHM (see https://github.com/polkadot-fellows/runtimes/pull/916\n- once/if we merge the current PR, we need to backport to `2507`, bump\nEPMB crate and update 916 accordingly)\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-22T18:35:50Z",
          "tree_id": "684a0dbea8fa9b1df3b27599220bf8ae8083c05d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4431e51b0dc638f6bd185c5664dfe49c69d9a8bb"
        },
        "date": 1758570435226,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00879242799999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027157637100000006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005442487329999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "agusrodriguez2456@gmail.com",
            "name": "Agustín Rodriguez",
            "username": "Agusrodri"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "a0a3b84738fdaef7f72be79d388f5b87565b2cb4",
          "message": "Snowbridge V2: Add `OnNewCommitment` hook to outbound-queue pallet (#8053)\n\n## Description\n\nThis PR adds a simple hook to `snowbridge-pallet-outbound-queue-v2`\nwhich allows to perform actions whenever there is a new commitment in\nthis pallet.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-09-23T09:15:18Z",
          "tree_id": "fa90e616c701e51e8f667ffa9602104b6fe6f4d7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a0a3b84738fdaef7f72be79d388f5b87565b2cb4"
        },
        "date": 1758623093074,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026297019299999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005096458659999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008601447649999993,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "82b8a501c87460fb384851e9424d60a68566c7de",
          "message": "Measure backed in block count vs backable  (#9417)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/9341\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Javier Viola <javier@parity.io>",
          "timestamp": "2025-09-23T09:59:54Z",
          "tree_id": "2100ed3a655566c495348fbe9ff49f17bec4b4a6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/82b8a501c87460fb384851e9424d60a68566c7de"
        },
        "date": 1758625748198,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0053047980099999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008731862649999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026363922799999993,
            "unit": "seconds"
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
          "id": "143eb77346d6102a80931f600fc9dce1ee1f9e54",
          "message": "Add collator selection to YAP (#9663)\n\nThis PR adds the collator selection pallet as well as other pallets\nneeded for its functionality to the Yet Another Parachain runtime.\n\nAlong with that, the YAP runtime is a little bit refactored to adopt the\nlatest FRAMEwork changes.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-23T10:34:08Z",
          "tree_id": "33a4a479609a736a1e1ffc35116059a8e5c4e26f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/143eb77346d6102a80931f600fc9dce1ee1f9e54"
        },
        "date": 1758629076862,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026462406499999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008610536189999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005116266479999993,
            "unit": "seconds"
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
          "id": "6875cc0cccbc418f229927baa1490110430f6275",
          "message": "Snowbridge Inbound Queue V2 relayer tip payout fix (#9746)\n\n# Description\n\nFixes a bug where Snowbridge Inbound V2 tips were not paid out to the\nrelayer.\n\n## Review Notes\n\nAny tips added to a message in the Inbound Queue v2 (Ethereum to\nPolkadot direction), were burned and added to storage, but never paid\nout to the relayer. This PR fixes this bug by adding the tip to the\nrelayer fee.\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2025-09-23T13:20:12Z",
          "tree_id": "ab6a21866ce69b2f099f3ee8b986dc90460d6644",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6875cc0cccbc418f229927baa1490110430f6275"
        },
        "date": 1758637918177,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026158379199999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00846195276999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.004942295889999993,
            "unit": "seconds"
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
          "id": "19320f104fc4b5cb6663cffc41f340b6b5239be8",
          "message": "FRAME: Register `on_initialize` after each pallet (#9756)\n\nBefore this pull request, FRAME was executing all pallets\n`on_initialize` and then register the weight, including the weight of\n`on_runtime_upgrade`. Thus, other pallets were not aware on how much\nweight was already used when they were executing their `on_initialize`\ncode. As some pallets are doing some work in `on_initialize`, they need\nto be aware of how much weight is still left.\nTo register the weight after each `on_initialize` call, a new trait is\nadded. This new trait is implemented for tuples of types that implement\n`OnInitialize` and then it registers the weight after each call to\n`on_initialize`.\n\n`pallet-scheduler` is changed to take the remaining weight into account\nand to not just assume that its configured weight is always available.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-23T17:26:05Z",
          "tree_id": "329e7d671c7b6beb98933de95fc234552598c017",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/19320f104fc4b5cb6663cffc41f340b6b5239be8"
        },
        "date": 1758652654317,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005028634699999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026483011499999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008568878589999997,
            "unit": "seconds"
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
          "id": "7dc67319065b18d4c02b4275e6b071ee59d40635",
          "message": "network/tests: Increase test timeout to fix flaky CI (#9810)\n\nThis PR bumps the `libp2p_disconnects_litep2p_substream` test timeout\nfrom 5 seconds to 1 minute.\n\nUnder load, the test may not have sufficient time to establish\nconnectivity and complete the test within the allotted time.\n\ncc @paritytech/networking\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-09-24T08:25:10Z",
          "tree_id": "29118a00bcb8d88cbb396a592265e3ebc32d5246",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7dc67319065b18d4c02b4275e6b071ee59d40635"
        },
        "date": 1758707892455,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008873682349999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267559399,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005337881149999995,
            "unit": "seconds"
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
          "distinct": false,
          "id": "e8f1aff5a174f420cdd77f5d5c854dd6dc8a3273",
          "message": "[pallet-revive] Add set_storage/set_storage_var_key methods (#9759)\n\n... to be used in polkadot foundry to make sure EVM state is in sync\nwith pallet-revive state.\n\nFixes: https://github.com/paritytech/foundry-polkadot/issues/275\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-09-24T10:33:54Z",
          "tree_id": "2e58ffa530937bbb0842bb596295fd616ef55271",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e8f1aff5a174f420cdd77f5d5c854dd6dc8a3273"
        },
        "date": 1758714208509,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026238391399999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008533184749999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00501136068999999,
            "unit": "seconds"
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
          "id": "80ee9c8f4cd6e2ea49cb8eceadde5b42f7e87a86",
          "message": "ci: Use `--locked` for cargo doc steps (#9828)\n\nThis PR adds the `--locked` option to the cargo doc tests.\n\nDetected by running the CI on PR:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/17972092266/job/51117118432\n\n```rust\nerror[E0277]: the trait bound `BoundedVec<u8, v3::MaxPalletNameLen>: JsonSchema` is not satisfied\n   --> polkadot/xcm/src/v3/mod.rs:228:12\n    |\n228 |     pub name: BoundedVec<u8, MaxPalletNameLen>,\n    |               ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `JsonSchema` is not implemented for `BoundedVec<u8, v3::MaxPalletNameLen>`\n    |\nnote: there are multiple different versions of crate `schemars` in the dependency graph\n   --> /usr/local/cargo/registry/src/index.crates.io-1949cf8c6b5b557f/schemars-0.8.22/src/lib.rs:133:1\n    |\n133 | pub trait JsonSchema {\n    | ^^^^^^^^^^^^^^^^^^^^ this is the required trait\n    |\n   ::: polkadot/xcm/src/v3/junction.rs:49:44\n    |\n49  | #[cfg_attr(feature = \"json-schema\", derive(schemars::JsonSchema))]\n    |                                            -------- one version of crate `schemars` used here, as a direct dependency of the current crate\n    |\n   ::: polkadot/xcm/src/lib.rs:31:5\n```\n\nThanks @bkchr for the suggestion here 🙏 \n\nThis has been detected while working on:\n- https://github.com/paritytech/polkadot-sdk/pull/9418\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2025-09-24T14:58:33Z",
          "tree_id": "884a671f0bbf4fe179c1013dce196003dfc20cc9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/80ee9c8f4cd6e2ea49cb8eceadde5b42f7e87a86"
        },
        "date": 1758730206717,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008697737899999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005303366449999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267507153,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "58a6df32ec9a145615061cd203875c55db5e6fa1",
          "message": "Elastic scaling runtime upgrade test (#9811)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/7259.\n\nTODO\n- [x] prdoc\n- [x] upgrade from sync backing\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Javier Viola <javier@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>",
          "timestamp": "2025-09-25T07:55:03Z",
          "tree_id": "3f4658109c44db355c366d8a854cb1975abcfa86",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/58a6df32ec9a145615061cd203875c55db5e6fa1"
        },
        "date": 1758791056536,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005135888830000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025916138600000018,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008546229949999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "afbe4258991a60a7b41270d0fe47d1cd94a5681c",
          "message": "bugfix revm set_storage gas cost (#9823)\n\nFixes bug in revm gasmetering where the initial charge was less than the\nadjusted charge.\n\n---------\n\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>",
          "timestamp": "2025-09-25T09:05:41Z",
          "tree_id": "093d3a6142cd89d6a78d461a8e82aa2251bcfeb6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/afbe4258991a60a7b41270d0fe47d1cd94a5681c"
        },
        "date": 1758795401805,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005147795699999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00868360990999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026035826699999996,
            "unit": "seconds"
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
          "id": "fbf98c8dee09e3dc02506a6fea26a9704cc9c05d",
          "message": "Elastic-scaling-guide: Mention slot duration (#9713)\n\nFollow-up to #9677 . I think it would be good to add our view on the\nslot duration, as it is often confused with the actual block production\ninterval. This short addition should clarify things a bit.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-25T09:48:11Z",
          "tree_id": "75703f988b95d5e94d435f35011ef1e17f150e54",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fbf98c8dee09e3dc02506a6fea26a9704cc9c05d"
        },
        "date": 1758797850007,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005052009330000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00258420562,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008571368829999992,
            "unit": "seconds"
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
          "id": "8978c005de6631dce20e204380bb43149127cdce",
          "message": "wasmtime: support for perfmap added (#9821)\n\nThis PR add  support for `perfmap` in wasmtime executor.\n\nFor more technical details refer to this\n[doc](https://docs.wasmtime.dev/examples-profiling-perf.html#profiling-with-perfmap).\n\nInstruction on how to configure profiling on substrate nodes (tested\nwith cumulus benchmarks) is\n[here](https://hackmd.io/o_Ghc86OT4KzCE4x04MeOg?view#Getting-the-right-flamegraph).\n\nThe following environment variable needs to be set when executing the\nnode binary:\n```\nexport WASMTIME_PROFILING_STRATEGY=perfmap\n```\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-25T10:38:47Z",
          "tree_id": "5014ce67276b6f7ddaf68d6ea916b50e1937a11d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8978c005de6631dce20e204380bb43149127cdce"
        },
        "date": 1758800902679,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005035006309999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008576938709999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265768088,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "7fc007deca8c14d0356367b2461300683bf890b4",
          "message": "pallet revive evm backend add tests for cross vm contract calls (#9768)\n\nfixes https://github.com/paritytech/polkadot-sdk/issues/9576\n\n---------\n\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-09-25T13:13:25Z",
          "tree_id": "2099121124c3880f9004a9d06de28043c05ace37",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7fc007deca8c14d0356367b2461300683bf890b4"
        },
        "date": 1758810529279,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.004921325429999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00850728177999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264947959,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "tsvetomir@parity.io",
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "ad4ae97793083c2b08369fe7b0e63331e7753a4c",
          "message": "Handle invulnerable AH collators with priority in collator-protocol/validator-side (#9458)\n\nImplements priority handling of invulnerable AH collators which consists\nof:\n1. Connection management - there is a connection limit in the networking\nstack of 100 peers after which no new connections are accepted. To make\nsure that the invulnerable collators can always connect to the\nvalidators permissionless collators are getting disconnected one the\nconnection count is close to the limit.\n2. Collations from permissionless collators are held off for some time\nbefore processing so that the invulnerables have got a chance to put a\ncollation on their own.\n\nTODOs:\n- [x] Add the invulnerables list.\n- [x] Test if the change works for collators claiming positions further\ninto the CQ.\n- [x] Find a good value for `HOLD_OFF_DURATION` and test it on a\ntestnet.\n- [x] Safetynet: Add a command line parameter which overrides\n`HOLD_OFF_DURATION`.\n- [x] Make the hold off more idiomatic.\n- [x] Hold off per relay parent.\n- [x] Fix failing tests.\n\n---------\n\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2025-09-26T05:37:51Z",
          "tree_id": "17846cf3dfa6de29d1559d0660723069ab287da8",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad4ae97793083c2b08369fe7b0e63331e7753a4c"
        },
        "date": 1758869371141,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008643726509999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026408965200000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005056125739999987,
            "unit": "seconds"
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
          "id": "9e0636567bebf312b065ca3acb285a8b32499df7",
          "message": "Add remove_by method in runtime interface and extension. (#9836)\n\nCurrently the runtime is responsible to remove statements from the\nstore. this is the only for the statements to expire and not grow\nindefinitely until the store gobal limits.\n\nIf we use a statements store with 4GiB of statements, the method\n`statements` and `remove` to query and remove statements from the\noffchain worker is unusable given `statements` cannot be called.\n\nI introduce the method `remove_by` which is safe.\n\nLater we can also introduce a method `valid_statement_change` which\nresize the usage of the statement store of one account given a new\nusage. But I don't have time for this now.\n\nThere are some other possibilities (both implemented in different commit\nof https://github.com/paritytech/polkadot-sdk/pull/9827):\n* Do no make the runtime responsible of cleaning the store: make the\nstatement store clean the statements after some duration like 7 days.\n* Make the user responsible to refresh their statements. The statement\nstore would clean statements by order of insertion. User with remaining\nallowance must resubmit their statements regularly. (the pace depends on\nhow fast the allowance of user is changing in the runtime).\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>",
          "timestamp": "2025-09-26T07:27:40Z",
          "tree_id": "3b0ad9124e47dd265152387ff55d9685f4566649",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9e0636567bebf312b065ca3acb285a8b32499df7"
        },
        "date": 1758875993556,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008781024559999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005194688389999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026819324099999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "diego2737@gmail.com",
            "name": "Diego",
            "username": "dimartiro"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d5473e6fa3633c3355f8ef19a8b8921673657a9f",
          "message": "Add new zepter duplicate-deps check as part of CI (#9809)\n\n# Description\n\nThis PR builds on my previous\n[PR](https://github.com/paritytech/polkadot-sdk/pull/9233) and addresses\nfeedback from Basti’s comment\n[here](https://github.com/paritytech/polkadot-sdk/pull/9283#issuecomment-3104712426).\n\nTo prevent the same situation from recurring in the future, I’ve\nintroduced a new **lint check** in **Zepter**, which is now also\nintegrated into the CI workflow. The purpose of this check is to\nautomatically detect and block cases where the same dependency is\ndeclared both under `[dependencies]` and `[dev-dependencies]`.",
          "timestamp": "2025-09-26T08:23:23Z",
          "tree_id": "0cfd615bddc52c65baf5cb7983f6d7f4e2d362c6",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5473e6fa3633c3355f8ef19a8b8921673657a9f"
        },
        "date": 1758879599127,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008570753879999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00264223625,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005113730359999994,
            "unit": "seconds"
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
          "id": "5e28de73b153391e15233ea51089874eb2544db7",
          "message": "xcm: Do not require `Asset` to be sorted on `decode` (#9842)\n\n`Asset` was requiring that all the assets are sorted at decoding. This\nis quite confusing for people writingg frontends, because this is not\nreally documented anywhere. There are also only at max 20 assets\navailable, we can just make everyones life easier and always sort the\nassets after decoding.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-27T06:33:19Z",
          "tree_id": "5d2e6f3e3e75d9c01fd3a700060f11ca07081231",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5e28de73b153391e15233ea51089874eb2544db7"
        },
        "date": 1758958983586,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008616151609999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026457856600000007,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005126657559999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "indirection42@outlook.com",
            "name": "Jiyuan Zheng",
            "username": "indirection42"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "615f664b4d250d627cc4fb84e2ca434f04664159",
          "message": "Add oracle pallet (part of Polkadot Stablecoin prerequisites) (#9815)\n\n# Description\nThis PR is part of #9765.\nThis PR introduces `pallet-oracle`, a new FRAME pallet that provides a\ndecentralized and trustworthy way to bring external, off-chain data onto\nthe blockchain. The pallet allows a configurable set of oracle operators\nto feed data, such as prices, into the system, which can then be\nconsumed by other pallets.\n\n## Integration\n\n### For Runtime Developers\n\nTo integrate `pallet-oracle` into your runtime:\n\n1. **Add dependency to your runtime's `Cargo.toml`**:\n\n   ```toml\n   pallet-oracle = { version = \"1.0.0\", default-features = false }\n   ```\n\n2. **Implement the `Config` trait** in your runtime:\n\n   ```rust\n   impl pallet_oracle::Config for Runtime {\n       type OnNewData = ();\n       type CombineData = pallet_oracle::DefaultCombineData;\n       type Time = Timestamp;\n       type OracleKey = AssetId;  // Your key type\n       type OracleValue = Price;     // Your value type\n       type RootOperatorAccountId = RootOperatorAccountId;\n       type Members = OracleMembers;\ntype WeightInfo = pallet_oracle::weights::SubstrateWeight<Runtime>;\n       type MaxHasDispatchedSize = ConstU32<100>;\n       type MaxFeedValues = ConstU32<50>;\n   }\n   ```\n\n3. **Add to `construct_runtime!`**:\n\n   ```rust\n   construct_runtime!(\n       pub enum Runtime {\n           // ... other pallets\n           Oracle: pallet_oracle,\n       }\n   );\n   ```\n\n### For Pallet Developers\n\nOther pallets can consume oracle data using the `DataProvider` trait:\n\n```rust\nuse pallet_oracle::traits::DataProvider;\n\n// Get current price\nif let Some(price) = <pallet_oracle::Pallet<T> as DataProvider<CurrencyId, Price>>::get(&currency_id) {\n    // Use the price data\n}\n```\n\n## Review Notes\n\n### Key Features\n\n- **Decentralized Data Feeding**: Uses `SortedMembers` trait to manage\noracle operators, allowing integration with `pallet-membership`\n- **Flexible Data Aggregation**: Configurable `CombineData`\nimplementation with default median-based aggregation\n- **Timestamped Data**: All data includes timestamps for freshness\nvalidation\n- **Root Operator Support**: Special account that can bypass membership\nchecks for emergency data updates\n- **Data Provider Traits**: Implements `DataProvider` and\n`DataProviderExtended` for easy consumption by other pallets\n\n### Implementation Details\n\nThe pallet uses a two-tier storage approach:\n\n- `RawValues`: Stores individual operator submissions with timestamps\n- `Values`: Stores aggregated values after applying the `CombineData`\nlogic\n\n### Security Considerations\n\n- Only authorized members can feed data (enforced via `SortedMembers`)\n- Root operator can bypass membership checks for emergency situations\n- One submission per operator per block to prevent spam\n- Configurable limits on maximum feed values per transaction\n\n### Testing\n\nThe pallet includes comprehensive tests covering:\n\n- Basic data feeding and retrieval\n- Member management and authorization\n- Data aggregation logic\n- Edge cases and error conditions\n- Benchmarking for weight calculation\n\n### Files Added\n\n- `substrate/frame/honzon/oracle/` - Complete pallet implementation\n- `substrate/frame/honzon/oracle/README.md` - Comprehensive\ndocumentation\n- Integration into umbrella workspace and node runtime\n- Runtime API for off-chain access to oracle data\n\n### Breaking Changes\n\nNone - this is a new pallet addition.\n\n### Migration Guide\n\nNo migration required - this is a new feature.\n\n# Checklist\n\n- [x] My PR includes a detailed description as outlined in the\n\"Description\" and its two subsections above.\n- [x] My PR follows the [labeling\nrequirements](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process)\nof this project (at minimum one label for `T` required)\n- External contributors: ask maintainers to put the right label on your\nPR.\n- [ ] I have made corresponding changes to the documentation (if\napplicable)\n- [x] I have added tests that prove my fix is effective or that my\nfeature works (if applicable)\n\n---------\n\nCo-authored-by: Bryan Chen <xlchen1291@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-28T20:42:49Z",
          "tree_id": "2ccab4db015604d87451d953a160f3a7f916e015",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/615f664b4d250d627cc4fb84e2ca434f04664159"
        },
        "date": 1759096391132,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008872960009999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005323426379999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027538489200000012,
            "unit": "seconds"
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
          "id": "2c0ed3c7aa804e290c29a93c31e5f82418185475",
          "message": "[pallet-revive] update rpc metadata (#9853)\n\nUpdate eth-rpc metadata files\n\nthe metadata should have been updated here\nhttps://github.com/paritytech/polkadot-sdk/pull/9759\nwhere a new variant was added to an enum used by the runtime api\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-29T11:21:45+02:00",
          "tree_id": "47f0d871cb3009fb0278692939a95d85a3bd4afc",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2c0ed3c7aa804e290c29a93c31e5f82418185475"
        },
        "date": 1759139739599,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008513801649999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005018024429999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00262739283,
            "unit": "seconds"
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
          "distinct": false,
          "id": "f239e76aadf90ed1023debaef155710239f9d865",
          "message": "Update `pallet-asset-rewards` to use BlockNumberProvider (#9826)\n\nresolves #9816\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-29T10:10:57Z",
          "tree_id": "f2485c637c99e6de2f3288a27f9353058cbe2d69",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f239e76aadf90ed1023debaef155710239f9d865"
        },
        "date": 1759144987122,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008721447119999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005167388659999986,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026726371500000005,
            "unit": "seconds"
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
          "distinct": false,
          "id": "d79598b79213c8d6e557115ed057d4894e6cd787",
          "message": "pallet-revive: allow changing immutables (#9801)\n\n... to be used in polkadot foundry to make sure EVM state is in sync\nwith pallet-revive state.\n\nFixes: https://github.com/paritytech/foundry-polkadot/issues/277\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-09-29T13:58:56Z",
          "tree_id": "cea97acfc73c4a52ceebe945241070a046fb01da",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d79598b79213c8d6e557115ed057d4894e6cd787"
        },
        "date": 1759158612400,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025963164599999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008462689899999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005106933339999992,
            "unit": "seconds"
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
          "id": "50372ea7fa3601d43db7e9200d8b249c67dbdf66",
          "message": "Allow sending transactions from an Ethereum address derived account id (#8757)\n\nWe always allowed signing transactions using an Bitcoin/Eth style\nSECP256k1 key. The account in this case is simply the blake2 hash of the\npublic key.\n\nThis address derivation is problematic: It requires the public key in\norder to derive the account id. On Ethereum you simply can't know the\npublic key of an address. This is why the mapping in pallet_revive is\ndefined as `address <-> account_id`.\n\nThis PR adds a new signature variant that allows signing a transaction\nwith an account id as origin that matches this mapping.\n\n## Why is this important?\n\n### Example1 \nA wallet contains an SECP256k1 key and wants to interact with native\nPolkadot APIs. It can sign the transaction using this key. However,\nwithout this change the origin of that transaction will be different\nthan the one it would appear under if it had signed an Ethereum\ntransaction.\n\n### Example2\nA chain using an Ethereum style address (like Mythical) wants to send\nsome tokens to one of their users account on AssetHub. How would they\nknow what is the address of that user on AssetHub? With this change they\ncan just pad the address with `0xEE` and rely on the fact that the user\ncan interact with AssetHub using their existing key.\n\n## Why a new variant?\nWe can't modify the existing variant. Otherwise the same signature would\nsuddenly map to a different account making people lose access to their\nfunds. Instead, we add a new variant that adds control over an\nadditional account for the same signature.\n\n## A new `KeccakSigner` and `KeccakSignature`\n\nAfter considering feedback by @Moliholy I am convinced that we should\nuse keccak instead of blake2b for this new `MultiSignature` variant.\nReasoning is that this will make it much simpler for Ethereum tooling to\ngenerate such signatures. Since this signature is specifically created\nfor Ethereum interop it just makes sense to also use keccak here.\n\nTo that end I made the `ecdsa::{KeccakSigner, KeccakSignature}` generic\nover their hash algorithm. Please note that I am using tags here and not\nthe `Hasher` trait directly. This makes things more complicated but it\nwas necessary: All Hasher implementations are in higher level crates and\ncan't be directly referenced here. But I would have to reference it in\norder to make this a non breaking change. The `Signer` and `Signature`\ntypes behave exactly the same way as before.\n\n---------\n\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-29T14:11:17Z",
          "tree_id": "30ab5c454ed6fb858003e6b6e13f2d4212883b0b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/50372ea7fa3601d43db7e9200d8b249c67dbdf66"
        },
        "date": 1759160959710,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.0051223089199999945,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026204728199999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00855498498999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "roberthambrock@gmail.com",
            "name": "Robert Hambrock",
            "username": "Lederstrumpf"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "66e9b9a941acd8ffa0b16b796323dc91dd5d25cf",
          "message": "Add `mmr_generateAncestryProof` rpc call (#9295)\n\n# Description\n\nAdds `generateAncestryProof` to the mmr RPC. An RPC method for\ngenerating ancestry proofs is required for cross-chain slashing by\ncross-chain fishermen https://github.com/Snowfork/snowbridge/pull/1493.\nConsequently, this PR also adds the mmr runtime api method\n`generate_ancestry_proof`. While such a method was already exposed by\nthe beefy-mmr runtime api, this PR opts for moving it to the mmr runtime\napi instead due to the following considerations:\n1. Invoking beefy-mmr's `generate_proof` method via RPC would require\nadding the offchain-db extension to the beefy-rpc, which is a more\ninvasive change with boilerplate that's not needed since the mmr RPC\nalready uses the offchain-db extension for generating leaf proofs.\n2. Since the ancestry proofs are for MMR, it is more natural to expose\nthe method directly on the mmr runtime api - the beefy-mmr pallet's\n`generate_proof` method is merely a wrapper around the mmr pallet's\n`generate_ancestry_proof` method.\n\nSome other misc. changes documented under `Review Notes`.\n\n## Integration\n\nThe integration is the same as for the beefy-mmr runtime api's\n`generate_proof` method.\n\n~~The integration is the same as for the beefy-mmr runtime api's\n`generate_proof` method, except that the optional `at` specifier is\nremoved for the method here since the method is idempotent wrt. the\nblock height invoked at, so long as `at` >= `best_known_block_number`.\nRemoving the specifier reduces likelihood of spurious errors from\nincorrect usage. I can revert the `at` specifier removal however if\ndesired for compatibility.~~\n\nFor example use, see https://github.com/Snowfork/snowbridge/pull/1493.\n\n## Review Notes\n\n- Adds `generate_ancestry_proof` method to mmr runtime api\n(https://github.com/lederstrumpf/polkadot-sdk/commit/682eb4a1411f7194a7277606b7ebe3688e8d5df1)\n- Adds `mmr_generateAncestryProof` rpc method\n(https://github.com/lederstrumpf/polkadot-sdk/commit/5d0eac9f1f48f049bec9846ef497763f5c2fc950,\nhttps://github.com/lederstrumpf/polkadot-sdk/commit/5d0eac9f1f)\n- Adds new `InvalidEquivocationProofSessionMember` error to beefy pallet\n(https://github.com/lederstrumpf/polkadot-sdk/commit/682eb4a141) (note:\nthis change is unrelated to the PR's main purpose, but helps\nimplementers with more granular error reporting. I'm open to removing\nthis change).\n- Deprecates\n`pallet_beefy::generate_ancestry_proof::generate_ancestry_proof` and\n`pallet_beefy::AncestryHelper::generate_proof`. Deprecation penciled in\nfor September 2025 - I'm open to change this date or undo the\ndeprecation.\n(https://github.com/lederstrumpf/polkadot-sdk/commit/6619169ecd)\n- ~~Removes `at` specifier for `pallet_mmr::generate_ancestry_proof`\n(https://github.com/lederstrumpf/polkadot-sdk/commit/bcadda2ce67cdb472b19db722d1ea3827e5869a5)\n(as mentioned in the `Integration` section, fine to undo)~~ *(Update:\nreverted removal of `at` specifier to allow fork handling).*\n\nPR can be tested using https://github.com/Snowfork/snowbridge/pull/1493.\n\nIf PR's approach is accepted, will open the associated PRs in\nhttps://github.com/polkadot-fellows/runtimes &\nhttps://github.com/polkadot-js/api.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-09-29T17:19:47Z",
          "tree_id": "81385a89241a9410f0f3e433b7ca69e1145f2455",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/66e9b9a941acd8ffa0b16b796323dc91dd5d25cf"
        },
        "date": 1759170679840,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00514543573999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008589132869999985,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267220418,
            "unit": "seconds"
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
          "id": "c3f62bf918ef6879390dc6a2cf9f91caac23f5b5",
          "message": "[pallet_transaction_payment]: Share withdrawn tx fee credit with other pallets (#9780)\n\nReplaces https://github.com/paritytech/polkadot-sdk/pull/9590.\n\nThe audit of #9590 showed that holding the txfee as held balance and\nespecially playing around with `providers` causes a lot of troubles.\n\nThis PR is a much lighter change. It keeps the original withdraw/deposit\npattern. It simply stores the withdrawn `Credit` and allows other\npallets to withdraw from it.\n\nIt is also better in terms of performance since all tx signers share a\nsingle storage item (instead of a named hold per account).\n\n---------\n\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-29T22:19:05Z",
          "tree_id": "6e4aac4ae217869a0122375a4f1a02bc4bbf3da7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c3f62bf918ef6879390dc6a2cf9f91caac23f5b5"
        },
        "date": 1759188680746,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008476332449999992,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050225210999999955,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025886531499999996,
            "unit": "seconds"
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
          "id": "28dda33b18ce3d1a3e50fe728b2cfb8b84a97f79",
          "message": "Bump the ci_dependencies group with 2 updates (#9858)\n\nBumps the ci_dependencies group with 2 updates:\n[actions/cache](https://github.com/actions/cache) and\n[actions-rust-lang/setup-rust-toolchain](https://github.com/actions-rust-lang/setup-rust-toolchain).\n\nUpdates `actions/cache` from 4.2.4 to 4.3.0\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions/cache/releases\">actions/cache's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v4.3.0</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Add note on runner versions by <a\nhref=\"https://github.com/GhadimiR\"><code>@​GhadimiR</code></a> in <a\nhref=\"https://redirect.github.com/actions/cache/pull/1642\">actions/cache#1642</a></li>\n<li>Prepare <code>v4.3.0</code> release by <a\nhref=\"https://github.com/Link\"><code>@​Link</code></a>- in <a\nhref=\"https://redirect.github.com/actions/cache/pull/1655\">actions/cache#1655</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/GhadimiR\"><code>@​GhadimiR</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/actions/cache/pull/1642\">actions/cache#1642</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions/cache/compare/v4...v4.3.0\">https://github.com/actions/cache/compare/v4...v4.3.0</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions/cache/blob/main/RELEASES.md\">actions/cache's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Releases</h1>\n<h3>4.3.0</h3>\n<ul>\n<li>Bump <code>@actions/cache</code> to <a\nhref=\"https://redirect.github.com/actions/toolkit/pull/2132\">v4.1.0</a></li>\n</ul>\n<h3>4.2.4</h3>\n<ul>\n<li>Bump <code>@actions/cache</code> to v4.0.5</li>\n</ul>\n<h3>4.2.3</h3>\n<ul>\n<li>Bump <code>@actions/cache</code> to v4.0.3 (obfuscates SAS token in\ndebug logs for cache entries)</li>\n</ul>\n<h3>4.2.2</h3>\n<ul>\n<li>Bump <code>@actions/cache</code> to v4.0.2</li>\n</ul>\n<h3>4.2.1</h3>\n<ul>\n<li>Bump <code>@actions/cache</code> to v4.0.1</li>\n</ul>\n<h3>4.2.0</h3>\n<p>TLDR; The cache backend service has been rewritten from the ground up\nfor improved performance and reliability. <a\nhref=\"https://github.com/actions/cache\">actions/cache</a> now integrates\nwith the new cache service (v2) APIs.</p>\n<p>The new service will gradually roll out as of <strong>February 1st,\n2025</strong>. The legacy service will also be sunset on the same date.\nChanges in these release are <strong>fully backward\ncompatible</strong>.</p>\n<p><strong>We are deprecating some versions of this action</strong>. We\nrecommend upgrading to version <code>v4</code> or <code>v3</code> as\nsoon as possible before <strong>February 1st, 2025.</strong> (Upgrade\ninstructions below).</p>\n<p>If you are using pinned SHAs, please use the SHAs of versions\n<code>v4.2.0</code> or <code>v3.4.0</code></p>\n<p>If you do not upgrade, all workflow runs using any of the deprecated\n<a href=\"https://github.com/actions/cache\">actions/cache</a> will\nfail.</p>\n<p>Upgrading to the recommended versions will not break your\nworkflows.</p>\n<h3>4.1.2</h3>\n<ul>\n<li>Add GitHub Enterprise Cloud instances hostname filters to inform API\nendpoint choices - <a\nhref=\"https://redirect.github.com/actions/cache/pull/1474\">#1474</a></li>\n<li>Security fix: Bump braces from 3.0.2 to 3.0.3 - <a\nhref=\"https://redirect.github.com/actions/cache/pull/1475\">#1475</a></li>\n</ul>\n<h3>4.1.1</h3>\n<ul>\n<li>Restore original behavior of <code>cache-hit</code> output - <a\nhref=\"https://redirect.github.com/actions/cache/pull/1467\">#1467</a></li>\n</ul>\n<h3>4.1.0</h3>\n<ul>\n<li>Ensure <code>cache-hit</code> output is set when a cache is missed -\n<a\nhref=\"https://redirect.github.com/actions/cache/pull/1404\">#1404</a></li>\n<li>Deprecate <code>save-always</code> input - <a\nhref=\"https://redirect.github.com/actions/cache/pull/1452\">#1452</a></li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions/cache/commit/0057852bfaa89a56745cba8c7296529d2fc39830\"><code>0057852</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/actions/cache/issues/1655\">#1655</a>\nfrom actions/Link-/prepare-4.3.0</li>\n<li><a\nhref=\"https://github.com/actions/cache/commit/4f5ea67f1cc87b2d4239690fa12a12fc32096d68\"><code>4f5ea67</code></a>\nUpdate licensed cache</li>\n<li><a\nhref=\"https://github.com/actions/cache/commit/9fcad95d03062fb8399cdbd79ae6041c7692b6c8\"><code>9fcad95</code></a>\nUpgrade actions/cache to 4.1.0 and prepare 4.3.0 release</li>\n<li><a\nhref=\"https://github.com/actions/cache/commit/638ed79f9dc94c1de1baef91bcab5edaa19451f4\"><code>638ed79</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/actions/cache/issues/1642\">#1642</a>\nfrom actions/GhadimiR-patch-1</li>\n<li><a\nhref=\"https://github.com/actions/cache/commit/3862dccb1765f1ff6e623be1f4fd3a5b47a30d27\"><code>3862dcc</code></a>\nAdd note on runner versions</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions/cache/compare/0400d5f644dc74513175e3cd8d07132dd4860809...0057852bfaa89a56745cba8c7296529d2fc39830\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\nUpdates `actions-rust-lang/setup-rust-toolchain` from 1.15.0 to 1.15.1\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/releases\">actions-rust-lang/setup-rust-toolchain's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v1.15.1</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Bump Swatinem/rust-cache from 2.8.0 to 2.8.1 by <a\nhref=\"https://github.com/dependabot\"><code>@​dependabot</code></a>[bot]\nin <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/pull/73\">actions-rust-lang/setup-rust-toolchain#73</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.15.0...v1.15.1\">https://github.com/actions-rust-lang/setup-rust-toolchain/compare/v1.15.0...v1.15.1</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/blob/main/CHANGELOG.md\">actions-rust-lang/setup-rust-toolchain's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Changelog</h1>\n<p>All notable changes to this project will be documented in this\nfile.</p>\n<p>The format is based on <a\nhref=\"https://keepachangelog.com/en/1.0.0/\">Keep a Changelog</a>,\nand this project adheres to <a\nhref=\"https://semver.org/spec/v2.0.0.html\">Semantic Versioning</a>.</p>\n<h2>[Unreleased]</h2>\n<h2>[1.15.1] - 2025-09-23</h2>\n<ul>\n<li>Update <code>Swatinem/rust-cache</code> to v2.8.1</li>\n</ul>\n<h2>[1.15.0] - 2025-09-14</h2>\n<ul>\n<li>Add support for non-root source directory.\nAccept source code and <code>rust-toolchain.toml</code> file in\nsubdirectories of the repository.\nAdds a new parameter <code>rust-src-dir</code> that controls the lookup\nfor toolchain files and sets a default value for the\n<code>cache-workspace</code> input. (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/69\">#69</a>\nby <a href=\"https://github.com/Kubaryt\"><code>@​Kubaryt</code></a>)</li>\n</ul>\n<h2>[1.14.1] - 2025-08-28</h2>\n<ul>\n<li>Pin <code>Swatinem/rust-cache</code> action to a full commit SHA (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/68\">#68</a>\nby <a\nhref=\"https://github.com/JohnTitor\"><code>@​JohnTitor</code></a>)</li>\n</ul>\n<h2>[1.14.0] - 2025-08-23</h2>\n<ul>\n<li>Add new parameters <code>cache-all-crates</code> and\n<code>cache-workspace-crates</code> that are propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-all-crates</code> and\n<code>cache-workspace-crates</code></li>\n</ul>\n<h2>[1.13.0] - 2025-06-16</h2>\n<ul>\n<li>Add new parameter <code>cache-provider</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-provider</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/65\">#65</a>\nby <a\nhref=\"https://github.com/mindrunner\"><code>@​mindrunner</code></a>)</li>\n</ul>\n<h2>[1.12.0] - 2025-04-23</h2>\n<ul>\n<li>Add support for installing rustup on Windows (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/58\">#58</a>\nby <a href=\"https://github.com/maennchen\"><code>@​maennchen</code></a>)\nThis adds support for using Rust on the GitHub provided Windows ARM\nrunners.</li>\n</ul>\n<h2>[1.11.0] - 2025-02-24</h2>\n<ul>\n<li>Add new parameter <code>cache-bin</code> that is propagated to\n<code>Swatinem/rust-cache</code> as <code>cache-bin</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/51\">#51</a>\nby <a\nhref=\"https://github.com/enkhjile\"><code>@​enkhjile</code></a>)</li>\n<li>Add new parameter <code>cache-shared-key</code> that is propagated\nto <code>Swatinem/rust-cache</code> as <code>shared-key</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/52\">#52</a>\nby <a\nhref=\"https://github.com/skanehira\"><code>@​skanehira</code></a>)</li>\n</ul>\n<h2>[1.10.1] - 2024-10-01</h2>\n<ul>\n<li>Fix problem matcher for rustfmt output.\nThe format has changed since <a\nhref=\"https://redirect.github.com/rust-lang/rustfmt/pull/5971\">rust-lang/rustfmt#5971</a>\nand now follows the form &quot;filename:line&quot;.\nThanks to <a\nhref=\"https://github.com/0xcypher02\"><code>@​0xcypher02</code></a> for\npointing out the problem.</li>\n</ul>\n<h2>[1.10.0] - 2024-09-23</h2>\n<ul>\n<li>Add new parameter <code>cache-directories</code> that is propagated\nto <code>Swatinem/rust-cache</code> (<a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/44\">#44</a>\nby <a\nhref=\"https://github.com/pranc1ngpegasus\"><code>@​pranc1ngpegasus</code></a>)</li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/02be93da58aa71fb456aa9c43b301149248829d8\"><code>02be93d</code></a>\nUpdate <code>Swatinem/rust-cache</code> to v2.8.1</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/69e48024603c91b996af4004a08116c7b9bf95c1\"><code>69e4802</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/actions-rust-lang/setup-rust-toolchain/issues/73\">#73</a>\nfrom actions-rust-lang/dependabot/github_actions/Swati...</li>\n<li><a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/commit/183cfebcbd070909e5077c3b4a44326e8e8418f5\"><code>183cfeb</code></a>\nBump Swatinem/rust-cache from 2.8.0 to 2.8.1</li>\n<li>See full diff in <a\nhref=\"https://github.com/actions-rust-lang/setup-rust-toolchain/compare/2fcdc490d667999e01ddbbf0f2823181beef6b39...02be93da58aa71fb456aa9c43b301149248829d8\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore <dependency name> major version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's major version (unless you unignore this specific\ndependency's major version or upgrade to it yourself)\n- `@dependabot ignore <dependency name> minor version` will close this\ngroup update PR and stop Dependabot creating any more for the specific\ndependency's minor version (unless you unignore this specific\ndependency's minor version or upgrade to it yourself)\n- `@dependabot ignore <dependency name>` will close this group update PR\nand stop Dependabot creating any more for the specific dependency\n(unless you unignore this specific dependency or upgrade to it yourself)\n- `@dependabot unignore <dependency name>` will remove all of the ignore\nconditions of the specified dependency\n- `@dependabot unignore <dependency name> <ignore condition>` will\nremove the ignore condition of the specified dependency and ignore\nconditions\n\n\n</details>\n\n---------\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2025-09-30T10:38:51+02:00",
          "tree_id": "a3afe168fd1dfafe2cbf054dd108250dd0200235",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/28dda33b18ce3d1a3e50fe728b2cfb8b84a97f79"
        },
        "date": 1759223626287,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00871144795999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026169470600000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005181347619999992,
            "unit": "seconds"
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
          "distinct": true,
          "id": "cfce3b96be3fe7348c88ba1deeaa701834240d38",
          "message": "`pallet-assets`: extract precompiles to a separate crate (#9796)\n\ncloses #9434 \n\n###  Description\n\nAssets pallet includes `pallet-revive` precompiles and subsequently pull\na lot of EVM related dependencies by default. This forces downstream\nusers that only want `pallet-assets` functionality to pull unrelated\ndependencies and causes confusion (why do we have bunch of ethereum\ncrates in the dependency tree of `pallet-assets`?). This extracts\nprecompiles into its own crate\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-09-30T09:11:50Z",
          "tree_id": "5425894d473ec0ee79c528aca01d4f7facb1e1dd",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cfce3b96be3fe7348c88ba1deeaa701834240d38"
        },
        "date": 1759227670218,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008678298649999985,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005176614969999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026747332699999997,
            "unit": "seconds"
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
          "id": "bfa664265cc94e201f297f070b8eadb90c634f64",
          "message": "bip39: Switch back to the main fork (#9872)\n\nClose: https://github.com/paritytech/polkadot-sdk/issues/9870\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-30T10:10:01Z",
          "tree_id": "9d0883eeb9d403dfa86abebc12991c0c05509f3f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bfa664265cc94e201f297f070b8eadb90c634f64"
        },
        "date": 1759232635244,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008588043069999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005120582109999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265915549,
            "unit": "seconds"
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
          "distinct": false,
          "id": "7648b4105c269d6a3395d90b053c2e04c5932bc3",
          "message": "Stronger WASM compression (#9875)\n\nUse strongest compression 22 instead of just 3. See\n[docs](https://docs.rs/zstd/0.13.3/zstd/stream/write/struct.Encoder.html#method.new).\nReduces our KAH compressed size by 25%.\n\nBuild time by compression level:\n\n| Compression | Build Time | Size    | Decomp Time |\n  |-------------|-----------|---------|-------------|\n  | 3           | 5:54      | 3192172 | 0.013039s   |\n  | 10          | 5:58      | 2716940 | 0.011962s   |\n  | 22          | 6:06      | 2387562 | 0.013745s   |\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2025-09-30T11:05:07Z",
          "tree_id": "2fe20eaaaaff302082b5a4507a9302b2265ff7ef",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7648b4105c269d6a3395d90b053c2e04c5932bc3"
        },
        "date": 1759236595978,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005180386099999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00255385359,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008483787379999993,
            "unit": "seconds"
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
          "id": "0e272fe76b037587908b52110a485749661aec97",
          "message": "Snowbridge: Refactor with Alloy primitives and clean up code (#9204)\n\nJust some cleanup — not blocking the V2 release if it can’t be included\nin time.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-09-30T12:45:39Z",
          "tree_id": "764fd3841a44da693a68d786bc5648b6f8bbc33f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0e272fe76b037587908b52110a485749661aec97"
        },
        "date": 1759241111746,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008629851509999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005141926839999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026021014500000016,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "22591718+RomarQ@users.noreply.github.com",
            "name": "Rodrigo Quelhas",
            "username": "RomarQ"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "2732b6a9157a9ac501cac43ca46f6abffc90f275",
          "message": "Remove deprecated/unused consensus code (#9869)\n\nThis pull request removes unused consensus-related code from the\ncodebase. The removed code was not referenced or required by any\nexisting logic or features, and its removal helps to:\n\n- Reduce technical debt\n- Simplify the codebase for future maintenance",
          "timestamp": "2025-09-30T19:30:48Z",
          "tree_id": "9b60638e2014955757ab321bdbfd3379674e622e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2732b6a9157a9ac501cac43ca46f6abffc90f275"
        },
        "date": 1759266605567,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00268219157,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0052733936599999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008780928109999983,
            "unit": "seconds"
          }
        ]
      },
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
          "distinct": true,
          "id": "ed4eebb461069f65fda4a88d44ee811dd8c010e3",
          "message": "Fix the deadlock during statements gossiping (#9868)\n\n# Description\n\nDuring statement store benchmarking we experienced deadlock-like\nbehavior which we found happened during statement propagation. Every\nsecond statements were propagating, locking the index which possibly\ncaused the deadlock. After the fix, the observed behavior no longer\noccurs.\n\nEven though there is a possibility to unsync the DB and the index for\nread operations and release locks earlier, which should be harmless, it\nleads to regressions. I suspect because of concurrent access to many\ncalls of db.get(). Checked with the benchmarks in\nhttps://github.com/paritytech/polkadot-sdk/pull/9884\n\n## Integration\n\nThis PR should not affect downstream projects.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-09-30T22:23:42Z",
          "tree_id": "c92518817efc8cc35ba8d467b45174e5df2e8d5d",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ed4eebb461069f65fda4a88d44ee811dd8c010e3"
        },
        "date": 1759275336525,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008663985769999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005041456289999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026437621600000002,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "d51c532f07c7e3307718b95f5a1f8859e14949a0",
          "message": "Collation metrics: exclude drops of fork-based collations to improve metrics accuracy (#9319)\n\n# Description\n\nThe polkadot_parachain_collation_expired metric is an indicator for\nparachain block confidence. However, this metric has a critical issue:\nnot every drop should be counted.\n\nLookahead collators intentionally build collations on a relay chain\nblock and its forks, so the drop of fork-based collations is an expected\nbehaviour. If we count them, the drop metrics show a picture that is\nworse than in reality. To improve tracking accuracy, we should exclude\nlegit drops\n\nThe minor issue is also present in the expiry mechanism. It doesn't take\ninto account that collation was moved to a different stage, e.g., from\n\"fetched\" to \"backed\", and can write a drop of fetched collation.\n\nTo solve this issue we should:\n\n- Track relay parent finalization. \n- Record expiration metrics only when relay parent was finalized. \n- Exclude drops of fork-based collation from the metrics. \n- Send metrics only for collations that either finalized or dropped.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-01T07:45:42Z",
          "tree_id": "5ff1ff060f12f57e2d514725daf23b84a820589a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d51c532f07c7e3307718b95f5a1f8859e14949a0"
        },
        "date": 1759309614908,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00862652213999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025911995899999996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005184620579999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "robertvaneerdewijk@gmail.com",
            "name": "0xRVE",
            "username": "0xRVE"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1320b70d33145fc379751864a77e6fc8186cec94",
          "message": "replace forloop solc fixture type with test-case macro (#9841)\n\nFor all tests of revm instructions replaced `for fixture_type` with\ntest-case macro\n\n---------\n\nCo-authored-by: Robert van Eerdewijk <robert@Roberts-MacBook-Pro.local>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-01T14:10:26Z",
          "tree_id": "e177de2bb84124c4df37c49fec3d858563d4fa2e",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1320b70d33145fc379751864a77e6fc8186cec94"
        },
        "date": 1759332166467,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002672466739999999,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008761595619999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005302948149999992,
            "unit": "seconds"
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
          "id": "8ac594d2f3b35a26eecfd892ac0b3d077ba809f9",
          "message": "Move revive fixtures into release directory (#9670)\n\nhttps://github.com/paritytech/polkadot-sdk/pull/8980 did fix the\nfellowship CI but it triggers a rebuild of the fixtures every single\ntime you run tests. Annoying during development.\n\nInstead of rebuilding, we just move the fixtures into the\n`target/release` directory where it should be cached by the fellowship\nCI.\n\nVerifying that it works here:\nhttps://github.com/polkadot-fellows/runtimes/pull/891\n\nWhy: Re-running when the output dir changes will make it re-run every\ntime. Since every run changes the output dir.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-01T20:52:36Z",
          "tree_id": "117de0ce30ead499d41c4c16996b474e72e4f7c7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8ac594d2f3b35a26eecfd892ac0b3d077ba809f9"
        },
        "date": 1759356264556,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026027525900000005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005187634099999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008597687629999986,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "54316454+sandreim@users.noreply.github.com",
            "name": "Andrei Sandu",
            "username": "sandreim"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "cf439301b2a9571e5fcb04e4550167a878187182",
          "message": "Bump PARENT_SEARCH_DEPTH  (#9906)\n\nA chain needs to have the `UNINCLUDED_SEGMENT_CAPACITY` configured to\n`(2 + RELAY_PARENT_OFFSET) *\nBLOCK_PROCESSING_VELOCITY + 1`.\n\nWhen the parachain is configured to build 12 blocks per relay parent and\nthe relay parent offset is 1, this number(37) is higher than the current\n`PARENT_SEARCH_DEPTH=30`.\n\nThis PR raises the limit to be sufficient for the larger unincluded\nsegment.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-02T09:28:36Z",
          "tree_id": "809add5f79c62c55ecf2cbb472f72251964c5e75",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cf439301b2a9571e5fcb04e4550167a878187182"
        },
        "date": 1759401551209,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008629762689999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026489266400000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005162541989999993,
            "unit": "seconds"
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
          "id": "bf235845f9ecb2b84264f255afd7aefdd5ddb603",
          "message": "[pallet-revive] rm checked-in metadata (#9865)\n\nRemoved eth-rpc generated metadata.\nThe metadata file will now be generated from the build.rs using the\nAH-westend runtime\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-03T09:35:39Z",
          "tree_id": "0b74083f3e62a8264adb9c7d7898749c2c904364",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bf235845f9ecb2b84264f255afd7aefdd5ddb603"
        },
        "date": 1759488453677,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005152817279999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025904835700000005,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008646406989999985,
            "unit": "seconds"
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
          "id": "3a366f1debbe9af536bf65b2c88d0fc18663c9f4",
          "message": "Rework gas mapping (#9803)\n\nReplacement of https://github.com/paritytech/polkadot-sdk/pull/9740.\n\nBuild on top of the new tx payment changes:\nhttps://github.com/paritytech/polkadot-sdk/pull/9780\n\nStarted a new PR because due to the rebase on top of the new tx payment\nchanges this PR is substantially different and I don't want to\ninvalidate the existing comments on\nhttps://github.com/paritytech/polkadot-sdk/pull/9740 which are not\nimplemented, yet.\n\n## Overview\n\nThis will change the weight to eth fee mapping according to [this\nmodel](https://shade-verse-e97.notion.site/Gas-Mapping-Challenges-Revised-26c8532a7ab580db8222c2ce3023669e).\n\n## Follow ups\n\nThis only changes the estimate returned from the dry run and how the\nweights are derived from an ethereum transaction. It does not change how\ncontracts observe the gas. This will be done in a follow up. More\nspecifically:\n\n1. The `GAS` opcode should return the new gas. As of right now it\nreturns `ref_time.`\n2. The `*_CALL` opcodes should use the passed `gas` parameter and decode\nit into `Weight`. As of right now the parameter is ignored.\n\nThat said, even without those follow ups this PR should fix all\n`InvalidTransaction` errors we are observing.\n\n### Increasing the dimension of `gas_price`\n\nWe should add a configurable divisor so that the gas_price is always at\nleast some gwei. That makes it easier to input the values.\n\n---------\n\nSigned-off-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>",
          "timestamp": "2025-10-03T13:53:27Z",
          "tree_id": "1c2b94a5b44dbb00e67fba2e82c49eb2c14b4241",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3a366f1debbe9af536bf65b2c88d0fc18663c9f4"
        },
        "date": 1759503781229,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005107024229999998,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008653061949999988,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00267471508,
            "unit": "seconds"
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
          "id": "3adf2e95a8531837891c7cfba7f0f25474531f60",
          "message": "pallet-revive: Bump PolkaVM (#9928)\n\nBumped `polkavm` to the latest version. No semantic changes in that\nupdate.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-03T16:09:05Z",
          "tree_id": "41b6f37d0c23a063be61148c793d0a9d212b5c48",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3adf2e95a8531837891c7cfba7f0f25474531f60"
        },
        "date": 1759511941264,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025780764799999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008521427609999993,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005149389559999998,
            "unit": "seconds"
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
          "id": "42510be8dec593b8149d439c2edac816424d81dc",
          "message": "CheckWeight: Take transaction length into account during validation (#9907)\n\nDuring validation of extrinsics we currently check that the benchmarked\nweight does not exceed the overall block limit for the given extrinsic\nclass. For the proof weight, this should also take into account the\nlength of the transaction. This creates parity with the checks we\nalready do at extrinsic application time where we take the length [into\naccount](https://github.com/paritytech/polkadot-sdk/blob/7086502b242c7984a13e198d2373e69a640e5c58/substrate/frame/system/src/extensions/check_weight.rs#L166-L166).\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-10-03T18:28:03Z",
          "tree_id": "ecc593ed4017b11a3b9068ba3578e633296989c5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/42510be8dec593b8149d439c2edac816424d81dc"
        },
        "date": 1759521154250,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005190702119999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00265494383,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008646981589999988,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "s.miasojed@gmail.com",
            "name": "Sebastian Miasojed",
            "username": "smiasojed"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "d36d48a25c80502c577df3b1a88eadb3b3d3b6be",
          "message": "[pallet-revive] Allows setting evm balance for non-existing account (#9911)\n\nAllows calling set_evm_balance for a non-existing account on\npallet-revive.\nIt is needed by foundry to inject EVM accounts.",
          "timestamp": "2025-10-04T08:29:58Z",
          "tree_id": "b5d7b6c44cb8e2b0c6f46622d02d14c56ee46bb7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d36d48a25c80502c577df3b1a88eadb3b3d3b6be"
        },
        "date": 1759571306093,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.00503508888,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008473730969999997,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026252330999999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "xlchen1291@gmail.com",
            "name": "Xiliang Chen",
            "username": "xlc"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "be423a31a5523934b9e5a4679fa254195811520d",
          "message": "Improve asset conversion (#9892)\n\nadded view functions #7374\nadded trait `MutateLiquidity` to allow other pallets to manage liquidity\n#9765\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Guillaume Thiolliere <guillaume.thiolliere@parity.io>",
          "timestamp": "2025-10-06T07:58:24Z",
          "tree_id": "e2833bd35476bbaa8d4884889609e0f94623f62f",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/be423a31a5523934b9e5a4679fa254195811520d"
        },
        "date": 1759741693074,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025626877200000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008477157639999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0050365908099999955,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "evgeny@parity.io",
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "0040bc9bd4f2a11edde9c30c7f0cbd65a3d14b08",
          "message": "Make `cargo-check-all-crate-macos` non-required (#9940)",
          "timestamp": "2025-10-06T16:05:45+04:00",
          "tree_id": "b7496b5160086277a2736288e9d932b7596812c4",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0040bc9bd4f2a11edde9c30c7f0cbd65a3d14b08"
        },
        "date": 1759754497368,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002616666729999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0052219470899999895,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008606527749999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "tsvetomir@parity.io",
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "9dd67bf672c3893de126452b8578cabc08bdccce",
          "message": "Additional Kusama and Polkadot invulnerable collator peer ids (#9936)\n\nPeerIds of some new invulnerable AH collators on Kusama and Polkadot.",
          "timestamp": "2025-10-06T13:48:42Z",
          "tree_id": "fe7b8aa561135c69b7db0f4b757a477090669f33",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9dd67bf672c3893de126452b8578cabc08bdccce"
        },
        "date": 1759763387684,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026145931099999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005225636840000003,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00860599761999999,
            "unit": "seconds"
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
          "id": "bbbe659465b26daddf4239bd106837f0527b4a5e",
          "message": "Fix all the warnings (#9943)\n\nFixes all the warnings that are appearing with the latest rust.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-06T19:10:41Z",
          "tree_id": "36680b82276fe6fd8daef20689b43b198dba8b94",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bbbe659465b26daddf4239bd106837f0527b4a5e"
        },
        "date": 1759782095498,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005413504239999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027040723500000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00893416503999998,
            "unit": "seconds"
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
          "id": "c2231626893533a91754384b62f9595ee97662d3",
          "message": "Snowbridge - Adds Fulu hardfork (#9938)\n\nAdds Fulu hardfork version. No other onchain changes required. \n\nFulu activation on Sepolia is 14 October, Mainnet 3 December.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2025-10-07T08:44:54Z",
          "tree_id": "90c6138e7315fe60f7f18d434c831d6c0b90ddd9",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c2231626893533a91754384b62f9595ee97662d3"
        },
        "date": 1759831050386,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025967429599999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00852521584999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005010206509999992,
            "unit": "seconds"
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
          "id": "1100fc4013a2b56255c2d2401e8832f761c27b9d",
          "message": "Fix executor param fetching session index (#9774)\n\nThis aims to fix #4292 \nWe have three cases of candidate validation where a proper set of\nexecutor environment parameters should be used:\n1) Backing, and we're currently doing the right thing, requesting the\nexecutor params at the relay parent at which the candidate was produced:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/e7f36ab82934a7142f3ebd7f8b5566f12f85339b/polkadot/node/core/backing/src/lib.rs#L1140-L1146\n2) Approval voting, where a wrong session was used, and this PR fixes\nthat;\n3) Disputes, where the session index, again, is hopefully derived from\nthe relay parent at which the candidate was produced:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/63958c454643ddafdde8be17af5334aa95954550/polkadot/node/subsystem-types/src/messages.rs#L295-L296\n\nSo, hopefully, this PR fixes the only wrong case and harmonizes the\nexecutor param fetching over all the existing use cases.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-07T16:34:21Z",
          "tree_id": "ab7eddf248c98875e44fb81f161711e49315b43b",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1100fc4013a2b56255c2d2401e8832f761c27b9d"
        },
        "date": 1759859194169,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00270207546,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008768862609999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005225672529999994,
            "unit": "seconds"
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
          "id": "0028bec86c19a8d75098a75cf7862a7f76569fcc",
          "message": "FinalityNotification: Directly include stale blocks (#9904)\n\nThe finality notification was already carrying the information about the\nstale heads. However, most users of the stale heads were expanding these\nstale heads to all the stale blocks. So, we were iterating the same\nforks multiple times in the node for each finality notification. Also in\na possible future where we start actually pruning headers as well,\nexpanding these forks would fail.\n\nSo, this pull request is changing the finality notification to directly\ncarry the stale blocks (which were calculated any way already).\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-07T18:53:50Z",
          "tree_id": "6dfb8de6c3c3d92f40d72ac424d374cb6e7bcc80",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0028bec86c19a8d75098a75cf7862a7f76569fcc"
        },
        "date": 1759867835869,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008585706869999987,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005273822649999993,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025727929599999996,
            "unit": "seconds"
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
          "id": "648cc16181998c8b53d51f1c996a866c81bed92f",
          "message": "pallet-revive: Fix dry run balance check logic (#9942)\n\nFix fault balance check logic during dry-run:\n\nWe should not enforce that the sender has enough balance for the fees in\ncase no `gas` is supplied.\n\ncc @TorstenStueber\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-08T08:03:22Z",
          "tree_id": "0299aca249bc9f342b43520d9a6de922c83ed2c7",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/648cc16181998c8b53d51f1c996a866c81bed92f"
        },
        "date": 1759915684405,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00256821832,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005038964669999991,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00848681020999999,
            "unit": "seconds"
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
          "id": "e32dee79990efc2cbccd0c0c0b86274772b9947d",
          "message": "RPC Spec V2: Fix flaky test (#9958)",
          "timestamp": "2025-10-08T10:49:22Z",
          "tree_id": "899d85cea6fc02d2a6983462a3cde1bdab98b9e1",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e32dee79990efc2cbccd0c0c0b86274772b9947d"
        },
        "date": 1759932531686,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00259605181,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00859025545999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005223893249999993,
            "unit": "seconds"
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
          "distinct": false,
          "id": "81abfd7af936260922f39f232943d303d0b55713",
          "message": "[DNM] Zombienet fix process logs (#9913)\n\nFix `send to loki` logic to support:\n - rstest (multiple tests per job)\n - fallback when network fail to spawn (zombie.json not present)\n \ncc: @DenzelPenzel\n\n---------\n\nCo-authored-by: DenzelPenzel <denis.samsonov@parity.io>\nCo-authored-by: Denzel <15388928+DenzelPenzel@users.noreply.github.com>",
          "timestamp": "2025-10-08T15:12:46Z",
          "tree_id": "52afc8743330bc09f004ec39ee64d793c8906200",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/81abfd7af936260922f39f232943d303d0b55713"
        },
        "date": 1759943956628,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002652846000000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008583040469999989,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005006279359999995,
            "unit": "seconds"
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
          "id": "92118ececd11ca1c02984f02fa3151b174fea2b3",
          "message": "Staking-Async: Chill stakers should not have a score (#9926)\n\nThat can re-instate them in the bags-list pallet\n\nIdentified by\nhttps://github.com/paritytech-secops/srlabs_findings/issues/559\n\nWhile no severe consequence, this bug could cause non-validator and\nnon-nominator stakers to retain a spot in the bags-list pallet,\npreventing other legit nominators/validators from taking their place.\n\nNote that previously, this was not a possibility, because `staking`\nwould always issue a `T::VoterList::on_remove` when someone `chill`s,\nensuring they are removed from the list. Moreover, an older version of\n`pallet_bags_list::Pallet::rebag` didn't allow new nodes to be added,\nonly the score of existing nodes to be adjusted.\n\nBut, in recent versions of `bags-list`, we added a `Lock` ability that\nwould block any changes to the bags list (during the election snapshot\nphase). This also had us update the `rebag` transaction to add or remove\nnodes from the list, which opened the door to this issue.\n\n---------\n\nCo-authored-by: Paolo La Camera <paolo@parity.io>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-08T18:12:57Z",
          "tree_id": "d940caa6f6c91d83a84cfa101206eacd525ba509",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92118ececd11ca1c02984f02fa3151b174fea2b3"
        },
        "date": 1759952593302,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008671722709999986,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005148057699999995,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026501654000000005,
            "unit": "seconds"
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
          "id": "b6c7f6e948b0d97d0907fc3475e35153d7670ec3",
          "message": "pallet-revive update basefee instruction (#9945)\n\nThe base fee instruction now returns the proper base price instead of a\nhard coded value.\n\n---------\n\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-08T20:38:43Z",
          "tree_id": "ce745e9150c1089c080fde908c63caec17e7b041",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6c7f6e948b0d97d0907fc3475e35153d7670ec3"
        },
        "date": 1759960722715,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025955095200000007,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008585115469999991,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005075182299999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Sajjon@users.noreply.github.com",
            "name": "Alexander Cyon",
            "username": "Sajjon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "1445af004bcac0ec3dd7b055a04bb1d7f9c110f1",
          "message": "Change `Debug` of ParaId to`\"<ID>\"` instead of `\"Id(<ID>)\"` (#9920)\n\nThe `std::fmt::Debug` impl (derived) of\n[`Id`](https://github.com/paritytech/polkadot-sdk/blob/32cc5d6163781a077c4bdb2cafdf1a538127ebd5/polkadot/parachain/src/primitives.rs#L176)\nresults in `\"Id(42)\"` instead of `\"42\"`, this causes discrepancies in\nlogs. Sometimes we log `\"para_id=Id(3392)\"` but sometimes we log\n`\"para_id=3392\"` (without the `\"Id()\"`).\n\nThis makes e.g. Grafana PromQL queries harder to do, and logs harder to\nsearch in general.\n\n## Example\nSeen in e.g.:\n\n### Without `ID(<ID>)`\n\n```\n2025-09-22 22:28:06.753 DEBUG tokio-runtime-worker parachain::candidate-backing: Candidate backed candidate_hash=0x0cd77dd25cb61040557bb66df8de5c7b637dff05a671765b40bea7222cfa2854 relay_parent=0x7d380118542b6180127a150a86f144c448982c6af40da1dfb9e3119ad7d4c1ab para_id=3392 traceID=17069631741811285016642302447800179835\n```\n\n### With `ID(<ID>)`\n\n```\n2025-09-22 22:28:06.342 DEBUG tokio-runtime-worker parachain::collator-protocol::stats: [Relaychain] Collation expired age=3 collation_state=\"fetched\" relay_parent=0x7b394ee6c4fa2a3cec8fa82fa2afe5a933314e6a64e09256d7c8a28086b16acf para_id=Id(3392) head=0x92959b2b5fe8aba268bce48baf8a4e6b952055984015b8c1c7c3142c60e66e31\n```\n\nThis PR changes the impl of `Debug` to be just `\"<ID>\"`.\n\n([Discussion on\nMatrix](https://matrix.to/#/!oRmZcZCtnViqLdLelR:parity.io/$mTaAp2dRtH5_xUkM-2Bc1NsR-hGEbhV2arGVzuq9g3o?via=parity.io))",
          "timestamp": "2025-10-09T07:30:25Z",
          "tree_id": "b5addd1bab93d6414d2240841e5ce7be2d72d353",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1445af004bcac0ec3dd7b055a04bb1d7f9c110f1"
        },
        "date": 1760001036150,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008453949089999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005031068679999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025974417600000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "mich@elmueller.net",
            "name": "Michael Müller",
            "username": "cmichi"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "efd1cd945911005a7ee946923b0c457d5c348e28",
          "message": "[pallet-revive] Migrate unstable storage host functions to `Storage` pre-compile (#9603)\n\nPart of closing https://github.com/paritytech/polkadot-sdk/issues/8572.\n\nIntroduces a new `Storage` pre-compile and migrates:\n* `clear_storage`\n* `take_storage`\n* `contains_storage`\n\nThe new `Storage` pre-compile is introduced, as it requires implementing\nthe `BuiltinPrecompile::call_with_info` function, which cannot be\nimplemented together with `BuiltinPrecompile::call` (implemented by the\n`System` pre-compile).\n\nI've added the `sol_utils` as I (on admittedly quick glance) couldn't\nfind a crate that supports those encodings (Solidity's `bytes`) without\nrequiring an allocator.\n\ncc @athei @pgherveou\n\n---------\n\nCo-authored-by: xermicus <bigcyrill@hotmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-10-09T11:42:58Z",
          "tree_id": "edf593ab8bee9b5fa253152dc2d04d2691820d5a",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efd1cd945911005a7ee946923b0c457d5c348e28"
        },
        "date": 1760014940275,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008514764369999987,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026304991499999995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.00514395131999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "Sajjon@users.noreply.github.com",
            "name": "Alexander Cyon",
            "username": "Sajjon"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1b1cef306d9ceebf963fd15a04b5c79ee2618bce",
          "message": "Fix bug in logging of expried collation, incorrect `relay_parent` used (#9976)\n\nFixes a bug where wrong `relay_parent` was logged about expired\ncollations",
          "timestamp": "2025-10-09T12:55:56Z",
          "tree_id": "8ead97054f7a37135b863fb8c8d207b56fc94f84",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1b1cef306d9ceebf963fd15a04b5c79ee2618bce"
        },
        "date": 1760020370659,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.005068058269999992,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0025977168899999994,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008531785469999985,
            "unit": "seconds"
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
          "id": "4697901f84463e423cf46cfbe84744f4c9ad1555",
          "message": "Return unified gas for `gas_left` syscalls and opcodes (#9968)\n\nIn https://github.com/paritytech/polkadot-sdk/pull/9803 we introduced\nthe new gas mapping. However, when contracts are querying the remaining\ngas we still returned the `ref_time`. This PR changes that.\n\n## Changes\n- Added a new `Stack::gas_left` function that calculates the remaining\ngas as eth gas that matches the gas passed in the transaction. It\nsupports both the `eth_` and non `eth_` flavors of dispatchables.\n- Changed the PVM syscall `ref_time_left` to return the new unified gas.\n- Changes the EVM `GAS` opcode to return the new unified gas\n- When calculating the consumed storage we now take into account what\nwas charged during the current frame\n- Removed `storage_deposit_limit` from `eth_*` dispatchables. It is\nalways uncapped in this case and the overall limit is conveyed using the\ntx credit.\n\n## Follow ups\nNow that we can return the proper remaining gas that also includes the\nstorage deposit we can change the EVM `call` instruction next to take\nthe passed `gas` into account. Since the unified gas takes both the\ntxfee and the deposit into account it will be able to limit both\neffectively.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: xermicus <cyrill@parity.io>",
          "timestamp": "2025-10-09T14:35:38Z",
          "tree_id": "542b34f5992d92006956917f952370a5e5861e98",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4697901f84463e423cf46cfbe84744f4c9ad1555"
        },
        "date": 1760030475023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026042298499999996,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.00856975748999999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005146759199999992,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "15388928+DenzelPenzel@users.noreply.github.com",
            "name": "Denzel",
            "username": "DenzelPenzel"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "5f69bea23dcd527dc22b696b3a7e7e5ee47e8083",
          "message": "chore: add issue numbers to flaky zombienet tests (#9957)\n\n# Description\n#9877 Improve tracking of disabled tests, add link to tracking issue.\n\n# Checklist\n\n* [x] Improve tracking of disabled tests, add link to tracking issue.\n\n---------\n\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>",
          "timestamp": "2025-10-09T20:00:03Z",
          "tree_id": "a9e08b82b5b3e5f06f6f1a303662a46ce3b63fd5",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5f69bea23dcd527dc22b696b3a7e7e5ee47e8083"
        },
        "date": 1760044186911,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008641460549999994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.0051585541999999915,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0027042380799999987,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "45130584+l0r1s@users.noreply.github.com",
            "name": "Loris Moulin",
            "username": "l0r1s"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": true,
          "id": "c189613a32a58745993059f2fb73080e460c4ae7",
          "message": "Fix utility::force_batch returning incorrect weights causing mismatch (#9983)\n\n# Description\n\nThe returned weights from the `pallet_utility::force_batch` dispatch is\nusing `batch` weights instead of its own `force_batch` causing a weight\nmismatch log:\n\n```\n2025-10-03 14:57:48 Post dispatch weight is greater than pre dispatch weight. Pre dispatch weight may underestimating the actual weight. Greater post dispatch weight components are ignored.\n                                        Pre dispatch weight: Weight { ref_time: 5963137729, proof_size: 3997 },\n                                        Post dispatch weight: Weight { ref_time: 5963148560, proof_size: 3997 }    \n2025-10-03 14:59:00 Post dispatch weight is greater than pre dispatch weight. Pre dispatch weight may underestimating the actual weight. Greater post dispatch weight components are ignored.\n                                        Pre dispatch weight: Weight { ref_time: 2837593294, proof_size: 8703 },\n                                        Post dispatch weight: Weight { ref_time: 2837604125, proof_size: 8703 }   \n``` \n\nThis PR correct the returned weights.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2025-10-10T07:26:12Z",
          "tree_id": "2b90b30c7bc456f310a278da53e3685fb1413266",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c189613a32a58745993059f2fb73080e460c4ae7"
        },
        "date": 1760085277059,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026022112,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008551411129999988,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005069584579999994,
            "unit": "seconds"
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
          "id": "949521966517fdb251f0c9bee574f065c78324c0",
          "message": "Cumulus `aura-ext`: Harden slot check between relay-parent and parachain (#9712)\n\nBefore this PR, we were preventing parachains from authoring blocks that\nhave a slot from the future. However, we permitted parachains to author\non past slots, which is unnecessary relaxed. This PR enforces that only\nthe author that owns the slot corresponding to the slot of the relay\nparent can author.\n\nThis should no impact on normal Parachain operation. Blocks are already\nproduced on the correct slots, this increased strictness improves some\nedge-cases however.\n\nEven before, the Parachains slot needed to strictly advance, so\nproduction on \"older\" slots was only possible if previous authors\nskipped production opportunities.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-10T09:00:01Z",
          "tree_id": "47927ccf57a39c58f47f534ca0a4ba402cd17cbb",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/949521966517fdb251f0c9bee574f065c78324c0"
        },
        "date": 1760090867981,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008462905329999989,
            "unit": "seconds"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026041145799999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005079862539999991,
            "unit": "seconds"
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
          "id": "c61e227f2c59fa0ce7df5f977dad4076ff470f00",
          "message": "`trace_block`: Support overwriting `execute_block` (#9871)\n\nThis is required for example for parachains that require special\nextensions to be registered (e.g. `ProofSizeExt`) to succeed the block\nexecution.\n\nThis pull request changes the signature of `spawn_tasks` which now\nrequires a `tracing_execute_block` parameter. If your chain is a\nsolochain, just set the parameter to `None` or overwrite it if you need\nany special handling. For parachain builders, this value can be set to\n`cumulus_service::ParachainTracingExecuteBlock`.\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-10T10:26:45Z",
          "tree_id": "3543a71fa505d9567ad4915814d4e03994df82aa",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c61e227f2c59fa0ce7df5f977dad4076ff470f00"
        },
        "date": 1760096779577,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.0026611179200000004,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008707314739999982,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005227367409999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "email": "marian@parity.io",
            "name": "Marian Radu",
            "username": "marian-radu"
          },
          "committer": {
            "email": "noreply@github.com",
            "name": "GitHub",
            "username": "web-flow"
          },
          "distinct": false,
          "id": "1cbdb4d2e384e35e27760f24ca895b2f3d601698",
          "message": " Wait for transaction receipt if instant seal is enabled (#9914)\n\nFixes https://github.com/paritytech/contract-issues/issues/165\n\nThe main changes in this PR are:\n\n1. Add a new API to revive-dev-node to check whether the node has\ninstant seal enabled.\n2. Add a new debug API to eth-rpc to check whether the node has instant\nseal enabled. (optional)\n3. Query and cache the node’s instant seal status during eth-rpc\ninitialization.\n4. If instant seal is enabled, wait for the transaction receipt to be\navailable\n\n---------\n\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>\nCo-authored-by: pgherveou <pgherveou@gmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2025-10-10T16:01:50Z",
          "tree_id": "dd524ef9e7b71c82b791e3c22045d3bcc07039ff",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1cbdb4d2e384e35e27760f24ca895b2f3d601698"
        },
        "date": 1760116892303,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.002613593220000001,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008503874149999984,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005228226769999995,
            "unit": "seconds"
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
          "id": "d65db6ef34c0e7b99655576fe2861105557afb97",
          "message": "[pallet-revive] Implement the consume_all_gas syscall (#9997)\n\nThis PR implements a new API `consume_all_gas` which is required for\n100% EVM `INVALID` opcode compatibility.\n\nSince ceding of all remaining gas is handled in the EVM interpreter, I\ndecided to not add a return flag but make this a dedicated syscall for\nconsistency instead.\n\nDidn't implement a benchmark since the first (and only) thing this does\nis consuming all remaining gas anyways.\n\n---------\n\nSigned-off-by: Cyrill Leutwiler <bigcyrill@hotmail.com>\nCo-authored-by: cmd[bot] <41898282+github-actions[bot]@users.noreply.github.com>",
          "timestamp": "2025-10-10T16:59:39Z",
          "tree_id": "8301f54c9ebb8ba81aad943ffffe9f1980d42d88",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d65db6ef34c0e7b99655576fe2861105557afb97"
        },
        "date": 1760120092488,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 23.800000000000004,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 227.09999999999997,
            "unit": "KiB"
          },
          {
            "name": "dispute-coordinator",
            "value": 0.00258997572,
            "unit": "seconds"
          },
          {
            "name": "dispute-distribution",
            "value": 0.008477951969999977,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.005159479869999996,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}