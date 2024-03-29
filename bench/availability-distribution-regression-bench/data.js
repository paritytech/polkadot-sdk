window.BENCHMARK_DATA = {
  "lastUpdate": 1711707825690,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "Benchmark": [
      {
        "commit": {
          "author": {
            "name": "Alexander Samusev",
            "username": "alvicsam",
            "email": "41779041+alvicsam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eb6f5abee64e979dba25924f71ef86d2b3ca2deb",
          "message": "[ci] fix subsystem-benchmarks gha (#3876)\n\nPR adds variables validation and app credentials for pushing into\ngh-pages\n\ncc https://github.com/paritytech/ci_cd/issues/934",
          "timestamp": "2024-03-28T16:42:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eb6f5abee64e979dba25924f71ef86d2b3ca2deb"
        },
        "date": 1711649817534,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.926666666663,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.009493043573333335,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.15172178257999994,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.02619895733333334,
            "unit": "seconds"
          },
          {
            "name": "availability-distribution",
            "value": 0.011713924659999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Sandu",
            "username": "sandreim",
            "email": "54316454+sandreim@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "30ef8651ed0ba821e59121545815280a3e9b2862",
          "message": "collation-genereation: fix tests (#3883)\n\nSomehow https://github.com/paritytech/polkadot-sdk/pull/3795 was merged\nbut tests are failing now on master. I suspect that CI is not even\nrunning these tests anymore which is a big issue.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-03-29T07:49:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/30ef8651ed0ba821e59121545815280a3e9b2862"
        },
        "date": 1711702894873,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.906666666662,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.013556550140000003,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16053591725999997,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.026928166213333323,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011546961813333343,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "s0me0ne-unkn0wn",
            "username": "s0me0ne-unkn0wn",
            "email": "48632512+s0me0ne-unkn0wn@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b310b575cd73928cf061e1ae0d184f7e900976d5",
          "message": "Remove transient code after `im-online` pallet removal (#3383)\n\nRemoves transient code introduced to clean up offchain database after\n`im-online` pallet removal.\n\nShould be merged after #2290 has been enacted.",
          "timestamp": "2024-03-29T09:12:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b310b575cd73928cf061e1ae0d184f7e900976d5"
        },
        "date": 1711707802331,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 433.3333333333332,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 18479.89333333333,
            "unit": "KiB"
          },
          {
            "name": "availability-distribution",
            "value": 0.014100756359999996,
            "unit": "seconds"
          },
          {
            "name": "availability-store",
            "value": 0.16313833589999993,
            "unit": "seconds"
          },
          {
            "name": "bitfield-distribution",
            "value": 0.027004377906666672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.011511754360000005,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}