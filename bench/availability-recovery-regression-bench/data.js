window.BENCHMARK_DATA = {
  "lastUpdate": 1711715934276,
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
        "date": 1711650130067,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666672,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.14907673656666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.19076506852,
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
        "date": 1711703176249,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666672,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 11.469302139360002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.22636818130666658,
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
        "date": 1711708098847,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666672,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 11.34961518349333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1837224126933334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Gheorghe",
            "username": "alexggh",
            "email": "49718502+alexggh@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5638d1a830dc70f56e5fdd7eded21a4f592d382c",
          "message": "Decorate mpsc-notification-to-protocol with the protocol name (#3873)\n\nCurrently, all protocols use the same metric name for\n`mpsc-notification-to-protocol` this is bad because we can't actually\ntell which protocol might cause problems.\n\nThis patch proposes we derive the name of the metric from the protocol\nname, so that we have separate metrics for each protocol and properly\ndetect which one is having problem processing its messages.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-03-29T11:24:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5638d1a830dc70f56e5fdd7eded21a4f592d382c"
        },
        "date": 1711715910192,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666672,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 11.50660094779333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20140178177999998,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}