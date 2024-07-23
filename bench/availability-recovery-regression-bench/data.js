window.BENCHMARK_DATA = {
  "lastUpdate": 1721759334278,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "availability-recovery-regression-bench": [
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
      },
      {
        "commit": {
          "author": {
            "name": "Dmitry Markin",
            "username": "dmitry-markin",
            "email": "dmitry@markin.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0d9324847391e902bb42f84f0e76096b1f764efe",
          "message": "Fix `addresses_to_publish_respects_existing_p2p_protocol` test in sc-authority-discovery (#3895)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3887.",
          "timestamp": "2024-03-29T13:13:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d9324847391e902bb42f84f0e76096b1f764efe"
        },
        "date": 1711722777802,
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
            "value": 0.20344677356666668,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.518641145546665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "41257069b062ea7feb2277f11a2e992d3c9d5089",
          "message": "Tokens in FRAME Docs (#2802)\n\nCloses https://github.com/paritytech/polkadot-sdk-docs/issues/70\n\nWIP PR for an overview of how to develop tokens in FRAME. \n\n- [x] Tokens in Substrate Ref Doc\n  - High-level overview of the token-related logic in FRAME\n- Improve docs with better explanation of how holds, freezes, ed, free\nbalance, etc, all work\n- [x] Update `pallet_balances` docs\n  - Clearly mark what is deprecated (currency)\n- [x] Write fungible trait docs\n- [x] Evaluate and if required update `pallet_assets`, `pallet_uniques`,\n`pallet_nfts` docs\n- [x] Absorb https://github.com/paritytech/polkadot-sdk/pull/2683/\n- [x] Audit individual trait method docs, and improve if possible\n\nFeel free to suggest additional TODOs for this PR in the comments\n\n---------\n\nCo-authored-by: Bill Laboon <laboon@users.noreply.github.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-03-31T09:59:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/41257069b062ea7feb2277f11a2e992d3c9d5089"
        },
        "date": 1711883818532,
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
            "value": 0.2248709717466667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.565722522426661,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "256d5aefdc83928090aa2e3f8c022484fab38e0a",
          "message": "Revert log level changes (#3913)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3906",
          "timestamp": "2024-03-31T20:47:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/256d5aefdc83928090aa2e3f8c022484fab38e0a"
        },
        "date": 1711922474203,
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
            "name": "test-environment",
            "value": 0.16571782401333326,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.456690761686664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gemini132",
            "username": "gemini132",
            "email": "164285545+gemini132@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aa44384e05e05705cbdfacd8d73972404be4be6f",
          "message": "Fix two typos (#3812)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-03-31T22:28:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa44384e05e05705cbdfacd8d73972404be4be6f"
        },
        "date": 1711928545121,
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
            "name": "test-environment",
            "value": 0.1744062902466666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.445610189439998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Matteo Muraca",
            "username": "muraca",
            "email": "56828990+muraca@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a2c9ab8c043221f4902739d678739b1fa9319cef",
          "message": "Removed `pallet::getter` usage from `pallet-alliance` (#3738)\n\nPart of #3326 \n\ncc @kianenigma @ggwpez @liamaharon \n\npolkadot address: 12poSUQPtcF1HUPQGY3zZu2P8emuW9YnsPduA4XG3oCEfJVp\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-01T05:17:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a2c9ab8c043221f4902739d678739b1fa9319cef"
        },
        "date": 1711953050433,
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
            "value": 11.483052576113334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19102473382666663,
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
          "id": "e0c081dbd46c1e6edca1ce2c62298f5f3622afdd",
          "message": "network:bridge: fix peer_count metric (#3711)\n\nThe metric records the current protocol_version of the validator that\njust connected with the peer_map.len(), which contains all peers that\nconnected, that has the effect the metric will be wrong since it won't\ntell us how many peers we have connected per version because it will\nalways record the total number of peers\n\nFix this by counting by version inside peer_map, additionally because\nthat might be a bit heavier than len(), publish it only on-active\nleaves.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T06:29:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0c081dbd46c1e6edca1ce2c62298f5f3622afdd"
        },
        "date": 1711957374564,
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
            "value": 11.262594899466665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16748281709333337,
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
          "id": "e6bd9205432bb524e94c9bd13048d645ec9aa5c7",
          "message": "Fix 0007-dispute-freshly-finalized.zndsl failing (#3893)\n\nTest started failing after\nhttps://github.com/paritytech/polkadot-sdk/commit/66051adb619d2119771920218e2de75fa037d7e8\nwhich enabled approval coalescing, that was expected to happen because\nthe test required an polkadot_parachain_approval_checking_finality_lag\nof 0, which can't happen with max_approval_coalesce_count greater than 1\nbecause we always delay the approval for no_show_duration_ticks/2 in\ncase we can coalesce it with other approvals.\n\n\nSo relax a bit the restrictions, since we don't actually care that the\nlags are 0, but the fact the finalities are progressing and are not\nstuck.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T10:23:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6bd9205432bb524e94c9bd13048d645ec9aa5c7"
        },
        "date": 1711968663173,
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
            "value": 0.16043637478000006,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.298870335459998,
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
          "id": "e6bd9205432bb524e94c9bd13048d645ec9aa5c7",
          "message": "Fix 0007-dispute-freshly-finalized.zndsl failing (#3893)\n\nTest started failing after\nhttps://github.com/paritytech/polkadot-sdk/commit/66051adb619d2119771920218e2de75fa037d7e8\nwhich enabled approval coalescing, that was expected to happen because\nthe test required an polkadot_parachain_approval_checking_finality_lag\nof 0, which can't happen with max_approval_coalesce_count greater than 1\nbecause we always delay the approval for no_show_duration_ticks/2 in\ncase we can coalesce it with other approvals.\n\n\nSo relax a bit the restrictions, since we don't actually care that the\nlags are 0, but the fact the finalities are progressing and are not\nstuck.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T10:23:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6bd9205432bb524e94c9bd13048d645ec9aa5c7"
        },
        "date": 1711971497826,
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
            "value": 11.379294688533335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16771078979999998,
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
          "id": "d6f68bb9062167537211cc05286809771fc8861a",
          "message": "primitives: Move out of staging released APIs (#3925)\n\nRuntime release 1.2 includes bumping of the ParachainHost APIs up to\nv10, so let's move all the released APIs out of vstaging folder, this PR\ndoes not include any logic changes only renaming of the modules and some\nmoving around.\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-01T13:03:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6f68bb9062167537211cc05286809771fc8861a"
        },
        "date": 1711981117398,
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
            "value": 11.973230799573328,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18138847673333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9805ba2cd01922f81621e0f3ac8adc0180fb7a49",
          "message": "Fix links (#3928)\n\nFix links\n\nRelated CI failure:\nhttps://github.com/paritytech/polkadot-sdk/actions/runs/8455425042/job/23162858534?pr=3859",
          "timestamp": "2024-04-01T20:18:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9805ba2cd01922f81621e0f3ac8adc0180fb7a49"
        },
        "date": 1712007425454,
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
            "value": 0.17992319420666658,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.702944064340002,
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
          "id": "52e103784945997cb3808cdfaaf72c468f8fc938",
          "message": "`im-online` removal final cleanup (#3902)\n\nRejoice! Rejoice! The story is nearly over.\n\nThis PR removes stale migrations, auxiliary structures, and package\ndependencies, thus making Rococo and Westend totally free from any\n`im-online`-related stuff.\n\n`im-online` still stays a part of the Substrate node and its runtime:\nhttps://github.com/paritytech/polkadot-sdk/blob/0d9324847391e902bb42f84f0e76096b1f764efe/substrate/bin/node/runtime/src/lib.rs#L2276-L2277\nI'm not sure if it makes sense to remove it from there considering that\nwe're not removing `im-online` from FRAME. Please share your opinion.",
          "timestamp": "2024-04-01T21:40:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/52e103784945997cb3808cdfaaf72c468f8fc938"
        },
        "date": 1712014770096,
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
            "value": 11.841901837053332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18430372197999995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sam Johnson",
            "username": "sam0x17",
            "email": "sam@durosoft.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9a62de27a98312741b4ece2fcd1c6e61b47ee3c2",
          "message": "Update derive syn parse 0.2.0 (+ docify) (#3920)\n\nderive-syn-parse v0.2.0 came out recently which (finally) adds support\nfor syn 2x.\n\nUpgrading to this will remove many of the places where syn 1x was still\ncompiling alongside syn 2x in the polkadot-sdk workspace.\n\nThis also upgrades `docify` to 0.2.8 which is the version that upgrades\nderive-syn-pasre to 0.2.0.\n\nAdditionally, this consolidates the `docify` versions in the repo to all\nuse the latest, and in one case upgrades to the 0.2x syntax where 0.1.x\nwas still being used.\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-02T05:53:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9a62de27a98312741b4ece2fcd1c6e61b47ee3c2"
        },
        "date": 1712044151243,
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
            "value": 11.38698822258666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17212005834666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d0ebb850ed2cefeb3e4ef8b8e0a16eb7fb6b3f3e",
          "message": "pallet-xcm: fix weights for all XTs and deprecate unlimited weight ones  (#3927)\n\nFix \"double-weights\" for extrinsics, use only the ones benchmarked in\nthe runtime.\n\nDeprecate extrinsics that don't specify WeightLimit, remove their usage\nacross the repo.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-02T07:57:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d0ebb850ed2cefeb3e4ef8b8e0a16eb7fb6b3f3e"
        },
        "date": 1712049323502,
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
            "name": "test-environment",
            "value": 0.1891002567999999,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.383471322273335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "12eb285dbe6271c365db7ba17cf643bfc77fe753",
          "message": "Fix parachain upgrade scheduling when done by the owner/root (#3341)\n\nWhen using `schedule_code_upgrade` to change the code of a parachain in\nthe relay chain runtime, we had already fixed to not set the `GoAhead`\nsignal. This was done to not brick any parachain after the upgrade,\nbecause they were seeing the signal without having any upgrade prepared.\nThe remaining problem is that the parachain code is only upgraded after\na parachain header was enacted, aka the parachain made some progress.\nHowever, this is quite complicated if the parachain is bricked (which is\nthe most common scenario why to manually schedule a code upgrade). Thus,\nthis pull request replaces `SetGoAhead` with `UpgradeStrategy` to signal\nto the logic kind of strategy want to use. The strategies are either\n`SetGoAheadSignal` or `ApplyAtExpectedBlock`. `SetGoAheadSignal` sets\nthe go ahead signal as before and awaits a parachain block.\n`ApplyAtExpectedBlock` schedules the upgrade and applies it directly at\nthe `expected_block` without waiting for the parachain to make any kind\nof progress.",
          "timestamp": "2024-04-02T09:44:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/12eb285dbe6271c365db7ba17cf643bfc77fe753"
        },
        "date": 1712056017506,
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
            "value": 11.459488167413335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1952109936466667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7430f413503f8008fe60eb2e4ebd76d14af12ea9",
          "message": "chainHead: Allow methods to be called from within a single connection context and limit connections (#3481)\n\nThis PR ensures that the chainHead RPC class can be called only from\nwithin the same connection context.\n\nThe chainHead methods are now registered as raw methods. \n- https://github.com/paritytech/jsonrpsee/pull/1297\nThe concept of raw methods is introduced in jsonrpsee, which is an async\nmethod that exposes the connection ID:\nThe raw method doesn't have the concept of a blocking method. Previously\nblocking methods are now spawning a blocking task to handle their\nblocking (ie DB) access. We spawn the same number of tasks as before,\nhowever we do that explicitly.\n\nAnother approach would be implementing a RPC middleware that captures\nand decodes the method parameters:\n- https://github.com/paritytech/polkadot-sdk/pull/3343\nHowever, that approach is prone to errors since the methods are\nhardcoded by name. Performace is affected by the double deserialization\nthat needs to happen to extract the subscription ID we'd like to limit.\nOnce from the middleware, and once from the methods itself.\n\nThis PR paves the way to implement the chainHead connection limiter:\n- https://github.com/paritytech/polkadot-sdk/issues/1505\nRegistering tokens (subscription ID / operation ID) on the\n`RpcConnections` could be extended to return an error when the maximum\nnumber of operations is reached.\n\nWhile at it, have added an integration-test to ensure that chainHead\nmethods can be called from within the same connection context.\n\nBefore this is merged, a new JsonRPC release should be made to expose\nthe `raw-methods`:\n- [x] Use jsonrpsee from crates io (blocked by:\nhttps://github.com/paritytech/jsonrpsee/pull/1297)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3207\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>",
          "timestamp": "2024-04-02T13:12:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7430f413503f8008fe60eb2e4ebd76d14af12ea9"
        },
        "date": 1712065856397,
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
            "value": 11.591048417266666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16394800546,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Clara van Staden",
            "username": "claravanstaden",
            "email": "claravanstaden64@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5d9826c2620aff205811edf0e6a07b55a52cbf50",
          "message": "Snowbridge: Synchronize from Snowfork repository (#3761)\n\nThis PR includes the following 2 improvements:\n\n## Ethereum Client\n\nAuthor: @yrong \n### Original Upstream PRs\n- https://github.com/Snowfork/polkadot-sdk/pull/123\n- https://github.com/Snowfork/polkadot-sdk/pull/125\n\n### Description\nThe Ethereum client syncs beacon headers as they are finalized, and\nimports every execution header. When a message is received, it is\nverified against the import execution header. This is unnecessary, since\nthe execution header can be sent with the message as proof. The recent\nDeneb Ethereum upgrade made it easier to locate the relevant beacon\nheader from an execution header, and so this improvement was made\npossible. This resolves a concern @svyatonik had in our initial Rococo\nPR:\nhttps://github.com/paritytech/polkadot-sdk/pull/2522#discussion_r1431270691\n\n## Inbound Queue\n\nAuthor: @yrong \n### Original Upstream PR\n- https://github.com/Snowfork/polkadot-sdk/pull/118\n\n### Description\nWhen the AH sovereign account (who pays relayer rewards) is depleted,\nthe inbound message will not fail. The relayer just will not receive\nrewards.\n\nBoth these changes were done by @yrong, many thanks. ❤️\n\n---------\n\nCo-authored-by: claravanstaden <Cats 4 life!>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>",
          "timestamp": "2024-04-02T13:53:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5d9826c2620aff205811edf0e6a07b55a52cbf50"
        },
        "date": 1712070798018,
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
            "value": 11.820352557753333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15768566792000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0becc45bd826aea6ec128da8525ed73b3657d474",
          "message": "sp_runtime: TryFrom<RuntimeString> for &str (#3942)\n\nAdded `TryFrom<&'a RuntimeString> for &'a str`",
          "timestamp": "2024-04-02T16:06:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0becc45bd826aea6ec128da8525ed73b3657d474"
        },
        "date": 1712079541512,
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
            "value": 0.15146185498000006,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.79058399485333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "665e3654ceca5a34e8ada66a9805fa7b76fc9ebb",
          "message": "Remove nextest filtration (#3885)\n\nFixes\nhttps://github.com/paritytech/polkadot-sdk/issues/3884#issuecomment-2026058687\n\nAfter moving regression tests to benchmarks\n(https://github.com/paritytech/polkadot-sdk/pull/3741) we don't need to\nfilter tests anymore.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Alin Dima <alin@parity.io>\nCo-authored-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>\nCo-authored-by: Dastan <88332432+dastansam@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-04-02T19:27:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/665e3654ceca5a34e8ada66a9805fa7b76fc9ebb"
        },
        "date": 1712090747902,
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
            "value": 11.73293235716667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1685576271666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9b378a2ffef1d5846872adc4336341805bffbc30",
          "message": "sp-wasm-interface: `wasmtime` should not be enabled by `std` (#3954)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3909",
          "timestamp": "2024-04-03T08:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b378a2ffef1d5846872adc4336341805bffbc30"
        },
        "date": 1712138716080,
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
            "value": 11.315078361813333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17346636862666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "287b116c3e50ff8be275b093674404b2f370c553",
          "message": "chainHead: Ensure reasonable distance between leaf and finalized block (#3562)\n\nThis PR ensure that the distance between any leaf and the finalized\nblock is within a reasonable distance.\n\nFor a new subscription, the chainHead has to provide all blocks between\nthe leaves of the chain and the finalized block.\n When the distance between a leaf and the finalized block is large:\n - The tree route is costly to compute\n - We could deliver an unbounded number of blocks (potentially millions)\n(For more details see\nhttps://github.com/paritytech/polkadot-sdk/pull/3445#discussion_r1507210283)\n\nThe configuration of the ChainHead is extended with:\n- suspend on lagging distance: When the distance between any leaf and\nthe finalized block is greater than this number, the subscriptions are\nsuspended for a given duration.\n- All active subscriptions are terminated with the `Stop` event, all\nblocks are unpinned and data discarded.\n- For incoming subscriptions, until the suspended period expires the\nsubscriptions will immediately receive the `Stop` event.\n    - Defaults to 128 blocks\n- suspended duration: The amount of time for which subscriptions are\nsuspended\n    - Defaults to 30 seconds\n \n \n cc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-04-03T11:46:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/287b116c3e50ff8be275b093674404b2f370c553"
        },
        "date": 1712149877451,
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
            "name": "test-environment",
            "value": 0.17090806727333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.289210646266662,
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
          "id": "0f4e849e0ac2de8c9880077c085985c5f656329c",
          "message": "Add ClaimQueue wrapper (#3950)\n\nRemove `fetch_next_scheduled_on_core` in favor of new wrapper and\nmethods for accessing it.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-03T15:01:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0f4e849e0ac2de8c9880077c085985c5f656329c"
        },
        "date": 1712161137499,
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
            "value": 11.365277032306667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17951487063333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3836376965104d7723a1659d52ee26232019b929",
          "message": "Renames `frame` crate to `polkadot-sdk-frame` (#3813)\n\nStep in https://github.com/paritytech/polkadot-sdk/issues/3155\n\nNeeded for https://github.com/paritytech/eng-automation/issues/6\n\nThis PR renames `frame` crate to `polkadot-sdk-frame` as `frame` is not\navailable on crates.io\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-04T02:20:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3836376965104d7723a1659d52ee26232019b929"
        },
        "date": 1712202180016,
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
            "value": 11.112623188226669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17415638780666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0bbda78d86bc6210cda123042d817aeaf45b3d77",
          "message": "Use 0.1.0 as minimum version for crates (#3941)\n\nCI will be enforcing this with next parity-publish release",
          "timestamp": "2024-04-04T09:26:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0bbda78d86bc6210cda123042d817aeaf45b3d77"
        },
        "date": 1712228073598,
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
            "value": 11.473803762820003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20081802856666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3",
          "message": "XCM builder pattern improvement - Accept `impl Into<T>` instead of just `T` (#3708)\n\nThe XCM builder pattern lets you build xcms like so:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128).into())\n    .buy_execution((Parent, 1u128).into())\n    .deposit_asset(All.into(), AccountId32 { id: [0u8; 32], network: None }.into())\n    .build();\n```\n\nAll the `.into()` become quite annoying to have to write.\nI accepted `impl Into<T>` instead of `T` in the generated methods from\nthe macro.\nNow the previous example can be simplified as follows:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128))\n    .buy_execution((Parent, 1u128))\n    .deposit_asset(All, [0u8; 32])\n    .build();\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-04T12:40:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3"
        },
        "date": 1712236384250,
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
            "value": 11.433170515266667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16790686006666675,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3",
          "message": "XCM builder pattern improvement - Accept `impl Into<T>` instead of just `T` (#3708)\n\nThe XCM builder pattern lets you build xcms like so:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128).into())\n    .buy_execution((Parent, 1u128).into())\n    .deposit_asset(All.into(), AccountId32 { id: [0u8; 32], network: None }.into())\n    .build();\n```\n\nAll the `.into()` become quite annoying to have to write.\nI accepted `impl Into<T>` instead of `T` in the generated methods from\nthe macro.\nNow the previous example can be simplified as follows:\n\n```rust\nlet xcm = Xcm::builder()\n    .withdraw_asset((Parent, 100u128))\n    .buy_execution((Parent, 1u128))\n    .deposit_asset(All, [0u8; 32])\n    .build();\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-04T12:40:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c130ea9939b01d0ce8c0da8e5e5094ffdb3479e3"
        },
        "date": 1712239555569,
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
            "value": 0.2070999653933333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.614488651466665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bda4e75ac49786a7246531cf729b25c208cd38e6",
          "message": "Migrate fee payment from `Currency` to `fungible` (#2292)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/226 \nRelated https://github.com/paritytech/polkadot-sdk/issues/1833\n\n- Deprecate `CurrencyAdapter` and introduce `FungibleAdapter`\n- Deprecate `ToStakingPot` and replace usage with `ResolveTo`\n- Required creating a new `StakingPotAccountId` struct that implements\n`TypedGet` for the staking pot account ID\n- Update parachain common utils `DealWithFees`, `ToAuthor` and\n`AssetsToBlockAuthor` implementations to use `fungible`\n- Update runtime XCM Weight Traders to use `ResolveTo` instead of\n`ToStakingPot`\n- Update runtime Transaction Payment pallets to use `FungibleAdapter`\ninstead of `CurrencyAdapter`\n- [x] Blocked by https://github.com/paritytech/polkadot-sdk/pull/1296,\nneeds the `Unbalanced::decrease_balance` fix",
          "timestamp": "2024-04-04T13:56:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bda4e75ac49786a7246531cf729b25c208cd38e6"
        },
        "date": 1712245579842,
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
            "value": 11.557731253486665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21290615929333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "68cdb126498df7d5d212f8e10b514a79502f88da",
          "message": "Added support for coretime-kusama/polkadot and people-kusama/polkadot (#3961)\n\n## Running  `./polkadot-parachain --chain coretime-kusama` works now:\n\n**Parachain genesis state and header** match expected ones from\nhttps://gist.github.com/bkontur/f74fc00fd726d09bc7f0f3a9f51ec113?permalink_comment_id=5009857#gistcomment-5009857\n```\n2024-04-03 12:03:58 [Parachain] 🔨 Initializing Genesis block/state (state: 0xc418…889c, header-hash: 0x638c…d050) \n...\n2024-04-03 12:04:04 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050)\n```\n\n**Relaychain genesis state and header** match expected ones:\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fkusama-rpc.polkadot.io#/explorer/query/0\n\n```\n2024-04-03 12:03:59 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n```\n\n\n\n\n\n**Full logs:**\n```\nbparity@bkontur-ThinkPad-P14s-Gen-2i:~/parity/polkadot-sdk$ ./target/debug/polkadot-parachain --chain coretime-kusama\n2024-04-03 12:03:52 Polkadot parachain    \n2024-04-03 12:03:52 ✌️  version 4.0.0-665e3654cec    \n2024-04-03 12:03:52 ❤️  by Parity Technologies <admin@parity.io>, 2017-2024    \n2024-04-03 12:03:52 📋 Chain specification: Kusama Coretime    \n2024-04-03 12:03:52 🏷  Node name: subsequent-quicksand-2382    \n2024-04-03 12:03:52 👤 Role: FULL    \n2024-04-03 12:03:52 💾 Database: RocksDb at /home/bparity/.local/share/polkadot-parachain/chains/coretime-kusama/db/full    \n2024-04-03 12:03:54 Parachain id: Id(1005)    \n2024-04-03 12:03:54 Parachain Account: 5Ec4AhPakEiNWFbAd26nRrREnaGQZo3uukPDC5xLr6314Dwg    \n2024-04-03 12:03:54 Is collating: no    \n2024-04-03 12:03:58 [Parachain] 🔨 Initializing Genesis block/state (state: 0xc418…889c, header-hash: 0x638c…d050)    \n2024-04-03 12:03:59 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n2024-04-03 12:03:59 [Relaychain] 👴 Loading GRANDPA authority set from genesis on what appears to be first startup.    \n2024-04-03 12:03:59 [Relaychain] 👶 Creating empty BABE epoch changes on what appears to be first startup.    \n2024-04-03 12:03:59 [Relaychain] 🏷  Local node identity is: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd    \n2024-04-03 12:03:59 [Relaychain] 💻 Operating system: linux    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU architecture: x86_64    \n2024-04-03 12:03:59 [Relaychain] 💻 Target environment: gnu    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-03 12:03:59 [Relaychain] 💻 CPU cores: 4    \n2024-04-03 12:03:59 [Relaychain] 💻 Memory: 31797MB    \n2024-04-03 12:03:59 [Relaychain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-03 12:03:59 [Relaychain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-03 12:03:59 [Relaychain] 💻 Virtual machine: no    \n2024-04-03 12:03:59 [Relaychain] 📦 Highest known block at #0    \n2024-04-03 12:03:59 [Relaychain] 〽️ Prometheus exporter started at 127.0.0.1:9616    \n2024-04-03 12:03:59 [Relaychain] Running JSON-RPC server: addr=127.0.0.1:9945, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-03 12:03:59 [Relaychain] 🏁 CPU score: 1.40 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Memory score: 15.42 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Disk score (seq. writes): 1.39 GiBs    \n2024-04-03 12:03:59 [Relaychain] 🏁 Disk score (rand. writes): 690.56 MiBs    \n2024-04-03 12:03:59 [Parachain] Using default protocol ID \"sup\" because none is configured in the chain specs    \n2024-04-03 12:03:59 [Parachain] 🏷  Local node identity is: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW    \n2024-04-03 12:03:59 [Parachain] 💻 Operating system: linux    \n2024-04-03 12:03:59 [Parachain] 💻 CPU architecture: x86_64    \n2024-04-03 12:03:59 [Parachain] 💻 Target environment: gnu    \n2024-04-03 12:03:59 [Parachain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-03 12:03:59 [Parachain] 💻 CPU cores: 4    \n2024-04-03 12:03:59 [Parachain] 💻 Memory: 31797MB    \n2024-04-03 12:03:59 [Parachain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-03 12:03:59 [Parachain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-03 12:03:59 [Parachain] 💻 Virtual machine: no    \n2024-04-03 12:03:59 [Parachain] 📦 Highest known block at #0    \n2024-04-03 12:03:59 [Parachain] 〽️ Prometheus exporter started at 127.0.0.1:9615    \n2024-04-03 12:03:59 [Parachain] Running JSON-RPC server: addr=127.0.0.1:9944, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-03 12:03:59 [Parachain] 🏁 CPU score: 1.40 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Memory score: 15.42 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Disk score (seq. writes): 1.39 GiBs    \n2024-04-03 12:03:59 [Parachain] 🏁 Disk score (rand. writes): 690.56 MiBs    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/192.168.1.100/tcp/30334/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/192.168.1.100/tcp/30333/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/172.18.0.1/tcp/30333/ws    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/172.17.0.1/tcp/30334/ws    \n2024-04-03 12:03:59 [Relaychain] discovered: 12D3KooWAAvNqXn8WPmvnEj36j7HsdbtpRpmWDPT9xtp4CuphvxW /ip4/172.17.0.1/tcp/30333/ws    \n2024-04-03 12:03:59 [Parachain] discovered: 12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd /ip4/172.18.0.1/tcp/30334/ws    \n2024-04-03 12:04:00 [Relaychain] 🔍 Discovered new external address for our node: /ip4/178.41.176.246/tcp/30334/ws/p2p/12D3KooWSfXNBZYimwSKBqfKf7F1X6adNQQD5HVQbdnvSyBFn8Wd    \n2024-04-03 12:04:00 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-03 12:04:00 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-03 12:04:04 [Relaychain] ⚙️  Syncing, target=#22575321 (7 peers), best: #738 (0x1803…bbef), finalized #512 (0xb9b6…7014), ⬇ 328.5kiB/s ⬆ 102.9kiB/s    \n2024-04-03 12:04:04 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:09 [Relaychain] ⚙️  Syncing 169.5 bps, target=#22575322 (8 peers), best: #1586 (0x405b…a8aa), finalized #1536 (0x55d1…fb04), ⬇ 232.3kiB/s ⬆ 55.9kiB/s    \n2024-04-03 12:04:09 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:14 [Relaychain] ⚙️  Syncing 168.0 bps, target=#22575323 (8 peers), best: #2426 (0x155f…d083), finalized #2048 (0xede6…f879), ⬇ 235.8kiB/s ⬆ 67.2kiB/s    \n2024-04-03 12:04:14 [Parachain] 💤 Idle (0 peers), best: #0 (0x638c…d050), finalized #0 (0x638c…d050), ⬇ 0 ⬆ 0    \n2024-04-03 12:04:19 [Relaychain] ⚙️  Syncing 170.0 bps, target=#22575324 (8 peers), best: #3276 (0x94d8…097e), finalized #3072 (0x0e4c…f587), ⬇ 129.0kiB/s ⬆ 34.0kiB/s\n...\n```\n\n## Running  `./polkadot-parachain --chain people-kusama` works now:\n\n**Parachain genesis state and header** match expected ones from\nhttps://gist.github.com/bkontur/f74fc00fd726d09bc7f0f3a9f51ec113?permalink_comment_id=5011798#gistcomment-5011798\n```\n2024-04-04 10:26:24 [Parachain] 🔨 Initializing Genesis block/state (state: 0x023a…2733, header-hash: 0x07b8…2645)    \n...\n2024-04-04 10:26:30 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n```\n\n**Relaychain genesis state and header** match expected ones:\nhttps://polkadot.js.org/apps/?rpc=wss%3A%2F%2Fkusama-rpc.polkadot.io#/explorer/query/0\n\n```\n2024-04-04 10:26:25 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)  \n```\n\n\n\n\n\n**Full logs:**\n```\nbparity@bkontur-ThinkPad-P14s-Gen-2i:~/parity/aaa/polkadot-sdk$ ./target/debug/polkadot-parachain --chain people-kusama\n2024-04-04 10:26:18 Polkadot parachain    \n2024-04-04 10:26:18 ✌️  version 4.0.0-39274bb75fc    \n2024-04-04 10:26:18 ❤️  by Parity Technologies <admin@parity.io>, 2017-2024    \n2024-04-04 10:26:18 📋 Chain specification: Kusama People    \n2024-04-04 10:26:18 🏷  Node name: knotty-flight-5398    \n2024-04-04 10:26:18 👤 Role: FULL    \n2024-04-04 10:26:18 💾 Database: RocksDb at /home/bparity/.local/share/polkadot-parachain/chains/people-kusama/db/full    \n2024-04-04 10:26:21 Parachain id: Id(1004)    \n2024-04-04 10:26:21 Parachain Account: 5Ec4AhPaYcfBz8fMoPd4EfnAgwbzRS7np3APZUnnFo12qEYk    \n2024-04-04 10:26:21 Is collating: no    \n2024-04-04 10:26:24 [Parachain] 🔨 Initializing Genesis block/state (state: 0x023a…2733, header-hash: 0x07b8…2645)    \n2024-04-04 10:26:25 [Relaychain] 🔨 Initializing Genesis block/state (state: 0xb000…ef6b, header-hash: 0xb0a8…dafe)    \n2024-04-04 10:26:25 [Relaychain] 👴 Loading GRANDPA authority set from genesis on what appears to be first startup.    \n2024-04-04 10:26:25 [Relaychain] 👶 Creating empty BABE epoch changes on what appears to be first startup.    \n2024-04-04 10:26:25 [Relaychain] 🏷  Local node identity is: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB    \n2024-04-04 10:26:25 [Relaychain] 💻 Operating system: linux    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU architecture: x86_64    \n2024-04-04 10:26:25 [Relaychain] 💻 Target environment: gnu    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-04 10:26:25 [Relaychain] 💻 CPU cores: 4    \n2024-04-04 10:26:25 [Relaychain] 💻 Memory: 31797MB    \n2024-04-04 10:26:25 [Relaychain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-04 10:26:25 [Relaychain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-04 10:26:25 [Relaychain] 💻 Virtual machine: no    \n2024-04-04 10:26:25 [Relaychain] 📦 Highest known block at #0    \n2024-04-04 10:26:25 [Relaychain] 〽️ Prometheus exporter started at 127.0.0.1:9616    \n2024-04-04 10:26:25 [Relaychain] Running JSON-RPC server: addr=127.0.0.1:9945, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-04 10:26:25 [Relaychain] 🏁 CPU score: 1.18 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Memory score: 15.61 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Disk score (seq. writes): 1.49 GiBs    \n2024-04-04 10:26:25 [Relaychain] 🏁 Disk score (rand. writes): 650.01 MiBs    \n2024-04-04 10:26:25 [Parachain] Using default protocol ID \"sup\" because none is configured in the chain specs    \n2024-04-04 10:26:25 [Parachain] 🏷  Local node identity is: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj    \n2024-04-04 10:26:25 [Parachain] 💻 Operating system: linux    \n2024-04-04 10:26:25 [Parachain] 💻 CPU architecture: x86_64    \n2024-04-04 10:26:25 [Parachain] 💻 Target environment: gnu    \n2024-04-04 10:26:25 [Parachain] 💻 CPU: 11th Gen Intel(R) Core(TM) i7-1185G7 @ 3.00GHz    \n2024-04-04 10:26:25 [Parachain] 💻 CPU cores: 4    \n2024-04-04 10:26:25 [Parachain] 💻 Memory: 31797MB    \n2024-04-04 10:26:25 [Parachain] 💻 Kernel: 5.15.0-101-generic    \n2024-04-04 10:26:25 [Parachain] 💻 Linux distribution: Ubuntu 20.04.6 LTS    \n2024-04-04 10:26:25 [Parachain] 💻 Virtual machine: no    \n2024-04-04 10:26:25 [Parachain] 📦 Highest known block at #0    \n2024-04-04 10:26:25 [Parachain] 〽️ Prometheus exporter started at 127.0.0.1:9615    \n2024-04-04 10:26:25 [Parachain] Running JSON-RPC server: addr=127.0.0.1:9944, allowed origins=[\"http://localhost:*\", \"http://127.0.0.1:*\", \"https://localhost:*\", \"https://127.0.0.1:*\", \"https://polkadot.js.org\"]    \n2024-04-04 10:26:25 [Parachain] 🏁 CPU score: 1.18 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Memory score: 15.61 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Disk score (seq. writes): 1.49 GiBs    \n2024-04-04 10:26:25 [Parachain] 🏁 Disk score (rand. writes): 650.01 MiBs    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/172.17.0.1/tcp/30334/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/172.18.0.1/tcp/30333/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/192.168.1.100/tcp/30333/ws    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/172.18.0.1/tcp/30334/ws    \n2024-04-04 10:26:25 [Relaychain] discovered: 12D3KooWS2WPQgtiZZYT6bLGjwGcJU7QVd5EeQvb4jHN3NVSWDdj /ip4/172.17.0.1/tcp/30333/ws    \n2024-04-04 10:26:25 [Parachain] discovered: 12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB /ip4/192.168.1.100/tcp/30334/ws    \n2024-04-04 10:26:26 [Relaychain] 🔍 Discovered new external address for our node: /ip4/178.41.176.246/tcp/30334/ws/p2p/12D3KooWPoTVhnrFNzVYJPR42HE9rYjXhkKHFDL9ut5nafDqJHKB    \n2024-04-04 10:26:27 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-04 10:26:27 [Relaychain] Sending fatal alert BadCertificate    \n2024-04-04 10:26:30 [Relaychain] ⚙️  Syncing, target=#22588722 (8 peers), best: #638 (0xa9cd…7c30), finalized #512 (0xb9b6…7014), ⬇ 345.6kiB/s ⬆ 108.7kiB/s    \n2024-04-04 10:26:30 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n2024-04-04 10:26:35 [Relaychain] ⚙️  Syncing 174.4 bps, target=#22588722 (9 peers), best: #1510 (0xec0b…72f0), finalized #1024 (0x3f17…fd7f), ⬇ 203.1kiB/s ⬆ 45.0kiB/s    \n2024-04-04 10:26:35 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n2024-04-04 10:26:40 [Relaychain] ⚙️  Syncing 168.9 bps, target=#22588723 (9 peers), best: #2355 (0xa68b…3a64), finalized #2048 (0xede6…f879), ⬇ 201.6kiB/s ⬆ 47.4kiB/s    \n2024-04-04 10:26:40 [Parachain] 💤 Idle (0 peers), best: #0 (0x07b8…2645), finalized #0 (0x07b8…2645), ⬇ 0 ⬆ 0    \n\n```\n\n## TODO\n- [x] double check\n`cumulus/polkadot-parachain/chain-specs/coretime-kusama.json`\n(safeXcmVersion=3) see\n[comment](https://github.com/paritytech/polkadot-sdk/pull/3961#discussion_r1549473587)\n- [x] check if ~~`start_generic_aura_node`~~ or\n`start_generic_aura_lookahead_node`\n- [x] generate chain-spec for `people-kusama`\n\n---------\n\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2024-04-04T15:26:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/68cdb126498df7d5d212f8e10b514a79502f88da"
        },
        "date": 1712249076057,
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
            "value": 11.456783279059996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18019931634666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f910a15c1ca7255457a6db17ced5bf9c525ec5f0",
          "message": "`GenesisConfig` presets for runtime (#2714)\n\nThe runtime now can provide a number of predefined presets of\n`RuntimeGenesisConfig` struct. This presets are intended to be used in\ndifferent deployments, e.g.: `local`, `staging`, etc, and should be\nincluded into the corresponding chain-specs.\n\nHaving `GenesisConfig` presets in runtime allows to fully decouple node\nfrom runtime types (the problem is described in #1984).\n\n**Summary of changes:**\n- The `GenesisBuilder` API was adjusted to enable this functionality\n(and provide better naming - #150):\n   ```rust\n    fn preset_names() -> Vec<PresetId>;\nfn get_preset(id: Option<PresetId>) -> Option<serde_json::Value>;\n//`None` means default\n    fn build_state(value: serde_json::Value);\n    pub struct PresetId(Vec<u8>);\n   ```\n\n- **Breaking change**: Old `create_default_config` method was removed,\n`build_config` was renamed to `build_state`. As a consequence a node\nwon't be able to interact with genesis config for older runtimes. The\ncleanup was made for sake of API simplicity. Also IMO maintaining\ncompatibility with old API is not so crucial.\n- Reference implementation was provided for `substrate-test-runtime` and\n`rococo` runtimes. For rococo new\n[`genesis_configs_presets`](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/polkadot/runtime/rococo/src/genesis_config_presets.rs#L530)\nmodule was added and is used in `GenesisBuilder`\n[_presets-related_](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/polkadot/runtime/rococo/src/lib.rs#L2462-L2485)\nmethods.\n\n- The `chain-spec-builder` util was also improved and allows to\n([_doc_](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/substrate/bin/utils/chain-spec-builder/src/lib.rs#L19)):\n   - list presets provided by given runtime (`list-presets`),\n- display preset or default config provided by the runtime\n(`display-preset`),\n   - build chain-spec using named preset (`create ... named-preset`),\n\n\n- The `ChainSpecBuilder` is extended with\n[`with_genesis_config_preset_name`](https://github.com/paritytech/polkadot-sdk/blob/3b41d66b97c5ff0ec4a1989da5ffd8b9f3f588e3/substrate/client/chain-spec/src/chain_spec.rs#L447)\nmethod which allows to build chain-spec using named preset provided by\nthe runtime. Sample usage on the node side\n[here](https://github.com/paritytech/polkadot-sdk/blob/2caffaae803e08a3d5b46c860e8016da023ff4ce/polkadot/node/service/src/chain_spec.rs#L404).\n\nImplementation of #1984.\nfixes: #150\npart of: #25\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-04T18:30:54Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f910a15c1ca7255457a6db17ced5bf9c525ec5f0"
        },
        "date": 1712260393397,
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
            "value": 11.520902741673332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20877738542,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ermal Kaleci",
            "username": "ermalkaleci",
            "email": "ermalkaleci@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5fb4397810e167a251d4c909e06784564452e56f",
          "message": "Update pr_3844.prdoc (#3988)",
          "timestamp": "2024-04-04T23:07:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5fb4397810e167a251d4c909e06784564452e56f"
        },
        "date": 1712276454586,
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
            "value": 0.1673822599,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.379502649853332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "cb0748b6ecb84d16f595bfdaf7a98fb46aa5c590",
          "message": "Revert \"[prdoc] Require SemVer bump level\" (#3987)\n\nReverts paritytech/polkadot-sdk#3816",
          "timestamp": "2024-04-05T09:51:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cb0748b6ecb84d16f595bfdaf7a98fb46aa5c590"
        },
        "date": 1712316697256,
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
            "value": 11.31661390116667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16403199918666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "ordian",
            "username": "ordian",
            "email": "write@reusable.software"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "03e9dd77e945433a1835813b49492eb9a045cb64",
          "message": "Update pr_3302.prdoc (#3985)\n\nProperly account for #3302, cc #3984.",
          "timestamp": "2024-04-05T11:53:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/03e9dd77e945433a1835813b49492eb9a045cb64"
        },
        "date": 1712322638561,
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
            "value": 11.47004411294,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18155972702666662,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ba0f8de0c74b1308a53942c406137a748ef79925",
          "message": "[pallet-broker] Fix claim revenue behaviour for zero timeslices (#3997)\n\nThis PR adds a check that `max_timeslices > 0` and errors if not. It\nalso adds a test for this behaviour and cleans up some misleading docs.",
          "timestamp": "2024-04-05T13:18:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ba0f8de0c74b1308a53942c406137a748ef79925"
        },
        "date": 1712329402270,
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
            "name": "test-environment",
            "value": 0.20983529252666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.52273683206,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alessandro Siniscalchi",
            "username": "asiniscalchi",
            "email": "asiniscalchi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "33bbdb3cae5911562edbeb55a53549650e66f3e1",
          "message": "[parachain-template] benchmarks into `mod benchmarks` (#3818)\n\nThis PR introduces a dedicated module for benchmarks within the\nparachain runtime. By segregating benchmarks into their own module, we\nachieve a cleaner project structure and improved readability,\nfacilitating easier maintenance and updates.\n\n### Key Changes:\n- **New Benchmarks Module**: A new file `benchmarks.rs` is added,\nencapsulating the benchmarking code for various pallets.\n- **Refactoring `lib.rs`**: The main runtime library file (`lib.rs`) has\nbeen updated to reflect the extraction of benchmark definitions. By\nmoving these definitions to `benchmarks.rs`, we reduce clutter in\n`lib.rs`, streamlining the runtime's core logic and configuration.\n\n### Benefits of This Refactoring:\n- **Focused Benchmarking**: Developers can now easily locate and modify\nbenchmarks without navigating through the core runtime logic, enabling\ntargeted performance improvements.\n- **Cleaner Codebase**: Segregating benchmarks from the main runtime\nlogic helps maintain a clean, well-organized codebase, simplifying\nnavigation and maintenance.\n- **Scalability**: As the parachain evolves, adding or updating\nbenchmarks becomes more straightforward, supporting scalability and\nadaptability of the runtime.\n\n### Summary of Changes:\n- Created `benchmarks.rs` to house the benchmarking suite.\n- Streamlined `lib.rs` by removing the inlined benchmark definitions and\nlinking to the new benchmarks module.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-05T14:46:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/33bbdb3cae5911562edbeb55a53549650e66f3e1"
        },
        "date": 1712333131126,
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
            "value": 11.37525047802,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17726469340666665,
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
          "id": "0832f0f36db3ff04545655f3c33bea03dc161987",
          "message": "Rococo/Westend: publish `claim_queue` Runtime API  (#4005)\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-05T15:50:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0832f0f36db3ff04545655f3c33bea03dc161987"
        },
        "date": 1712336921463,
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
            "value": 0.18511896495333335,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.475710754086672,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "05b97068f9a440f89246c5fdea532fda369e7794",
          "message": "Bump h2 from 0.3.24 to 0.3.26 (#4008)\n\nBumps [h2](https://github.com/hyperium/h2) from 0.3.24 to 0.3.26.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/hyperium/h2/releases\">h2's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v0.3.26</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>Limit number of CONTINUATION frames for misbehaving\nconnections.</li>\n</ul>\n<p>See <a\nhref=\"https://seanmonstar.com/blog/hyper-http2-continuation-flood/\">https://seanmonstar.com/blog/hyper-http2-continuation-flood/</a>\nfor more info.</p>\n<h2>v0.3.25</h2>\n<h2>What's Changed</h2>\n<ul>\n<li>perf: optimize header list size calculations by <a\nhref=\"https://github.com/Noah-Kennedy\"><code>@​Noah-Kennedy</code></a>\nin <a\nhref=\"https://redirect.github.com/hyperium/h2/pull/750\">hyperium/h2#750</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/hyperium/h2/compare/v0.3.24...v0.3.25\">https://github.com/hyperium/h2/compare/v0.3.24...v0.3.25</a></p>\n</blockquote>\n</details>\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/hyperium/h2/blob/v0.3.26/CHANGELOG.md\">h2's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>0.3.26 (April 3, 2024)</h1>\n<ul>\n<li>Limit number of CONTINUATION frames for misbehaving\nconnections.</li>\n</ul>\n<h1>0.3.25 (March 15, 2024)</h1>\n<ul>\n<li>Improve performance decoding many headers.</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/357127e279c06935830fe2140378312eac801494\"><code>357127e</code></a>\nv0.3.26</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/1a357aaefc7243fdfa9442f45d90be17794a4004\"><code>1a357aa</code></a>\nfix: limit number of CONTINUATION frames allowed</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/5b6c9e0da092728d702dff3607626aafb7809d77\"><code>5b6c9e0</code></a>\nrefactor: cleanup new unused warnings (<a\nhref=\"https://redirect.github.com/hyperium/h2/issues/757\">#757</a>)</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/3a798327211345b9b2bf797e2e4f3aca4e0ddfee\"><code>3a79832</code></a>\nv0.3.25</li>\n<li><a\nhref=\"https://github.com/hyperium/h2/commit/94e80b1c72bec282bb5d13596803e6fb341fec4c\"><code>94e80b1</code></a>\nperf: optimize header list size calculations (<a\nhref=\"https://redirect.github.com/hyperium/h2/issues/750\">#750</a>)</li>\n<li>See full diff in <a\nhref=\"https://github.com/hyperium/h2/compare/v0.3.24...v0.3.26\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=h2&package-manager=cargo&previous-version=0.3.24&new-version=0.3.26)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-04-05T18:37:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/05b97068f9a440f89246c5fdea532fda369e7794"
        },
        "date": 1712349034377,
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
            "value": 0.16516486598666671,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.648293687759999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sergej Sakac",
            "username": "Szegoo",
            "email": "73715684+Szegoo@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1c85bfe901741f9c456af1ac92008d647660a2f4",
          "message": "Broker: sale price runtime api (#3485)\n\nDefines a runtime api for `pallet-broker` for getting the current price\nof a core if there is an ongoing sale.\n\nCloses: #3413\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-05T23:29:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1c85bfe901741f9c456af1ac92008d647660a2f4"
        },
        "date": 1712364242929,
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
            "value": 11.626425716360002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18514961744666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9d6261892814fa27c97881c0321c008d7340b54b",
          "message": "`pallet-uniques`: decrement `total_deposit` when clearing collection metadata (#3976)\n\nDecrements `total_deposit` when collection metadata is cleared in\n`pallet-nfts` and `pallet-uniques`.",
          "timestamp": "2024-04-06T05:10:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9d6261892814fa27c97881c0321c008d7340b54b"
        },
        "date": 1712384823760,
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
            "value": 11.822138839686668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23804992951333329,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "74d6309c0cecf0636edd729365e7b723a62c8c72",
          "message": "Improve frame umbrella crate doc experience (#4007)\n\n1. Add `#[doc(no_inline)]` to frame umbrella crate re-exports that\neventually resolve to `frame_support_procedural` so docs don't look like\nthe screenshot below and instead link to the proper `frame-support`\ndocs.\n<img width=\"1512\" alt=\"Screenshot 2024-04-05 at 20 05 01\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/16665596/a41daa4c-ebca-44a4-9fea-f9f336314e13\">\n\n\n2. Remove `\"Rust-Analyzer Users: \"` prefix from\n`frame_support_procedural` doc comments, since these doc comments are\nvisible in the web documentation and possible to stumble upon especially\nwhen navigating from the frame umbrella crate.\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-06T10:02:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/74d6309c0cecf0636edd729365e7b723a62c8c72"
        },
        "date": 1712402467486,
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
            "value": 0.20519104653333348,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.761446654873328,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Squirrel",
            "username": "gilescope",
            "email": "gilescope@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "994003854363b3c5fd0d7343f93aa1e54edb1ad0",
          "message": "Major bump of tracing-subscriber version (#3891)\n\nI don't think there are any more releases to the 0.2.x versions, so best\nwe're on the 0.3.x release.\n\nNo change on the benchmarks, fast local time is still just as fast as\nbefore:\n\nnew version bench:\n```\nfast_local_time         time:   [30.551 ns 30.595 ns 30.668 ns]\n```\n\nold version bench:\n```\nfast_local_time         time:   [30.598 ns 30.646 ns 30.723 ns]\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-06T13:54:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/994003854363b3c5fd0d7343f93aa1e54edb1ad0"
        },
        "date": 1712416262438,
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
            "value": 11.417551235580001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17464516855333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "HongKuang",
            "username": "HongKuang",
            "email": "166261675+HongKuang@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bd4471b4fcf46123df6115b70b32e47276a7ae60",
          "message": "Fix some typos (#4018)\n\nSigned-off-by: hongkuang <liurenhong@outlook.com>",
          "timestamp": "2024-04-08T04:21:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd4471b4fcf46123df6115b70b32e47276a7ae60"
        },
        "date": 1712554862177,
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
            "name": "test-environment",
            "value": 0.22045573987999997,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.820600835306669,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "59f868d1e9502cb4e434127cac6e439d01d7dd2b",
          "message": "Deprecate `para_id()` from `CoreState` in polkadot primitives (#3979)\n\nWith Coretime enabled we can no longer assume there is a static 1:1\nmapping between core index and para id. This mapping should be obtained\nfrom the scheduler/claimqueue on block by block basis.\n\nThis PR modifies `para_id()` (from `CoreState`) to return the scheduled\n`ParaId` for occupied cores and removes its usages in the code.\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/3948\n\n---------\n\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-04-08T05:58:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59f868d1e9502cb4e434127cac6e439d01d7dd2b"
        },
        "date": 1712561651083,
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
            "value": 11.677002498946665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1820540862866666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c1063a530e46c4af0a934ca50422b69182255e60",
          "message": "sc-beefy-consensus: Remove unneeded stream. (#4015)\n\nThe stream was just used to communicate from the validator the peer\nreports back to the gossip engine. Internally the gossip engine just\nforwards these reports to the networking engine. So, we can just do this\ndirectly.\n\nThe reporting stream was also pumped [in the worker behind the\nengine](https://github.com/paritytech/polkadot-sdk/blob/9d6261892814fa27c97881c0321c008d7340b54b/substrate/client/consensus/beefy/src/worker.rs#L939).\nThis means if there was a lot of data incoming over the engine, the\nreporting stream was almost never processed and thus, it could have\nstarted to grow and we have seen issues around this.\n\nPartly Closes: https://github.com/paritytech/polkadot-sdk/issues/3945",
          "timestamp": "2024-04-08T08:28:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c1063a530e46c4af0a934ca50422b69182255e60"
        },
        "date": 1712569611924,
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
            "value": 0.18391057995999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.477345601633333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fdb1dba2e1eded282ff2eaf745d55c378f777cc2",
          "message": "Add best block indicator to informant message + print parent block on import  message (#4021)\n\nSometimes you need to debug some issues just by the logs and reconstruct\nwhat happened.\nIn these scenarios it would be nice to know if a block was imported as\nbest block, and what it parent was.\nSo here I propose to change the output of the informant to this:\n\n```\n2024-04-05 20:38:22.004  INFO ⋮substrate: [Parachain] ✨ Imported #18 (0xe7b3…4555 -> 0xbd6f…ced7)    \n2024-04-05 20:38:24.005  INFO ⋮substrate: [Parachain] ✨ Imported #19 (0xbd6f…ced7 -> 0x4dd0…d81f)    \n2024-04-05 20:38:24.011  INFO ⋮substrate: [jobless-children-5352] 🌟 Imported #42 (0xed2e…27fc -> 0x718f…f30e)    \n2024-04-05 20:38:26.005  INFO ⋮substrate: [Parachain] ✨ Imported #20 (0x4dd0…d81f -> 0x6e85…e2b8)    \n2024-04-05 20:38:28.004  INFO ⋮substrate: [Parachain] 🌟 Imported #21 (0x6e85…e2b8 -> 0xad53…2a97)    \n2024-04-05 20:38:30.004  INFO ⋮substrate: [Parachain] 🌟 Imported #22 (0xad53…2a97 -> 0xa874…890f)    \n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-08T13:30:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fdb1dba2e1eded282ff2eaf745d55c378f777cc2"
        },
        "date": 1712585161317,
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
            "value": 11.607628992293332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17498935557333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "216509dbaa2c2941ee75fbcc9a086deab5e2c8a6",
          "message": "Github workflow to automate release draft creation (#3978)\n\nThis PR introduces the github flow which will create a release draft\nautomatically when the rc tag is pushed. The flow contains the following\nsteps:\n\n- Gets the info about rust version used to build the node\n- Builds the runtimes using `srtool`\n- Extracts the info about each runtime \n- Aggregates the changelog from the prdocs\n- Creates the release draft containing all the info related to the\nrelease (changelog, runtimes, rust versions)\n- Attaches the runtimes to the draft\n- Posts the message to the RelEng internal channel to inform that the\nbuild is done.\n\nRelated to the #3295\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-08T13:36:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/216509dbaa2c2941ee75fbcc9a086deab5e2c8a6"
        },
        "date": 1712588773815,
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
            "value": 11.617422773873335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19856731390000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Aaro Altonen",
            "username": "altonen",
            "email": "48052676+altonen@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "80616f6d03661106326b621e9cc3ee1d2fa283ed",
          "message": "Integrate litep2p into Polkadot SDK (#2944)\n\n[litep2p](https://github.com/altonen/litep2p) is a libp2p-compatible P2P\nnetworking library. It supports all of the features of `rust-libp2p`\nthat are currently being utilized by Polkadot SDK.\n\nCompared to `rust-libp2p`, `litep2p` has a quite different architecture\nwhich is why the new `litep2p` network backend is only able to use a\nlittle of the existing code in `sc-network`. The design has been mainly\ninfluenced by how we'd wish to structure our networking-related code in\nPolkadot SDK: independent higher-levels protocols directly communicating\nwith the network over links that support bidirectional backpressure. A\ngood example would be `NotificationHandle`/`RequestResponseHandle`\nabstractions which allow, e.g., `SyncingEngine` to directly communicate\nwith peers to announce/request blocks.\n\nI've tried running `polkadot --network-backend litep2p` with a few\ndifferent peer configurations and there is a noticeable reduction in\nnetworking CPU usage. For high load (`--out-peers 200`), networking CPU\nusage goes down from ~110% to ~30% (80 pp) and for normal load\n(`--out-peers 40`), the usage goes down from ~55% to ~18% (37 pp).\n\nThese should not be taken as final numbers because:\n\na) there are still some low-hanging optimization fruits, such as\nenabling [receive window\nauto-tuning](https://github.com/libp2p/rust-yamux/pull/176), integrating\n`Peerset` more closely with `litep2p` or improving memory usage of the\nWebSocket transport\nb) fixing bugs/instabilities that incorrectly cause `litep2p` to do less\nwork will increase the networking CPU usage\nc) verification in a more diverse set of tests/conditions is needed\n\nNevertheless, these numbers should give an early estimate for CPU usage\nof the new networking backend.\n\nThis PR consists of three separate changes:\n* introduce a generic `PeerId` (wrapper around `Multihash`) so that we\ndon't have use `NetworkService::PeerId` in every part of the code that\nuses a `PeerId`\n* introduce `NetworkBackend` trait, implement it for the libp2p network\nstack and make Polkadot SDK generic over `NetworkBackend`\n  * implement `NetworkBackend` for litep2p\n\nThe new library should be considered experimental which is why\n`rust-libp2p` will remain as the default option for the time being. This\nPR currently depends on the master branch of `litep2p` but I'll cut a\nnew release for the library once all review comments have been\naddresses.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-08T16:44:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/80616f6d03661106326b621e9cc3ee1d2fa283ed"
        },
        "date": 1712599478651,
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
            "value": 0.16600557246666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.550451701786667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Léa Narzis",
            "username": "lean-apple",
            "email": "78718413+lean-apple@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8",
          "message": "Adapt `RemoteExternalities` and its related types to be used with generic hash parameters (#3953)\n\nCloses  https://github.com/paritytech/polkadot-sdk/issues/3737\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-08T21:56:41Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8"
        },
        "date": 1712618037452,
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
            "value": 0.21343971101999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.778383631099999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9d6c0f446a0b3d5774a2d667b67ecce2d4655209",
          "message": "Removed unused deps from Snowbridge deps (#4029)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-09T08:45:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9d6c0f446a0b3d5774a2d667b67ecce2d4655209"
        },
        "date": 1712657349878,
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
            "name": "test-environment",
            "value": 0.16061193283333336,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.592265496140001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dmitry Markin",
            "username": "dmitry-markin",
            "email": "dmitry@markin.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a26d25d5c75d4feb02250f775cd162cc4952d8d2",
          "message": "Detect closed notification substreams instead of evicting all peers (#3983)\n\nThis PR brings the fix\nhttps://github.com/paritytech/substrate/pull/13396 to polkadot-sdk.\n\nIn the past, due to insufficient inbound slot count on polkadot &\nkusama, this fix led to low peer count. The situation has improved since\nthen after changing the default ratio between `--in-peers` &\n`--out-peers`.\n\nNevertheless, it's expected that the reported total peer count with this\nfix is going to be lower than without it. This should be seen as the\ncorrect number of working connections reported, as opposed to also\nreporting already closed connections, and not as lower count of working\nconnections with peers.\n\nThis PR also removes the peer eviction mechanism, as closed substream\ndetection is a more granular way of detecting peers that stopped syncing\nwith us.\n\nThe burn-in has been already performed as part of testing these changes\nin https://github.com/paritytech/polkadot-sdk/pull/3426.\n\n---------\n\nCo-authored-by: Aaro Altonen <a.altonen@hotmail.com>",
          "timestamp": "2024-04-09T12:40:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a26d25d5c75d4feb02250f775cd162cc4952d8d2"
        },
        "date": 1712671116396,
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
            "value": 11.197698932079998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1567376294066667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "598e95577dbf10c4d940393c29a6476be91d8fd2",
          "message": "rpc-v2/transaction: Generate `Invalid` events and add tests (#3784)\n\nThis PR ensures that the transaction API generates an `Invalid` events\nfor transaction bytes that fail to decode.\n\nThe spec mentioned the `Invalid` event at the jsonrpc error section,\nhowever this spec PR makes things clearer:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/146\n\nWhile at it have discovered an inconsistency with the generated events.\nThe drop event from the transaction pool was incorrectly mapped to the\n`invalid` event.\n\nAdded tests for the API stabilize the API soon:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/144\n\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3083\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-09T13:57:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/598e95577dbf10c4d940393c29a6476be91d8fd2"
        },
        "date": 1712676060027,
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
            "value": 11.47383377676,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23429282756000006,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "df818d2974e059008cd8fa531f70b6657787b5be",
          "message": "Move cumulus zombienet tests to aura & async backing (#3568)\n\nCumulus test-parachain node and test runtime were still using relay\nchain consensus and 12s blocktimes. With async backing around the corner\non the major chains we should switch our tests too.\n\nAlso needed to nicely test the changes coming to collators in #3168.\n\n### Changes Overview\n- Followed the [migration\nguide](https://wiki.polkadot.network/docs/maintain-guides-async-backing)\nfor async backing for the cumulus-test-runtime\n- Adjusted the cumulus-test-service to use the correct import-queue,\nlookahead collator etc.\n- The block validation function now uses the Aura Ext Executor so that\nthe seal of the block is validated\n- Previous point requires that we seal block before calling into\n`validate_block`, I introduced a helper function for that\n- Test client adjusted to provide a slot to the relay chain proof and\nthe aura pre-digest",
          "timestamp": "2024-04-09T16:53:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/df818d2974e059008cd8fa531f70b6657787b5be"
        },
        "date": 1712697091180,
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
            "value": 11.42727962979333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19716123428666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "df818d2974e059008cd8fa531f70b6657787b5be",
          "message": "Move cumulus zombienet tests to aura & async backing (#3568)\n\nCumulus test-parachain node and test runtime were still using relay\nchain consensus and 12s blocktimes. With async backing around the corner\non the major chains we should switch our tests too.\n\nAlso needed to nicely test the changes coming to collators in #3168.\n\n### Changes Overview\n- Followed the [migration\nguide](https://wiki.polkadot.network/docs/maintain-guides-async-backing)\nfor async backing for the cumulus-test-runtime\n- Adjusted the cumulus-test-service to use the correct import-queue,\nlookahead collator etc.\n- The block validation function now uses the Aura Ext Executor so that\nthe seal of the block is validated\n- Previous point requires that we seal block before calling into\n`validate_block`, I introduced a helper function for that\n- Test client adjusted to provide a slot to the relay chain proof and\nthe aura pre-digest",
          "timestamp": "2024-04-09T16:53:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/df818d2974e059008cd8fa531f70b6657787b5be"
        },
        "date": 1712711365211,
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
            "value": 11.19417283856,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17280806947333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2d927b077263f2d39586a49cf993ecf3885e4de5",
          "message": "Contracts: Fix legacy uapi (#3994)\n\nFix some broken legacy definitions of pallet_contracts_uapi storage host\nfunctions",
          "timestamp": "2024-04-10T05:05:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2d927b077263f2d39586a49cf993ecf3885e4de5"
        },
        "date": 1712730512169,
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
            "value": 11.430980926446669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.208015568,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d38f6e6728b2a0ac337b0b7b9f87862af5cb87b4",
          "message": "Update benchmarking macros (#3934)\n\nCurrent benchmarking macro returns a closure with the captured\nbenchmarked code.\nThis can cause issues when the benchmarked code has complex lifetime\nrequirements.\n\nThis PR updates the existing macro by injecting the recording parameter\nand invoking the start / stop method around the benchmarked block\ninstead of returning a closure\n\nOne other added benefit is that you can write this kind of code now as\nwell:\n\n```rust\nlet v;\n#[block]\n{ v = func.call(); }\ndbg!(v); // or assert something on v\n```\n\n\n[Weights compare\nlink](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=pg/fix-weights&new=pg/bench_update&path_pattern=substrate/frame/**/src/weights.rs,polkadot/runtime/*/src/weights/**/*.rs,polkadot/bridges/modules/*/src/weights.rs,cumulus/**/weights/*.rs,cumulus/**/weights/xcm/*.rs,cumulus/**/src/weights.rs)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-04-10T06:44:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d38f6e6728b2a0ac337b0b7b9f87862af5cb87b4"
        },
        "date": 1712737463944,
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
            "value": 11.44102709568,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19278916063333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Matteo Muraca",
            "username": "muraca",
            "email": "56828990+muraca@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "92e142555d45f97aa88d241665d9952d12f4ae40",
          "message": "Removed `pallet::getter` usage from Polkadot Runtime pallets (#3660)\n\nPart of #3326 \n\n@kianenigma @ggwpez \n\npolkadot address: 12poSUQPtcF1HUPQGY3zZu2P8emuW9YnsPduA4XG3oCEfJVp\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nCo-authored-by: ordian <write@reusable.software>",
          "timestamp": "2024-04-10T08:35:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92e142555d45f97aa88d241665d9952d12f4ae40"
        },
        "date": 1712744663981,
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
            "value": 11.08063457244667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18043470737999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0d71753e0eb1c0fdbeb09af5db7a2e1b5f150b13",
          "message": "Contracts: Only exec parsed code in benchmarks (#3915)\n\n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Fbench_tweaks&path_pattern=substrate%2Fframe%2F**%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\nNote: Raw weights change does not mean much here, as this PR reduce the\nscope of what is benchmarked, they are therefore decreased by a good\nmargin. One should instead print the Schedule using\n\ncargo test --features runtime-benchmarks bench_print_schedule --\n--nocapture\nor following the instructions from the\n[README](https://github.com/paritytech/polkadot-sdk/tree/pg/bench_tweaks/substrate/frame/contracts#schedule)\nfor looking at the Schedule of a specific runtime\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-10T13:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0d71753e0eb1c0fdbeb09af5db7a2e1b5f150b13"
        },
        "date": 1712761523308,
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
            "value": 0.20591315715333341,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.462354974453334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "cd010925e12c3c1d22b47cc9185c394366e65c5f",
          "message": "net/strategy: Log bad peerId from on_validated_block_announce (#4051)\n\nThis tiny PR extends the `on_validated_block_announce` log with the bad\nPeerID.\nUsed to identify if the peerID is malicious by correlating with other\nlogs (ie peer-set).\n\nWhile at it, have removed the `\\n` from a multiline log, which did not\nplay well with\n[sub-triage-logs](https://github.com/lexnv/sub-triage-logs/tree/master).\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-10T15:29:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cd010925e12c3c1d22b47cc9185c394366e65c5f"
        },
        "date": 1712773015860,
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
            "value": 11.497953049586668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23198031086,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Milos Kriz",
            "username": "miloskriz",
            "email": "82968568+miloskriz@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d21a41f23847f1aeca637ace60f58723c38f6bf3",
          "message": "Amend chainspecs for `people-westend` and add IBP bootnodes (#4072)\n\nDear team, dear @NachoPal @joepetrowski @bkchr @ggwpez,\n\nThis is a retry of #3957, after merging master as advised!,\n\nMany thanks!\n\n**_Milos_**\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-10T19:31:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d21a41f23847f1aeca637ace60f58723c38f6bf3"
        },
        "date": 1712779878083,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 2.2333333333333356,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307997.38,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 11.53667422076,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2083107538266666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Milos Kriz",
            "username": "miloskriz",
            "email": "82968568+miloskriz@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d21a41f23847f1aeca637ace60f58723c38f6bf3",
          "message": "Amend chainspecs for `people-westend` and add IBP bootnodes (#4072)\n\nDear team, dear @NachoPal @joepetrowski @bkchr @ggwpez,\n\nThis is a retry of #3957, after merging master as advised!,\n\nMany thanks!\n\n**_Milos_**\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-10T19:31:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d21a41f23847f1aeca637ace60f58723c38f6bf3"
        },
        "date": 1712782202498,
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
            "value": 11.101794399239996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1736486705933333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "643aa2be2a2c0611eeb648cfc21eb4cb3c1c9cd8",
          "message": "Contracts: Remove ED from base deposit (#3536)\n\n- Update internal logic so that the storage_base_deposit does not\ninclude ED\n- add v16 migration to update ContractInfo struct with this change\n\nBefore:\n<img width=\"820\" alt=\"Screenshot 2024-03-21 at 11 23 29\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/521091/a0a8df0d-e743-42c5-9e16-cf2ec1aa949c\">\n\nAfter:\n![Screenshot 2024-03-21 at 11 23\n42](https://github.com/paritytech/polkadot-sdk/assets/521091/593235b0-b866-4915-b653-2071d793228b)\n\n---------\n\nCo-authored-by: Cyrill Leutwiler <cyrill@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-10T20:32:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/643aa2be2a2c0611eeb648cfc21eb4cb3c1c9cd8"
        },
        "date": 1712787895281,
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
            "name": "test-environment",
            "value": 0.21392959954000001,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.464937940086672,
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
          "id": "69cc7f2090e169e736d9c998c29467040521881d",
          "message": "Fix ClaimQueue case of nothing scheduled on session boundary  (#4065)\n\nSame issue but about av-cores was fixed in\nhttps://github.com/paritytech/polkadot-sdk/pull/1403\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-11T08:37:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/69cc7f2090e169e736d9c998c29467040521881d"
        },
        "date": 1712829258564,
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
            "value": 11.253908213973334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15999301305999997,
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
          "id": "9ede4152ef0d539019875e6aff97dbe0744a4053",
          "message": "collation-generation: Avoid using `para_backing_state` if runtime is ancient (#4070)\n\nfixes https://github.com/paritytech/polkadot-sdk/issues/4067\n\nAlso add an early bail out for look ahead collator such that we don't\nwaste time if a CollatorFn is not set.\n\nTODO:\n- [x] add test.\n- [x] Polkadot System Parachain burn-in.\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-11T10:36:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9ede4152ef0d539019875e6aff97dbe0744a4053"
        },
        "date": 1712837714836,
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
            "value": 11.272893857826668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16653987614666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "832570545b0311f533499ead57d764a2bc04145c",
          "message": "Fix link check (#4074)\n\nCloses #4041\n\nChanges:\n- Increase cache size and reduce retries.\n- Ignore Substrate SE links :(\n- Fix broken link.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-11T10:55:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/832570545b0311f533499ead57d764a2bc04145c"
        },
        "date": 1712843506910,
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
            "name": "test-environment",
            "value": 0.17689647383999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.23242465047333,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "6ebf491b50a45d1814a72a6ac4287f4a15ba39ce",
          "message": "[ci] Divide subsystem-regression-tests into 2 jobs (#4076)\n\nCurrently `subsystem-regression-tests` job fails if the first benchmarks\nfail and there is no result for the second benchmark. Also dividing the\njob makes the pipeline faster (currently it's a longest job)\n\ncc https://github.com/paritytech/ci_cd/issues/969\ncc @AndreiEres\n\n---------\n\nCo-authored-by: Andrei Eres <eresav@me.com>",
          "timestamp": "2024-04-11T15:29:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ebf491b50a45d1814a72a6ac4287f4a15ba39ce"
        },
        "date": 1712854853426,
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
            "value": 0.21189825042000002,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 11.457229359566663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "25f038aa8e381911832450b2e2452d5cc64dfe37",
          "message": "Run subsystem-benchmark without network latency (#4068)\n\nImplements the idea from\nhttps://github.com/paritytech/polkadot-sdk/pull/3899\n- Removed latencies\n- Number of runs reduced from 50 to 5, according to local runs it's\nquite enough\n- Network message is always sent in a spawned task, even if latency is\nzero. Without it, CPU time sometimes spikes.\n- Removed the `testnet` profile because we probably don't need that\ndebug additions.\n\nAfter the local tests I can't say that it brings a significant\nimprovement in the stability of the results. However, I belive it is\nworth trying and looking at the results over time.",
          "timestamp": "2024-04-11T16:54:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/25f038aa8e381911832450b2e2452d5cc64dfe37"
        },
        "date": 1712859218538,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.2009481294,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24930246080000001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "39b1f50f1c251def87c1625d68567ed252dc6272",
          "message": "Remove deprecated `TryRuntime` subcommand (#4017)\n\nCompletes the removal of `try-runtime-cli` logic from `polkadot-sdk`.",
          "timestamp": "2024-04-11T20:01:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/39b1f50f1c251def87c1625d68567ed252dc6272"
        },
        "date": 1712870113902,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.15880675726667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2156480974,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "eskimor",
            "username": "eskimor",
            "email": "eskimor@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a64009ae00f954acf907593309af2b1f1797e87d",
          "message": "Improve docs of broker pallet (#3980)\n\nSmall adjustments which should make understanding what is going on much\neasier for future readers.\n\nInitialization is a bit messy, the very least we should do is adding\ndocumentation to make it harder to use wrongly.\n\nI was thinking about calling `request_core_count` right from\n`start_sales`, but as explained in the docs, this is not necessarily\nwhat you want.\n\n---------\n\nCo-authored-by: eskimor <eskimor@no-such-url.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>",
          "timestamp": "2024-04-12T10:23:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a64009ae00f954acf907593309af2b1f1797e87d"
        },
        "date": 1712935748683,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2193420735333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.143650919399999,
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
          "id": "2dfe5f745cd6daa362c6d8371e723fe4f0429b67",
          "message": "Runtime API: introduce `candidates_pending_availability` (#4027)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3576\n\nRequired by elastic scaling collators.\nDeprecates old API: `candidate_pending_availability`.\n\nTODO:\n- [x] PRDoc\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-04-12T10:50:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2dfe5f745cd6daa362c6d8371e723fe4f0429b67"
        },
        "date": 1712940237873,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2569376419999999,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.004310170466667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "wersfeds",
            "username": "wersfeds",
            "email": "wqq1479794@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "480d5d0feabcb68b9097bfd5cb2aa07523f2bfbc",
          "message": "chore: fix some typos (#4095)",
          "timestamp": "2024-04-12T14:32:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/480d5d0feabcb68b9097bfd5cb2aa07523f2bfbc"
        },
        "date": 1712945596197,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2074772158,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.806169883199995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c963dc283af77824ceeeecc20e205f3a17968746",
          "message": "Synchronize templates (#4040)\n\n- Progresses https://github.com/paritytech/polkadot-sdk/issues/3155\n\n### What's inside\n\nA job, that will take each of the three\n[templates](https://github.com/paritytech/polkadot-sdk/tree/master/templates),\nyank them out of the monorepo workspace, and push to individual\nrepositories\n([1](https://github.com/paritytech/polkadot-sdk-minimal-template),\n[2](https://github.com/paritytech/polkadot-sdk-parachain-template),\n[3](https://github.com/paritytech/polkadot-sdk-solochain-template)).\n\nIn case the build/test does not succeed, a PR such as [this\none](https://github.com/paritytech-stg/polkadot-sdk-solochain-template/pull/2)\ngets created instead.\n\nI'm proposing a manual dispatch trigger for now - so we can test and\niterate faster - and change it to fully automatic triggered by releases\nlater.\n\nThe manual trigger looks like this:\n\n<img width=\"340px\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/12039224/e87e0fda-23a3-4735-9035-af801e8417fc\"/>\n\n### How it works\n\nThe job replaces dependencies [referenced by\ngit](https://github.com/paritytech/polkadot-sdk/blob/d733c77ee2d2e8e2d5205c552a5efb2e5b5242c8/templates/minimal/pallets/template/Cargo.toml#L25)\nwith a reference to released crates using\n[psvm](https://github.com/paritytech/psvm).\n\nIt creates a new workspace for the template, and adapts what's needed\nfrom the `polkadot-sdk` workspace.\n\n### See the results\n\nThe action has been tried out in staging, and the results can be\nobserved here:\n\n- [minimal\nstg](https://github.com/paritytech-stg/polkadot-sdk-minimal-template/)\n- [parachain\nstg](https://github.com/paritytech-stg/polkadot-sdk-parachain-template/)\n- [solochain\nstg](https://github.com/paritytech-stg/polkadot-sdk-solochain-template/)\n\nThese are based on the `1.9.0` release (using `release-crates-io-v1.9.0`\nbranch).",
          "timestamp": "2024-04-12T17:24:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c963dc283af77824ceeeecc20e205f3a17968746"
        },
        "date": 1712951199951,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.978819099600003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.22865346686666674,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Vedhavyas Singareddi",
            "username": "vedhavyas",
            "email": "vedhavyas.singareddi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5b513cc0e995140b17e200d75442d7a3f2436243",
          "message": "define block hash provider and default impl using frame_system (#4080)\n\nThis PR introduces `BlockHashProvider` into `pallet_mmr::Config`\nThis type is used to get `block_hash` for a given `block_number` rather\nthan directly using `frame_system::Pallet::block_hash`\n\nThe `DefaultBlockHashProvider` uses `frame_system::Pallet::block_hash`\nto get the `block_hash`\n\nCloses: #4062",
          "timestamp": "2024-04-12T21:57:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5b513cc0e995140b17e200d75442d7a3f2436243"
        },
        "date": 1712965787590,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1902417802666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.863522850333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8220c980084e70be55d956a69c5ebeebe47c9b9c",
          "message": "Fix zombienet-bridges-0001-asset-transfer-works (#4069)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3999\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-13T07:04:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8220c980084e70be55d956a69c5ebeebe47c9b9c"
        },
        "date": 1712997244843,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.958771620600004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21315963479999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7c698502d12b317c29838bfa6c0b928377477b19",
          "message": "sc_network_test: Announce only the highest block (#4111)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4100",
          "timestamp": "2024-04-13T08:40:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7c698502d12b317c29838bfa6c0b928377477b19"
        },
        "date": 1713001878636,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.480885335666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17554544033333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aa437974376a6c862af4afff2a3b74b13cb3596e",
          "message": "Use Github Issue Sync to automate issues in Parachain board (#3694)\n\nThis workflow will automatically add issues related to async backing to\nthe Parachain team board, updating a custom \"meta\" field.\n\nRequested by @the-right-joyce",
          "timestamp": "2024-04-13T09:33:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa437974376a6c862af4afff2a3b74b13cb3596e"
        },
        "date": 1713005168530,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2019766936666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.827565032533334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1bca825cc27599dfea7b254d0ce00e3c51e632ea",
          "message": "Use `master` environment in the synchronize templates workflow (#4114)",
          "timestamp": "2024-04-13T11:00:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1bca825cc27599dfea7b254d0ce00e3c51e632ea"
        },
        "date": 1713008862695,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.799656464333335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20963609379999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "30c58fa22ad9fc6630e07e43ee2307675462995a",
          "message": "Deploy `pallet-parameters` to rococo and fix dynamic_params name expand (#4006)\n\nChanges:\n- Add pallet-parameters to Rococo to configure the NIS and preimage\npallet.\n- Fix names of expanded dynamic params. Apparently, `to_class_case`\nremoves suffix `s`, and `Nis` becomes `Ni` 😑. Now using\n`to_pascal_case`.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Alessandro Siniscalchi <asiniscalchi@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-13T11:20:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/30c58fa22ad9fc6630e07e43ee2307675462995a"
        },
        "date": 1713012386243,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22164129346666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.845435249466666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jonathan Udd",
            "username": "jonathanudd",
            "email": "jonathan@dwellir.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6688eac5ab21e7d9138b7ce6c90d1ce88d1f8962",
          "message": "Adding Dwellir bootnodes for Coretime Westend, People Westend and Paseo (#4066)\n\nVerified by running a node using `--reserved-only` and\n`--reserved-nodes`.",
          "timestamp": "2024-04-14T13:23:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6688eac5ab21e7d9138b7ce6c90d1ce88d1f8962"
        },
        "date": 1713105281792,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2237714726666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.86077245186667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "88fe94516cf7c802e20aae3846d684c108765757",
          "message": "rococo_contracts: Adds missing migration (#4112)\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-14T20:39:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88fe94516cf7c802e20aae3846d684c108765757"
        },
        "date": 1713131403550,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.722635476533329,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20363307246666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6acf4787e168eea447b82b0a1f32e47bc794ae28",
          "message": "Bridge: slash destination may be an explicit account (#4106)\n\nExtracted to a separate PR as requested here:\nhttps://github.com/paritytech/parity-bridges-common/pull/2873#discussion_r1562459573",
          "timestamp": "2024-04-15T06:37:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6acf4787e168eea447b82b0a1f32e47bc794ae28"
        },
        "date": 1713167310004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.55380405726667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17742760686666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d1b0ef76a8b060437ec7dfc1bf6b400626cd6208",
          "message": "sp-api: Use macro to detect if `frame-metadata` is enabled (#4117)\n\nWhile `sp-api-proc-macro` isn't used directly and thus, it should have\nthe same features enabled as `sp-api`. However, I have seen issues\naround `frame-metadata` not being enabled for `sp-api`, but for\n`sp-api-proc-macro`. This can be prevented by using the\n`frame_metadata_enabled` macro from `sp-api` that ensures we have the\nsame feature set between both crates.",
          "timestamp": "2024-04-15T08:11:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1b0ef76a8b060437ec7dfc1bf6b400626cd6208"
        },
        "date": 1713173664640,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.838922510933335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24532125740000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8b4cfda7589325d1a34f70b3770ab494a9d4052c",
          "message": "added script to require a review post push (#3431)\n\nCloses https://github.com/paritytech/opstooling/issues/174\n\nAdded a new step in the action that triggers review bot to stop approval\nfrom new pushes.\n\nThis step works in the following way:\n- If the **author of the PR**, who **is not** a member of the org,\npushed a new commit then:\n- Review-Trigger requests new reviews from the reviewers and fails.\n\nIt *does not dismiss reviews*. It simply request them again, but they\nwill still be available.\n\nThis way, if the author changed something in the code, they will still\nneed to have this latest change approved to stop them from uploading\nmalicious code.\n\nFind the requested issue linked to this PR (it is from a private repo so\nI can't link it here)",
          "timestamp": "2024-04-15T13:46:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8b4cfda7589325d1a34f70b3770ab494a9d4052c"
        },
        "date": 1713191057171,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.794661547933334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20392643253333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d1f9fe0a994febff13f21b0edd223e838754e2fc",
          "message": "logging(fix): Use the proper log target for logging (#4124)\n\nThis PR ensures the proper logging target (ie `libp2p_tcp` or `beefy`)\nis displayed.\n\nThe issue has been introduced in:\nhttps://github.com/paritytech/polkadot-sdk/pull/4059, which removes the\nnormalized metadata of logs.\n\nFrom\n[documentation](https://docs.rs/tracing-log/latest/tracing_log/trait.NormalizeEvent.html#tymethod.normalized_metadata):\n\n> In tracing-log, an Event produced by a log (through\n[AsTrace](https://docs.rs/tracing-log/latest/tracing_log/trait.AsTrace.html))\nhas an hard coded “log” target\n\n>\n[normalized_metadata](https://docs.rs/tracing-log/latest/tracing_log/trait.NormalizeEvent.html#tymethod.normalized_metadata):\nIf this Event comes from a log, this method provides a new normalized\nMetadata which has all available attributes from the original log,\nincluding file, line, module_path and target\n\nThis has low implications if a version was deployed containing the\nmentioned pull request, as we'll lose the ability to distinguish between\nlog targets.\n\n### Before this PR\n\n```\n2024-04-15 12:45:40.327  INFO main log: Parity Polkadot\n2024-04-15 12:45:40.328  INFO main log: ✌️  version 1.10.0-d1b0ef76a8b\n2024-04-15 12:45:40.328  INFO main log: ❤️  by Parity Technologies <admin@parity.io>, 2017-2024\n2024-04-15 12:45:40.328  INFO main log: 📋 Chain specification: Development\n2024-04-15 12:45:40.328  INFO main log: 🏷  Node name: yellow-eyes-2963\n2024-04-15 12:45:40.328  INFO main log: 👤 Role: AUTHORITY\n2024-04-15 12:45:40.328  INFO main log: 💾 Database: RocksDb at /tmp/substrated39i9J/chains/rococo_dev/db/full\n2024-04-15 12:45:44.508  WARN main log: Took active validators from set with wrong size\n...\n\n2024-04-15 12:45:45.805  INFO                 main log: 👶 Starting BABE Authorship worker\n2024-04-15 12:45:45.806  INFO tokio-runtime-worker log: 🥩 BEEFY gadget waiting for BEEFY pallet to become available...\n2024-04-15 12:45:45.806 DEBUG tokio-runtime-worker log: New listen address: /ip6/::1/tcp/30333\n2024-04-15 12:45:45.806 DEBUG tokio-runtime-worker log: New listen address: /ip4/127.0.0.1/tcp/30333\n```\n\n### After this PR\n\n```\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: Parity Polkadot\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: ✌️  version 1.10.0-d1b0ef76a8b\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: ❤️  by Parity Technologies <admin@parity.io>, 2017-2024\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 📋 Chain specification: Development\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 🏷  Node name: helpless-lizards-0550\n2024-04-15 12:59:45.623  INFO main sc_cli::runner: 👤 Role: AUTHORITY\n...\n2024-04-15 12:59:50.204  INFO tokio-runtime-worker beefy: 🥩 BEEFY gadget waiting for BEEFY pallet to become available...\n2024-04-15 12:59:50.204 DEBUG tokio-runtime-worker libp2p_tcp: New listen address: /ip6/::1/tcp/30333\n2024-04-15 12:59:50.204 DEBUG tokio-runtime-worker libp2p_tcp: New listen address: /ip4/127.0.0.1/tcp/30333\n```\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-15T14:33:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1f9fe0a994febff13f21b0edd223e838754e2fc"
        },
        "date": 1713197596648,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.698327042066673,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19700537266666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0c9ad5306ce8bbc815d862121a42778c1ea734be",
          "message": "[pallet-broker] add tests for renewing leases (#4099)\n\nThe first test proves that parachains who were migrated over on a legacy\nlease can renew without downtime.\n\nThe exception is if their lease expires in period 0 - aka within\n`region_length` timeslices after `start_sales` is called. The second\ntest is designed such that it passes if the issue exists and should be\nfixed.\nThis will require an intervention on Kusama to add these renewals to\nstorage as it is too tight to schedule a runtime upgrade before the\nstart_sales call. All leases will still have at least two full regions\nof coretime.",
          "timestamp": "2024-04-15T16:28:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0c9ad5306ce8bbc815d862121a42778c1ea734be"
        },
        "date": 1713202791307,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.73661113026667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18936899626666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gui",
            "username": "thiolliere",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a8f4f4f00f8fc0da512a09e1450bf4cda954d70d",
          "message": "pallet assets: Fix errors (#4118)\n\n`LiveAsset` is an error to be returned when an asset is not supposed to\nbe live.\nAnd `AssetNotLive` is an error to be returned when an asset is supposed\nto be live, I don't think frozen qualifies as live.",
          "timestamp": "2024-04-15T18:45:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a8f4f4f00f8fc0da512a09e1450bf4cda954d70d"
        },
        "date": 1713211152606,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.9139278406,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20274400873333326,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alin Dima",
            "username": "alindima",
            "email": "alin@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4b5c3fd0cbb47f3484671ffce284b7586311126a",
          "message": "move fragment_tree module to its own folder (#4148)\n\nWill make https://github.com/paritytech/polkadot-sdk/pull/4035 easier to\nreview (the mentioned PR already does this move so the diff will be\nclearer).\n\nAlso called out as part of:\nhttps://github.com/paritytech/polkadot-sdk/pull/3233#discussion_r1490867383",
          "timestamp": "2024-04-16T07:25:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4b5c3fd0cbb47f3484671ffce284b7586311126a"
        },
        "date": 1713256504223,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22178554079999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.79222751306667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maksym H",
            "username": "mordamax",
            "email": "1177472+mordamax@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "61d45ed72b2f8afade997e1a973327f2ada02aa0",
          "message": "Update review-trigger.yml (#4137)\n\nFollowup after https://github.com/paritytech/polkadot-sdk/pull/3431\nPer\nhttps://stackoverflow.com/questions/63188674/github-actions-detect-author-association\nand https://michaelheap.com/github-actions-check-permission/\nlooks like just checking NOT a MEMBER is not correct, Not a CONTRIBUTORs\ncheck should be included",
          "timestamp": "2024-04-16T10:11:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/61d45ed72b2f8afade997e1a973327f2ada02aa0"
        },
        "date": 1713266602503,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.7398984588,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19437880333333338,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "753bf2d860e083b5da25fe4171c0e540ddad4888",
          "message": "[prdoc] Update docs (#3998)\n\nUpdating the prdoc doc file to be a bit more useful for new contributors\nand adding a SemVer section.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-16T15:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/753bf2d860e083b5da25fe4171c0e540ddad4888"
        },
        "date": 1713285027783,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.596873096133331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16416971600000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6f3d890ed35bfdee3e3f7d59018345635a62d1cd",
          "message": "FRAME: Unity Balance Conversion for Different IDs of Native Asset (#3659)\n\nIntroduce types to define 1:1 balance conversion for different relative\nasset ids/locations of native asset.\n\nExamples:\nnative asset on Asset Hub presented as `VersionedLocatableAsset` type in\nthe context of Relay Chain is\n```\n{\n  `location`: (0, Parachain(1000)),\n  `asset_id`: (1, Here),\n}\n```\nand it's balance should be converted 1:1 by implementations of\n`ConversionToAssetBalance` trait.\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-16T16:11:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6f3d890ed35bfdee3e3f7d59018345635a62d1cd"
        },
        "date": 1713288376611,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.667044661399999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1711067200666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e81322bc3e8192b536067fed3ef9e20f2752c376",
          "message": "Contracts verify benchmark block (#4130)\n\nAdd verify statement to ensure that benchmarks call do not revert\n\nAlso updated\n[benchmarks](https://weights.tasty.limo/compare?unit=time&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg/verify-benchmarks&path_pattern=substrate%2Fframe%2Fcontracts%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-16T23:38:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e81322bc3e8192b536067fed3ef9e20f2752c376"
        },
        "date": 1713314900476,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21458435506666662,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.829880137666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4be9f93cd7e81c71ead1a8b5445bc695d98b10ba",
          "message": "Adjust `xcm-bridge-hub-router`'s `SendXcm::validate` behavior for `NotApplicable` (#4162)\n\nThis PR adjusts `xcm-bridge-hub-router` to be usable in the chain of\nrouters when a `NotApplicable` error occurs.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4133\n\n## TODO\n\n- [ ] backport to polkadot-sdk 1.10.0 crates.io release",
          "timestamp": "2024-04-17T09:09:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4be9f93cd7e81c71ead1a8b5445bc695d98b10ba"
        },
        "date": 1713346828844,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.549001118933333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18394468333333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sergej Sakac",
            "username": "Szegoo",
            "email": "73715684+Szegoo@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e6f3106d894277deba043a83e91565de24263a1b",
          "message": "XCM coretime region transfers (#3455)\n\nThis PR introduces changes enabling the transfer of coretime regions via\nXCM.\n\nTL;DR: There are two primary issues that are resolved in this PR:\n\n1. The `mint` and `burn` functions were not implemented for coretime\nregions. These operations are essential for moving assets to and from\nthe XCM holding register.\n2. The transfer of non-fungible assets through XCM was previously\ndisallowed. This was due to incorrectly benchmarking non-fungible asset\ntransfers via XCM, which led to assigning it a weight of `Weight::Max`,\neffectively preventing its execution.\n\n### `mint_into` and `burn` implementation\n\nThis PR addresses the issue with cross-chain transferring regions back\nto the Coretime chain. Remote reserve transfers are performed by\nwithdrawing and depositing the asset to and from the holding registry.\nThis requires the asset to support burning and minting functionality.\n\nThis PR adds burning and minting; however, they work a bit differently\nthan usual so that the associated region record is not lost when\nburning. Instead of removing all the data, burning will set the owner of\nthe region to `None`, and when minting it back, it will set it to an\nactual value. So, when cross-chain transferring, withdrawing into the\nregistry will remove the region from its original owner, and when\ndepositing it from the registry, it will set its owner to another\naccount\n\nThis was originally implemented in this PR: #3455, however we decided to\nmove all of it to this single PR\n(https://github.com/paritytech/polkadot-sdk/pull/3455#discussion_r1547324892)\n\n### Fixes made in this PR\n\n- Update the `XcmReserveTransferFilter` on coretime chain since it is\nmeant as a reserve chain for coretime regions.\n- Update the XCM benchmark to use `AssetTransactor` instead of assuming\n`pallet-balances` for fungible transfers.\n- Update the XCM benchmark to properly measure weight consumption for\nnonfungible reserve asset transfers. ATM reserve transfers via the\nextrinsic do not work since the weight for it is set to `Weight::max()`.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/865\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>",
          "timestamp": "2024-04-17T09:25:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e6f3106d894277deba043a83e91565de24263a1b"
        },
        "date": 1713350634814,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.23329906419999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.85856696373333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4e10d3b0a6ec2eccf58c471e7739948c1a867acf",
          "message": "Asset Conversion: Pool Account ID derivation with additional Pallet ID seed (#3250)\n\nIntroduce `PalletId` as an additional seed parameter for pool's account\nid derivation.\n\nThe PR also introduces the `pallet_asset_conversion_ops` pallet with a\ncall to migrate a given pool to thew new account. Additionally\n`fungibles::lifetime::ResetTeam` and `fungible::lifetime::Refund`\ntraits, to facilitate the migration of pools.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-17T10:39:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4e10d3b0a6ec2eccf58c471e7739948c1a867acf"
        },
        "date": 1713356500419,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.740651882000003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19209161439999994,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bfbf7f5d6f5c491a820bb0a4fb9508ce52192a06",
          "message": "chainHead: Report unique hashes for pruned blocks (#3667)\n\nThis PR ensures that the reported pruned blocks are unique.\n\nWhile at it, ensure that the best block event is properly generated when\nthe last best block is a fork that will be pruned in the future.\n\nTo achieve this, the chainHead keeps a LRU set of reported pruned blocks\nto ensure the following are not reported twice:\n\n```bash\n\t finalized -> block 1 -> block 2 -> block 3\n\t\n\t                      -> block 2 -> block 4 -> block 5\n\t\n\t           -> block 1 -> block 2_f -> block 6 -> block 7 -> block 8\n```\n\nWhen block 7 is finalized the branch [block 2; block 3] is reported as\npruned.\nWhen block 8 is finalized the branch [block 2; block 4; block 5] should\nbe reported as pruned, however block 2 was already reported as pruned at\nthe previous step.\n\nThis is a side-effect of the pruned blocks being reported at level N -\n1. For example, if all pruned forks would be reported with the first\nencounter (when block 6 is finalized we know that block 3 and block 5\nare stale), we would not need the LRU cache.\n\ncc @paritytech/subxt-team  \n\nCloses https://github.com/paritytech/polkadot-sdk/issues/3658\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-04-17T15:29:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bfbf7f5d6f5c491a820bb0a4fb9508ce52192a06"
        },
        "date": 1713372839661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.658884640399995,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15859666359999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "305d311d5c732fcc4629f3295768f1ed44ef434c",
          "message": "Asset Conversion: Pool Touch Call (#3251)\n\nIntroduce `touch` call designed to address operational prerequisites\nbefore providing liquidity to a pool.\n\nThis function ensures that essential requirements, such as the presence\nof the pool's accounts, are fulfilled. It is particularly beneficial in\nscenarios where a pool creator removes the pool's accounts without\nproviding liquidity.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-04-17T16:45:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/305d311d5c732fcc4629f3295768f1ed44ef434c"
        },
        "date": 1713378251520,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.989279013466668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21943879793333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tin Chung",
            "username": "chungquantin",
            "email": "56880684+chungquantin@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d591b16f6b1dec88003323cdae0c3abe3b5c9cbe",
          "message": "Remove NotConcrete error (#3867)\n\n# Description\n- Link to issue: https://github.com/paritytech/polkadot-sdk/issues/3651\n\npolkadot address: 19nSqFQorfF2HxD3oBzWM3oCh4SaCRKWt1yvmgaPYGCo71J",
          "timestamp": "2024-04-18T06:44:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d591b16f6b1dec88003323cdae0c3abe3b5c9cbe"
        },
        "date": 1713427277823,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.862787047866666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.209936026,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "b6fab8046e42283d14e9fa6beda32c878b3e801e",
          "message": "[ci] Run `test-linux-stable-int` on self-hosted GitHub Runners (#4178)\n\nPR adds `test-linux-stable-int` and `quick-benchmarks` as github action\njobs. It's a copy of `test-linux-stable-int` and `quick-benchmarks` from\ngitlab ci and now it's needed to make a stress test for self-hosted\ngithub runners. `test-linux-stable-int` and `quick-benchmarks` in gitlab\nare still `Required` whereas this workflow is allowed to fail.\n\ncc https://github.com/paritytech/infrastructure/issues/46",
          "timestamp": "2024-04-18T07:40:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6fab8046e42283d14e9fa6beda32c878b3e801e"
        },
        "date": 1713430575242,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2393249596666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.973515769666662,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "76719da221d33117aadf6b7b9cc74e4fbeb25b34",
          "message": "[ci] Update ci image with rust 1.77 and 2024-04-10 (#4077)\n\ncc https://github.com/paritytech/ci_cd/issues/974\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-04-18T09:24:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/76719da221d33117aadf6b7b9cc74e4fbeb25b34"
        },
        "date": 1713437193539,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17212971646666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.612123962400002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ff906127ab513bb42a4288968e0f421f630809e0",
          "message": "Improve changelog in the release notes (#4179)\n\nThis PR adds description to each of the sections of the Changelog part.\nChanges are based on feedback that it wasn't that clear what exactly\n`Node Dev`, `Runtime Dev` etc. means. Now, the description for each of\nthose parts is taken directly from the `prdoc` schema.\nCloses https://github.com/paritytech/release-engineering/issues/197",
          "timestamp": "2024-04-18T10:30:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff906127ab513bb42a4288968e0f421f630809e0"
        },
        "date": 1713440803450,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2251924087333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.813954746333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "ordian",
            "username": "ordian",
            "email": "write@reusable.software"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "91d4a207af43f8f81f56e4f24af74f7c6f590148",
          "message": "chain-selection: allow reverting current block (#4103)\n\nBlock reversion of the current block is technically possible as can be\nseen from\n\nhttps://github.com/paritytech/polkadot-sdk/blob/39b1f50f1c251def87c1625d68567ed252dc6272/polkadot/runtime/parachains/src/disputes.rs#L1215-L1223\n\n- [x] Fix the test",
          "timestamp": "2024-04-18T14:32:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/91d4a207af43f8f81f56e4f24af74f7c6f590148"
        },
        "date": 1713456007995,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.838443908999997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21839375486666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c891fdabf4d519b25829490723fb70b1a2ffc0e5",
          "message": "tx: Remove tx_broadcast transaction from the pool (#4050)\n\nThis PR ensures that broadcast future cleans-up the submitted extrinsic\nfrom the pool, iff the `broadcast_stop` operation has been called.\n\nThis effectively cleans-up transactions from the pool when the\n`broadcast_stop` is called.\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-18T15:57:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c891fdabf4d519b25829490723fb70b1a2ffc0e5"
        },
        "date": 1713460502426,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.658906021399998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1811935364666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "88a2f360238787bf5256cfdd14b40c08f519b38e",
          "message": "chainHead: Stabilize chainHead to version 1 (#4168)\n\nThis PR stabilizes the chainHead API to version 1.\n\nNeeds:\n- https://github.com/paritytech/polkadot-sdk/pull/3667\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-18T17:19:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/88a2f360238787bf5256cfdd14b40c08f519b38e"
        },
        "date": 1713465383424,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.792774056533336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.22745959133333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "98a364fe6e7abf10819f5fddd3de0588f7c38700",
          "message": "rpc-v2: Limit transactionBroadcast calls to 16 (#3772)\n\nThis PR limits the number of active calls to the transactionBroadcast\nAPIs to 16.\n\ncc @paritytech/subxt-team \n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/3081\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: James Wilson <james@jsdw.me>",
          "timestamp": "2024-04-19T04:34:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98a364fe6e7abf10819f5fddd3de0588f7c38700"
        },
        "date": 1713505826138,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666667,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21505232139999997,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.900978519466667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "21308d893ef0594538aee73cbdc3905189be0b7b",
          "message": "Fixed GrandpaConsensusLogReader::find_scheduled_change (#4208)",
          "timestamp": "2024-04-19T08:34:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/21308d893ef0594538aee73cbdc3905189be0b7b"
        },
        "date": 1713520288180,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.24561350383333327,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.935558009466666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eba3deca3e61855c237a33013e8a5e82c479e958",
          "message": "txWatch: Stabilize txWatch to version 1 (#4171)\n\nThis PR stabilizes the txBroadcast API to version 1.\n\nNeeds from spec:\n- https://github.com/paritytech/json-rpc-interface-spec/pull/153 \n- https://github.com/paritytech/json-rpc-interface-spec/pull/154\n\n\ncc @paritytech/subxt-team\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-04-19T09:48:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eba3deca3e61855c237a33013e8a5e82c479e958"
        },
        "date": 1713525732011,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19378807793333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.874484071700001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "maksimryndin",
            "username": "maksimryndin",
            "email": "maksim.ryndin@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4eabe5e0dddc4cd31ad9dab5645350360d4d36a5",
          "message": "Pvf refactor execute worker errors follow up (#4071)\n\nfollow up of https://github.com/paritytech/polkadot-sdk/pull/2604\ncloses https://github.com/paritytech/polkadot-sdk/pull/2604\n\n- [x] take relevant changes from Marcin's PR \n- [x] extract common duplicate code for workers (low-hanging fruits)\n\n~Some unpassed ci problems are more general and should be fixed in\nmaster (see https://github.com/paritytech/polkadot-sdk/pull/4074)~\n\nProposed labels: **T0-node**, **R0-silent**, **I4-refactor**\n\n-----\n\nkusama address: FZXVQLqLbFV2otNXs6BMnNch54CFJ1idpWwjMb3Z8fTLQC6\n\n---------\n\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>",
          "timestamp": "2024-04-19T13:36:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4eabe5e0dddc4cd31ad9dab5645350360d4d36a5"
        },
        "date": 1713538512932,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.23433761183333335,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.93635579663334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e504c41a5adbd5e6d9a7764c07f6dcf47b2dae77",
          "message": "Allow privileged virtual bond in Staking pallet (#3889)\n\nThis is the first PR in preparation for\nhttps://github.com/paritytech/polkadot-sdk/issues/454.\n\n## Follow ups:\n- https://github.com/paritytech/polkadot-sdk/pull/3904.\n- https://github.com/paritytech/polkadot-sdk/pull/3905.\n\nOverall changes are documented here (lot more visual 😍):\nhttps://hackmd.io/@ak0n/454-np-governance\n\n[Maybe followup](https://github.com/paritytech/polkadot-sdk/issues/4217)\nwith migration of storage item `VirtualStakers` as a bool or enum in\n`Ledger`.\n\n## Context\nWe want to achieve a way for a user (`Delegator`) to delegate their\nfunds to another account (`Agent`). Delegate implies the funds are\nlocked in delegator account itself. Agent can act on behalf of delegator\nto stake directly on Staking pallet.\n\nThe delegation feature is added to Staking via another pallet\n`delegated-staking` worked on\n[here](https://github.com/paritytech/polkadot-sdk/pull/3904).\n\n## Introduces:\n### StakingUnchecked Trait\nAs the name implies, this trait allows unchecked (non-locked) mutation\nof staking ledger. These apis are only meant to be used by other pallets\nin the runtime and should not be exposed directly to user code path.\nAlso related: https://github.com/paritytech/polkadot-sdk/issues/3888.\n\n### Virtual Bond\nAllows other pallets to stake via staking pallet while managing the\nlocks on these accounts themselves. Introduces another storage\n`VirtualStakers` that whitelist these accounts.\n\nWe also restrict virtual stakers to set reward account as themselves.\nSince the account has no locks, we cannot support compounding of\nrewards. Conservatively, we require them to set a separate account\ndifferent from the staker. Since these are code managed, it should be\neasy for another pallet to redistribute reward and rebond them.\n\n### Slashes\nSince there is no actual lock maintained by staking-pallet for virtual\nstakers, this pallet does not apply any slashes. It is then important\nfor pallets managing virtual stakers to listen to slashing events and\napply necessary slashes.",
          "timestamp": "2024-04-20T00:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e504c41a5adbd5e6d9a7764c07f6dcf47b2dae77"
        },
        "date": 1713575973713,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.8785590512,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2277690894666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gui",
            "username": "thiolliere",
            "email": "gui.thiolliere@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f3c3ebb6a99295816ac4ee0a26364d736094c147",
          "message": "Fix case in type in macro generation (#4223)\n\nGenerated type is not camel case this generate some warnings from IDE\n\nlabel should be R0",
          "timestamp": "2024-04-20T08:20:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f3c3ebb6a99295816ac4ee0a26364d736094c147"
        },
        "date": 1713605487336,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2294512655333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.983299136400001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "253778c94dd64e6bc174ed1e03ac7e0b43990129",
          "message": "ci: disallow westend migration failure (#4205)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-04-22T05:08:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/253778c94dd64e6bc174ed1e03ac7e0b43990129"
        },
        "date": 1713766921480,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.509376390400003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15905229056666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "921265ca7889b9c9bc615af0eced9c6918c8af9f",
          "message": "Added prdoc for 4208 (#4239)",
          "timestamp": "2024-04-22T12:06:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/921265ca7889b9c9bc615af0eced9c6918c8af9f"
        },
        "date": 1713790140148,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.643938811666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15469902343333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a2a049db2bd669a88f6ab410b22b780ebcc8baee",
          "message": "[subsystem-benchmark] Add approval-voting benchmark to CI (#4216)\n\nCo-authored-by: alvicsam <alvicsam@gmail.com>",
          "timestamp": "2024-04-22T12:45:54Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a2a049db2bd669a88f6ab410b22b780ebcc8baee"
        },
        "date": 1713795496484,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.579602472299998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15036025349999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3380e21cd92690c2066f686164a954ba7cd17244",
          "message": "Use default branch of `psvm` when synchronizing templates (#4240)\n\nWe cannot lock to a specific version of `psvm`, because we will need to\nkeep it up-to-date - each release currently requires a change in `psvm`\nsuch as [this one](https://github.com/paritytech/psvm/pull/2/files).\n\nThere is no `stable` branch in `psvm` repo or anything so using the\ndefault branch.",
          "timestamp": "2024-04-22T16:34:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3380e21cd92690c2066f686164a954ba7cd17244"
        },
        "date": 1713808350092,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18804110043333339,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.690300285366668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "bd9287f766bded2022036a63d12fb86a2f7174a0",
          "message": "wasm-builder: Make it easier to build a WASM binary (#4177)\n\nBasically combines all the recommended calls into one\n`build_using_defaults()` call or `init_with_defaults()` when there are\nsome custom changes required.",
          "timestamp": "2024-04-22T19:28:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/bd9287f766bded2022036a63d12fb86a2f7174a0"
        },
        "date": 1713820375540,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.758253258766661,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18211845053333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "84c294c3821baf8b81693ce6e5615b9e157b5303",
          "message": "[testnets] remove XCM SafeCallFilter for chains using Weights::v3 (#4199)\n\nWeights::v3 also accounts for PoV weight so we no longer need the\nSafeCallFilter. All calls are allowed as long as they \"fit in the\nblock\".",
          "timestamp": "2024-04-22T22:10:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/84c294c3821baf8b81693ce6e5615b9e157b5303"
        },
        "date": 1713829444099,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20918905059999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.801833620633332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ac4f421f0b99b73bbf80710206e9ac1463e8cb0b",
          "message": "parachains_coretime: Expose `MaxXCMTransactWeight` (#4189)\n\nThis should be configured on the runtime level and not somewhere inside\nthe pallet.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-04-23T09:51:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ac4f421f0b99b73bbf80710206e9ac1463e8cb0b"
        },
        "date": 1713867797004,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21886011716666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.945297898066666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "118cd6f922acc9c4b3938645cd34098275d41c93",
          "message": "Ensure outbound XCMs are decodable with limits + add `EnsureDecodableXcm` router (for testing purposes) (#4186)\n\nThis PR:\n- adds `EnsureDecodableXcm` (testing) router that attempts to *encode*\nand *decode* passed XCM `message` to ensure that the receiving side will\nbe able to decode, at least with the same XCM version.\n- fixes `pallet_xcm` / `pallet_xcm_benchmarks` assets data generation\n\nRelates to investigation of\nhttps://substrate.stackexchange.com/questions/11288 and missing fix\nhttps://github.com/paritytech/polkadot-sdk/pull/2129 which did not get\ninto the fellows 1.1.X release.\n\n## Questions/TODOs\n\n- [x] fix XCM benchmarks, which produces undecodable data - new router\ncatched at least two cases\n  - `BoundedVec exceeds its limit`\n  - `Fungible asset of zero amount is not allowed`  \n- [x] do we need to add `sort` to the `prepend_with` as we did for\nreanchor [here](https://github.com/paritytech/polkadot-sdk/pull/2129)?\n@serban300 (**created separate/follow-up PR**:\nhttps://github.com/paritytech/polkadot-sdk/pull/4235)\n- [x] We added decoding check to `XcmpQueue` -> `validate_xcm_nesting`,\nwhy not to added to the `ParentAsUmp` or `ChildParachainRouter`?\n@franciscoaguirre (**created separate/follow-up PR**:\nhttps://github.com/paritytech/polkadot-sdk/pull/4236)\n- [ ] `SendController::send_blob` replace `VersionedXcm::<()>::decode(`\nwith `VersionedXcm::<()>::decode_with_depth_limit(MAX_XCM_DECODE_DEPTH,\ndata)` ?\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-23T11:40:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/118cd6f922acc9c4b3938645cd34098275d41c93"
        },
        "date": 1713878183652,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1819753658333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.710489965933334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "joe petrowski",
            "username": "joepetrowski",
            "email": "25483142+joepetrowski@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eda5e5c31f9bffafd6afd6d14fb95001a10dba9a",
          "message": "Fix Stuck Collator Funds (#4229)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4206\n\nIn #1340 one of the storage types was changed from `Candidates` to\n`CandidateList`. Since the actual key includes the hash of this value,\nall of the candidates stored here are (a) \"missing\" and (b) unable to\nunreserve their candidacy bond.\n\nThis migration kills the storage values and refunds the deposit held for\neach candidate.\n\n---------\n\nSigned-off-by: georgepisaltu <george.pisaltu@parity.io>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: georgepisaltu <george.pisaltu@parity.io>",
          "timestamp": "2024-04-23T12:53:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eda5e5c31f9bffafd6afd6d14fb95001a10dba9a"
        },
        "date": 1713882646861,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.673304727800002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17984596733333338,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ffbce2a817ec2e7c8b7ce49f7ed6794584f19667",
          "message": "pallet_broker: Let `start_sales` calculate and request the correct core count (#4221)",
          "timestamp": "2024-04-23T15:37:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667"
        },
        "date": 1713892640631,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2342792633,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.926268684599995,
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
          "id": "9a0049d0da59b8b842f64fae441b34dba3408430",
          "message": "Plumbing to increase pvf workers configuration based on chain id (#4252)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4126 we want\nto safely increase the execute_workers_max_num gradually from chain to\nchain and assess if there are any negative impacts.\n\nThis PR performs the necessary plumbing to be able to increase it based\non the chain id, it increase the number of execution workers from 2 to 4\non test network but lives kusama and polkadot unchanged until we gather\nmore data.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-04-24T06:15:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9a0049d0da59b8b842f64fae441b34dba3408430"
        },
        "date": 1713944949804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.632659835800002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16703196979999996,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Kalankhodzhaev",
            "username": "kalaninja",
            "email": "kalansoft@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c594b10a803e218f63c1bd97d2b27454efb4e852",
          "message": "Remove unnecessary cloning (#4263)\n\nSeems like Externalities already [return a\nvector](https://github.com/paritytech/polkadot-sdk/blob/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667/substrate/primitives/externalities/src/lib.rs#L86),\nso calling `to_vec` on a vector just results in an unneeded copying.\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-24T09:30:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c594b10a803e218f63c1bd97d2b27454efb4e852"
        },
        "date": 1713954394034,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.676387199266667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1842744405666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexander Kalankhodzhaev",
            "username": "kalaninja",
            "email": "kalansoft@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c594b10a803e218f63c1bd97d2b27454efb4e852",
          "message": "Remove unnecessary cloning (#4263)\n\nSeems like Externalities already [return a\nvector](https://github.com/paritytech/polkadot-sdk/blob/ffbce2a817ec2e7c8b7ce49f7ed6794584f19667/substrate/primitives/externalities/src/lib.rs#L86),\nso calling `to_vec` on a vector just results in an unneeded copying.\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-04-24T09:30:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c594b10a803e218f63c1bd97d2b27454efb4e852"
        },
        "date": 1713957311721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18553074919999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.708886988866666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4f3d43a0c4e75caf73c1034a85590f81a9ae3809",
          "message": "Revert `execute_blob` and `send_blob` (#4266)\n\nRevert \"pallet-xcm: Deprecate `execute` and `send` in favor of\n`execute_blob` and `send_blob` (#3749)\"\n\nThis reverts commit feee773d15d5237765b520b03854d46652181de5.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Javier Bullrich <javier@bullrich.dev>",
          "timestamp": "2024-04-24T15:49:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4f3d43a0c4e75caf73c1034a85590f81a9ae3809"
        },
        "date": 1713976220270,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.15983137919999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.641608834100003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a633e954f3b88697aa797d9792e8a5b5cf310b7e",
          "message": "Bridge: make some headers submissions free (#4102)\n\nsupersedes https://github.com/paritytech/parity-bridges-common/pull/2873\n\nDraft because of couple of TODOs:\n- [x] fix remaining TODOs;\n- [x] double check that all changes from\nhttps://github.com/paritytech/parity-bridges-common/pull/2873 are\ncorrectly ported;\n- [x] create a separate PR (on top of that one or a follow up?) for\nhttps://github.com/paritytech/polkadot-sdk/tree/sv-try-new-bridge-fees;\n- [x] fix compilation issues (haven't checked, but there should be\nmany).\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-25T05:26:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a633e954f3b88697aa797d9792e8a5b5cf310b7e"
        },
        "date": 1714028828334,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17091496756666674,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.630267654366667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7e68b2b8da9caf634ff4f6c6d96d2d7914c44fb7",
          "message": "Bridge: added free headers submission support to the substrate-relay (#4157)\n\nOriginal PR:\nhttps://github.com/paritytech/parity-bridges-common/pull/2884. Since\nchain-specific code lives in the `parity-bridges-common` repo, some\nparts of original PR will require another PR\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-04-25T07:20:17Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7e68b2b8da9caf634ff4f6c6d96d2d7914c44fb7"
        },
        "date": 1714035479704,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.631353990166668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16840501126666668,
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
          "id": "c26cf3f6f2d2b7f7783703308ece440c338459f8",
          "message": "Do not re-prepare PVFs if not needed (#4211)\n\nCurrently, PVFs are re-prepared if any execution environment parameter\nchanges. As we've recently seen on Kusama and Polkadot, that may lead to\na severe finality lag because every validator has to re-prepare every\nPVF. That cannot be avoided altogether; however, we could cease\nre-preparing PVFs when a change in the execution environment can't lead\nto a change in the artifact itself. For example, it's clear that\nchanging the execution timeout cannot affect the artifact.\n\nIn this PR, I'm introducing a separate hash for the subset of execution\nenvironment parameters that changes only if a preparation-related\nparameter changes. It introduces some minor code duplication, but\nwithout that, the scope of changes would be much bigger.\n\nTODO:\n- [x] Add a test to ensure the artifact is not re-prepared if\nnon-preparation-related parameter is changed\n- [x] Add a test to ensure the artifact is re-prepared if a\npreparation-related parameter is changed\n- [x] Add comments, warnings, and, possibly, a test to ensure a new\nparameter ever added to the executor environment parameters will be\nevaluated by the author of changes with respect to its artifact\npreparation impact and added to the new hash preimage if needed.\n\nCloses #4132",
          "timestamp": "2024-04-25T10:16:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c26cf3f6f2d2b7f7783703308ece440c338459f8"
        },
        "date": 1714042236656,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.773990656366669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2088572432,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ff2b178206f9952c3337638659450c67fd700e7e",
          "message": "remote-externalities: retry get child keys query (#4280)",
          "timestamp": "2024-04-25T12:01:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ff2b178206f9952c3337638659450c67fd700e7e"
        },
        "date": 1714052621602,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.62672678896667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1571307034333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dd5b06e622c6c5c301a1554286ec1f4995c7daca",
          "message": "[subsystem-benchmarks] Log standart deviation for subsystem-benchmarks (#4285)\n\nShould help us to understand more what's happening between individual\nruns and possibly adjust the number of runs",
          "timestamp": "2024-04-25T15:06:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd5b06e622c6c5c301a1554286ec1f4995c7daca"
        },
        "date": 1714059240610,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1496908670666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.519128675533334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dd5b06e622c6c5c301a1554286ec1f4995c7daca",
          "message": "[subsystem-benchmarks] Log standart deviation for subsystem-benchmarks (#4285)\n\nShould help us to understand more what's happening between individual\nruns and possibly adjust the number of runs",
          "timestamp": "2024-04-25T15:06:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dd5b06e622c6c5c301a1554286ec1f4995c7daca"
        },
        "date": 1714063431790,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20012158373333336,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.008727292366666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Noah Jelich",
            "username": "njelich",
            "email": "12912633+njelich@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8f8c49deffe56567ba5cde0e1047de15b660bb0e",
          "message": "Fix bad links (#4231)\n\nThe solochain template links to parachain template instead of solochain.",
          "timestamp": "2024-04-26T07:03:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8f8c49deffe56567ba5cde0e1047de15b660bb0e"
        },
        "date": 1714119409310,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.23025869936666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.280784673666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c66d8a84687f5d68c0192122aa513b4b340794ca",
          "message": "Bump bridges relay version + uncomment bridges zombeinet tests (#4289)\n\nTODOs:\n- [x] wait and see if test `1` works;\n- [x] ~think of whether we need remaining tests.~ I think we should keep\nit - will try to revive and update it",
          "timestamp": "2024-04-26T09:24:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c66d8a84687f5d68c0192122aa513b4b340794ca"
        },
        "date": 1714125225170,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21278176526666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.1185213226,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c66d8a84687f5d68c0192122aa513b4b340794ca",
          "message": "Bump bridges relay version + uncomment bridges zombeinet tests (#4289)\n\nTODOs:\n- [x] wait and see if test `1` works;\n- [x] ~think of whether we need remaining tests.~ I think we should keep\nit - will try to revive and update it",
          "timestamp": "2024-04-26T09:24:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c66d8a84687f5d68c0192122aa513b4b340794ca"
        },
        "date": 1714129312132,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22973867730000003,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.096298073966665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "988e30f102b155ab68d664d62ac5c73da171659a",
          "message": "Implementation of the new validator disabling strategy (#2226)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/1966,\nhttps://github.com/paritytech/polkadot-sdk/issues/1963 and\nhttps://github.com/paritytech/polkadot-sdk/issues/1962.\n\nDisabling strategy specification\n[here](https://github.com/paritytech/polkadot-sdk/pull/2955). (Updated\n13/02/2024)\n\nImplements:\n* validator disabling for a whole era instead of just a session\n* no more than 1/3 of the validators in the active set are disabled\nRemoves:\n* `DisableStrategy` enum - now each validator committing an offence is\ndisabled.\n* New era is not forced if too many validators are disabled.\n\nBefore this PR not all offenders were disabled. A decision was made\nbased on [`enum\nDisableStrategy`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/primitives/staking/src/offence.rs#L54).\nSome offenders were disabled for a whole era, some just for a session,\nsome were not disabled at all.\n\nThis PR changes the disabling behaviour. Now a validator committing an\noffense is disabled immediately till the end of the current era.\n\nSome implementation notes:\n* `OffendingValidators` in pallet session keeps all offenders (this is\nnot changed). However its type is changed from `Vec<(u32, bool)>` to\n`Vec<u32>`. The reason is simple - each offender is getting disabled so\nthe bool doesn't make sense anymore.\n* When a validator is disabled it is first added to\n`OffendingValidators` and then to `DisabledValidators`. This is done in\n[`add_offending_validator`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/slashing.rs#L325)\nfrom staking pallet.\n* In\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\nthe `end_session` also calls\n[`end_era`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L490)\nwhen an era ends. In this case `OffendingValidators` are cleared\n**(1)**.\n* Then in\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\n`DisabledValidators` are cleared **(2)**\n* And finally (still in `rotate_session`) a call to\n[`start_session`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L430)\nrepopulates the disabled validators **(3)**.\n* The reason for this complication is that session pallet knows nothing\nabut eras. To overcome this on each new session the disabled list is\nrepopulated (points 2 and 3). Staking pallet knows when a new era starts\nso with point 1 it ensures that the offenders list is cleared.\n\n---------\n\nCo-authored-by: ordian <noreply@reusable.software>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>",
          "timestamp": "2024-04-26T13:28:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/988e30f102b155ab68d664d62ac5c73da171659a"
        },
        "date": 1714140242949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.866819798666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17966568133333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "988e30f102b155ab68d664d62ac5c73da171659a",
          "message": "Implementation of the new validator disabling strategy (#2226)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/1966,\nhttps://github.com/paritytech/polkadot-sdk/issues/1963 and\nhttps://github.com/paritytech/polkadot-sdk/issues/1962.\n\nDisabling strategy specification\n[here](https://github.com/paritytech/polkadot-sdk/pull/2955). (Updated\n13/02/2024)\n\nImplements:\n* validator disabling for a whole era instead of just a session\n* no more than 1/3 of the validators in the active set are disabled\nRemoves:\n* `DisableStrategy` enum - now each validator committing an offence is\ndisabled.\n* New era is not forced if too many validators are disabled.\n\nBefore this PR not all offenders were disabled. A decision was made\nbased on [`enum\nDisableStrategy`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/primitives/staking/src/offence.rs#L54).\nSome offenders were disabled for a whole era, some just for a session,\nsome were not disabled at all.\n\nThis PR changes the disabling behaviour. Now a validator committing an\noffense is disabled immediately till the end of the current era.\n\nSome implementation notes:\n* `OffendingValidators` in pallet session keeps all offenders (this is\nnot changed). However its type is changed from `Vec<(u32, bool)>` to\n`Vec<u32>`. The reason is simple - each offender is getting disabled so\nthe bool doesn't make sense anymore.\n* When a validator is disabled it is first added to\n`OffendingValidators` and then to `DisabledValidators`. This is done in\n[`add_offending_validator`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/slashing.rs#L325)\nfrom staking pallet.\n* In\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\nthe `end_session` also calls\n[`end_era`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L490)\nwhen an era ends. In this case `OffendingValidators` are cleared\n**(1)**.\n* Then in\n[`rotate_session`](https://github.com/paritytech/polkadot-sdk/blob/bdbe98297032e21a553bf191c530690b1d591405/substrate/frame/session/src/lib.rs#L623)\n`DisabledValidators` are cleared **(2)**\n* And finally (still in `rotate_session`) a call to\n[`start_session`](https://github.com/paritytech/polkadot-sdk/blob/bbb6631641f9adba30c0ee6f4d11023a424dd362/substrate/frame/staking/src/pallet/impls.rs#L430)\nrepopulates the disabled validators **(3)**.\n* The reason for this complication is that session pallet knows nothing\nabut eras. To overcome this on each new session the disabled list is\nrepopulated (points 2 and 3). Staking pallet knows when a new era starts\nso with point 1 it ensures that the offenders list is cleared.\n\n---------\n\nCo-authored-by: ordian <noreply@reusable.software>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: Ankan <10196091+Ank4n@users.noreply.github.com>",
          "timestamp": "2024-04-26T13:28:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/988e30f102b155ab68d664d62ac5c73da171659a"
        },
        "date": 1714144564602,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.8957000278,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17314057933333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "antiyro",
            "username": "antiyro",
            "email": "74653697+antiyro@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2a497d297575947b613fe0f3bbac9273a48fd6b0",
          "message": "fix(seal): shameless fix on sealing typo (#4304)",
          "timestamp": "2024-04-26T16:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2a497d297575947b613fe0f3bbac9273a48fd6b0"
        },
        "date": 1714152173863,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.8957000278,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17314057933333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "antiyro",
            "username": "antiyro",
            "email": "74653697+antiyro@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2a497d297575947b613fe0f3bbac9273a48fd6b0",
          "message": "fix(seal): shameless fix on sealing typo (#4304)",
          "timestamp": "2024-04-26T16:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2a497d297575947b613fe0f3bbac9273a48fd6b0"
        },
        "date": 1714154395354,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18249619086666663,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.994634203166665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "73b9a8391fa0b18308fa35f905e31cec77f5618f",
          "message": "[Staking] Runtime api if era rewards are pending to be claimed (#4301)\n\ncloses https://github.com/paritytech/polkadot-sdk/issues/426.\nrelated to https://github.com/paritytech/polkadot-sdk/pull/1189.\n\nWould help offchain programs to query if there are unclaimed pages of\nrewards for a given era.\n\nThe logic could look like below\n\n```js\n// loop as long as all era pages are claimed.\nwhile (api.call.stakingApi.pendingRewards(era, validator_stash)) {\n  api.tx.staking.payout_stakers(validator_stash, era)\n}\n```",
          "timestamp": "2024-04-28T12:35:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73b9a8391fa0b18308fa35f905e31cec77f5618f"
        },
        "date": 1714314065803,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.105759225233331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2083474417666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "92a348f57deed44789511df73d3fbbbcb58d98cb",
          "message": "Bump snow from 0.9.3 to 0.9.6 (#4061)\n\nBumps [snow](https://github.com/mcginty/snow) from 0.9.3 to 0.9.6.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/mcginty/snow/releases\">snow's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v0.9.6</h2>\n<ul>\n<li>Validate invalid PSK positions when building a Noise protocol.</li>\n<li>Raise errors in various typos/mistakes in Noise patterns when\nparsing.</li>\n<li>Deprecate the <code>sodiumoxide</code> backend, as that crate is no\nlonger maintained. We may eventually migrate it to a maintaned version\nof the crate, but for now it's best to warn users.</li>\n<li>Set a hard limit in <code>read_message()</code> in transport mode to\n65535 to be fully compliant with the Noise specification.</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6\">https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6</a></p>\n<h2>v0.9.5</h2>\n<p>This is a security release that fixes a logic flaw in decryption in\n<code>TransportState</code> (i.e. the stateful one), where the nonce\ncould increase even when decryption failed, which can cause a desync\nbetween the sender and receiver, opening this up as a denial of service\nvector if the attacker has the ability to inject packets in the channel\nNoise is talking over.</p>\n<p>More details can be found in the advisory: <a\nhref=\"https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3\">https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3</a></p>\n<p>All users are encouraged to update.</p>\n<h2>v0.9.4</h2>\n<p>This is a dependency version bump release because a couple of\nimportant dependencies released new versions that needed a\n<code>Cargo.toml</code> bump:</p>\n<ul>\n<li><code>ring</code> 0.17</li>\n<li><code>pqcrypto-kyber</code> 0.8</li>\n<li><code>aes-gcm</code> 0.10</li>\n<li><code>chacha20poly1305</code> 0.10</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/a4be73faa042c5967f39662aa66919f774831a9a\"><code>a4be73f</code></a>\nmeta: v0.9.6 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/9e53dcf5bbea869b5e3e9ed26866d683906bc848\"><code>9e53dcf</code></a>\nTransportState: limit read_message size to 65535</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/faf05609e19f4106cd47b78123415dfeb9330861\"><code>faf0560</code></a>\nDeprecate sodiumoxide resolver</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/308a24d23da13cb01a173f0ec23f140898801fb9\"><code>308a24d</code></a>\nAdd warnings about multiple calls to same method in Builder</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/f280991ae408685d72e098545314f2be160e57f9\"><code>f280991</code></a>\nError when extraneous parameters are included in string to parse</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/dbdcc4803aae6e5d9910163a7d52e0df8def4310\"><code>dbdcc48</code></a>\nError on duplicate modifiers in parameter string</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/8b1a819c93ceae98f9ba0a1be192fa61fdec78c2\"><code>8b1a819</code></a>\nValidate PSK index in pattern to avoid panic</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/74e30cf591d6d89c8a1670ee713ecc4e9607e38f\"><code>74e30cf</code></a>\nmeta: v0.9.5 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/12e8ae55547ae297d5f70599e5c884ea891303eb\"><code>12e8ae5</code></a>\nStateful nonce desync fix</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/02c26b7551cb7e221792a9d3d3a94730e6a34e8a\"><code>02c26b7</code></a>\nRemove clap from simple example</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.3...v0.9.6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=snow&package-manager=cargo&previous-version=0.9.3&new-version=0.9.6)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-04-28T16:36:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92a348f57deed44789511df73d3fbbbcb58d98cb"
        },
        "date": 1714324112831,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.015817710866665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20425477056666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "92a348f57deed44789511df73d3fbbbcb58d98cb",
          "message": "Bump snow from 0.9.3 to 0.9.6 (#4061)\n\nBumps [snow](https://github.com/mcginty/snow) from 0.9.3 to 0.9.6.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/mcginty/snow/releases\">snow's\nreleases</a>.</em></p>\n<blockquote>\n<h2>v0.9.6</h2>\n<ul>\n<li>Validate invalid PSK positions when building a Noise protocol.</li>\n<li>Raise errors in various typos/mistakes in Noise patterns when\nparsing.</li>\n<li>Deprecate the <code>sodiumoxide</code> backend, as that crate is no\nlonger maintained. We may eventually migrate it to a maintaned version\nof the crate, but for now it's best to warn users.</li>\n<li>Set a hard limit in <code>read_message()</code> in transport mode to\n65535 to be fully compliant with the Noise specification.</li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6\">https://github.com/mcginty/snow/compare/v0.9.5...v0.9.6</a></p>\n<h2>v0.9.5</h2>\n<p>This is a security release that fixes a logic flaw in decryption in\n<code>TransportState</code> (i.e. the stateful one), where the nonce\ncould increase even when decryption failed, which can cause a desync\nbetween the sender and receiver, opening this up as a denial of service\nvector if the attacker has the ability to inject packets in the channel\nNoise is talking over.</p>\n<p>More details can be found in the advisory: <a\nhref=\"https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3\">https://github.com/mcginty/snow/security/advisories/GHSA-7g9j-g5jg-3vv3</a></p>\n<p>All users are encouraged to update.</p>\n<h2>v0.9.4</h2>\n<p>This is a dependency version bump release because a couple of\nimportant dependencies released new versions that needed a\n<code>Cargo.toml</code> bump:</p>\n<ul>\n<li><code>ring</code> 0.17</li>\n<li><code>pqcrypto-kyber</code> 0.8</li>\n<li><code>aes-gcm</code> 0.10</li>\n<li><code>chacha20poly1305</code> 0.10</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/a4be73faa042c5967f39662aa66919f774831a9a\"><code>a4be73f</code></a>\nmeta: v0.9.6 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/9e53dcf5bbea869b5e3e9ed26866d683906bc848\"><code>9e53dcf</code></a>\nTransportState: limit read_message size to 65535</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/faf05609e19f4106cd47b78123415dfeb9330861\"><code>faf0560</code></a>\nDeprecate sodiumoxide resolver</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/308a24d23da13cb01a173f0ec23f140898801fb9\"><code>308a24d</code></a>\nAdd warnings about multiple calls to same method in Builder</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/f280991ae408685d72e098545314f2be160e57f9\"><code>f280991</code></a>\nError when extraneous parameters are included in string to parse</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/dbdcc4803aae6e5d9910163a7d52e0df8def4310\"><code>dbdcc48</code></a>\nError on duplicate modifiers in parameter string</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/8b1a819c93ceae98f9ba0a1be192fa61fdec78c2\"><code>8b1a819</code></a>\nValidate PSK index in pattern to avoid panic</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/74e30cf591d6d89c8a1670ee713ecc4e9607e38f\"><code>74e30cf</code></a>\nmeta: v0.9.5 release</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/12e8ae55547ae297d5f70599e5c884ea891303eb\"><code>12e8ae5</code></a>\nStateful nonce desync fix</li>\n<li><a\nhref=\"https://github.com/mcginty/snow/commit/02c26b7551cb7e221792a9d3d3a94730e6a34e8a\"><code>02c26b7</code></a>\nRemove clap from simple example</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/mcginty/snow/compare/v0.9.3...v0.9.6\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=snow&package-manager=cargo&previous-version=0.9.3&new-version=0.9.6)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-04-28T16:36:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/92a348f57deed44789511df73d3fbbbcb58d98cb"
        },
        "date": 1714327960289,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.5965764349,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1632646501,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tin Chung",
            "username": "chungquantin",
            "email": "56880684+chungquantin@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f34d8e3cf033e2a22a41b505c437972a5dc83d78",
          "message": "Remove hard-coded indices from pallet-xcm tests (#4248)\n\n# ISSUE\n- Link to issue: https://github.com/paritytech/polkadot-sdk/issues/4237\n\n# DESCRIPTION\nRemove all ModuleError with hard-coded indices to pallet Error. For\nexample:\n```rs\nErr(DispatchError::Module(ModuleError {\n\tindex: 4,\n\terror: [2, 0, 0, 0],\n\tmessage: Some(\"Filtered\")\n}))\n```\nTo \n```rs\nlet expected_result = Err(crate::Error::<Test>::Filtered.into());\nassert_eq!(result, expected_result);\n```\n# TEST OUTCOME\n```\ntest result: ok. 74 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 0.02s\n```\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-04-29T07:13:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f34d8e3cf033e2a22a41b505c437972a5dc83d78"
        },
        "date": 1714381361879,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1561578603,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.628943989999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0031d49d1ec083c62a4e2b5bf594b7f45f84ab0d",
          "message": "[Staking] Not allow reap stash for virtual stakers (#4311)\n\nRelated to https://github.com/paritytech/polkadot-sdk/pull/3905.\n\nSince virtual stakers does not have a real balance, they should not be\nallowed to be reaped.\n\nThe proper reaping for agents slashed will be done in a separate PR.",
          "timestamp": "2024-04-29T15:55:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0031d49d1ec083c62a4e2b5bf594b7f45f84ab0d"
        },
        "date": 1714412090268,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.569638401066666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16539890046666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Shawn Tabrizi",
            "username": "shawntabrizi",
            "email": "shawntabrizi@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4875ea11aeef4f3fc7d724940e5ffb703830619b",
          "message": "Refactor XCM Simulator Example (#4220)\n\nThis PR does a \"developer experience\" refactor of the XCM Simulator\nExample.\n\nI was looking for existing code / documentation where developers could\nbetter learn about working with and configuring XCM.\n\nThe XCM Simulator was a natural starting point due to the fact that it\ncan emulate end to end XCM scenarios, without needing to spawn multiple\nreal chains.\n\nHowever, the XCM Simulator Example was just 3 giant files with a ton of\nconfigurations, runtime, pallets, and tests mashed together.\n\nThis PR breaks down the XCM Simulator Example in a way that I believe is\nmore approachable by a new developer who is looking to navigate the\nvarious components of the end to end example, and modify it themselves.\n\nThe basic structure is:\n\n- xcm simulator example\n    - lib (tries to only use the xcm simulator macros)\n    - tests\n    - relay-chain\n        - mod (basic runtime that developers should be familiar with)\n        - xcm-config\n            - mod (contains the `XcmConfig` type\n            - various files for each custom configuration  \n    - parachain\n        - mock_msg_queue (custom pallet for simulator example)\n        - mod (basic runtime that developers should be familiar with)\n        - xcm-config\n            - mod (contains the `XcmConfig` type\n            - various files for each custom configuration\n\nI would like to add more documentation to this too, but I think this is\na first step to be accepted which will affect how documentation is added\nto the example\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-29T21:22:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4875ea11aeef4f3fc7d724940e5ffb703830619b"
        },
        "date": 1714431683039,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.684166024833333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1754596751666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "31dc8bb1de9a73c57863c4698ea23559ef729f67",
          "message": "Improvements in minimal template (#4119)\n\nThis PR makes a few improvements in the docs for the minimal template.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-04-30T05:39:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/31dc8bb1de9a73c57863c4698ea23559ef729f67"
        },
        "date": 1714459444429,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.913967429966666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2054592840666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c973fe86f8c668462186c95655a58fda04508e9a",
          "message": "Contracts: revert reverted changes from 4266 (#4277)\n\nrevert some reverted changes from #4266",
          "timestamp": "2024-04-30T14:29:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c973fe86f8c668462186c95655a58fda04508e9a"
        },
        "date": 1714493139679,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.956190886266668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21751357750000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6d392c7eea496e0874a9ea37f4a8ea447ebc330e",
          "message": "Statement Distribution Per Peer Rate Limit (#3444)\n\n- [x] Drop requests from a PeerID that is already being served by us.\n- [x] Don't sent requests to a PeerID if we already are requesting\nsomething from them at that moment (prioritise other requests or wait).\n- [x] Tests\n- [ ] ~~Add a small rep update for unsolicited requests (same peer\nrequest)~~ not included in original PR due to potential issues with\nnodes slowly updating\n- [x] Add a metric to track the amount of dropped requests due to peer\nrate limiting\n- [x] Add a metric to track how many time a node reaches the max\nparallel requests limit in v2+\n\nHelps with but does not close yet:\nhttps://github.com/paritytech-secops/srlabs_findings/issues/303",
          "timestamp": "2024-05-01T17:17:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6d392c7eea496e0874a9ea37f4a8ea447ebc330e"
        },
        "date": 1714589730485,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.003413914033334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.22859903413333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e5a93fbcd4a6acec7ab83865708e5c5df3534a7b",
          "message": "HRMP - set `DefaultChannelSizeAndCapacityWithSystem` with dynamic values according to the `ActiveConfig` (#4332)\n\n## Summary\nThis PR enhances the capability to set\n`DefaultChannelSizeAndCapacityWithSystem` for HRMP. Currently, all\ntestnets (Rococo, Westend) have a hard-coded value set as 'half of the\nmaximum' determined by the live `ActiveConfig`. While this approach\nappears satisfactory, potential issues could arise if the live\n`ActiveConfig` are adjusted below these hard-coded values, necessitating\na new runtime release with updated values. Additionally, hard-coded\nvalues have consequences, such as Rococo's benchmarks not functioning:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6082656.\n\n## Solution\nThe proposed solution here is to utilize\n`ActiveConfigHrmpChannelSizeAndCapacityRatio`, which reads the current\n`ActiveConfig` and calculates `DefaultChannelSizeAndCapacityWithSystem`,\nfor example, \"half of the maximum\" based on live data. This way,\nwhenever `ActiveConfig` is modified,\n`ActiveConfigHrmpChannelSizeAndCapacityRatio` automatically returns\nadjusted values with the appropriate ratio. Thus, manual adjustments and\nnew runtime releases become unnecessary.\n\n\nRelates to a comment/discussion:\nhttps://github.com/paritytech/polkadot-sdk/pull/3721/files#r1541001420\nRelates to a comment/discussion:\nhttps://github.com/paritytech/polkadot-sdk/pull/3721/files#r1549291588\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-01T20:01:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5a93fbcd4a6acec7ab83865708e5c5df3534a7b"
        },
        "date": 1714599469694,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22762047733333332,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.877189181299997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "14c4afc5382dcb18095af5c090e2aa1af65ecae6",
          "message": "[Backport] Version bumps and reorg prdocs from 1.11.0 (#4336)\n\nThis PR backports version bumps and reorganization of the `prdocs` from\n`1.11.0` release branch back to master",
          "timestamp": "2024-05-02T07:21:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/14c4afc5382dcb18095af5c090e2aa1af65ecae6"
        },
        "date": 1714640474600,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.666004171333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1559735497333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "171bedc2b319e18d51a7b510d8bd4cfd2e645c31",
          "message": "Bridge: ignore client errors when calling recently added `*_free_headers_interval` methods (#4350)\n\nsee https://github.com/paritytech/parity-bridges-common/issues/2974 : we\nstill need to support unupgraded chains (BHK and BHP) in relay\n\nWe may need to revert this change when all chains are upgraded",
          "timestamp": "2024-05-02T10:02:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/171bedc2b319e18d51a7b510d8bd4cfd2e645c31"
        },
        "date": 1714647958277,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.23968256163333335,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.996888690100002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "877617c44629e84ee36ac7194f4fe00fe3fa0b71",
          "message": "cargo: Update experimental litep2p to latest version (#4344)\n\nThis PR updates the litep2p crate to the latest version.\n\nThis fixes the build for developers that want to perform `cargo update`\non all their dependencies:\nhttps://github.com/paritytech/polkadot-sdk/pull/4343, by porting the\nlatest changes.\n\nThe peer records were introduced to litep2p to be able to distinguish\nand update peers with outdated records.\nIt is going to be properly used in substrate via:\nhttps://github.com/paritytech/polkadot-sdk/pull/3786, however that is\npending the commit to merge on litep2p master:\nhttps://github.com/paritytech/litep2p/pull/96.\n\nCloses: https://github.com/paritytech/polkadot-sdk/pull/4343\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-05-02T10:54:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/877617c44629e84ee36ac7194f4fe00fe3fa0b71"
        },
        "date": 1714653676661,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1954006051333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.793335692333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "df84ea789f3fd0de20bd801e344ffa30172ffb55",
          "message": "sc-tracing: enable env-filter feature (#4357)\n\nThis crate uses this feature however it appears to still work without\nthis feature enabled. I believe this is due to feature unification of\nthe workspace. Some other crate enables this feature so it also ends up\nenabled here. But when this crate is pushed to crates.io and compiled\nindividualy it fails to compile.",
          "timestamp": "2024-05-02T13:34:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/df84ea789f3fd0de20bd801e344ffa30172ffb55"
        },
        "date": 1714658594285,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.15487407163333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.583811420333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6580101ef3d5c36e1d84a820136fb87f398b04a3",
          "message": "Deprecate `NativeElseWasmExecutor` (#4329)\n\nThe native executor is deprecated and downstream users should stop using\nit.",
          "timestamp": "2024-05-02T15:19:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6580101ef3d5c36e1d84a820136fb87f398b04a3"
        },
        "date": 1714664718814,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.949166972033334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21909873703333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6580101ef3d5c36e1d84a820136fb87f398b04a3",
          "message": "Deprecate `NativeElseWasmExecutor` (#4329)\n\nThe native executor is deprecated and downstream users should stop using\nit.",
          "timestamp": "2024-05-02T15:19:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6580101ef3d5c36e1d84a820136fb87f398b04a3"
        },
        "date": 1714669100342,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.845626544700002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2005854000666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a9aeabe923dae63ab76ab290951cb9183c51f59c",
          "message": "Allow for 0 existential deposit in benchmarks for `pallet_staking`, `pallet_session`, and `pallet_balances` (#4346)\n\nThis PR ensures non-zero values are available in benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` where required\nfor them to run.\n\nThis small change makes it possible to run the benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` in a runtime\nfor which existential deposit is set to 0.\n\nThe benchmarks for `pallet_staking` and `pallet_session` will still fail\nin runtimes that use `U128CurrencyToVote`, but that is easy to work\naround by creating a new `CurrencyToVote` implementation for\nbenchmarking.\n\nThe changes are implemented by checking if existential deposit equals 0\nand using 1 if so.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-02T20:16:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a9aeabe923dae63ab76ab290951cb9183c51f59c"
        },
        "date": 1714686635812,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.845626544700002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2005854000666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a9aeabe923dae63ab76ab290951cb9183c51f59c",
          "message": "Allow for 0 existential deposit in benchmarks for `pallet_staking`, `pallet_session`, and `pallet_balances` (#4346)\n\nThis PR ensures non-zero values are available in benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` where required\nfor them to run.\n\nThis small change makes it possible to run the benchmarks for\n`pallet_staking`, `pallet_session`, and `pallet_balances` in a runtime\nfor which existential deposit is set to 0.\n\nThe benchmarks for `pallet_staking` and `pallet_session` will still fail\nin runtimes that use `U128CurrencyToVote`, but that is easy to work\naround by creating a new `CurrencyToVote` implementation for\nbenchmarking.\n\nThe changes are implemented by checking if existential deposit equals 0\nand using 1 if so.\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-02T20:16:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a9aeabe923dae63ab76ab290951cb9183c51f59c"
        },
        "date": 1714688743120,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1961195154666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.825112093333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "871281783c1be03157319d5143096fd3dd860d0a",
          "message": "Bridge: fix zombienet tests (#4367)\n\nDue to recent bump of Rococo/Westend versions + the fact that\nhttps://github.com/paritytech/parity-bridges-common/pull/2894 has\nfinally reached this repo, tests now fail, because we've started\nchecking all client versions (even source) unless we specify\n`--source-version-mode Auto` in CLI arguments. This looks like an\noverkill, but all those version checks will be fixed by\nhttps://github.com/paritytech/polkadot-sdk/pull/4256, so now it makes\nsense just to add this CLI option. We also need to propagate it to\nrunning relays eventually.",
          "timestamp": "2024-05-03T11:49:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/871281783c1be03157319d5143096fd3dd860d0a"
        },
        "date": 1714738851980,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.637800826833331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1813060995,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kris Bitney",
            "username": "krisbitney",
            "email": "kris@dorg.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "519862334feec5c72709afbf595ed61ddfb2298a",
          "message": "Fix: dust unbonded for zero existential deposit (#4364)\n\nWhen a staker unbonds and withdraws, it is possible that their stash\nwill contain less currency than the existential deposit. If that\nhappens, their stash is reaped. But if the existential deposit is zero,\nthe reap is not triggered. This PR adjusts `pallet_staking` to reap a\nstash in the special case that the stash value is zero and the\nexistential deposit is zero.\n\nThis change is important for blockchains built on substrate that require\nan existential deposit of zero, becuase it conserves valued storage\nspace.\n\nThere are two places in which ledgers are checked to determine if their\nvalue is less than the existential deposit and they should be reaped: in\nthe methods `do_withdraw_unbonded` and `reap_stash`. When the check is\nmade, the condition `ledger_total == 0` is also checked. If\n`ledger_total` is zero, then it must be below any existential deposit\ngreater than zero and equal to an existential deposit of 0.\n\nI added a new test for each method to confirm the change behaves as\nexpected.\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/4340\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-03T12:31:45Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/519862334feec5c72709afbf595ed61ddfb2298a"
        },
        "date": 1714745327055,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.9475899338,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20101351246666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "cheme",
            "username": "cheme",
            "email": "emericchevalier.pro@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4c09a0631334c3e021a30aa49a503815aa047b29",
          "message": "State trie migration on asset-hub westend and collectives westend (#4185)",
          "timestamp": "2024-05-03T14:04:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4c09a0631334c3e021a30aa49a503815aa047b29"
        },
        "date": 1714750888859,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.663602053833333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16807474140000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f45847b8b10db9cea664d0ce90083947c3843b10",
          "message": "Add validate field to prdoc (#4368)",
          "timestamp": "2024-05-03T15:07:35Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f45847b8b10db9cea664d0ce90083947c3843b10"
        },
        "date": 1714756928516,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.757756877066665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19150167073333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c0234becc185f88445dd63105b6f363c9e5990ce",
          "message": "Publish `polkadot-sdk-frame`  crate (#4370)",
          "timestamp": "2024-05-04T05:36:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c0234becc185f88445dd63105b6f363c9e5990ce"
        },
        "date": 1714806841458,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2351015968,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.088252197133334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "73c89d308fefcedfc3619f0273e13b6623766b81",
          "message": "Introduces `TypeWithDefault<T, D: Get<T>>` (#4034)\n\nNeeded for: https://github.com/polkadot-fellows/runtimes/issues/248\n\nThis PR introduces a new type `TypeWithDefault<T, D: Get<T>>` to be able\nto provide a custom default for any type. This can, then, be used to\nprovide the nonce type that returns the current block number as the\ndefault, to avoid replay of immortal transactions.\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-06T03:59:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/73c89d308fefcedfc3619f0273e13b6623766b81"
        },
        "date": 1714974510745,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.962487835033333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23447211169999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6e3059a873b78c7a4e2da18b4c298847713140ee",
          "message": "Upgrade a few deps (#4381)\n\nSplit from #4374\n\nThis PR helps to reduce dependencies and align versions, which would\nhelp to move them to workspace dep",
          "timestamp": "2024-05-06T11:49:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6e3059a873b78c7a4e2da18b4c298847713140ee"
        },
        "date": 1715002480997,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17891854629999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.9495353308,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e434176e0867d17336301388b46a6796b366a976",
          "message": "Improve Create release draft workflow + templates for the free notes and docker images sections in the notes (#4371)\n\nThis PR has the following changes:\n\n- New templates for the free notes and docker images sections in the\nrelease notes. There is going to be a section for the manual additions\nto the release notes + a section with the links to the docker images for\n`polkadot` and `polkadot-parachain` binaries at the end of the release\ndraft.\n- Fix for matrix section in the Create release draft flow (adds the\nrelease environment variable)\n- Reduction of the message which is posted to the announcement chats, as\nthe current one with the full release notes text is too big.",
          "timestamp": "2024-05-06T14:39:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e434176e0867d17336301388b46a6796b366a976"
        },
        "date": 1715012330633,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.0752850534,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20524586876666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b709dccd063507d468db3e10f491bb60cd80ac64",
          "message": "Add support for versioned notification for HRMP pallet (#4281)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4003 (please\nsee for the problem description)\n\n## TODO\n- [x] add more tests covering `WrapVersion` corner cases (e.g. para has\nlower version, ...)\n- [x] regenerate benchmarks `runtime_parachains::hrmp` (fix for Rococo\nis here: https://github.com/paritytech/polkadot-sdk/pull/4332)\n\n## Questions / possible improvements\n- [ ] A `WrapVersion` implementation for `pallet_xcm` initiates version\ndiscovery with\n[note_unknown_version](https://github.com/paritytech/polkadot-sdk/blob/master/polkadot/xcm/pallet-xcm/src/lib.rs#L2527C5-L2527C25),\nthere is possibility to avoid this overhead in this HRMP case to create\nnew `WrapVersion` adapter for `pallet_xcm` which would not use\n`note_unknown_version`. Is it worth to do it or not?\n- [ ] There's a possibility to decouple XCM functionality from the HRMP\npallet, allowing any relay chain to generate its own notifications. This\napproach wouldn't restrict notifications solely to the XCM. However,\nit's uncertain whether it's worthwhile or desirable to do so? It means\nmaking HRMP pallet more generic. E.g. hiding HRMP notifications behind\nsome trait:\n\t```\n\ttrait HrmpNotifications {\n\n\t\tfn on_channel_open_request(\n\t\t\tsender: ParaId,\n\t\t\tproposed_max_capacity: u32,\n\t\t\tproposed_max_message_size: u32) -> primitives::DownwardMessage;\n\nfn on_channel_accepted(recipient: ParaId) ->\nprimitives::DownwardMessage;\n\nfn on_channel_closing(initiator: ParaId, sender: ParaId, recipient:\nParaId) -> primitives::DownwardMessage;\n\t}\n\t```\nand then we could have whatever adapter, `impl HrmpNotifications for\nVersionedXcmHrmpNotifications {...}`,\n\t```\n\timpl parachains_hrmp::Config for Runtime {\n\t..\n\t\ttype HrmpNotifications = VersionedXcmHrmpNotifications;\n\t..\n\t}\n\t```\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-07T08:33:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b709dccd063507d468db3e10f491bb60cd80ac64"
        },
        "date": 1715076849819,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.819818544233334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18689803710000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "jimdssd",
            "username": "jimdssd",
            "email": "wqq1479791@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "29c8130bab0ed8216f48e47a78c602e7f0c5c1f2",
          "message": "chore: fix typos (#4395)",
          "timestamp": "2024-05-07T10:23:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/29c8130bab0ed8216f48e47a78c602e7f0c5c1f2"
        },
        "date": 1715083465565,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16671571089999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.806041489433335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Evgeny Snitko",
            "username": "AndWeHaveAPlan",
            "email": "evgeny@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1c8595adb89b7b6ac443e9a1caf0b20a6e1231a5",
          "message": "Code coverage preparations (#4387)\n\nAdded manual jobs for code coverage (triggered via `codecov-start` job):\n - **codecov-start** - initialize Codecov report for commit/pr\n- **test-linux-stable-codecov** - perform `nextest run` and upload\ncoverage data parts\n- **codecov-finish** - finalize uploading of data parts and generate\nCodecov report\n\nCoverage requires code to be built with `-C instrument-coverage` which\ncauses build errors (e .g. ```error[E0275]: overflow evaluating the\nrequirement `<mock::Test as pallet::Config>::KeyOwnerProof == _\\` ```,\nseems like related to\n[2641](https://github.com/paritytech/polkadot-sdk/issues/2641)) and\nunstable tests behavior\n([example](https://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6004731)).\nThis is where we'll nee the developers assistance\n\nclosing [[polkadot-sdk] Add code coverage\n#902](https://github.com/paritytech/ci_cd/issues/902)",
          "timestamp": "2024-05-07T15:14:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1c8595adb89b7b6ac443e9a1caf0b20a6e1231a5"
        },
        "date": 1715097288048,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.868036125433331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18793676800000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b6dcd1b65436a6f7c087ad659617a9caf29a233a",
          "message": "Update prdoc for 2226 (#4401)\n\nMention that offenders are no longer chilled and suggest node operators\nand nominators to monitor their nodes/nominees closely.\n\n---------\n\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>",
          "timestamp": "2024-05-07T19:18:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6dcd1b65436a6f7c087ad659617a9caf29a233a"
        },
        "date": 1715115440657,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.699614023066662,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1525231151666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c91c13b9c1d6eaf12d89dbf088c1e16b25261822",
          "message": "Generate XCM weights for coretimes (#4396)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/3455#issuecomment-2094829076\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-07T21:50:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c91c13b9c1d6eaf12d89dbf088c1e16b25261822"
        },
        "date": 1715121966147,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18698998220000002,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.757190778166665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c91c13b9c1d6eaf12d89dbf088c1e16b25261822",
          "message": "Generate XCM weights for coretimes (#4396)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/3455#issuecomment-2094829076\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-07T21:50:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c91c13b9c1d6eaf12d89dbf088c1e16b25261822"
        },
        "date": 1715124452473,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.688975108466664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1786225116333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "17b56fae2d976a3df87f34076875de8c26da0355",
          "message": "Bridge: check bridge GRANDPA pallet call limits from signed extension (#4385)\n\nsilent, because it'll be deployed with the\nhttps://github.com/paritytech/polkadot-sdk/pull/4102, where this code\nhas been introduced\n\nI've planned originally to avoid doing that check in the runtime code,\nbecause it **may be** checked offchain. But actually, the check is quite\ncheap and we could do that onchain too.",
          "timestamp": "2024-05-08T08:26:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/17b56fae2d976a3df87f34076875de8c26da0355"
        },
        "date": 1715159820492,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.750198739899997,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15768795936666669,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "17b56fae2d976a3df87f34076875de8c26da0355",
          "message": "Bridge: check bridge GRANDPA pallet call limits from signed extension (#4385)\n\nsilent, because it'll be deployed with the\nhttps://github.com/paritytech/polkadot-sdk/pull/4102, where this code\nhas been introduced\n\nI've planned originally to avoid doing that check in the runtime code,\nbecause it **may be** checked offchain. But actually, the check is quite\ncheap and we could do that onchain too.",
          "timestamp": "2024-05-08T08:26:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/17b56fae2d976a3df87f34076875de8c26da0355"
        },
        "date": 1715162560432,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17214886270000002,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.826898730966665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "37b1544b51aeba183350d4c8d76987c32e6c9ca7",
          "message": "Adds benchmarking and try-runtime support in frame crate (#4406)",
          "timestamp": "2024-05-08T11:50:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/37b1544b51aeba183350d4c8d76987c32e6c9ca7"
        },
        "date": 1715175546690,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16845726916666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.75028313013333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6fdb522ded3813f43a539964af78d5fc6d9f1e97",
          "message": "Add semver CI check  (#4279)\n\nThis checks changed files against API surface changes against what the\nprdoc says.\n\nIt will error if the detected semver change is greater than the one\nlisted in the prdoc. It will also error if any crates were touched but\nnot mentioned in the prdoc.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-08T16:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6fdb522ded3813f43a539964af78d5fc6d9f1e97"
        },
        "date": 1715191230203,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.838880774899996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20245163433333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d37719da022879b4e2ef7947f5c9d2187f666ae7",
          "message": "rpc: add option to `whitelist ips` in rate limiting (#3701)\n\nThis PR adds two new CLI options to disable rate limiting for certain ip\naddresses and whether to trust \"proxy header\".\nAfter going back in forth I decided to use ip addr instead host because\nwe don't want rely on the host header which can be spoofed but another\nsolution is to resolve the ip addr from the socket to host name.\n\nExample:\n\n```bash\n$ polkadot --rpc-rate-limit 10 --rpc-rate-limit-whitelisted-ips 127.0.0.1/8 --rpc-rate-limit-trust-proxy-headers\n```\n\nThe ip addr is read from the HTTP proxy headers `Forwarded`,\n`X-Forwarded-For` `X-Real-IP` if `--rpc-rate-limit-trust-proxy-headers`\nis enabled if that is not enabled or the headers are not found then the\nip address is read from the socket.\n\n//cc @BulatSaif can you test this and give some feedback on it?",
          "timestamp": "2024-05-09T07:23:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d37719da022879b4e2ef7947f5c9d2187f666ae7"
        },
        "date": 1715245630026,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.429737253466666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23048429893333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "657df04cd901559cc6e33a8dfe70395bddb079f2",
          "message": "Fixes `frame-support` reference in `try_decode_entire_state` (#4425)",
          "timestamp": "2024-05-10T09:19:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/657df04cd901559cc6e33a8dfe70395bddb079f2"
        },
        "date": 1715338958126,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.810063628566663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19133395966666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "00440779d42b754292783612fc0f7e99d7cde2d2",
          "message": "Disabling Strategy Implementers Guide (#2955)\n\nCloses #1961",
          "timestamp": "2024-05-10T12:31:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/00440779d42b754292783612fc0f7e99d7cde2d2"
        },
        "date": 1715348269733,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.810063628566663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19133395966666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Maciej",
            "username": "Overkillus",
            "email": "maciej.zyszkiewicz@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "00440779d42b754292783612fc0f7e99d7cde2d2",
          "message": "Disabling Strategy Implementers Guide (#2955)\n\nCloses #1961",
          "timestamp": "2024-05-10T12:31:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/00440779d42b754292783612fc0f7e99d7cde2d2"
        },
        "date": 1715350016962,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.810063628566663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19133395966666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dónal Murray",
            "username": "seadanda",
            "email": "donal.murray@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a993513c9c54313bc3c7093563d2f4ff6fe42ae2",
          "message": "Add docs to request_core_count (#4423)\n\nThe fact that this takes two sessions to come into effect is not\nobvious. Just added some docs to explain that.\n\nAlso tidied up uses of \"broker chain\" -> \"coretime chain\"",
          "timestamp": "2024-05-10T19:41:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a993513c9c54313bc3c7093563d2f4ff6fe42ae2"
        },
        "date": 1715375975711,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.410597385933333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.28959793273333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "32deb605a09adf28ba30319b06a4197a2d048ef7",
          "message": "Remove `pallet::getter` usage from authority-discovery pallet (#4091)\n\nAs per #3326, removes pallet::getter usage from the pallet\nauthority-discovery. The syntax `StorageItem::<T, I>::get()` should be\nused instead.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-10T21:28:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/32deb605a09adf28ba30319b06a4197a2d048ef7"
        },
        "date": 1715382543945,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.042699766966669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2774504136666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9e0e5fcd0a814ab30d15b3f8920c8d9ab3970e11",
          "message": "xcm-emlator: Use `BlockNumberFor` instead of `parachains_common::BlockNumber=u32` (#4434)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4428",
          "timestamp": "2024-05-12T15:16:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9e0e5fcd0a814ab30d15b3f8920c8d9ab3970e11"
        },
        "date": 1715534997989,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18733056646666668,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.706392843133333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dastan",
            "username": "dastansam",
            "email": "88332432+dastansam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "efc2132fa2ece419d36af03c935b3c2c60440eb5",
          "message": "migrations: `take()`should consume read and write operation weight (#4302)\n\n#### Problem\n`take()` consumes only 1 read worth of weight in\n`single-block-migrations` example, while `take()`\n[is](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/support/src/storage/unhashed.rs#L63)\n`get() + kill()`, i.e should be 1 read + 1 write. I think this could\nmislead developers who follow this example to write their migrations\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-12T22:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efc2132fa2ece419d36af03c935b3c2c60440eb5"
        },
        "date": 1715556513866,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.874160463499999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18500746376666669,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dastan",
            "username": "dastansam",
            "email": "88332432+dastansam@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "efc2132fa2ece419d36af03c935b3c2c60440eb5",
          "message": "migrations: `take()`should consume read and write operation weight (#4302)\n\n#### Problem\n`take()` consumes only 1 read worth of weight in\n`single-block-migrations` example, while `take()`\n[is](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/support/src/storage/unhashed.rs#L63)\n`get() + kill()`, i.e should be 1 read + 1 write. I think this could\nmislead developers who follow this example to write their migrations\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-12T22:35:53Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/efc2132fa2ece419d36af03c935b3c2c60440eb5"
        },
        "date": 1715559350916,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18124401026666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.715557176999997,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "477a120893ecd35f5e4f808cba10525424b5711d",
          "message": "[ci] Add forklift to GHA ARC (#4372)\n\nPR adds forklift settings and forklift to test-github-actions\n\ncc https://github.com/paritytech/ci_cd/issues/939",
          "timestamp": "2024-05-13T09:54:32Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/477a120893ecd35f5e4f808cba10525424b5711d"
        },
        "date": 1715597097564,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.799551029033335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19286438683333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tsvetomir Dimitrov",
            "username": "tdimitrov",
            "email": "tsvetomir@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fb7362f67e3ac345073b203e029bcb561822f09c",
          "message": "Bump `proc-macro-crate` to the latest version (#4409)\n\nThis PR bumps `proc-macro-crate` to the latest version.\n\nIn order to test a runtime from\nhttps://github.com/polkadot-fellows/runtimes/ with the latest version of\npolkadot-sdk one needs to use `cargo vendor` to extract all runtime\ndependencies, patch them by hand and then build the runtime.\n\nHowever at the moment 'vendored' builds fail due to\nhttps://github.com/bkchr/proc-macro-crate/issues/48. To fix this\n`proc-macro-crate` should be updated to version `3.0.1` or higher.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-13T14:58:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fb7362f67e3ac345073b203e029bcb561822f09c"
        },
        "date": 1715619668793,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.646253565833332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17048967126666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Éloïs",
            "username": "librelois",
            "email": "c@elo.tf"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "594c3ed5750bc7ab97f82fb8387f82661eca1cc4",
          "message": " improve MockValidationDataInherentDataProvider to support async backing (#4442)\n\nSupport async backing in `--dev` mode\n\nThis PR improve the relay mock `MockValidationDataInherentDataProvider`\nto mach expectations of async backing runtimes.\n\n* Add para_head in the mock relay proof\n* Add relay slot in the mock relay proof \n\nfix https://github.com/paritytech/polkadot-sdk/issues/4437",
          "timestamp": "2024-05-13T22:03:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/594c3ed5750bc7ab97f82fb8387f82661eca1cc4"
        },
        "date": 1715645662036,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1857499762333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.729527823499998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "115c2477eb287df55107cd95594100ba395ed239",
          "message": "Bridge: use *-uri CLI arguments when starting relayer (#4451)\n\n`*-host` and `*-port` are obsolete and we'll hopefully remove them in\nthe future (already WIP for Rococo <> Westend relayer)",
          "timestamp": "2024-05-14T08:34:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/115c2477eb287df55107cd95594100ba395ed239"
        },
        "date": 1715683512384,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.993156198166668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21508494206666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6487ac1ede14b5785be1429655ed8c387d82be9a",
          "message": "Remove pallet::getter usage from the bounties and child-bounties pallets (#4392)\n\nAs per #3326, removes pallet::getter usage from the bounties and\nchild-bounties pallets. The syntax `StorageItem::<T, I>::get()` should\nbe used instead.\n\nChanges to one pallet involved changes in the other, so I figured it'd\nbe best to combine these two.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-16T09:01:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6487ac1ede14b5785be1429655ed8c387d82be9a"
        },
        "date": 1715855895173,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.34633479846666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 15.3795441866,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4adfa37d14c0d81f09071687afb270ecdd5c2076",
          "message": "[Runtime] Bound XCMP queue (#3952)\n\nRe-applying #2302 after increasing the `MaxPageSize`.  \n\nRemove `without_storage_info` from the XCMP queue pallet. Part of\nhttps://github.com/paritytech/polkadot-sdk/issues/323\n\nChanges:\n- Limit the number of messages and signals a HRMP channel can have at\nmost.\n- Limit the number of HRML channels.\n\nA No-OP migration is put in place to ensure that all `BoundedVec`s still\ndecode and not truncate after upgrade. The storage version is thereby\nbumped to 5 to have our tooling remind us to deploy that migration.\n\n## Integration\n\nIf you see this error in your try-runtime-cli:  \n```pre\nMax message size for channel is too large. This means that the V5 migration can be front-run and an\nattacker could place a large message just right before the migration to make other messages un-decodable.\nPlease either increase `MaxPageSize` or decrease the `max_message_size` for this channel. Channel max:\n102400, MaxPageSize: 65535\n```\n\nThen increase the `MaxPageSize` of the `cumulus_pallet_xcmp_queue` to\nsomething like this:\n```rust\ntype MaxPageSize = ConstU32<{ 103 * 1024 }>;\n```\n\nThere is currently no easy way for on-chain governance to adjust the\nHRMP max message size of all channels, but it could be done:\nhttps://github.com/paritytech/polkadot-sdk/issues/3145.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>",
          "timestamp": "2024-05-16T10:43:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4adfa37d14c0d81f09071687afb270ecdd5c2076"
        },
        "date": 1715862055537,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.931179521866664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17779873166666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Clara van Staden",
            "username": "claravanstaden",
            "email": "claravanstaden64@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "943eb46ed54c2fcd9fab693b86ef59ce18c0f792",
          "message": "Snowbridge - Ethereum Client - Reject finalized updates without a sync committee in next store period (#4478)\n\nWhile syncing Ethereum consensus updates to the Snowbridge Ethereum\nlight client, the syncing process stalled due to error\n`InvalidSyncCommitteeUpdate` when importing the next sync committee for\nperiod `1087`.\n\nThis bug manifested specifically because our light client checkpoint is\na few weeks old (submitted to governance weeks ago) and had to catchup\nuntil a recent block. Since then, we have done thorough testing of the\ncatchup sync process.\n\n### Symptoms\n- Import next sync committee for period `1086` (essentially period\n`1087`). Light client store period = `1086`.\n- Import header in period `1087`. Light client store period = `1087`.\nThe current and next sync committee is not updated, and is now in an\noutdated state. (current sync committee = `1086` and current sync\ncommittee = `1087`, where it should be current sync committee = `1087`\nand current sync committee = `None`)\n- Import next sync committee for period `1087` (essentially period\n`1088`) fails because the expected next sync committee's roots don't\nmatch.\n\n### Bug\nThe bug here is that the current and next sync committee's didn't\nhandover when an update in the next period was received.\n\n### Fix\nThere are two possible fixes here:\n1. Correctly handover sync committees when a header in the next period\nis received.\n2. Reject updates in the next period until the next sync committee\nperiod is known.\n\nWe opted for solution 2, which is more conservative and requires less\nchanges.\n\n### Polkadot-sdk versions\nThis fix should be backported in polkadot-sdk versions 1.7 and up.\n\nSnowfork PR: https://github.com/Snowfork/polkadot-sdk/pull/145\n\n---------\n\nCo-authored-by: Vincent Geddes <117534+vgeddes@users.noreply.github.com>",
          "timestamp": "2024-05-16T13:54:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/943eb46ed54c2fcd9fab693b86ef59ce18c0f792"
        },
        "date": 1715869685818,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.781672119066666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1743826401333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Clara van Staden",
            "username": "claravanstaden",
            "email": "claravanstaden64@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "943eb46ed54c2fcd9fab693b86ef59ce18c0f792",
          "message": "Snowbridge - Ethereum Client - Reject finalized updates without a sync committee in next store period (#4478)\n\nWhile syncing Ethereum consensus updates to the Snowbridge Ethereum\nlight client, the syncing process stalled due to error\n`InvalidSyncCommitteeUpdate` when importing the next sync committee for\nperiod `1087`.\n\nThis bug manifested specifically because our light client checkpoint is\na few weeks old (submitted to governance weeks ago) and had to catchup\nuntil a recent block. Since then, we have done thorough testing of the\ncatchup sync process.\n\n### Symptoms\n- Import next sync committee for period `1086` (essentially period\n`1087`). Light client store period = `1086`.\n- Import header in period `1087`. Light client store period = `1087`.\nThe current and next sync committee is not updated, and is now in an\noutdated state. (current sync committee = `1086` and current sync\ncommittee = `1087`, where it should be current sync committee = `1087`\nand current sync committee = `None`)\n- Import next sync committee for period `1087` (essentially period\n`1088`) fails because the expected next sync committee's roots don't\nmatch.\n\n### Bug\nThe bug here is that the current and next sync committee's didn't\nhandover when an update in the next period was received.\n\n### Fix\nThere are two possible fixes here:\n1. Correctly handover sync committees when a header in the next period\nis received.\n2. Reject updates in the next period until the next sync committee\nperiod is known.\n\nWe opted for solution 2, which is more conservative and requires less\nchanges.\n\n### Polkadot-sdk versions\nThis fix should be backported in polkadot-sdk versions 1.7 and up.\n\nSnowfork PR: https://github.com/Snowfork/polkadot-sdk/pull/145\n\n---------\n\nCo-authored-by: Vincent Geddes <117534+vgeddes@users.noreply.github.com>",
          "timestamp": "2024-05-16T13:54:28Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/943eb46ed54c2fcd9fab693b86ef59ce18c0f792"
        },
        "date": 1715873787474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.036294688266668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1953264811,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jesse Chejieh",
            "username": "Doordashcon",
            "email": "jesse.chejieh@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d5fe478e4fe2d62b0800888ae77b00ff0ba28b28",
          "message": "Adds `MaxRank` Config in `pallet-core-fellowship` (#3393)\n\nresolves #3315\n\n---------\n\nCo-authored-by: doordashcon <jessechejieh@doordashcon.local>\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-16T16:22:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5fe478e4fe2d62b0800888ae77b00ff0ba28b28"
        },
        "date": 1715882740585,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22548167126666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.115176670866669,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f86f2131fe0066cf9009cb909e843da664b3df98",
          "message": "Contracts: remove kitchensink dynamic parameters (#4489)\n\nUsing Dynamic Parameters for contracts seems like a bad idea for now.\n\nGiven that we have benchmarks for each host function (in addition to our\nextrinsics), parameter storage reads will be counted multiple times. We\nwill work on updates to the benchmarking framework to mitigate this\nissue in future iterations.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-17T05:52:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f86f2131fe0066cf9009cb909e843da664b3df98"
        },
        "date": 1715930975780,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.810423469666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18680149109999997,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2c48b9ddb0a5de4499d4ed699b79eacc354f016a",
          "message": "Bridge: fixed relayer version metric value (#4492)\n\nBefore relayer crates have been moved + merged, the `MetricsParams` type\nhas been created from a `substrate-relay` crate (binary) and hence it\nhas been setting the `substrate_relay_build_info` metic value properly -\nto the binary version. Now it is created from the\n`substrate-relay-helper` crate, which has the fixed (it isn't published)\nversion `0.1.0`, so our relay provides incorrect metric value. This\n'breaks' our monitoring tools - we see that all relayers have that\nincorrect version, which is not cool.\n\nThe idea is to have a global static variable (shame on me) that is\ninitialized by the binary during initialization like we do with the\nlogger initialization already. Was considering some alternative options:\n- adding a separate argument to every relayer subcommand and propagating\nit to the `MetricsParams::new()` causes a lot of changes and introduces\neven more noise to the binary code, which is supposed to be as small as\npossible in the new design. But I could do that if team thinks it is\nbetter;\n- adding a `structopt(skip) pub relayer_version: RelayerVersion`\nargument to all subcommand params won't work, because it will be\ninitialized by default and `RelayerVersion` needs to reside in some util\ncrate (not the binary), so it'll have the wrong value again.",
          "timestamp": "2024-05-17T08:00:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2c48b9ddb0a5de4499d4ed699b79eacc354f016a"
        },
        "date": 1715939016872,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.230266221633332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2347218199666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2e36f571e5c9486819b85561d12fa4001018e953",
          "message": "Allow pool to be destroyed with an extra (erroneous) consumer reference on the pool account (#4503)\n\naddresses https://github.com/paritytech/polkadot-sdk/issues/4440 (will\nclose once we have this in prod runtimes).\nrelated: https://github.com/paritytech/polkadot-sdk/issues/2037.\n\nAn extra consumer reference is preventing pools to be destroyed. When a\npool is ready to be destroyed, we\ncan safely clear the consumer references if any. Notably, I only check\nfor one extra consumer reference since that is a known bug. Anything\nmore indicates possibly another issue and we probably don't want to\nsilently absorb those errors as well.\n\nAfter this change, pools with extra consumer reference should be able to\ndestroy normally.",
          "timestamp": "2024-05-17T12:09:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2e36f571e5c9486819b85561d12fa4001018e953"
        },
        "date": 1715953560218,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1884651952666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.9194908425,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a90d324d5b3252033e00a96d9f9f4890b1cfc982",
          "message": "Contracts: Remove topics for internal events (#4510)",
          "timestamp": "2024-05-17T13:47:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a90d324d5b3252033e00a96d9f9f4890b1cfc982"
        },
        "date": 1715961133321,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1803669195,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.837346158899999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "jimwfs",
            "username": "jimwfs",
            "email": "wqq1479787@163.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "247358a86f874bfa109575dd086a6478dbc96eb4",
          "message": "chore: fix typos (#4515)\n\nCo-authored-by: jimwfs <169986508+jimwfs@users.noreply.github.com>",
          "timestamp": "2024-05-19T15:31:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/247358a86f874bfa109575dd086a6478dbc96eb4"
        },
        "date": 1716138602976,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.367614011599994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2614306414333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Liam Aharon",
            "username": "liamaharon",
            "email": "liam.aharon@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e7b6d7dffd6459174f02598bd8b84fe4b1cb6e72",
          "message": "`remote-externalities`: `rpc_child_get_keys` to use paged scraping (#4512)\n\nReplace usage of deprecated\n`substrate_rpc_client::ChildStateApi::storage_keys` with\n`substrate_rpc_client::ChildStateApi::storage_keys_paged`.\n\nRequired for successful scraping of Aleph Zero state.",
          "timestamp": "2024-05-19T17:53:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e7b6d7dffd6459174f02598bd8b84fe4b1cb6e72"
        },
        "date": 1716147306476,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.795155988399994,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16728990623333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "313fe0f9a277f27a4228634f0fb15a1c3fa21271",
          "message": "Remove usage of the pallet::getter macro from pallet-fast-unstake (#4514)\n\nAs per #3326, removes pallet::getter macro usage from\npallet-fast-unstake. The syntax `StorageItem::<T, I>::get()` should be\nused instead.\n\ncc @muraca\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>",
          "timestamp": "2024-05-20T06:36:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/313fe0f9a277f27a4228634f0fb15a1c3fa21271"
        },
        "date": 1716192931295,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21410013586666668,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.095683697666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alin Dima",
            "username": "alindima",
            "email": "alin@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "278486f9bf7db06c174203f098eec2f91839757a",
          "message": "Remove the prospective-parachains subsystem from collators (#4471)\n\nImplements https://github.com/paritytech/polkadot-sdk/issues/4429\n\nCollators only need to maintain the implicit view for the paraid they\nare collating on.\nIn this case, bypass prospective-parachains entirely. It's still useful\nto use the GetMinimumRelayParents message from prospective-parachains\nfor validators, because the data is already present there.\n\nThis enables us to entirely remove the subsystem from collators, which\nconsumed resources needlessly\n\nAims to resolve https://github.com/paritytech/polkadot-sdk/issues/4167 \n\nTODO:\n- [x] fix unit tests",
          "timestamp": "2024-05-21T08:14:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/278486f9bf7db06c174203f098eec2f91839757a"
        },
        "date": 1716285227028,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17691376976666662,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.582821097766665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d54feeb101b3779422323224c8e1ac43d3a1fafa",
          "message": "Fixed RPC subscriptions leak when subscription stream is finished (#4533)\n\ncloses https://github.com/paritytech/parity-bridges-common/issues/3000\n\nRecently we've changed our bridge configuration for Rococo <> Westend\nand our new relayer has started to submit transactions every ~ `30`\nseconds. Eventually, it switches itself into limbo state, where it can't\nsubmit more transactions - all `author_submitAndWatchExtrinsic` calls\nare failing with the following error: `ERROR bridge Failed to send\ntransaction to BridgeHubRococo node: Call(ErrorObject { code:\nServerError(-32006), message: \"Too many subscriptions on the\nconnection\", data: Some(RawValue(\"Exceeded max limit of 1024\")) })`.\n\nSome links for those who want to explore:\n- server side (node) has a strict limit on a number of active\nsubscriptions. It fails to open a new subscription if this limit is hit:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/server/src/middleware/rpc/layer/rpc_service.rs#L122-L132.\nThe limit is set to `1024` by default;\n- internally this limit is a semaphore with `limit` permits:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/core/src/server/subscription.rs#L461-L485;\n- semaphore permit is acquired in the first link;\n- the permit is \"returned\" when the `SubscriptionSink` is dropped:\nhttps://github.com/paritytech/jsonrpsee/blob/a4533966b997e83632509ad97eea010fc7c3efc0/core/src/server/subscription.rs#L310-L325;\n- the `SubscriptionSink` is dropped when [this `polkadot-sdk`\nfunction](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L58-L94)\nreturns. In other words - when the connection is closed, the stream is\nfinished or internal subscription buffer limit is hit;\n- the subscription has the internal buffer, so sending an item contains\nof two steps: [reading an item from the underlying\nstream](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L125-L141)\nand [sending it over the\nconnection](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L111-L116);\n- when the underlying stream is finished, the `inner_pipe_from_stream`\nwants to ensure that all items are sent to the subscriber. So it: [waits\nuntil the current send operation\ncompletes](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L146-L148)\nand then [send all remaining items from the internal\nbuffer](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L150-L155).\nOnce it is done, the function returns, the `SubscriptionSink` is\ndropped, semaphore permit is dropped and we are ready to accept new\nsubscriptions;\n- unfortunately, the code just calls the `pending_fut.await.is_err()` to\nensure that [the current send operation\ncompletes](https://github.com/paritytech/polkadot-sdk/blob/278486f9bf7db06c174203f098eec2f91839757a/substrate/client/rpc/src/utils.rs#L146-L148).\nBut if there are no current send operation (which is normal), then the\n`pending_fut` is set to terminated future and the `await` never\ncompletes. Hence, no return from the function, no drop of\n`SubscriptionSink`, no drop of semaphore permit, no new subscriptions\nallowed (once number of susbcriptions hits the limit.\n\nI've illustrated the issue with small test - you may ensure that if e.g.\nthe stream is initially empty, the\n`subscription_is_dropped_when_stream_is_empty` will hang because\n`pipe_from_stream` never exits.",
          "timestamp": "2024-05-21T10:41:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d54feeb101b3779422323224c8e1ac43d3a1fafa"
        },
        "date": 1716295643887,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.652691542600005,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17977665003333337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e0e1f2d6278885d1ffebe3263315089e48572a26",
          "message": "Bridge: added force_set_pallet_state call to pallet-bridge-grandpa (#4465)\n\ncloses https://github.com/paritytech/parity-bridges-common/issues/2963\n\nSee issue above for rationale\nI've been thinking about adding similar calls to other pallets, but:\n- for parachains pallet I haven't been able to think of a case when we\nwill need that given how long referendum takes. I.e. if storage proof\nformat changes and we want to unstuck the bridge, it'll take a large a\nfew weeks to sync a single parachain header, then another weeks for\nanother and etc.\n- for messages pallet I've made the similar call initially, but it just\nchanges a storage key (`OutboundLanes` and/or `InboundLanes`), so\nthere's no any logic here and it may be simply done using\n`system.set_storage`.\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-21T13:46:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0e1f2d6278885d1ffebe3263315089e48572a26"
        },
        "date": 1716306738968,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20568314613333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.777823742400003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Dmitry Markin",
            "username": "dmitry-markin",
            "email": "dmitry@markin.tech"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d05786ffb5523c334f10d16870c2e73674661a52",
          "message": "Replace `Multiaddr` & related types with substrate-specific types (#4198)\n\nThis PR introduces custom types / substrate wrappers for `Multiaddr`,\n`multiaddr::Protocol`, `Multihash`, `ed25519::*` and supplementary types\nlike errors and iterators.\n\nThis is needed to unblock `libp2p` upgrade PR\nhttps://github.com/paritytech/polkadot-sdk/pull/1631 after\nhttps://github.com/paritytech/polkadot-sdk/pull/2944 was merged.\n`libp2p` and `litep2p` currently depend on different versions of\n`multiaddr` crate, and introduction of this \"common ground\" types is\nneeded to support independent version upgrades of `multiaddr` and\ndependent crates in `libp2p` & `litep2p`.\n\nWhile being just convenient to not tie versions of `libp2p` & `litep2p`\ndependencies together, it's currently not even possible to keep `libp2p`\n& `litep2p` dependencies updated to the same versions as `multiaddr` in\n`libp2p` depends on `libp2p-identity` that we can't include as a\ndependency of `litep2p`, which has it's own `PeerId` type. In the\nfuture, to keep things updated on `litep2p` side, we will likely need to\nfork `multiaddr` and make it use `litep2p` `PeerId` as a payload of\n`/p2p/...` protocol.\n\nWith these changes, common code in substrate uses these custom types,\nand `litep2p` & `libp2p` backends use corresponding libraries types.",
          "timestamp": "2024-05-21T16:10:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d05786ffb5523c334f10d16870c2e73674661a52"
        },
        "date": 1716314467977,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.603178703,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17299199356666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Viola",
            "username": "pepoviola",
            "email": "363911+pepoviola@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ec46106c33f2220d16a9dc7ad604d564d42ee009",
          "message": "chore: bump zombienet version (#4535)\n\nThis version includes the latest release of pjs/api\n(https://github.com/polkadot-js/api/releases/tag/v11.1.1).\nThx!",
          "timestamp": "2024-05-21T21:33:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ec46106c33f2220d16a9dc7ad604d564d42ee009"
        },
        "date": 1716333985178,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2300246692,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.937239446466666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "joe petrowski",
            "username": "joepetrowski",
            "email": "25483142+joepetrowski@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c7cb1f25d1f8bb1a922d466e39ee935f5f027266",
          "message": "Add Extra Check in Primary Username Setter (#4534)",
          "timestamp": "2024-05-22T07:21:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c7cb1f25d1f8bb1a922d466e39ee935f5f027266"
        },
        "date": 1716368709169,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.760006015866665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19805844703333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b06306c42c969eaa0b828413dd03dc3b7a844976",
          "message": "[Backport] Version bumps and prdocs reordering from 1.12.0 (#4538)\n\nThis PR backports version bumps and reorganisation of the prdoc files\nfrom the `1.12.0` release branch back to `master`",
          "timestamp": "2024-05-22T11:29:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b06306c42c969eaa0b828413dd03dc3b7a844976"
        },
        "date": 1716379454982,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18504999290000007,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.744722336799999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Riko",
            "username": "fasteater",
            "email": "49999458+fasteater@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad54bc36c1b2ce9517d023f2df9d6bdec9ca64e1",
          "message": "fixed link (#4539)",
          "timestamp": "2024-05-22T11:55:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad54bc36c1b2ce9517d023f2df9d6bdec9ca64e1"
        },
        "date": 1716384921850,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17272699146666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.688903119366667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8949856d840c7f97c0c0c58a3786ccce5519a8fe",
          "message": "Refactor Nomination Pool to support multiple staking strategies (#3905)\n\nThird and final PR in the set, closes\nhttps://github.com/paritytech/polkadot-sdk/issues/454.\n\nOriginal PR: https://github.com/paritytech/polkadot-sdk/pull/2680\n\n## Precursors:\n- https://github.com/paritytech/polkadot-sdk/pull/3889.\n- https://github.com/paritytech/polkadot-sdk/pull/3904.\n\n## Follow up issues/improvements\n- https://github.com/paritytech/polkadot-sdk/issues/4404\n\nOverall changes are documented here (lot more visual 😍):\nhttps://hackmd.io/@ak0n/454-np-governance\n\n## Summary of various roles 🤯\n### Pallet Staking\n**Nominator**: An account that directly stakes on `pallet-staking` and\nnominates a set of validators.\n**Stakers**: Common term for nominators and validators.\nVirtual Stakers: Same as stakers, but they are keyless accounts and\ntheir locks are managed by a pallet external to `pallet-staking`.\n\n### Pallet Delegated Staking\n**Agent**: An account that receives delegation from other accounts\n(delegators) and stakes on their behalf. They are also Virtual Stakers\nin `pallet-staking` where `pallet-delegated-staking` manages its locks.\n**Delegator**: An account that delegates some funds to an agent.\n\n### Pallet Nomination Pools\n**Pool account**: Keyless account of a pool where funds are pooled.\nMembers pledge their funds towards the pools. These are going to become\n`Agent` accounts in `pallet-delegated-staking`.\n**Pool Members**: They are individual members of the pool who\ncontributed funds to it. They are also `Delegator` in\n`pallet-delegated-staking`.\n\n## Changes\n### Multiple Stake strategies\n\n**TransferStake**: The current nomination pool logic can be considered a\nstaking strategy where delegators transfer funds to pool and stake. In\nthis scenario, funds are locked in pool account, and users lose the\ncontrol of their funds.\n\n**DelegateStake**: With this PR, we introduce a new staking strategy\nwhere individual delegators delegate fund to pool. `Delegate` implies\nfunds are locked in delegator account itself. Important thing to note\nis, pool does not have funds of its own, but it has authorization from\nits members to use these funds for staking.\n\nWe extract out all the interaction of pool with staking interface into a\nnew trait `StakeStrategy`. This is the logic that varies between the\nabove two staking strategies. We use the trait `StakeStrategy` to\nimplement above two strategies: `TransferStake` and `DelegateStake`.\n\n### NominationPool\nConsumes an implementation of `StakeStrategy` instead of\n`StakingInterface`. I have renamed it from `Staking` to `StakeAdapter`\nto clarify the difference from the earlier used trait.\n\nTo enable delegation based staking in pool, Nomination pool can be\nconfigured as:\n```\ntype StakeAdapter = pallet_nomination_pools::adapter::DelegateStake<Self, DelegatedStaking>;\n```\n\nNote that with the following configuration, the changes in the PR are\nno-op.\n```\ntype StakeAdapter = pallet_nomination_pools::adapter::TransferStake<Self, Staking>;\n```\n\n## Deployment roadmap\nPlan to enable this only in Westend. In production runtimes, we can keep\npool to use `TransferStake` which will be no functional change.\n\nOnce we have a full audit, we can enable this in Kusama followed by\nPolkadot.\n\n## TODO\n- [x] Runtime level (Westend) migration for existing nomination pools.\n- [x] Permissionless call/ pallet::tasks for claiming delegator funds.\n- [x] Add/update benches.\n- [x] Migration tests.\n- [x] Storage flag to mark `DelegateStake` migration and integrity\nchecks to not allow `TransferStake` for migrated runtimes.\n\n---------\n\nSigned-off-by: Matteo Muraca <mmuraca247@gmail.com>\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nSigned-off-by: divdeploy <chenguangxue@outlook.com>\nSigned-off-by: dependabot[bot] <support@github.com>\nSigned-off-by: hongkuang <liurenhong@outlook.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: gemini132 <164285545+gemini132@users.noreply.github.com>\nCo-authored-by: Matteo Muraca <56828990+muraca@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Alexandru Gheorghe <49718502+alexggh@users.noreply.github.com>\nCo-authored-by: Alessandro Siniscalchi <asiniscalchi@gmail.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Ross Bulat <ross@parity.io>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>\nCo-authored-by: Sam Johnson <sam@durosoft.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Niklas Adolfsson <niklasadolfsson1@gmail.com>\nCo-authored-by: Dastan <88332432+dastansam@users.noreply.github.com>\nCo-authored-by: Clara van Staden <claravanstaden64@gmail.com>\nCo-authored-by: Ron <yrong1997@gmail.com>\nCo-authored-by: Vincent Geddes <vincent@snowfork.com>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Michal Kucharczyk <1728078+michalkucharczyk@users.noreply.github.com>\nCo-authored-by: Dino Pačandi <3002868+Dinonard@users.noreply.github.com>\nCo-authored-by: Andrei Eres <eresav@me.com>\nCo-authored-by: Alin Dima <alin@parity.io>\nCo-authored-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>\nCo-authored-by: gupnik <nikhilgupta.iitk@gmail.com>\nCo-authored-by: Vladimir Istyufeev <vladimir@parity.io>\nCo-authored-by: Lulu <morgan@parity.io>\nCo-authored-by: Juan Girini <juangirini@gmail.com>\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Dónal Murray <donal.murray@parity.io>\nCo-authored-by: Shawn Tabrizi <shawntabrizi@gmail.com>\nCo-authored-by: Kutsal Kaan Bilgin <kutsalbilgin@gmail.com>\nCo-authored-by: Ermal Kaleci <ermalkaleci@gmail.com>\nCo-authored-by: ordian <write@reusable.software>\nCo-authored-by: divdeploy <166095818+divdeploy@users.noreply.github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Sergej Sakac <73715684+Szegoo@users.noreply.github.com>\nCo-authored-by: Squirrel <gilescope@gmail.com>\nCo-authored-by: HongKuang <166261675+HongKuang@users.noreply.github.com>\nCo-authored-by: Tsvetomir Dimitrov <tsvetomir@parity.io>\nCo-authored-by: Egor_P <egor@parity.io>\nCo-authored-by: Aaro Altonen <48052676+altonen@users.noreply.github.com>\nCo-authored-by: Dmitry Markin <dmitry@markin.tech>\nCo-authored-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Léa Narzis <78718413+lean-apple@users.noreply.github.com>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: georgepisaltu <52418509+georgepisaltu@users.noreply.github.com>\nCo-authored-by: command-bot <>\nCo-authored-by: PG Herveou <pgherveou@gmail.com>\nCo-authored-by: jimwfs <wqq1479787@163.com>\nCo-authored-by: jimwfs <169986508+jimwfs@users.noreply.github.com>\nCo-authored-by: polka.dom <polkadotdom@gmail.com>",
          "timestamp": "2024-05-22T19:26:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8949856d840c7f97c0c0c58a3786ccce5519a8fe"
        },
        "date": 1716412614884,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20992337246666665,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.122909128766661,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fd161917108a14e791c1444ea1c767e9f6134bdf",
          "message": "Fix README.md Logo URL (#4546)\n\nThis one also works and it is easier.",
          "timestamp": "2024-05-23T08:03:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fd161917108a14e791c1444ea1c767e9f6134bdf"
        },
        "date": 1716457535191,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21169110466666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.879599625,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "493ba5e2a144140d5018647b25744b0aac854ffd",
          "message": "Contracts: Rework host fn benchmarks (#4233)\n\nfix https://github.com/paritytech/polkadot-sdk/issues/4163\n\nThis PR does the following:\nUpdate to pallet-contracts-proc-macro: \n- Parse #[cfg] so we can add a dummy noop host function for benchmark.\n- Generate BenchEnv::<host_fn> so we can call host functions directly in\nthe benchmark.\n- Add the weight of the noop host function before calling the host\nfunction itself\n\nUpdate benchmarks:\n- Update all host function benchmark, a host function benchmark now\nsimply call the host function, instead of invoking the function n times\nfrom within a contract.\n- Refactor RuntimeCosts & Schedule, for most host functions, we can now\nuse the generated weight function directly instead of computing the diff\nwith the cost! macro\n\n```rust\n// Before\n#[benchmark(pov_mode = Measured)]\nfn seal_input(r: Linear<0, API_BENCHMARK_RUNS>) {\n    let code = WasmModule::<T>::from(ModuleDefinition {\n        memory: Some(ImportedMemory::max::<T>()),\n        imported_functions: vec![ImportedFunction {\n            module: \"seal0\",\n            name: \"seal_input\",\n            params: vec![ValueType::I32, ValueType::I32],\n            return_type: None,\n        }],\n        data_segments: vec![DataSegment { offset: 0, value: 0u32.to_le_bytes().to_vec() }],\n        call_body: Some(body::repeated(\n            r,\n            &[\n                Instruction::I32Const(4), // ptr where to store output\n                Instruction::I32Const(0), // ptr to length\n                Instruction::Call(0),\n            ],\n        )),\n        ..Default::default()\n    });\n\n    call_builder!(func, code);\n\n    let res;\n    #[block]\n    {\n        res = func.call();\n    }\n    assert_eq!(res.did_revert(), false);\n}\n```\n\n```rust\n// After\nfn seal_input(n: Linear<0, { code::max_pages::<T>() * 64 * 1024 - 4 }>) {\n    let mut setup = CallSetup::<T>::default();\n    let (mut ext, _) = setup.ext();\n    let mut runtime = crate::wasm::Runtime::new(&mut ext, vec![42u8; n as usize]);\n    let mut memory = memory!(n.to_le_bytes(), vec![0u8; n as usize],);\n    let result;\n    #[block]\n    {\n        result = BenchEnv::seal0_input(&mut runtime, &mut memory, 4, 0)\n    }\n    assert_ok!(result);\n    assert_eq!(&memory[4..], &vec![42u8; n as usize]);\n}\n``` \n\n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Frework-host-benchs&path_pattern=substrate%2Fframe%2Fcontracts%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-05-23T11:17:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/493ba5e2a144140d5018647b25744b0aac854ffd"
        },
        "date": 1716464907877,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.571768636633333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16201044813333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "03bbc17e92d1d04b6b4b9aef7669c403d08bc28c",
          "message": "Define `OpaqueValue` (#4550)\n\nDefine `OpaqueValue` and use it instead of\n`grandpa::OpaqueKeyOwnershipProof` and `beefy:OpaqueKeyOwnershipProof`\n\nRelated to\nhttps://github.com/paritytech/polkadot-sdk/pull/4522#discussion_r1608278279\n\nWe'll need to introduce a runtime API method that calls the\n`report_fork_voting_unsigned()` extrinsic. This method will need to\nreceive the ancestry proof as a paramater. I'm still not sure, but there\nis a chance that we'll send the ancestry proof as an opaque type.\n\nSo let's introduce this `OpaqueValue`. We can already use it to replace\n`grandpa::OpaqueKeyOwnershipProof` and `beefy:OpaqueKeyOwnershipProof`\nand maybe we'll need it for the ancestry proof as well.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-23T12:38:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/03bbc17e92d1d04b6b4b9aef7669c403d08bc28c"
        },
        "date": 1716473844684,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.883666869566664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2062630374666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "48d4f654612a67787426de426e462bd40f6f70f6",
          "message": "Mention new XCM docs in sdk docs (#4558)\n\nThe XCM docs were pretty much moved to the new rust docs format in\nhttps://github.com/paritytech/polkadot-sdk/pull/2633, with the addition\nof the XCM cookbook, which I plan to add more examples to shortly.\n\nThese docs were not mentioned in the polkadot-sdk rust docs, this PR\njust mentions them there, so people can actually find them.",
          "timestamp": "2024-05-23T21:04:41Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48d4f654612a67787426de426e462bd40f6f70f6"
        },
        "date": 1716503933745,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.77632876273333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1903303948333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "700d5910580fdc17a0737925d4fe2472eb265f82",
          "message": "Use polkadot-ckb-merkle-mountain-range dependency (#4562)\n\nWe need to use the `polkadot-ckb-merkle-mountain-range` dependency\npublished on `crates.io` in order to unblock the release of the\n`sp-mmr-primitives` crate",
          "timestamp": "2024-05-24T07:43:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/700d5910580fdc17a0737925d4fe2472eb265f82"
        },
        "date": 1716542620089,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.645457949533334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.172430785,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ef144b1a88c6478e5d6dac945ffe12053f05d96a",
          "message": "Attempt to avoid specifying `BlockHashCount` for different `mocking::{MockBlock, MockBlockU32, MockBlockU128}` (#4543)\n\nWhile doing some migration/rebase I came in to the situation, where I\nneeded to change `mocking::MockBlock` to `mocking::MockBlockU32`:\n```\n#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]\nimpl frame_system::Config for TestRuntime {\n\ttype Block = frame_system::mocking::MockBlockU32<TestRuntime>;\n\ttype AccountData = pallet_balances::AccountData<ThisChainBalance>;\n}\n```\nBut actual `TestDefaultConfig` for `frame_system` is using `ConstU64`\nfor `type BlockHashCount = frame_support::traits::ConstU64<10>;`\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/system/src/lib.rs#L303).\nBecause of this, it force me to specify and add override for `type\nBlockHashCount = ConstU32<10>`.\n\nThis PR tries to fix this with `TestBlockHashCount` implementation for\n`TestDefaultConfig` which supports `u32`, `u64` and `u128` as a\n`BlockNumber`.\n\n### How to simulate error\nJust by removing `type BlockHashCount = ConstU32<250>;`\n[here](https://github.com/paritytech/polkadot-sdk/blob/master/substrate/frame/multisig/src/tests.rs#L44)\n```\n:~/parity/olkadot-sdk$ cargo test -p pallet-multisig\n   Compiling pallet-multisig v28.0.0 (/home/bparity/parity/aaa/polkadot-sdk/substrate/frame/multisig)\nerror[E0277]: the trait bound `ConstU64<10>: frame_support::traits::Get<u32>` is not satisfied\n   --> substrate/frame/multisig/src/tests.rs:41:1\n    |\n41  | #[derive_impl(frame_system::config_preludes::TestDefaultConfig)]\n    | ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^ the trait `frame_support::traits::Get<u32>` is not implemented for `ConstU64<10>`\n    |\n    = help: the following other types implement trait `frame_support::traits::Get<T>`:\n              <ConstU64<T> as frame_support::traits::Get<u64>>\n              <ConstU64<T> as frame_support::traits::Get<std::option::Option<u64>>>\nnote: required by a bound in `frame_system::Config::BlockHashCount`\n   --> /home/bparity/parity/aaa/polkadot-sdk/substrate/frame/system/src/lib.rs:535:24\n    |\n535 |         type BlockHashCount: Get<BlockNumberFor<Self>>;\n    |                              ^^^^^^^^^^^^^^^^^^^^^^^^^ required by this bound in `Config::BlockHashCount`\n    = note: this error originates in the attribute macro `derive_impl` which comes from the expansion of the macro `frame_support::macro_magic::forward_tokens_verbatim` (in Nightly builds, run with -Z macro-backtrace for more info)\n\nFor more information about this error, try `rustc --explain E0277`.\nerror: could not compile `pallet-multisig` (lib test) due to 1 previous error \n```\n\n\n\n\n## For reviewers:\n\n(If there is a better solution, please let me know!)\n\nThe first commit contains actual attempt to fix the problem:\nhttps://github.com/paritytech/polkadot-sdk/commit/3c5499e539f2218503fbd6ce9be085b03c31ee13.\nThe second commit is just removal of `BlockHashCount` from all other\nplaces where not needed by default.\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/1657\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-24T10:01:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ef144b1a88c6478e5d6dac945ffe12053f05d96a"
        },
        "date": 1716551689183,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1780304012333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.595197149466669,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "49bd6a6e94b8b6f4ef3497e930cfb493b8ec0fd0",
          "message": "Remove litep2p git dependency (#4560)\n\n@serban300 could you please do the same for the MMR crate? Am not sure\nwhat commit was released since there are no release tags in the repo.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-24T11:55:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/49bd6a6e94b8b6f4ef3497e930cfb493b8ec0fd0"
        },
        "date": 1716557791859,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18578459020000002,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.72030781003333,
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
          "id": "f469fbfb0a44c4e223488b07ec641ca02b2fb8f1",
          "message": "availability-recovery: bump chunk fetch threshold to 1MB for Polkadot and 4MB for Kusama + testnets (#4399)\n\nDoing this change ensures that we minimize the CPU usage we spend in\nreed-solomon by only doing the re-encoding into chunks if PoV size is\nless than 4MB (which means all PoVs right now)\n \nBased on susbystem benchmark results we concluded that it is safe to\nbump this number higher. At worst case scenario the network pressure for\na backing group of 5 is around 25% of the network bandwidth in hw specs.\n\nAssuming 6s block times (max_candidate_depth 3) and needed_approvals 30\nthe amount of bandwidth usage of a backing group used would hover above\n`30 * 4 * 3 = 360MB` per relay chain block. Given a backing group of 5\nthat gives 72MB per block per validator -> 12 MB/s.\n\n<details>\n<summary>Reality check on Kusama PoV sizes (click for chart)</summary>\n<br>\n<img width=\"697\" alt=\"Screenshot 2024-05-07 at 14 30 38\"\nsrc=\"https://github.com/paritytech/polkadot-sdk/assets/54316454/bfed32d4-8623-48b0-9ec0-8b95dd2a9d8c\">\n</details>\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-05-24T14:14:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f469fbfb0a44c4e223488b07ec641ca02b2fb8f1"
        },
        "date": 1716562986483,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19103945910000003,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.712022890066667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e192b764971f99975e876380f9ebbf2c08f0c17d",
          "message": "Avoid using `xcm::v4` and use latest instead for AssetHub benchmarks (#4567)",
          "timestamp": "2024-05-24T20:59:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e192b764971f99975e876380f9ebbf2c08f0c17d"
        },
        "date": 1716590183497,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.7198679797,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17248071203333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Svyatoslav Nikolsky",
            "username": "svyatonik",
            "email": "svyatonik@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f6cca7ee187d0946e4f3d1fa33928beacfce6e40",
          "message": "Bridge: check submit_finality_proof limits before submission (#4549)\n\ncloses https://github.com/paritytech/parity-bridges-common/issues/2982\ncloses https://github.com/paritytech/parity-bridges-common/issues/2730\n\nThe main change is in the\nbridges/relays/lib-substrate-relay/src/finality/target.rs, changes in\nother files are just moving the code\n\n~I haven't been able to run zn tests locally - don't know why, but it\nkeeps failing for me locally with: `\nError running script:\n/home/svyatonik/dev/polkadot-sdk/bridges/testing/framework/js-helpers/wait-hrmp-channel-opened.js\nError: Timeout(300), \"custom-js\n/home/svyatonik/dev/polkadot-sdk/bridges/testing/framework/js-helpers/wait-hrmp-channel-opened.js\nwithin 300 secs\" didn't complete on time.`~ The issue was an obsolete\n`polkadot-js-api` binary - did `yarn global upgrade` and it is ok now",
          "timestamp": "2024-05-27T07:23:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f6cca7ee187d0946e4f3d1fa33928beacfce6e40"
        },
        "date": 1716796371980,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.67797875913333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.188685635,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e0edb062e55e80cf21490fb140e4bbc3b7d7c89d",
          "message": "Add release version to commits and branch names of template synchronization job (#4353)\n\nJust to have some information what is the release number that was used\nto push a particular commit or PR in the templates repositories.",
          "timestamp": "2024-05-27T08:42:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e0edb062e55e80cf21490fb140e4bbc3b7d7c89d"
        },
        "date": 1716801961278,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16411549769999997,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.651756818399999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2352982717edc8976b55525274b1f9c9aa01aadd",
          "message": "Make markdown lint CI job pass (#4593)\n\nWas constantly failing, so here a fix.",
          "timestamp": "2024-05-27T09:39:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2352982717edc8976b55525274b1f9c9aa01aadd"
        },
        "date": 1716811033690,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17787338556666663,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.712298269633331,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "16887b6fd5ea637f3c2891d4a41180e9534e63db",
          "message": "chain-spec-builder: help updated (#4597)\n\nAdded some clarification on output file.",
          "timestamp": "2024-05-27T15:10:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/16887b6fd5ea637f3c2891d4a41180e9534e63db"
        },
        "date": 1716824732557,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.123122599666669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2941471433666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "70dd67a5d129745da6a05bce958824504a4c9d83",
          "message": "check-weight: Disable total pov size check for mandatory extrinsics (#4571)\n\nSo in some pallets we like\n[here](https://github.com/paritytech/polkadot-sdk/blob/5dc522d02fe0b53be1517f8b8979176e489a388b/substrate/frame/session/src/lib.rs#L556)\nwe use `max_block` as return value for `on_initialize` (ideally we would\nnot).\n\nThis means the block is already full when we try to apply the inherents,\nwhich lead to the error seen in #4559 because we are unable to include\nthe required inherents. This was not erroring before #4326 because we\nwere running into this branch:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/e4b89cc50c8d17868d6c8b122f2e156d678c7525/substrate/frame/system/src/extensions/check_weight.rs#L222-L224\n\nThe inherents are of `DispatchClass::Mandatory` and therefore have a\n`reserved` value of `None` in all runtimes I have inspected. So they\nwill always pass the normal check.\n\nSo in this PR I adjust the `check_combined_proof_size` to return an\nearly `Ok(())` for mandatory extrinsics.\n\nIf we agree on this PR I will backport it to the 1.12.0 branch.\n\ncloses #4559\n\n---------\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-05-27T17:12:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/70dd67a5d129745da6a05bce958824504a4c9d83"
        },
        "date": 1716835967109,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.64384096753333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16562262136666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a7097681b76bdaef21dcde9aec8c33205f480e44",
          "message": "[subsystem-benchmarks] Add statement-distribution benchmarks (#3863)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3748\n\nAdds a subsystem benchmark for statements-distribution subsystem.\n\nResults in CI (reference hw):\n```\n$ cargo bench -p polkadot-statement-distribution --bench statement-distribution-regression-bench --features subsystem-benchmarks\n\n[Sent to peers] standart_deviation 0.07%\n[Received from peers] standart_deviation 0.00%\n[statement-distribution] standart_deviation 0.97%\n[test-environment] standart_deviation 1.03%\n\nNetwork usage, KiB                     total   per block\nReceived from peers                1088.0000    108.8000\nSent to peers                      1238.1800    123.8180\n\nCPU usage, seconds                     total   per block\nstatement-distribution                0.3897      0.0390\ntest-environment                      0.4715      0.0472\n```",
          "timestamp": "2024-05-27T19:23:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a7097681b76bdaef21dcde9aec8c33205f480e44"
        },
        "date": 1716844102095,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.737868100066665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19285222126666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2d3a6932de35fc53da4e4b6bc195b1cc69550300",
          "message": "`sc-chain-spec`: deprecated code removed (#4410)\n\nThis PR removes deprecated code:\n- The `RuntimeGenesisConfig` generic type parameter in\n`GenericChainSpec` struct.\n- `ChainSpec::from_genesis` method allowing to create chain-spec using\nclosure providing runtime genesis struct\n- `GenesisSource::Factory` variant together with no longer needed\n`GenesisSource`'s generic parameter `G` (which was intended to be a\nruntime genesis struct).\n\n\nhttps://github.com/paritytech/polkadot-sdk/blob/17b56fae2d976a3df87f34076875de8c26da0355/substrate/client/chain-spec/src/chain_spec.rs#L559-L563",
          "timestamp": "2024-05-27T21:29:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2d3a6932de35fc53da4e4b6bc195b1cc69550300"
        },
        "date": 1716851435904,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.15570831219999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.6884085771,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3bf283ff22224e7713cf0c1b9878e9137dc6dbf7",
          "message": "[subsytem-bench] Remove redundant banchmark_name param (#4540)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/3601\n\nSince we print benchmark results manually, we don't need to save\nbenchmark_name anywhere, better just put the name inside `println!`.",
          "timestamp": "2024-05-28T08:51:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3bf283ff22224e7713cf0c1b9878e9137dc6dbf7"
        },
        "date": 1716888044904,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1890555179,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.650744687266663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6ed020037f4c2b6a6b542be6e5a15e86b0b7587b",
          "message": "[CI] Deny adding git deps (#4572)\n\nAdds a small CI check to match the existing Git deps agains a known-bad\nlist.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-05-28T11:23:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6ed020037f4c2b6a6b542be6e5a15e86b0b7587b"
        },
        "date": 1716901525136,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.21473280133333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.291844198266668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bolaji Ahmad",
            "username": "bolajahmad",
            "email": "56865496+bolajahmad@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "650b124fd81f4a438c212cb010cc0a730bac5c2d",
          "message": "Improve On_demand_assigner events (#4339)\n\ntitle: Improving `on_demand_assigner` emitted events\n\ndoc:\n  - audience: Rutime User\ndescription: OnDemandOrderPlaced event that is useful for indexers to\nsave data related to on demand orders. Check [discussion\nhere](https://substrate.stackexchange.com/questions/11366/ondemandassignmentprovider-ondemandorderplaced-event-was-removed/11389#11389).\n\nCloses #4254 \n\ncrates: [ 'runtime-parachain]\n\n---------\n\nCo-authored-by: Maciej <maciej.zyszkiewicz@parity.io>",
          "timestamp": "2024-05-28T14:44:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/650b124fd81f4a438c212cb010cc0a730bac5c2d"
        },
        "date": 1716912815667,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17819757779999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.923392966866663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2b1c606a338c80c5220c502c56a4b489f6d51488",
          "message": "parachain-inherent: Make `para_id` more prominent (#4555)\n\nThis should make it more obvious that at instantiation of the\n`MockValidationDataInherentDataProvider` the `para_id` needs to be\npassed.",
          "timestamp": "2024-05-28T16:08:31Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2b1c606a338c80c5220c502c56a4b489f6d51488"
        },
        "date": 1716919505152,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.03005464593333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21766130936666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d6cf147c1bda601e811bf5813b0d46ca1c8ad9b9",
          "message": "Filter workspace dependencies in the templates (#4599)\n\nThis detaches the templates from monorepo's workspace dependencies.\n\nCurrently the templates [re-use the monorepo's\ndependencies](https://github.com/paritytech/polkadot-sdk-minimal-template/blob/bd8afe66ec566d61f36b0e3d731145741a9e9e19/Cargo.toml#L45-L58),\nmost of which are not needed.\n\nThe simplest approach is to specify versions directly and not use\nworkspace dependencies in the templates.\n\nAnother approach would be to programmatically filter dependencies that\nare actually needed - but not sure if it's worth it, given that it would\ncomplicate the synchronization job.\n\ncc @kianenigma @gupnik",
          "timestamp": "2024-05-28T17:57:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d6cf147c1bda601e811bf5813b0d46ca1c8ad9b9"
        },
        "date": 1716925170404,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 14.170871531233335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20779268390000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5f68c93039fce08d7f711025eddc5343b0272111",
          "message": "Moves runtime macro out of experimental flag (#4249)\n\nStep in https://github.com/paritytech/polkadot-sdk/issues/3688\n\nNow that the `runtime` macro (Construct Runtime V2) has been\nsuccessfully deployed on Westend, this PR moves it out of the\nexperimental feature flag and makes it generally available for runtime\ndevs.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-29T03:41:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5f68c93039fce08d7f711025eddc5343b0272111"
        },
        "date": 1716959974940,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.896629547133333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17154987216666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "89604daa0f4244bc83782bd489918cfecb81a7d0",
          "message": "Add omni bencher & chain-spec-builder bins to release (#4557)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4354\n\nThis PR adds the steps to build and attach `frame-omni-bencher` and\n`chain-spec-builder` binaries to the release draft\n\n## TODO\n- [x] add also chain-spec-builder binary\n- [ ] ~~check/investigate Kian's comment: `chain spec builder. Ideally I\nwant it to match the version of the sp-genesis-builder crate`~~ see\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4518#issuecomment-2134731355)\n- [ ] Backport to `polkadot-sdk@1.11` release, so we can use it for next\nfellows release: https://github.com/polkadot-fellows/runtimes/pull/324\n- [ ] Backport to `polkadot-sdk@1.12` release\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-05-29T05:50:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/89604daa0f4244bc83782bd489918cfecb81a7d0"
        },
        "date": 1716967937949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.153210294666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24121880223333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dfcfa4ab37819fddb4278eaac306adc0f194fd27",
          "message": "Publish `chain-spec-builder` (#4518)\n\nmarking it as release-able, attaching the same version number that is\nattached to other binaries such as `polkadot` and `polkadot-parachain`.\n\nI have more thoughts about the version number, though. The chain-spec\nbuilder is mainly a user of the `sp-genesis-builder` api. So the\nversioning should be such that it helps users know give a version of\n`sp-genesis-builder` in their runtime, which version of\n`chain-spec-builder` should they use?\n\nWith this, we can possibly alter the version number to always match\n`sp-genesis-builder`.\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4352\n\n- [x] Add to release artifacts ~~similar to\nhttps://github.com/paritytech/polkadot-sdk/pull/4405~~ done here:\nhttps://github.com/paritytech/polkadot-sdk/pull/4557\n\n---------\n\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-05-29T08:34:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dfcfa4ab37819fddb4278eaac306adc0f194fd27"
        },
        "date": 1716977761795,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18491835803333337,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.83701780703333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Joshua Cheong",
            "username": "joshuacheong",
            "email": "jrc96@cantab.ac.uk"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aa32faaebf64426becb2feeede347740eb7a3908",
          "message": "Update README.md (#4623)\n\nMinor edit to a broken link for Rust Docs on the README.md\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-29T10:11:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aa32faaebf64426becb2feeede347740eb7a3908"
        },
        "date": 1716983796955,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17328813010000005,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.727062430233328,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d5053ac4161b6e3f634a3ffb6df07637058e9f55",
          "message": "Change `XcmDryRunApi::dry_run_extrinsic` to take a call instead (#4621)\n\nFollow-up to the new `XcmDryRunApi` runtime API introduced in\nhttps://github.com/paritytech/polkadot-sdk/pull/3872.\n\nTaking an extrinsic means the frontend has to sign first to dry-run and\nonce again to submit.\nThis is bad UX which is solved by taking an `origin` and a `call`.\nThis also has the benefit of being able to dry-run as any account, since\nit needs no signature.\n\nThis is a breaking change since I changed `dry_run_extrinsic` to\n`dry_run_call`, however, this API is still only on testnets.\nThe crates are bumped accordingly.\n\nAs a part of this PR, I changed the name of the API from `XcmDryRunApi`\nto just `DryRunApi`, since it can be used for general dry-running :)\n\nStep towards https://github.com/paritytech/polkadot-sdk/issues/690.\n\nExample of calling the API with PAPI, not the best code, just testing :)\n\n```ts\n// We just build a call, the arguments make it look very big though.\nconst call = localApi.tx.XcmPallet.transfer_assets({\n  dest: XcmVersionedLocation.V4({ parents: 0, interior: XcmV4Junctions.X1(XcmV4Junction.Parachain(1000)) }),\n  beneficiary: XcmVersionedLocation.V4({ parents: 0, interior: XcmV4Junctions.X1(XcmV4Junction.AccountId32({ network: undefined, id: Binary.fromBytes(encodeAccount(account.address)) })) }),\n  weight_limit: XcmV3WeightLimit.Unlimited(),\n  assets: XcmVersionedAssets.V4([{\n    id: { parents: 0, interior: XcmV4Junctions.Here() },\n    fun: XcmV3MultiassetFungibility.Fungible(1_000_000_000_000n) }\n  ]),\n  fee_asset_item: 0,\n});\n// We call the API passing in a signed origin \nconst result = await localApi.apis.XcmDryRunApi.dry_run_call(\n  WestendRuntimeOriginCaller.system(DispatchRawOrigin.Signed(account.address)),\n  call.decodedCall\n);\nif (result.success && result.value.execution_result.success) {\n  // We find the forwarded XCM we want. The first one going to AssetHub in this case.\n  const xcmsToAssetHub = result.value.forwarded_xcms.find(([location, _]) => (\n    location.type === \"V4\" &&\n      location.value.parents === 0 &&\n      location.value.interior.type === \"X1\"\n      && location.value.interior.value.type === \"Parachain\"\n      && location.value.interior.value.value === 1000\n  ))!;\n\n  // We can even find the delivery fees for that forwarded XCM.\n  const deliveryFeesQuery = await localApi.apis.XcmPaymentApi.query_delivery_fees(xcmsToAssetHub[0], xcmsToAssetHub[1][0]);\n\n  if (deliveryFeesQuery.success) {\n    const amount = deliveryFeesQuery.value.type === \"V4\" && deliveryFeesQuery.value.value[0].fun.type === \"Fungible\" && deliveryFeesQuery.value.value[0].fun.value.valueOf() || 0n;\n    // We store them in state somewhere.\n    setDeliveryFees(formatAmount(BigInt(amount)));\n  }\n}\n```\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-29T19:57:17Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d5053ac4161b6e3f634a3ffb6df07637058e9f55"
        },
        "date": 1717018814627,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19755990123333336,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.898098487333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4ab078d6754147ce731523292dd1882f8a7b5775",
          "message": "pallet-staking: Put tests behind `cfg(debug_assertions)` (#4620)\n\nOtherwise these tests are failing if you don't run with\n`debug_assertions` enabled, which happens if you run tests locally in\nrelease mode.",
          "timestamp": "2024-05-29T21:23:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4ab078d6754147ce731523292dd1882f8a7b5775"
        },
        "date": 1717023805650,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.150855565900002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23331318046666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jeeyong Um",
            "username": "conr2d",
            "email": "conr2d@proton.me"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d539778c3cc4e0376769472fdad37856f4051dc5",
          "message": "Fix broken windows build (#4636)\n\nFixes #4625.\n\nSpecifically, the `cfg` attribute `windows` refers to the compile target\nand not the build environment, and in the case of cross-compilation, the\nbuild environment and target can differ. However, the line modified is\nrelated to documentation generation, so there should be no critical\nissue with this change.",
          "timestamp": "2024-05-30T10:44:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d539778c3cc4e0376769472fdad37856f4051dc5"
        },
        "date": 1717068183537,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.96428641893333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19988337846666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "78c24ec9e24ea04b2f8513b53a8d1246ff6b35ed",
          "message": "Adds ability to specify chain type in chain-spec-builder (#4542)\n\nCurrently, `chain-spec-builder` only creates a spec with `Live` chain\ntype. This PR adds the ability to specify it while keeping the same\ndefault.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-05-31T02:09:12Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/78c24ec9e24ea04b2f8513b53a8d1246ff6b35ed"
        },
        "date": 1717127191040,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.842454155466669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1825572453,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "71f4f5a80bb9ef00d651c62a58c6e8192d4d9707",
          "message": "Update `runtime_type` ref doc with the new \"Associated Type Bounds\" (#4624)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-31T04:58:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/71f4f5a80bb9ef00d651c62a58c6e8192d4d9707"
        },
        "date": 1717137218760,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1926279451666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.940070551666668,
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
          "id": "0ae721970909efc3b2a049632c9c904d9fa4fed1",
          "message": "collator-protocol: remove `elastic-scaling-experimental` feature (#4595)\n\nValidators already have been upgraded so they could already receive the\nnew `CollationWithParentHeadData` response when fetching collation.\nHowever this is only sent by collators when the parachain has more than\n1 core is assigned.\n\nTODO:\n- [x] PRDoc\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-05-31T06:34:43Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0ae721970909efc3b2a049632c9c904d9fa4fed1"
        },
        "date": 1717143393702,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1832691027333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.908608508666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8d8c0e13a7dc8d067367ac55fb142b12ac8a6d13",
          "message": "Use Unlicense for templates (#4628)\n\nAddresses\n[this](https://github.com/paritytech/polkadot-sdk/issues/3155#issuecomment-2134411391).",
          "timestamp": "2024-05-31T10:15:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8d8c0e13a7dc8d067367ac55fb142b12ac8a6d13"
        },
        "date": 1717156900376,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.012786687366667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19357397536666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "fc6c31829fc2e24e11a02b6a2adec27bc5d8918f",
          "message": "Implement `XcmPaymentApi` and `DryRunApi` on all system parachains (#4634)\n\nDepends on https://github.com/paritytech/polkadot-sdk/pull/4621.\n\nImplemented the\n[`XcmPaymentApi`](https://github.com/paritytech/polkadot-sdk/pull/3607)\nand [`DryRunApi`](https://github.com/paritytech/polkadot-sdk/pull/3872)\non all system parachains.\n\nMore scenarios can be tested on both rococo and westend if all system\nparachains implement this APIs.\nThe objective is for all XCM-enabled runtimes to implement them.\nAfter demonstrating fee estimation in a UI on the testnets, come the\nfellowship runtimes.\n\nStep towards https://github.com/paritytech/polkadot-sdk/issues/690.",
          "timestamp": "2024-05-31T15:38:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/fc6c31829fc2e24e11a02b6a2adec27bc5d8918f"
        },
        "date": 1717176085889,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.887552562000002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1753202740333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "nikhilgupta.iitk@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f81751e0ce56b0ef50b3a0b5aa0ff4fb16c9ea37",
          "message": "Better error for missing index in CRV2 (#4643)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4552\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-06-02T18:39:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f81751e0ce56b0ef50b3a0b5aa0ff4fb16c9ea37"
        },
        "date": 1717360134060,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.24672424503333334,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.1400851085,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "tugy",
            "username": "tugytur",
            "email": "33746108+tugytur@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5779ec5b775f86fb86be02783ab5c02efbf307ca",
          "message": "update amforc westend and its parachain bootnodes (#4641)\n\nTested each bootnode with `--reserved-only --reserved-nodes`",
          "timestamp": "2024-06-02T20:11:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5779ec5b775f86fb86be02783ab5c02efbf307ca"
        },
        "date": 1717365194346,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.145689257899999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.22913618700000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f66e693a6befef0956a3129254fbe568247c9c57",
          "message": "Add chain-spec-builder docker image (#4655)\n\nThis PR adds possibility to publish container images for the\n`chain-spec-builder` binary on the regular basis.\nRelated to: https://github.com/paritytech/release-engineering/issues/190",
          "timestamp": "2024-06-03T08:30:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f66e693a6befef0956a3129254fbe568247c9c57"
        },
        "date": 1717409807260,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2464471947666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.2495749954,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6f228e7d220bb14c113dcc27c931590737f9d0ab",
          "message": " [ci] Delete unused flow (#4676)",
          "timestamp": "2024-06-03T12:22:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6f228e7d220bb14c113dcc27c931590737f9d0ab"
        },
        "date": 1717419289375,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.872284858733334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17415657423333333,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "cbe45121c9a7bb956101bf28e6bb23f0efd3cbbf",
          "message": "[ci] Increase timeout for check-runtime-migration workflow (#4674)\n\n`[check-runtime-migration` now takes more than 30 minutes. Quick fix\nwith increased timeout.",
          "timestamp": "2024-06-03T13:42:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cbe45121c9a7bb956101bf28e6bb23f0efd3cbbf"
        },
        "date": 1717428328351,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.899960732600004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17121180273333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Miasojed",
            "username": "smiasojed",
            "email": "s.miasojed@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3e8416456a27197bd1bbb7fc8149d941d9167f4d",
          "message": "Add READ_ONLY flag to contract call function  (#4418)\n\nThis PR implements the `READ_ONLY` flag to be used as a `Callflag` in\nthe `call` function.\nThe flag indicates that the callee is restricted from modifying the\nstate during call execution.\nIt is equivalent to Ethereum's\n[STATICCALL](https://eips.ethereum.org/EIPS/eip-214).\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Andrew Jones <ascjones@gmail.com>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-06-04T07:58:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3e8416456a27197bd1bbb7fc8149d941d9167f4d"
        },
        "date": 1717490040391,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16143456536666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.84447419096667,
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
          "id": "a09ec64d149b400de16f144ca02f0fa958d2bb13",
          "message": "Forward put_record requests to authorithy-discovery (#4683)\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-04T10:05:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a09ec64d149b400de16f144ca02f0fa958d2bb13"
        },
        "date": 1717501534112,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.820012705833335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1762741477,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9b76492302f7184ff00bd6141c9b4163611e9d45",
          "message": "Use `parachain_info` in cumulus-test-runtime (#4672)\n\nThis allows to use custom para_ids with cumulus-test-runtime. \n\nZombienet is patching the genesis entries for `ParachainInfo`. This did\nnot work with `test-parachain` because it was using the `test_pallet`\nfor historic reasons I guess.",
          "timestamp": "2024-06-04T13:32:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9b76492302f7184ff00bd6141c9b4163611e9d45"
        },
        "date": 1717513830944,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.872428000299996,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18698870890000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "42ddb5b06578f6c2b2fc469de5161035b04fc79a",
          "message": "`chain-spec`/presets reference docs added (#4678)\n\nAdded reference doc about:\n- the pallet genesis config and genesis build, \n- runtime `genesis-builder` API,\n- presets,\n- interacting with the `chain-spec-builder` tool\n\nI've added [minimal\nruntime](https://github.com/paritytech/polkadot-sdk/tree/mku-chain-spec-guide/docs/sdk/src/reference_docs/chain_spec_runtime)\nto demonstrate above topics.\n\nI also sneaked in some little improvement to `chain-spec-builder` which\nallows to parse output of the `list-presets` command.\n\n---------\n\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-04T18:04:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/42ddb5b06578f6c2b2fc469de5161035b04fc79a"
        },
        "date": 1717530432611,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.892162011700004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20428361193333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "georgepisaltu",
            "username": "georgepisaltu",
            "email": "52418509+georgepisaltu@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3977f389cce4a00fd7100f95262e0563622b9aa4",
          "message": "[Identity] Remove double encoding username signature payload (#4646)\n\nIn order to receive a username in `pallet-identity`, users have to,\namong other things, provide a signature of the desired username. Right\nnow, there is an [extra encoding\nstep](https://github.com/paritytech/polkadot-sdk/blob/4ab078d6754147ce731523292dd1882f8a7b5775/substrate/frame/identity/src/lib.rs#L1119)\nwhen generating the payload to sign.\n\nEncoding a `Vec` adds extra bytes related to the length, which changes\nthe payload. This is unnecessary and confusing as users expect the\npayload to sign to be just the username bytes. This PR fixes this issue\nby validating the signature directly against the username bytes.\n\n---------\n\nSigned-off-by: georgepisaltu <george.pisaltu@parity.io>",
          "timestamp": "2024-06-05T07:38:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3977f389cce4a00fd7100f95262e0563622b9aa4"
        },
        "date": 1717579291906,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1880765814333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.869214824133337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Przemek Rzad",
            "username": "rzadp",
            "email": "przemek@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "8ffe22903a37f4dab2aa1b15ec899f2c38439f60",
          "message": "Update the `polkadot_builder` Dockerfile (#4638)\n\nThis Dockerfile seems outdated - it currently fails to build (on my\nmachine).\nI don't see it being built anywhere on CI.\n\nI did a couple of tweaks to make it build.\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>",
          "timestamp": "2024-06-05T09:55:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/8ffe22903a37f4dab2aa1b15ec899f2c38439f60"
        },
        "date": 1717587445804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.870008936166665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17141300943333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Michal Kucharczyk",
            "username": "michalkucharczyk",
            "email": "1728078+michalkucharczyk@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f65beb7f7a66a79f4afd0a308bece2bd5c8ba780",
          "message": "chain-spec-doc: some minor fixes (#4700)\n\nsome minor text fixes.",
          "timestamp": "2024-06-05T11:52:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f65beb7f7a66a79f4afd0a308bece2bd5c8ba780"
        },
        "date": 1717594440793,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.756374216133334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15906308036666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d2fd53645654d3b8e12cbf735b67b93078d70113",
          "message": "Unify dependency aliases (#4633)\n\nInherited workspace dependencies cannot be renamed by the crate using\nthem (see [1](https://github.com/rust-lang/cargo/issues/12546),\n[2](https://stackoverflow.com/questions/76792343/can-inherited-dependencies-in-rust-be-aliased-in-the-cargo-toml-file)).\nSince we want to use inherited workspace dependencies everywhere, we\nfirst need to unify all aliases that we use for a dependency throughout\nthe workspace.\nThe umbrella crate is currently excluded from this procedure, since it\nshould be able to export the crates by their original name without much\nhassle.\n\nFor example: one crate may alias `parity-scale-codec` to `codec`, while\nanother crate does not alias it at all. After this change, all crates\nhave to use `codec` as name. The problematic combinations were:\n- conflicting aliases: most crates aliases as `A` but some use `B`.\n- missing alias: most of the crates alias a dep but some dont.\n- superfluous alias: most crates dont alias a dep but some do.\n\nThe script that i used first determines whether most crates opted to\nalias a dependency or not. From that info it decides whether to use an\nalias or not. If it decided to use an alias, the most common one is used\neverywhere.\n\nTo reproduce, i used\n[this](https://github.com/ggwpez/substrate-scripts/blob/master/uniform-crate-alias.py)\npython script in combination with\n[this](https://github.com/ggwpez/zepter/blob/38ad10585fe98a5a86c1d2369738bc763a77057b/renames.json)\nerror output from Zepter.\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-05T13:54:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d2fd53645654d3b8e12cbf735b67b93078d70113"
        },
        "date": 1717604577911,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19024902973333332,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.957775946933333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2460cddf57660a88844d201f769eb17a7accce5a",
          "message": "fix build on MacOS: bump secp256k1 and secp256k1-sys to patched versions (#4709)\n\n`secp256k1 v0.28.0` and `secp256k1-sys v0.9.0` were yanked because\nbuilding them fails for `aarch64-apple-darwin` targets.\n\nUse the `secp256k1 v0.28.2` and `secp256k1-sys v0.9.2` patched versions\nthat build fine on ARM chipset MacOS.",
          "timestamp": "2024-06-05T18:14:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2460cddf57660a88844d201f769eb17a7accce5a"
        },
        "date": 1717617756880,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.9891003872,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18252621333333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Oliver Tale-Yazdi",
            "username": "ggwpez",
            "email": "oliver.tale-yazdi@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583",
          "message": "[CI] Delete cargo-deny config (#4677)\n\nNobody seems to maintain this and the job is disabled since months. I\nthink unless the Security team wants to pick this up we delete it for\nnow.\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-06T14:48:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5fb4c40a3ea24ae3ab2bdfefb3f3a40badc2a583"
        },
        "date": 1717691922804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1925098628,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.9961593546,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "494448b7fed02e098fbf38bad517d9245b056d1d",
          "message": "Cleanup PVF artifact by cache limit and stale time (#4662)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4324\nWe don't change but extend the existing cleanup strategy. \n- We still don't touch artifacts being stale less than 24h\n- First time we attempt pruning only when we hit cache limit (10 GB)\n- If somehow happened that after we hit 10 GB and least used artifact is\nstale less than 24h we don't remove it.\n\n---------\n\nCo-authored-by: s0me0ne-unkn0wn <48632512+s0me0ne-unkn0wn@users.noreply.github.com>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-06-06T19:22:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/494448b7fed02e098fbf38bad517d9245b056d1d"
        },
        "date": 1717705273721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16130551669999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.9188485868,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "426956f87cc91f94ce71e2ed74ca34d88766e1d8",
          "message": "Update the README to include a link to the Polkadot SDK Version Manager (#4718)\n\nAdds a link to the [Polkadot SDK Version\nManager](https://github.com/paritytech/psvm) since this tool is not well\nknown, but very useful for developers using the Polkadot SDK.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-06T20:06:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/426956f87cc91f94ce71e2ed74ca34d88766e1d8"
        },
        "date": 1717707863043,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.743749400966669,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16447173199999998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "426956f87cc91f94ce71e2ed74ca34d88766e1d8",
          "message": "Update the README to include a link to the Polkadot SDK Version Manager (#4718)\n\nAdds a link to the [Polkadot SDK Version\nManager](https://github.com/paritytech/psvm) since this tool is not well\nknown, but very useful for developers using the Polkadot SDK.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-06T20:06:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/426956f87cc91f94ce71e2ed74ca34d88766e1d8"
        },
        "date": 1717710531657,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17326698656666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.883895945966666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d783ca9d9bfb42ae938f8d4ce9899b6aa3cc00c6",
          "message": "New reference doc for Custom RPC V2 (#4654)\n\nThanks for @xlc for the original seed info, I've just fixed it up a bit\nand added example links.\n\nI've moved the comparison between eth-rpc-api and frontier outside, as\nit is opinionation. I think the content there was good but should live\nin the README of the corresponding repos. No strong opinion, happy\neither way.\n\n---------\n\nCo-authored-by: Bryan Chen <xlchen1291@gmail.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Gonçalo Pestana <g6pestana@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-07T11:26:52Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d783ca9d9bfb42ae938f8d4ce9899b6aa3cc00c6"
        },
        "date": 1717766745432,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16891148489999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.738936997333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "PG Herveou",
            "username": "pgherveou",
            "email": "pgherveou@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "48d875d0e60c6d5e4c0c901582cc8edfb76f2f42",
          "message": "Contracts:  update wasmi to 0.32 (#3679)\n\ntake over #2941 \n[Weights\ncompare](https://weights.tasty.limo/compare?unit=weight&ignore_errors=true&threshold=10&method=asymptotic&repo=polkadot-sdk&old=master&new=pg%2Fwasmi-to-v0.32.0-beta.7&path_pattern=substrate%2Fframe%2F**%2Fsrc%2Fweights.rs%2Cpolkadot%2Fruntime%2F*%2Fsrc%2Fweights%2F**%2F*.rs%2Cpolkadot%2Fbridges%2Fmodules%2F*%2Fsrc%2Fweights.rs%2Ccumulus%2F**%2Fweights%2F*.rs%2Ccumulus%2F**%2Fweights%2Fxcm%2F*.rs%2Ccumulus%2F**%2Fsrc%2Fweights.rs)\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Alexander Theißen <alex.theissen@me.com>",
          "timestamp": "2024-06-07T14:40:10Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/48d875d0e60c6d5e4c0c901582cc8edfb76f2f42"
        },
        "date": 1717777689368,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.877939776933331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19022879846666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "07cfcf0b3c9df971c673162b9d16cb5c17fbe97d",
          "message": "frame/proc-macro: Refactor code for better readability (#4712)\n\nSmall refactoring PR to improve the readability of the proc macros.\n- small improvement in docs\n- use new `let Some(..) else` expression\n- removed extra indentations by early returns\n\nDiscovered during metadata v16 poc, extracted from:\nhttps://github.com/paritytech/polkadot-sdk/pull/4358\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>\nCo-authored-by: gupnik <mail.guptanikhil@gmail.com>",
          "timestamp": "2024-06-08T07:48:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/07cfcf0b3c9df971c673162b9d16cb5c17fbe97d"
        },
        "date": 1717838909383,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.841941453799999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1814288942666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "batman",
            "username": "iammasterbrucewayne",
            "email": "iammasterbrucewayne@protonmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "cdb297b15ad9c1d952c0501afaf6b764e5fd147c",
          "message": "Update README.md to move the PSVM link under a \"Tooling\" section under the \"Releases\" section (#4734)\n\nThis update implements the suggestion from @kianenigma mentioned in\nhttps://github.com/paritytech/polkadot-sdk/pull/4718#issuecomment-2153777367\n\nReplaces the \"Other useful resources and tooling\" section at the bottom\nwith a new (nicer) \"Tooling\" section just under the \"Releases\" section.",
          "timestamp": "2024-06-08T11:37:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/cdb297b15ad9c1d952c0501afaf6b764e5fd147c"
        },
        "date": 1717852734416,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.7455857503,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15243267716666667,
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
          "id": "2869fd6aba61f429ea2c006c2aae8dd5405dc5aa",
          "message": "approval-voting: Add no shows debug information (#4726)\n\nAdd some debug logs to be able to identify the validators and parachains\nthat have most no-shows, this metric is valuable because it will help us\nidentify validators and parachains that regularly have this problem.\n\nFrom the validator_index we can then query the on-chain information and\nidentify the exact validator that is causing the no-shows.\n\n---------\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-10T09:44:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2869fd6aba61f429ea2c006c2aae8dd5405dc5aa"
        },
        "date": 1718018541712,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.830473221833335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17110966156666668,
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
          "id": "b65313e81465dd730e48d4ce00deb76922618375",
          "message": "Remove unncessary call remove_from_peers_set (#4742)\n\n... this is superfluous because set_reserved_peers implementation\nalready calls this method here:\n\nhttps://github.com/paritytech/polkadot-sdk/blob/cdb297b15ad9c1d952c0501afaf6b764e5fd147c/substrate/client/network/src/protocol_controller.rs#L571,\nso the call just ends producing this warnings whenever we manipulate the\npeers set.\n\n```\nTrying to remove unknown reserved node 12D3KooWRCePWvHoBbz9PSkw4aogtdVqkVDhiwpcHZCqh4hdPTXC from SetId(3)\npeerset warnings (from different peers)\n```\n\nSigned-off-by: Alexandru Gheorghe <alexandru.gheorghe@parity.io>",
          "timestamp": "2024-06-10T12:54:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b65313e81465dd730e48d4ce00deb76922618375"
        },
        "date": 1718030077754,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.8816023434,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1779183513,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "96ab6869bafb06352b282576a6395aec8e9f2705",
          "message": "finalization: Skip tree route calculation if no forks present (#4721)\n\n## Issue\n\nCurrently, syncing parachains from scratch can lead to a very long\nfinalization time once they reach the tip of the chain. The problem is\nthat we try to finalize everything from 0 to the tip, which can be\nthousands or even millions of blocks.\n\nWe finalize sequentially and try to compute displaced branches during\nfinalization. So for every block on the way, we compute an expensive\ntree route.\n\n## Proposed Improvements\n\nIn this PR, I propose improvements that solve this situation:\n\n- **Skip tree route calculation if `leaves().len() == 1`:** This should\nbe enough for 90% of cases where there is only one leaf after sync.\n- **Optimize finalization for long distances:** It can happen that the\nparachain has imported some leaf and then receives a relay chain\nnotification with the finalized block. In that case, the previous\noptimization will not trigger. A second mechanism should ensure that we\ndo not need to compute the full tree route. If the finalization distance\nis long, we check the lowest common ancestor of all the leaves. If it is\nabove the to-be-finalized block, we know that there are no displaced\nleaves. This is fast because forks are short and close to the tip, so we\ncan leverage the header cache.\n\n## Alternative Approach\n\n- The problem was introduced in #3962. Reverting that PR is another\npossible strategy.\n- We could store for every fork where it begins, however sounds a bit\nmore involved to me.\n\n\nfixes #4614",
          "timestamp": "2024-06-11T13:02:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/96ab6869bafb06352b282576a6395aec8e9f2705"
        },
        "date": 1718116996566,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20594859139999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.936707524066671,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "cheme",
            "username": "cheme",
            "email": "emericchevalier.pro@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad8620922bd7c0477b25c7dfd6fc233641cb27ae",
          "message": "Append overlay optimization. (#1223)\n\nThis branch propose to avoid clones in append by storing offset and size\nin previous overlay depth.\nThat way on rollback we can just truncate and change size of existing\nvalue.\nTo avoid copy it also means that :\n\n- append on new overlay layer if there is an existing value: create a\nnew Append entry with previous offsets, and take memory of previous\noverlay value.\n- rollback on append: restore value by applying offsets and put it back\nin previous overlay value\n- commit on append: appended value overwrite previous value (is an empty\nvec as the memory was taken). offsets of commited layer are dropped, if\nthere is offset in previous overlay layer they are maintained.\n- set value (or remove) when append offsets are present: current\nappended value is moved back to previous overlay value with offset\napplied and current empty entry is overwrite (no offsets kept).\n\nThe modify mechanism is not needed anymore.\nThis branch lacks testing and break some existing genericity (bit of\nduplicated code), but good to have to check direction.\n\nGenerally I am not sure if it is worth or we just should favor\ndifferents directions (transients blob storage for instance), as the\ncurrent append mechanism is a bit tricky (having a variable length in\nfirst position means we sometime need to insert in front of a vector).\n\nFix #30.\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: EgorPopelyaev <egor@parity.io>\nCo-authored-by: Alexandru Vasile <60601340+lexnv@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Bastian Köcher <info@kchr.de>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-11T22:15:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad8620922bd7c0477b25c7dfd6fc233641cb27ae"
        },
        "date": 1718150238786,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.923715275200001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17326682760000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c4aa2ab642419e6751400a6aabaf5df611a4ea37",
          "message": "Hide `tuplex` dependency and re-export by macro (#4774)\n\nAddressing comment:\nhttps://github.com/paritytech/polkadot-sdk/pull/4102/files#r1635502496\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-12T14:38:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c4aa2ab642419e6751400a6aabaf5df611a4ea37"
        },
        "date": 1718209118111,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 15.148528015666665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.3044420112999999,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "eca1052ea1eddeede91da8f9f7452ea8b57e7942",
          "message": "Update the pallet guide in `sdk-docs` (#4735)\n\nAfter using this tutorial in PBA, there was a few areas to improve it.\nMoreover, I have:\n\n- Improve `your_first_pallet`, link it in README, improve the parent\n`guide` section.\n- Updated the templates page, in light of recent efforts related to in\nhttps://github.com/paritytech/polkadot-sdk/issues/3155\n- Added small ref docs about metadata, completed the one about native\nruntime, added one about host functions.\n- Remove a lot of unfinished stuff from sdk-docs\n- update diagram for `Hooks`",
          "timestamp": "2024-06-13T02:36:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/eca1052ea1eddeede91da8f9f7452ea8b57e7942"
        },
        "date": 1718253390490,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.969226591633335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20023366533333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "988103d7578ad515b13c69578da1237b28fa9f36",
          "message": "Use aggregated types for `RuntimeFreezeReason` and better examples of `MaxFreezes` (#4615)\n\nThis PR aligns the settings for `MaxFreezes`, `RuntimeFreezeReason`, and\n`FreezeIdentifier`.\n\n#### Future work and improvements\nhttps://github.com/paritytech/polkadot-sdk/issues/2997 (remove\n`MaxFreezes` and `FreezeIdentifier`)",
          "timestamp": "2024-06-13T08:44:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/988103d7578ad515b13c69578da1237b28fa9f36"
        },
        "date": 1718275249804,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18823820013333334,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.860400892299998,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7b6b783cd1a3953ef5fa6e53f3965b1454e3efc8",
          "message": "[Backport] Version bumps and prdoc reorg from 1.13.0 (#4784)\n\nThis PR backports regular version bumps and prdocs reordering from the\nrelease branch back to master",
          "timestamp": "2024-06-13T16:27:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7b6b783cd1a3953ef5fa6e53f3965b1454e3efc8"
        },
        "date": 1718302163389,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20397591509999996,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.978985275533333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7f7f5fa857502b6e3649081abb6b53c3512bfedb",
          "message": "`polkadot-parachain-bin`: small cosmetics and improvements (#4666)\n\nRelated to: https://github.com/paritytech/polkadot-sdk/issues/5\n\nA couple of cosmetics and improvements related to\n`polkadot-parachain-bin`:\n\n- Adding some convenience traits in order to avoid declaring long\nduplicate bounds\n- Specifically check if the runtime exposes `AuraApi` when executing\n`start_lookahead_aura_consensus()`\n- Some fixes for the `RelayChainCli`. Details in the commits description",
          "timestamp": "2024-06-14T06:29:04Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7f7f5fa857502b6e3649081abb6b53c3512bfedb"
        },
        "date": 1718352882949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.893818158633334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20061393490000007,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "977254ccb1afca975780987ff9f19f356e99378f",
          "message": "Bridges - changes for Bridges V2 - relay client part (#4494)\n\nContains mainly changes/nits/refactors related to the relayer code\n(`client-substrate` and `lib-substrate-relay`) migrated from the Bridges\nV2 [branch](https://github.com/paritytech/polkadot-sdk/pull/4427).\n\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2976\nCompanion: https://github.com/paritytech/parity-bridges-common/pull/2988\n\n\n## TODO\n- [x] fix comments\n\n## Questions\n- [x] Do we need more testing for client V2 stuff? If so, how/what is\nthe ultimate test? @svyatonik\n- [x] check\n[comment](https://github.com/paritytech/polkadot-sdk/pull/4494#issuecomment-2117181144)\nfor more testing\n\n---------\n\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: Serban Iorga <serban@parity.io>",
          "timestamp": "2024-06-14T11:30:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/977254ccb1afca975780987ff9f19f356e99378f"
        },
        "date": 1718370822947,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19397533646666668,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.038507396066668,
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
          "id": "ae0b3bf6733e7b9e18badb16128a6b25bef1923b",
          "message": "CheckWeight: account for extrinsic len as proof size (#4765)\n\nFix https://github.com/paritytech/polkadot-sdk/issues/4743 which allows\nus to remove the defensive limit on pov size in Cumulus after relay\nchain gets upgraded with these changes. Also add unit test to ensure\n`CheckWeight` - `StorageWeightReclaim` integration works.\n\nTODO:\n- [x] PRDoc\n- [x] Add a len to all the other tests in storage weight reclaim and\ncall `CheckWeight::pre_dispatch`\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-06-14T12:42:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ae0b3bf6733e7b9e18badb16128a6b25bef1923b"
        },
        "date": 1718375513474,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2658007621,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.290467444733334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2f643816d79a76155aec790a35b9b72a5d8bb726",
          "message": "add ref doc for logging practices in FRAME (#4768)",
          "timestamp": "2024-06-17T03:31:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f643816d79a76155aec790a35b9b72a5d8bb726"
        },
        "date": 1718601113989,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.7554228767,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16897729526666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d91cbbd453c1d4553d7e3dc8753a2007fc4c5a67",
          "message": "Impl and use default config for pallet-staking in tests (#4797)",
          "timestamp": "2024-06-17T12:35:15Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d91cbbd453c1d4553d7e3dc8753a2007fc4c5a67"
        },
        "date": 1718630957763,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.7551773866,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1704034014666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tom Mi",
            "username": "hitchhooker",
            "email": "tommi@niemi.lol"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6cb3bd23910ec48ab37a3c95a6b03286ff2979bf",
          "message": "Ibp bootnodes for Kusama People (#6) (#4741)\n\n* fix rotko's pcollectives bootnode\n* Update people-kusama.json\n* Add Dwellir People Kusama bootnode\n* add Gatotech bootnodes to `people-kusama`\n* Add Dwellir People Kusama bootnode\n* Update Amforc bootnodes for Kusama and Polkadot (#4668)\n\n---------\n\nCo-authored-by: RadiumBlock <info@radiumblock.com>\nCo-authored-by: Jonathan Udd <jonathan@dwellir.com>\nCo-authored-by: Milos Kriz <milos_kriz@hotmail.com>\nCo-authored-by: tugy <33746108+tugytur@users.noreply.github.com>\nCo-authored-by: Kutsal Kaan Bilgin <kutsalbilgin@gmail.com>\nCo-authored-by: Petr Mensik <petr.mensik1@gmail.com>\nCo-authored-by: Tommi <tommi@romeblockchain>",
          "timestamp": "2024-06-17T15:11:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6cb3bd23910ec48ab37a3c95a6b03286ff2979bf"
        },
        "date": 1718643724038,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17215800846666668,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.785578088266668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Florian Franzen",
            "username": "FlorianFranzen",
            "email": "Florian.Franzen@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5055294521021c0ffa1c449d6793ec9d264e5bd5",
          "message": "node-inspect: do not depend on rocksdb (#4783)\n\nThe crate `sc-cli` otherwise enables the `rocksdb` feature.",
          "timestamp": "2024-06-17T18:47:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5055294521021c0ffa1c449d6793ec9d264e5bd5"
        },
        "date": 1718656573552,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.650665220233336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17088862596666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kantapat chankasem",
            "username": "tesol2y090",
            "email": "gliese090@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "55a13abcd2f67e7fdfc8843f5c4a54798e26a9df",
          "message": "remove pallet::getter usage from pallet-timestamp (#3374)\n\nthis pr is a part of #3326\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-17T22:30:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/55a13abcd2f67e7fdfc8843f5c4a54798e26a9df"
        },
        "date": 1718669585217,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.6870126554,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18276585003333334,
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
          "id": "1dc68de8eec934b3c7f35a330f869d1172943da4",
          "message": "glutton: also increase parachain block length (#4728)\n\nGlutton currently is useful mostly for stress testing relay chain\nvalidators. It is unusable for testing the collator networking and block\nannouncement and import scenarios. This PR resolves that by improving\nglutton pallet to also buff up the blocks, up to the runtime configured\n`BlockLength`.\n\n### How it works\nIncludes an additional inherent in each parachain block. The `garbage`\nargument passed to the inherent is filled with trash data. It's size is\ncomputed by applying the newly introduced `block_length` percentage to\nthe maximum block length for mandatory dispatch class. After\nhttps://github.com/paritytech/polkadot-sdk/pull/4765 is merged, the\nlength of inherent extrinsic will be added to the total block proof\nsize.\n\nThe remaining weight is burnt in `on_idle` as configured by the\n`storage` percentage parameter.\n\n\nTODO:\n- [x] PRDoc\n- [x] Readme update\n- [x] Add tests\n\n---------\n\nSigned-off-by: Andrei Sandu <andrei-mihail@parity.io>",
          "timestamp": "2024-06-18T08:57:57Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1dc68de8eec934b3c7f35a330f869d1172943da4"
        },
        "date": 1718703700894,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1681526961,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.845413473533336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6daa939bc7c3f26c693a876d5a4b7ea00c6b2d7f",
          "message": "Migrated commands to github actions (#4701)\n\nMigrated commands individually to work as GitHub actions with a\n[`workflow_dispatch`](https://docs.github.com/en/actions/using-workflows/events-that-trigger-workflows#workflow_dispatch)\nevent.\n\nThis will not disable the command-bot yet, but it's the first step\nbefore disabling it.\n\n### Commands migrated\n- [x] bench-all\n- [x] bench-overhead\n- [x] bench\n- [x] fmt\n- [x] update-ui\n\nAlso created an action that will inform users about the new\ndocumentation when they comment `bot`.\n\n### Created documentation \nCreated a detailed documentation on how to use this action. Found the\ndocumentation\n[here](https://github.com/paritytech/polkadot-sdk/blob/bullrich/cmd-action/.github/commands-readme.md).\n\n---------\n\nCo-authored-by: Alexander Samusev <41779041+alvicsam@users.noreply.github.com>\nCo-authored-by: Przemek Rzad <przemek@parity.io>",
          "timestamp": "2024-06-18T13:12:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6daa939bc7c3f26c693a876d5a4b7ea00c6b2d7f"
        },
        "date": 1718719120799,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.8675659598,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20282814790000003,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6c857609a9425902d6dfe5445afb16c6b23ad86c",
          "message": "rpc server: add `health/readiness endpoint` (#4802)\n\nPrevious attempt https://github.com/paritytech/substrate/pull/14314\n\nClose #4443 \n\nIdeally, we should move /health and /health/readiness to the prometheus\nserver but because it's was quite easy to implement on the RPC server\nand that RPC server already exposes /health.\n\nManual tests on a polkadot node syncing:\n\n```bash\n➜ polkadot-sdk (na-fix-4443) ✗ curl -v localhost:9944/health\n* Host localhost:9944 was resolved.\n* IPv6: ::1\n* IPv4: 127.0.0.1\n*   Trying [::1]:9944...\n* connect to ::1 port 9944 from ::1 port 55024 failed: Connection refused\n*   Trying 127.0.0.1:9944...\n* Connected to localhost (127.0.0.1) port 9944\n> GET /health HTTP/1.1\n> Host: localhost:9944\n> User-Agent: curl/8.5.0\n> Accept: */*\n>\n< HTTP/1.1 200 OK\n< content-type: application/json; charset=utf-8\n< content-length: 53\n< date: Fri, 14 Jun 2024 16:12:23 GMT\n<\n* Connection #0 to host localhost left intact\n{\"peers\":0,\"isSyncing\":false,\"shouldHavePeers\":false}%\n➜ polkadot-sdk (na-fix-4443) ✗ curl -v localhost:9944/health/readiness\n* Host localhost:9944 was resolved.\n* IPv6: ::1\n* IPv4: 127.0.0.1\n*   Trying [::1]:9944...\n* connect to ::1 port 9944 from ::1 port 54328 failed: Connection refused\n*   Trying 127.0.0.1:9944...\n* Connected to localhost (127.0.0.1) port 9944\n> GET /health/readiness HTTP/1.1\n> Host: localhost:9944\n> User-Agent: curl/8.5.0\n> Accept: */*\n>\n< HTTP/1.1 500 Internal Server Error\n< content-type: application/json; charset=utf-8\n< content-length: 0\n< date: Fri, 14 Jun 2024 16:12:36 GMT\n<\n* Connection #0 to host localhost left intact\n```\n\n//cc @BulatSaif you may be interested in this..\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-19T16:20:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6c857609a9425902d6dfe5445afb16c6b23ad86c"
        },
        "date": 1718816304912,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.153771440666668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23174888340000005,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "74decbbdf22a7b109209448307563c6f3d62abac",
          "message": "Bump curve25519-dalek from 4.1.2 to 4.1.3 (#4824)\n\nBumps\n[curve25519-dalek](https://github.com/dalek-cryptography/curve25519-dalek)\nfrom 4.1.2 to 4.1.3.\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/5312a0311ec40df95be953eacfa8a11b9a34bc54\"><code>5312a03</code></a>\ncurve: Bump version to 4.1.3 (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/660\">#660</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/b4f9e4df92a4689fb59e312a21f940ba06ba7013\"><code>b4f9e4d</code></a>\nSECURITY: fix timing variability in backend/serial/u32/scalar.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/661\">#661</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/415892acf1cdf9161bd6a4c99bc2f4cb8fae5e6a\"><code>415892a</code></a>\nSECURITY: fix timing variability in backend/serial/u64/scalar.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/659\">#659</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/56bf398d0caed63ef1d1edfbd35eb5335132aba2\"><code>56bf398</code></a>\nUpdates license field to valid SPDX format (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/647\">#647</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/9252fa5c0d09054fed4ac4d649e63c40fad7abaf\"><code>9252fa5</code></a>\nMitigate check-cfg until MSRV 1.77 (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/652\">#652</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/1efe6a93b176c4389b78e81e52b2cf85d728aac6\"><code>1efe6a9</code></a>\nFix a minor typo in signing.rs (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/649\">#649</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/cc3421a22fa7ee1f557cbe9243b450da53bbe962\"><code>cc3421a</code></a>\nIndicate that the rand_core feature is required (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/641\">#641</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/858c4ca8ae03d33fe8b71b4504c4d3f5ff5b45c0\"><code>858c4ca</code></a>\nAddress new nightly clippy unnecessary qualifications (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/639\">#639</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/31ccb6705067d68782cb135e23c79b640a6a06ee\"><code>31ccb67</code></a>\nRemove platforms in favor using CARGO_CFG_TARGET_POINTER_WIDTH (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/636\">#636</a>)</li>\n<li><a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/commit/19c7f4a5d5e577adc9cc65a837abef9ed7ebf0a4\"><code>19c7f4a</code></a>\nFix new nightly redundant import lint warns (<a\nhref=\"https://redirect.github.com/dalek-cryptography/curve25519-dalek/issues/638\">#638</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/dalek-cryptography/curve25519-dalek/compare/curve25519-4.1.2...curve25519-4.1.3\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=curve25519-dalek&package-manager=cargo&previous-version=4.1.2&new-version=4.1.3)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-06-20T08:56:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/74decbbdf22a7b109209448307563c6f3d62abac"
        },
        "date": 1718880407830,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.842860885166663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17398711983333331,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "a23abb17232107275089040a33ff38e6a801e648",
          "message": "Bump ws from 8.16.0 to 8.17.1 in /bridges/testing/framework/utils/generate_hex_encoded_call (#4825)\n\nBumps [ws](https://github.com/websockets/ws) from 8.16.0 to 8.17.1.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/websockets/ws/releases\">ws's\nreleases</a>.</em></p>\n<blockquote>\n<h2>8.17.1</h2>\n<h1>Bug fixes</h1>\n<ul>\n<li>Fixed a DoS vulnerability (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2231\">#2231</a>).</li>\n</ul>\n<p>A request with a number of headers exceeding\nthe[<code>server.maxHeadersCount</code>][]\nthreshold could be used to crash a ws server.</p>\n<pre lang=\"js\"><code>const http = require('http');\nconst WebSocket = require('ws');\n<p>const wss = new WebSocket.Server({ port: 0 }, function () {\nconst chars =\n&quot;!#$%&amp;'*+-.0123456789abcdefghijklmnopqrstuvwxyz^_`|~&quot;.split('');\nconst headers = {};\nlet count = 0;</p>\n<p>for (let i = 0; i &lt; chars.length; i++) {\nif (count === 2000) break;</p>\n<pre><code>for (let j = 0; j &amp;lt; chars.length; j++) {\n  const key = chars[i] + chars[j];\n  headers[key] = 'x';\n\n  if (++count === 2000) break;\n}\n</code></pre>\n<p>}</p>\n<p>headers.Connection = 'Upgrade';\nheaders.Upgrade = 'websocket';\nheaders['Sec-WebSocket-Key'] = 'dGhlIHNhbXBsZSBub25jZQ==';\nheaders['Sec-WebSocket-Version'] = '13';</p>\n<p>const request = http.request({\nheaders: headers,\nhost: '127.0.0.1',\nport: wss.address().port\n});</p>\n<p>request.end();\n});\n</code></pre></p>\n<p>The vulnerability was reported by <a\nhref=\"https://github.com/rrlapointe\">Ryan LaPointe</a> in <a\nhref=\"https://redirect.github.com/websockets/ws/issues/2230\">websockets/ws#2230</a>.</p>\n<p>In vulnerable versions of ws, the issue can be mitigated in the\nfollowing ways:</p>\n<ol>\n<li>Reduce the maximum allowed length of the request headers using the\n[<code>--max-http-header-size=size</code>][] and/or the\n[<code>maxHeaderSize</code>][] options so\nthat no more headers than the <code>server.maxHeadersCount</code> limit\ncan be sent.</li>\n</ol>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/3c56601092872f7d7566989f0e379271afd0e4a1\"><code>3c56601</code></a>\n[dist] 8.17.1</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/e55e5106f10fcbaac37cfa89759e4cc0d073a52c\"><code>e55e510</code></a>\n[security] Fix crash when the Upgrade header cannot be read (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2231\">#2231</a>)</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/6a00029edd924499f892aed8003cef1fa724cfe5\"><code>6a00029</code></a>\n[test] Increase code coverage</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/ddfe4a804d79e7788ab136290e609f91cf68423f\"><code>ddfe4a8</code></a>\n[perf] Reduce the amount of <code>crypto.randomFillSync()</code>\ncalls</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/b73b11828d166e9692a9bffe9c01a7e93bab04a8\"><code>b73b118</code></a>\n[dist] 8.17.0</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/29694a5905fa703e86667928e6bacac397469471\"><code>29694a5</code></a>\n[test] Use the <code>highWaterMark</code> variable</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/934c9d6b938b93c045cb13e5f7c19c27a8dd925a\"><code>934c9d6</code></a>\n[ci] Test on node 22</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/1817bac06e1204bfb578b8b3f4bafd0fa09623d0\"><code>1817bac</code></a>\n[ci] Do not test on node 21</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/96c9b3deddf56cacb2d756aaa918071e03cdbc42\"><code>96c9b3d</code></a>\n[major] Flip the default value of <code>allowSynchronousEvents</code>\n(<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2221\">#2221</a>)</li>\n<li><a\nhref=\"https://github.com/websockets/ws/commit/e5f32c7e1e6d3d19cd4a1fdec84890e154db30c1\"><code>e5f32c7</code></a>\n[fix] Emit at most one event per event loop iteration (<a\nhref=\"https://redirect.github.com/websockets/ws/issues/2218\">#2218</a>)</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/websockets/ws/compare/8.16.0...8.17.1\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=ws&package-manager=npm_and_yarn&previous-version=8.16.0&new-version=8.17.1)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\nYou can disable automated security fix PRs for this repo from the\n[Security Alerts\npage](https://github.com/paritytech/polkadot-sdk/network/alerts).\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>",
          "timestamp": "2024-06-21T07:23:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/a23abb17232107275089040a33ff38e6a801e648"
        },
        "date": 1718960684178,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.849137999233331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19322272383333333,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "b301218db8785c6d425ca9a9ef90daa80780f2ce",
          "message": "[ci] Change storage type for forklift in GHA (#4850)\n\nPR changes forklift authentication to gcs\n\ncc https://github.com/paritytech/ci_cd/issues/987",
          "timestamp": "2024-06-21T09:33:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b301218db8785c6d425ca9a9ef90daa80780f2ce"
        },
        "date": 1718970354300,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.482584798400001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.3094688998666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c4b3c1c6c6e492c4196e06fbba824a58e8119a3b",
          "message": "Bump time to fix compilation on latest nightly (#4862)\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4748",
          "timestamp": "2024-06-21T15:05:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c4b3c1c6c6e492c4196e06fbba824a58e8119a3b"
        },
        "date": 1718989496114,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 15.120303276799998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2855882720666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "812dbff17513cbd2aeb2ff9c41214711bd1c0004",
          "message": "Frame: `Consideration` trait generic over `Footprint` and indicates zero cost (#4596)\n\n`Consideration` trait generic over `Footprint` and indicates zero cost\nfor a give footprint.\n\n`Consideration` trait is generic over `Footprint` (currently defined\nover the type with the same name). This makes it possible to setup a\ncustom footprint (e.g. current number of proposals in the storage).\n\n`Consideration::new` and `Consideration::update` return an\n`Option<Self>` instead `Self`, this make it possible to indicate a no\ncost for a specific footprint (e.g. if current number of proposals in\nthe storage < max_proposal_count / 2 then no cost).\n\nThese cases need to be handled for\nhttps://github.com/paritytech/polkadot-sdk/pull/3151",
          "timestamp": "2024-06-22T13:54:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/812dbff17513cbd2aeb2ff9c41214711bd1c0004"
        },
        "date": 1719071653278,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.2080500815,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.26467845883333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "girazoki",
            "username": "girazoki",
            "email": "gorka.irazoki@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f8feebc12736c04d60040e0f291615479f9951a5",
          "message": "Reinitialize should allow to override existing config in collationGeneration (#4833)\n\nCurrently the `Initialize` and `Reinitialize` messages in the\ncollationGeneration subsystem fail if:\n-  `Initialize` if there exists already another configuration and\n- `Reinitialize` if another configuration does not exist\n\nI propose to instead change the behaviour of `Reinitialize` to always\nset the config regardless of whether one exists or not.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>",
          "timestamp": "2024-06-23T09:35:36Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8feebc12736c04d60040e0f291615479f9951a5"
        },
        "date": 1719141245563,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2516142361,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.334943034400002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "686aa233e67c619bcdbc8b758a9ddf92c3315cf1",
          "message": "Block import cleanups (#4842)\n\nI carried these things in a fork for a long time, I think wouldn't hurt\nto have it upstream.\n\nOriginally submitted as part of\nhttps://github.com/paritytech/polkadot-sdk/pull/1598 that went nowhere.\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-23T11:36:34Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/686aa233e67c619bcdbc8b758a9ddf92c3315cf1"
        },
        "date": 1719149530729,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.20826613173333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.077047803433334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b7767168b7dd93964f40e8543b853097e3570621",
          "message": "Ensure earliest allowed block is at minimum the next block (#4823)\n\nWhen `min_enactment_period == 0` and `desired == At(n)` where `n` is\nsmaller than the current block number, the scheduling would fail. This\nhappened for example here:\nhttps://collectives.subsquare.io/fellowship/referenda/126\n\nTo ensure that this doesn't happen again, ensure that the earliest\nallowed block is at minimum the next block.",
          "timestamp": "2024-06-24T10:37:13Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b7767168b7dd93964f40e8543b853097e3570621"
        },
        "date": 1719226918521,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18431625953333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.888673773466667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "5e62782d27a18d8c57da28617181c66cd57076b5",
          "message": "treasury pallet: remove unused config parameters (#4831)\n\nRemove unused config parameters `ApproveOrigin` and `OnSlash` from the\ntreasury pallet. Add `OnSlash` config parameter to the bounties and tips\npallets.\n\npart of https://github.com/paritytech/polkadot-sdk/issues/3800",
          "timestamp": "2024-06-24T12:31:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/5e62782d27a18d8c57da28617181c66cd57076b5"
        },
        "date": 1719235551389,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17606470336666669,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.708579094666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dashangcun",
            "username": "dashangcun",
            "email": "907225865@qq.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "63e264446f6cabff06be72912eae902662dcb699",
          "message": "chore: remove repeat words (#4869)\n\nSigned-off-by: dashangcun <jchaodaohang@foxmail.com>\nCo-authored-by: dashangcun <jchaodaohang@foxmail.com>",
          "timestamp": "2024-06-24T15:00:20Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/63e264446f6cabff06be72912eae902662dcb699"
        },
        "date": 1719248591085,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18557484166666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.827494063333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "909bfc2d7c00a0fed7a5fd4e5292aa3fbe2299b6",
          "message": "[subsystem-bench] Trigger own assignments in approval-voting (#4772)",
          "timestamp": "2024-06-25T09:08:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/909bfc2d7c00a0fed7a5fd4e5292aa3fbe2299b6"
        },
        "date": 1719312679321,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.165249286233333,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.26059736626666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "3c213726cf165d8b1155d5151b9c548e879b5ff8",
          "message": "chain-spec-builder: Add support for `codeSubstitutes`  (#4685)\n\nWhile working on https://github.com/paritytech/polkadot-sdk/pull/4600 I\nfound that it would be nice if `chain-spec-builder` supported\n`codeSubstitutes`. After this PR is merged you can do:\n\n```\nchain-spec-builder add-code-substitute chain_spec.json my_runtime.compact.compressed.wasm 1234\n```\n\nIn addition, the `chain-spec-builder` was silently removing\n`relay_chain` and `para_id` fields when used on parachain chain-specs.\nThis is now fixed by providing a custom chain-spec extension that has\nthese fields marked as optional.",
          "timestamp": "2024-06-25T12:58:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/3c213726cf165d8b1155d5151b9c548e879b5ff8"
        },
        "date": 1719322956478,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.3005391946333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 14.068230286833332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "2f3a1bf8736844272a7eb165780d9f283b19d5c0",
          "message": "Use real rust type for pallet alias in `runtime` macro (#4769)\n\nFixes https://github.com/paritytech/polkadot-sdk/issues/4723. Also,\ncloses https://github.com/paritytech/polkadot-sdk/issues/4622\n\nAs stated in the linked issue, this PR adds the ability to use a real\nrust type for pallet alias in the new `runtime` macro:\n```rust\n#[runtime::pallet_index(0)]\npub type System = frame_system::Pallet<Runtime>;\n```\n\nPlease note that the current syntax still continues to be supported.\n\nCC: @shawntabrizi @kianenigma\n\n---------\n\nCo-authored-by: command-bot <>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-25T14:31:40Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/2f3a1bf8736844272a7eb165780d9f283b19d5c0"
        },
        "date": 1719330016295,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1799143456333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.858013604033331,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "20aecadbc7ed2e9fe3b8a7d345f1be301fc00ba0",
          "message": "[FRAME] Remove storage migration type (#3828)\n\nIntroduce migration type to remove data associated with a specific\nstorage of a pallet.\n\nBased on existing `RemovePallet` migration type.\n\nRequired for https://github.com/paritytech/polkadot-sdk/pull/3820\n\n---------\n\nCo-authored-by: Liam Aharon <liam.aharon@hotmail.com>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-06-26T08:13:50Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/20aecadbc7ed2e9fe3b8a7d345f1be301fc00ba0"
        },
        "date": 1719392331949,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.870034642766672,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.182724081,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7a2592e8458f8b3c5d9683eb02380a0f5959b5b3",
          "message": "rpc: upgrade jsonrpsee v0.23 (#4730)\n\nThis is PR updates jsonrpsee v0.23 which mainly changes:\n- Add `Extensions` which we now is using to get the connection id (used\nby the rpc spec v2 impl)\n- Update hyper to v1.0, http v1.0, soketto and related crates\n(hyper::service::make_service_fn is removed)\n- The subscription API for the client is modified to know why a\nsubscription was closed.\n\nFull changelog here:\nhttps://github.com/paritytech/jsonrpsee/releases/tag/v0.23.0\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-26T10:25:24Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7a2592e8458f8b3c5d9683eb02380a0f5959b5b3"
        },
        "date": 1719404739643,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.801909922400004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16511349033333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7084463a49f2359dc2f378f5834c7252af02ed4d",
          "message": "Update parity publish (#4878)\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-06-26T12:20:47Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7084463a49f2359dc2f378f5834c7252af02ed4d"
        },
        "date": 1719410731380,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1811950191333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.97256566036667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "929a273ae1ba647628c4ba6e2f8737e58b596d6a",
          "message": "pallet assets: optional auto-increment for the asset ID (#4757)\n\nIntroduce an optional auto-increment setup for the IDs of new assets.\n\n---------\n\nCo-authored-by: joe petrowski <25483142+joepetrowski@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-06-26T16:36:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/929a273ae1ba647628c4ba6e2f8737e58b596d6a"
        },
        "date": 1719426431784,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.16959921253333332,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.048321381600001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d604e84ee71d685ac3b143b19976d0093b95a1e2",
          "message": "Ensure key ownership proof is optimal (#4699)\n\nEnsure that the key ownership proof doesn't contain duplicate or\nunneeded nodes.\n\nWe already have these checks for the bridge messages proof. Just making\nthem more generic and performing them also for the key ownership proof.\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-06-27T11:17:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d604e84ee71d685ac3b143b19976d0093b95a1e2"
        },
        "date": 1719493353142,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18165393486666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.155668257166663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dee18249742c4abbf81fcca62b40a868a394c3d4",
          "message": "BridgeHubs fresh weights for bridging pallets (#4891)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-27T13:38:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dee18249742c4abbf81fcca62b40a868a394c3d4"
        },
        "date": 1719500246971,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18165393486666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.155668257166663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "dee18249742c4abbf81fcca62b40a868a394c3d4",
          "message": "BridgeHubs fresh weights for bridging pallets (#4891)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-06-27T13:38:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/dee18249742c4abbf81fcca62b40a868a394c3d4"
        },
        "date": 1719501442955,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18165393486666664,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.155668257166663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "de41ae85ec600189c4621aaf9e58afc612f101f7",
          "message": "chore(deps): upgrade prometheous server to hyper v1 (#4898)\n\nPartly fixes\nhttps://github.com/paritytech/polkadot-sdk/pull/4890#discussion_r1655548633\n\nStill the offchain API needs to be updated to hyper v1.0 and I opened an\nissue for it, it's using low-level http body features that have been\nremoved",
          "timestamp": "2024-06-27T15:45:29Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/de41ae85ec600189c4621aaf9e58afc612f101f7"
        },
        "date": 1719509528252,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2094158689666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.095988775966665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18a6a56cf35590062792a7122404a1ca09ab7fe8",
          "message": "Add `Runtime::OmniNode` variant to `polkadot-parachain` (#4805)\n\nAdding `Runtime::OmniNode` variant + small changes\n\n---------\n\nCo-authored-by: kianenigma <kian@parity.io>",
          "timestamp": "2024-06-28T06:02:30Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18a6a56cf35590062792a7122404a1ca09ab7fe8"
        },
        "date": 1719560636584,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1775654887333334,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.785180261333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "016f394854a3fad6762913a0f208cece181c34fe",
          "message": "[Rococo<>Westend bridge] Allow any asset over the lane between the two Asset Hubs (#4888)\n\nOn Westend Asset Hub, we allow Rococo Asset Hub to act as reserve for\nany asset native to the Rococo or Ethereum ecosystems (practically\nproviding Westend access to Ethereum assets through double bridging:\nW<>R<>Eth).\n\nOn Rococo Asset Hub, we allow Westend Asset Hub to act as reserve for\nany asset native to the Westend ecosystem. We also allow Ethereum\ncontracts to act as reserves for the foreign assets identified by the\nsame respective contracts locations.\n\n- [x] add emulated tests for various assets (native, trust-based,\nforeign/bridged) going AHR -> AHW,\n- [x] add equivalent tests for the other direction AHW -> AHR.\n\nThis PR is a prerequisite to doing the same for Polkadot<>Kusama bridge.",
          "timestamp": "2024-06-28T09:09:38Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/016f394854a3fad6762913a0f208cece181c34fe"
        },
        "date": 1719568222050,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.912520311300003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17982241866666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "aaf0443591b134a0da217d575161872796e75059",
          "message": "network: Sync peerstore constants between libp2p and litep2p (#4906)\n\nCounterpart of: https://github.com/paritytech/polkadot-sdk/pull/4031\n\ncc @paritytech/networking\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-06-28T13:43:22Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/aaf0443591b134a0da217d575161872796e75059"
        },
        "date": 1719588326857,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.745265229599998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1680305803,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "333f4c78109345debb5cf6e8c8b3fe75a7bbe3fb",
          "message": "Remove getters from pallet-membership (#4840)\n\nAs per #3326 , removes pallet::getter macro usage from\npallet-membership. The syntax StorageItem::<T, I>::get() should be used\ninstead. Also converts some syntax to turbo and reimplements the removed\ngetters, following #223\n\ncc @muraca\n\n---------\n\nCo-authored-by: Dónal Murray <donalm@seadanda.dev>\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-01T12:25:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/333f4c78109345debb5cf6e8c8b3fe75a7bbe3fb"
        },
        "date": 1719844210988,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.821962944100004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18374575066666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Yuri Volkov",
            "username": "mutantcornholio",
            "email": "0@mcornholio.ru"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18228a9bdbe5b560a39a31810fdf2e3fba59a40d",
          "message": "prdoc upgrade (#4918)\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-01T14:54:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18228a9bdbe5b560a39a31810fdf2e3fba59a40d"
        },
        "date": 1719852383128,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.846709581633334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18160602733333325,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kazunobu Ndong",
            "username": "ndkazu",
            "email": "33208377+ndkazu@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "18ed309a37036db8429665f1e91fb24ab312e646",
          "message": "Pallet Name Customisation (#4806)\n\nAdded Instructions for pallet name customisation in the ReadMe\r\n\r\n---------\r\n\r\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\r\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\r\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-01T20:18:27Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/18ed309a37036db8429665f1e91fb24ab312e646"
        },
        "date": 1719869939959,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.308395039933336,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.27320049076666664,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "98ce675a6bfafa145dd6be74c95d7768917392c1",
          "message": "bridge tests: send bridged assets from random parachain to bridged asset hub (#4870)\n\n- Send bridged WNDs: Penpal Rococo -> AH Rococo -> AH Westend\n- Send bridged ROCs: Penpal Westend -> AH Westend -> AH Rococo\n\nThe tests send both ROCs and WNDs, for each direction the native asset\nis only used to pay for the transport fees on the local AssetHub, and\nare not sent over the bridge.\n\nIncluding the native asset won't be necessary anymore once we get #4375.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-03T09:07:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98ce675a6bfafa145dd6be74c95d7768917392c1"
        },
        "date": 1720000420152,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17744932119999995,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.957214444366667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Adrian Catangiu",
            "username": "acatangiu",
            "email": "adrian@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "98ce675a6bfafa145dd6be74c95d7768917392c1",
          "message": "bridge tests: send bridged assets from random parachain to bridged asset hub (#4870)\n\n- Send bridged WNDs: Penpal Rococo -> AH Rococo -> AH Westend\n- Send bridged ROCs: Penpal Westend -> AH Westend -> AH Rococo\n\nThe tests send both ROCs and WNDs, for each direction the native asset\nis only used to pay for the transport fees on the local AssetHub, and\nare not sent over the bridge.\n\nIncluding the native asset won't be necessary anymore once we get #4375.\n\n---------\n\nSigned-off-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-03T09:07:55Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/98ce675a6bfafa145dd6be74c95d7768917392c1"
        },
        "date": 1720005025856,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2142080747333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.333725722933337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec",
          "message": "Fixes warnings in `frame-support-procedural` crate (#4915)\n\nThis PR fixes the unused warnings in `frame-support-procedural` crate,\nraised by the latest stable rust release.",
          "timestamp": "2024-07-03T11:32:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec"
        },
        "date": 1720010507985,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2142080747333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.333725722933337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec",
          "message": "Fixes warnings in `frame-support-procedural` crate (#4915)\n\nThis PR fixes the unused warnings in `frame-support-procedural` crate,\nraised by the latest stable rust release.",
          "timestamp": "2024-07-03T11:32:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e5791a56dcc35e308a80985cc3b6b7f2ed1eb6ec"
        },
        "date": 1720012073057,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2142080747333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.333725722933337,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b6f18232449b003d716a0d630f3da3a9dce669ab",
          "message": "[BEEFY] Add runtime support for reporting fork voting (#4522)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4523\n\nExtracting part of https://github.com/paritytech/polkadot-sdk/pull/1903\n(credits to @Lederstrumpf for the high-level strategy), but also\nintroducing significant adjustments both to the approach and to the\ncode. The main adjustment is the fact that the `ForkVotingProof` accepts\nonly one vote, compared to the original version which accepted a\n`vec![]`. With this approach more calls are needed in order to report\nmultiple equivocated votes on the same commit, but it simplifies a lot\nthe checking logic. We can add support for reporting multiple signatures\nat once in the future.\n\nThere are 2 things that are missing in order to consider this issue\ndone, but I would propose to do them in a separate PR since this one is\nalready pretty big:\n- benchmarks/computing a weight for the new extrinsic (this wasn't\npresent in https://github.com/paritytech/polkadot-sdk/pull/1903 either)\n- exposing an API for generating the ancestry proof. I'm not sure if we\nshould do this in the Mmr pallet or in the Beefy pallet\n\nCo-authored-by: Robert Hambrock <roberthambrock@gmail.com>\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>",
          "timestamp": "2024-07-03T13:44:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b6f18232449b003d716a0d630f3da3a9dce669ab"
        },
        "date": 1720020748413,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.108674880966666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18081050793333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "282eaaa5f49749f68508752a993d6c79d64f6162",
          "message": "[Staking] Delegators can stake but stakers can't delegate (#4904)\n\nRelated: https://github.com/paritytech/polkadot-sdk/pull/4804.\nFixes the try state error in Westend:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6564522.\nPasses here:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6580393\n\n## Context\nCurrently in Kusama and Polkadot, an account can do both, directly\nstake, and join a pool.\n\nWith the migration of pools to `DelegateStake` (See\nhttps://github.com/paritytech/polkadot-sdk/pull/3905), the funds of pool\nmembers are locked in a different way than for direct stakers.\n- Pool member funds uses `holds`.\n- `pallet-staking` uses deprecated locks (analogous to freeze) which can\noverlap with holds.\n\nAn existing delegator can stake directly since pallet-staking only uses\nfree balance. But once an account becomes staker, we cannot allow them\nto be delegator as this risks an account to use already staked (frozen)\nfunds in pools.\n\nWhen an account gets into a situation where it is participating in both\npools and staking, it would no longer would be able to add any extra\nbond to the pool but they can still withdraw funds.\n\n## Changes\n- Add test for the above scenario.\n- Removes the assumption that a delegator cannot be a staker.",
          "timestamp": "2024-07-03T17:16:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/282eaaa5f49749f68508752a993d6c79d64f6162"
        },
        "date": 1720033536048,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.132111234100003,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17933753803333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Axay Sagathiya",
            "username": "axaysagathiya",
            "email": "axaysagathiya@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "51e98273d78ebd9c1f2b259c9d67e75780c0192a",
          "message": "rename the candidate backing message from `GetBackedCandidates` to `GetBackableCandidates` (#4921)\n\n**Backable Candidate**: If a candidate receives enough supporting\nStatements from the Parachain Validators currently assigned, that\ncandidate is considered backable.\n**Backed Candidate**: A Backable Candidate noted in a relay-chain block\n\n---\n\nWhen the candidate backing subsystem receives the `GetBackedCandidates`\nmessage, it sends back **backable** candidates, not **backed**\ncandidates. So we should rename this message to `GetBackableCandidates`\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-03T19:37:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/51e98273d78ebd9c1f2b259c9d67e75780c0192a"
        },
        "date": 1720041901570,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.727420193000004,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.26575256663333324,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "polka.dom",
            "username": "PolkadotDom",
            "email": "polkadotdom@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "924728cf19523f826a08e1c0eeee711ca3bb8ee7",
          "message": "Remove getter macro from pallet-insecure-randomness-collective-flip (#4839)\n\nAs per #3326, removes pallet::getter macro usage from the\npallet-insecure-randomness-collective-flip. The syntax `StorageItem::<T,\nI>::get()` should be used instead.\n\nExplicitly implements the getters that were removed as well, following\n#223\n\nAlso makes the storage values public and converts some syntax to turbo\n\ncc @muraca\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-03T20:53:09Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/924728cf19523f826a08e1c0eeee711ca3bb8ee7"
        },
        "date": 1720049015291,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.141372223433331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19235233913333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e44f61af440316f4050f69df024fe964ffcd9346",
          "message": "Introduce basic slot-based collator (#4097)\n\nPart of #3168 \nOn top of #3568\n\n### Changes Overview\n- Introduces a new collator variant in\n`cumulus/client/consensus/aura/src/collators/slot_based/mod.rs`\n- Two tasks are part of that module, one for block building and one for\ncollation building and submission.\n- Introduces a new variant of `cumulus-test-runtime` which has 2s slot\nduration, used for zombienet testing\n- Zombienet tests for the new collator\n\n**Note:** This collator is considered experimental and should only be\nused for testing and exploration for now.\n\n### Comparison with `lookahead` collator\n- The new variant is slot based, meaning it waits for the next slot of\nthe parachain, then starts authoring\n- The search for potential parents remains mostly unchanged from\nlookahead\n- As anchor, we use the current best relay parent\n- In general, the new collator tends to be anchored to one relay parent\nearlier. `lookahead` generally waits for a new relay block to arrive\nbefore it attempts to build a block. This means the actual timing of\nparachain blocks depends on when the relay block has been authored and\nimported. With the slot-triggered approach we are authoring directly on\nthe slot boundary, were a new relay chain block has probably not yet\narrived.\n\n### Limitations\n- Overall, the current implementation focuses on the \"happy path\"\n- We assume that we want to collate close to the tip of the relay chain.\nIt would be useful however to have some kind of configurable drift, so\nthat we could lag behind a bit.\nhttps://github.com/paritytech/polkadot-sdk/issues/3965\n- The collation task is pretty dumb currently. It checks if we have\ncores scheduled and if yes, submits all the messages we have received\nfrom the block builder until we have something submitted for every core.\nIdeally we should do some extra checks, i.e. we do not need to submit if\nthe built block is already too old (build on a out of range relay\nparent) or was authored with a relay parent that is not an ancestor of\nthe relay block we are submitting at.\nhttps://github.com/paritytech/polkadot-sdk/issues/3966\n- There is no throttling, we assume that we can submit _velocity_ blocks\nevery relay chain block. There should be communication between the\ncollator task and block-builder task.\n- The parent search and ConsensusHook are not yet properly adjusted. The\nparent search makes assumptions about the pending candidate which no\nlonger hold. https://github.com/paritytech/polkadot-sdk/issues/3967\n- Custom triggers for block building not implemented.\n\n---------\n\nCo-authored-by: Davide Galassi <davxy@datawok.net>\nCo-authored-by: Andrei Sandu <54316454+sandreim@users.noreply.github.com>\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Javier Viola <363911+pepoviola@users.noreply.github.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-05T09:00:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e44f61af440316f4050f69df024fe964ffcd9346"
        },
        "date": 1720176357608,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.234108184299998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1827450771666667,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "299aacb56f4f11127b194d12692b00066e91ac92",
          "message": "[ci] Increase timeout for ci jobs (#4950)\n\nRelated to recent discussion. PR makes timeout less strict.\n\ncc https://github.com/paritytech/ci_cd/issues/996",
          "timestamp": "2024-07-05T11:13:23Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/299aacb56f4f11127b194d12692b00066e91ac92"
        },
        "date": 1720184053254,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.129241486999998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17877142340000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Nazar Mokrynskyi",
            "username": "nazar-pc",
            "email": "nazar@mokrynskyi.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "221eddc90cd1efc4fc3c822ce5ccf289272fb41d",
          "message": "Optimize finalization performance (#4922)\n\nThis PR largely fixes\nhttps://github.com/paritytech/polkadot-sdk/issues/4903 by addressing it\nfrom a few different directions.\n\nThe high-level observation is that complexity of finalization was\nunfortunately roughly `O(n^3)`. Not only\n`displaced_leaves_after_finalizing` was extremely inefficient on its\nown, especially when large ranges of blocks were involved, it was called\nonce upfront and then on every single block that was finalized over and\nover again.\n\nThe first commit refactores code adjacent to\n`displaced_leaves_after_finalizing` to optimize memory allocations. For\nexample things like `BTreeMap<_, Vec<_>>` were very bad in terms of\nnumber of allocations and after analyzing code paths was completely\nunnecessary and replaced with `Vec<(_, _)>`. In other places allocations\nof known size were not done upfront and some APIs required unnecessary\ncloning of vectors.\n\nI checked invariants and didn't find anything that was violated after\nrefactoring.\n\nSecond commit completely replaces `displaced_leaves_after_finalizing`\nimplementation with a much more efficient one. In my case with ~82k\nblocks and ~13k leaves it takes ~5.4s to finish\n`client.apply_finality()` now.\n\nThe idea is to avoid querying the same blocks over and over again as\nwell as introducing temporary local cache for blocks related to leaves\nabove block that is being finalized as well as local cache of the\nfinalized branch of the chain. I left some comments in the code and\nwrote tests that I belive should check all code invariants for\ncorrectness. `lowest_common_ancestor_multiblock` was removed as\nunnecessary and not great in terms of performance API, domain-specific\ncode should be written instead like done in\n`displaced_leaves_after_finalizing`.\n\nAfter these changes I noticed finalization is still horribly slow,\nturned out that even though `displaced_leaves_after_finalizing` was way\nfaster that before (probably order of magnitude), it was called for\nevery single of those 82k blocks :facepalm:\n\nThe quick hack I came up with in the third commit to handle this edge\ncase was to not call it when finalizing multiple blocks at once until\nthe very last moment. It works and allows to finish the whole\nfinalization in just 14 seconds (5.4+5.4 of which are two calls to\n`displaced_leaves_after_finalizing`). I'm really not happy with the fact\nthat `displaced_leaves_after_finalizing` is called twice, but much\nheavier refactoring would be necessary to get rid of second call.\n\n---\n\nNext steps:\n* assuming the changes are acceptable I'll write prdoc\n* https://github.com/paritytech/polkadot-sdk/pull/4920 or something\nsimilar in spirit should be implemented to unleash efficient parallelsm\nwith rayon in `displaced_leaves_after_finalizing`, which will allow to\nfurther (and significant!) scale its performance rather that being\nCPU-bound on a single core, also reading database sequentially should\nideally be avoided\n* someone should look into removal of the second\n`displaced_leaves_after_finalizing` call\n* further cleanups are possible if `undo_finalization` can be removed\n\n---\n\nPolkadot Address: 1vSxzbyz2cJREAuVWjhXUT1ds8vBzoxn2w4asNpusQKwjJd\n\n---------\n\nCo-authored-by: Sebastian Kunert <skunert49@gmail.com>",
          "timestamp": "2024-07-05T19:02:18Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/221eddc90cd1efc4fc3c822ce5ccf289272fb41d"
        },
        "date": 1720209746495,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19304615823333343,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.121886510900001,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Deepak Chaudhary",
            "username": "Aideepakchaudhary",
            "email": "54492415+Aideepakchaudhary@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d3cdfc4469ca9884403d52c94f2cb14bc62e6697",
          "message": "remove getter from babe pallet (#4912)\n\n### ISSUE\nLink to the issue:\nhttps://github.com/paritytech/polkadot-sdk/issues/3326\ncc @muraca \n\nDeliverables\n - [Deprecation] remove pallet::getter usage from all pallet-babe\n\n### Test Outcomes\n___\nSuccessful tests by running `cargo test -p pallet-babe --features\nruntime-benchmarks`\n\n\nrunning 32 tests\ntest\nmock::__pallet_staking_reward_curve_test_module::reward_curve_piece_count\n... ok\ntest mock::__construct_runtime_integrity_test::runtime_integrity_tests\n... ok\ntest mock::test_genesis_config_builds ... ok\n2024-06-28T17:02:11.158812Z ERROR runtime::storage: Corrupted state at\n`0x1cb6f36e027abb2091cfb5110ab5087f9aab0a5b63b359512deee557c9f4cf63`:\nError { cause: Some(Error { cause: None, desc: \"Could not decode\n`NextConfigDescriptor`, variant doesn't exist\" }), desc: \"Could not\ndecode `Option::Some(T)`\" }\n2024-06-28T17:02:11.159752Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::add_epoch_configurations_migration_works ... ok\ntest tests::author_vrf_output_for_secondary_vrf ... ok\ntest benchmarking::bench_check_equivocation_proof ... ok\n2024-06-28T17:02:11.160537Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::can_estimate_current_epoch_progress ... ok\ntest tests::author_vrf_output_for_primary ... ok\ntest tests::authority_index ... ok\n2024-06-28T17:02:11.162327Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::empty_randomness_is_correct ... ok\ntest tests::check_module ... ok\n2024-06-28T17:02:11.163492Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::current_slot_is_processed_on_initialization ... ok\ntest tests::can_enact_next_config ... ok\n2024-06-28T17:02:11.164987Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.165007Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::can_predict_next_epoch_change ... ok\ntest tests::first_block_epoch_zero_start ... ok\ntest tests::initial_values ... ok\n2024-06-28T17:02:11.168430Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.168685Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.170982Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.171220Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::only_root_can_enact_config_change ... ok\ntest tests::no_author_vrf_output_for_secondary_plain ... ok\ntest tests::can_fetch_current_and_next_epoch_data ... ok\n2024-06-28T17:02:11.172960Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_has_valid_weight ... ok\n2024-06-28T17:02:11.173873Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177084Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_after_skipped_epochs_works ...\n2024-06-28T17:02:11.177694Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177703Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177925Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.177927Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\nok\n2024-06-28T17:02:11.179678Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.181446Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.183665Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.183874Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.185732Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.185951Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189332Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189559Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.189587Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::generate_equivocation_report_blob ... ok\ntest tests::disabled_validators_cannot_author_blocks - should panic ...\nok\n2024-06-28T17:02:11.190552Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.192279Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.194735Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.196136Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.197240Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::skipping_over_epochs_works ... ok\n2024-06-28T17:02:11.202783Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.202846Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.203029Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.205242Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::tracks_block_numbers_when_current_and_previous_epoch_started\n... ok\n2024-06-28T17:02:11.208965Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_current_session_works ... ok\ntest tests::report_equivocation_invalid_key_owner_proof ... ok\n2024-06-28T17:02:11.216431Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\n2024-06-28T17:02:11.216855Z ERROR runtime::timestamp:\n`pallet_timestamp::UnixTime::now` is called at genesis, invalid value\nreturned: 0\ntest tests::report_equivocation_validate_unsigned_prevents_duplicates\n... ok\ntest tests::report_equivocation_invalid_equivocation_proof ... ok\ntest tests::valid_equivocation_reports_dont_pay_fees ... ok\ntest tests::report_equivocation_old_session_works ... ok\ntest\nmock::__pallet_staking_reward_curve_test_module::reward_curve_precision\n... ok\n\ntest result: ok. 32 passed; 0 failed; 0 ignored; 0 measured; 0 filtered\nout; finished in 0.20s\n\n   Doc-tests pallet-babe\n\nrunning 0 tests\n\ntest result: ok. 0 passed; 0 failed; 0 ignored; 0 measured; 0 filtered\nout; finished in 0.00s\n\n---\n\nPolkadot Address: 16htXkeVhfroBhL6nuqiwknfXKcT6WadJPZqEi2jRf9z4XPY",
          "timestamp": "2024-07-06T21:29:19Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d3cdfc4469ca9884403d52c94f2cb14bc62e6697"
        },
        "date": 1720304096879,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.101830824733332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16342931963333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Muharem",
            "username": "muharem",
            "email": "ismailov.m.h@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f7dd85d053dc44ee0a6851e7e507083f31b01bd3",
          "message": "Assets: can_decrease/increase for destroying asset is not successful (#3286)\n\nFunctions `can_decrease` and `can_increase` do not return successful\nconsequence results for assets undergoing destruction; instead, they\nreturn the `UnknownAsset` consequence variant.\n\nThis update aligns their behavior with similar functions, such as\n`reducible_balance`, `increase_balance`, `decrease_balance`, and `burn`,\nwhich return an `AssetNotLive` error for assets in the process of being\ndestroyed.",
          "timestamp": "2024-07-07T11:45:16Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f7dd85d053dc44ee0a6851e7e507083f31b01bd3"
        },
        "date": 1720359337297,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.28249880510000003,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.72451289496667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "e1460b5ee5f4490b428035aa4a72c1c99a262459",
          "message": "[Backport] Version bumps  and  prdocs reordering from 1.14.0 (#4955)\n\nThis PR backports regular version bumps and prdocs reordering from the\n1.14.0 release branch to master",
          "timestamp": "2024-07-08T07:40:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/e1460b5ee5f4490b428035aa4a72c1c99a262459"
        },
        "date": 1720430964590,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.37059141126667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24295596396666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7290042ad4975e9d42633f228e331f286397025e",
          "message": "Make `tracing::log` work in the runtime (#4863)\n\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-08T14:19:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7290042ad4975e9d42633f228e331f286397025e"
        },
        "date": 1720456142013,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.872255338066665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18310919553333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d4657f86208a13d8fcc7933018d4558c3a96f634",
          "message": "litep2p/peerstore: Fix bump last updated time (#4971)\n\nThis PR bumps the last time of a reputation update of a peer.\nDoing so ensures the peer remains in the peerstore for longer than 1\nhour.\n\nLibp2p updates the `last_updated` field as well.\n\nSmall summary for the peerstore:\n- A: when peers are reported the `last_updated` time is set to current\ntime (not done before this PR)\n- B: peers that were not updated for 1 hour are removed from the\npeerstore\n- the reputation of the peers is decaying to zero over time\n- peers are reported with a reputation change (positive or negative\ndepending on the behavior)\n\nBecause, (A) was not updating the `last_updated` time, we might lose the\nreputation of peers that are constantly updated after 1hour because of\n(B).\n\ncc @paritytech/networking\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-08T15:40:07Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d4657f86208a13d8fcc7933018d4558c3a96f634"
        },
        "date": 1720462176450,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.06375061906667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17704402196666663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Or Grinberg",
            "username": "orgr",
            "email": "or.grin@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "72dab6d897d9519881b619d466ef5be3c7df9a34",
          "message": "allow clear_origin in safe xcm builder (#4777)\n\nFixes #3770 \n\nAdded `clear_origin` as an allowed command after commands that load the\nholdings register, in the safe xcm builder.\n\nChecklist\n- [x] My PR includes a detailed description as outlined in the\n\"Description\" section above\n- [x] My PR follows the [labeling\nrequirements](https://github.com/paritytech/polkadot-sdk/blob/master/docs/contributor/CONTRIBUTING.md#Process)\nof this project (at minimum one label for T required)\n- [x] I have made corresponding changes to the documentation (if\napplicable)\n- [x] I have added tests that prove my fix is effective or that my\nfeature works (if applicable)\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: gupnik <mail.guptanikhil@gmail.com>",
          "timestamp": "2024-07-09T03:40:39Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/72dab6d897d9519881b619d466ef5be3c7df9a34"
        },
        "date": 1720502934364,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.17571058566666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.078514967866663,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "01e0fc23d8844461adcd4501815dac64f3a4986f",
          "message": "`polkadot-parachain` simplifications and deduplications (#4916)\n\n`polkadot-parachain` simplifications and deduplications\n\nDetails in the commit messages. Just copy-pasting the last commit\ndescription since it introduces the biggest changes:\n\n```\n    Implement a more structured way to define a node spec\n    \n    - use traits instead of bounds for `rpc_ext_builder()`,\n      `build_import_queue()`, `start_consensus()`\n    - add a `NodeSpec` trait for defining the specifications of a node\n    - deduplicate the code related to building a node's components /\n      starting a node\n```\n\nThe other changes are much smaller, most of them trivial and are\nisolated in separate commits.",
          "timestamp": "2024-07-09T08:30:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/01e0fc23d8844461adcd4501815dac64f3a4986f"
        },
        "date": 1720520726023,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.047915750066664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1786377161333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Francisco Aguirre",
            "username": "franciscoaguirre",
            "email": "franciscoaguirreperez@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9403a5d40214b0d223c87c8d7b13139672edfe95",
          "message": "Add `MAX_INSTRUCTIONS_TO_DECODE` to XCMv2 (#4978)\n\nIt was added to v4 and v3 but was missing from v2",
          "timestamp": "2024-07-09T13:49:01Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9403a5d40214b0d223c87c8d7b13139672edfe95"
        },
        "date": 1720534865683,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.020804292866663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19224703880000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Kian Paimani",
            "username": "kianenigma",
            "email": "5588131+kianenigma@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "02e50adf7ba6cc65a9ef5c332b3e2974c8d23f48",
          "message": "Explain usage of `<T: Config>` in FRAME storage + Update parachain pallet template  (#4941)\n\nExplains one of the annoying parts of FRAME storage that we have seen\nmultiple times in PBA everyone gets stuck on.\n\nI have not updated the other two templates for now, and only reflected\nit in the parachain template. That can happen in a follow-up.\n\n- [x] Update possible answers in SE about the same topic.\n\n---------\n\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-10T16:46:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/02e50adf7ba6cc65a9ef5c332b3e2974c8d23f48"
        },
        "date": 1720636122191,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18245507319999998,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.00779246036667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Javier Bullrich",
            "username": "Bullrich",
            "email": "javier@bullrich.dev"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "6dd777ffe62cff936e04a76134ccf07de9dee429",
          "message": "fixed cmd bot commenting not working (#5000)\n\nFixed the mentioned issue:\nhttps://github.com/paritytech/command-bot/issues/113#issuecomment-2222277552\n\nNow it will properly comment when the old bot gets triggered.",
          "timestamp": "2024-07-11T13:02:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/6dd777ffe62cff936e04a76134ccf07de9dee429"
        },
        "date": 1720711130749,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.176581682400002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20053501526666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "53598b8ef5c17d4328dab47f1540bfa80649b1a0",
          "message": "Remove usage of `sp-std` on templates (#5001)\n\nFollowing PR for https://github.com/paritytech/polkadot-sdk/pull/4941\nthat removes usage of `sp-std` on templates\n\n`sp-std` crate was proposed to deprecate on\nhttps://github.com/paritytech/polkadot-sdk/issues/2101\n\n@kianenigma\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-11T23:35:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/53598b8ef5c17d4328dab47f1540bfa80649b1a0"
        },
        "date": 1720750006721,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.066661278066665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17302450836666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1f8e44831d0a743b116dfb5948ea9f4756955962",
          "message": "Bridges V2 refactoring backport and `pallet_bridge_messages` simplifications (#4935)\n\n## Summary\n\nThis PR contains migrated code from the Bridges V2\n[branch](https://github.com/paritytech/polkadot-sdk/pull/4427) from the\nold `parity-bridges-common`\n[repo](https://github.com/paritytech/parity-bridges-common/tree/bridges-v2).\nEven though the PR looks large, it does not (or should not) contain any\nsignificant changes (also not relevant for audit).\nThis PR is a requirement for permissionless lanes, as they were\nimplemented on top of these changes.\n\n## TODO\n\n- [x] generate fresh weights for BridgeHubs\n- [x] run `polkadot-fellows` bridges zombienet tests with actual runtime\n1.2.5. or 1.2.6 to check compatibility\n- :ballot_box_with_check: working, checked with 1.2.8 fellows BridgeHubs\n- [x] run `polkadot-sdk` bridges zombienet tests\n  - :ballot_box_with_check: with old relayer in CI (1.6.5) \n- [x] run `polkadot-sdk` bridges zombienet tests (locally) - with the\nrelayer based on this branch -\nhttps://github.com/paritytech/parity-bridges-common/pull/3022\n- [x] check/fix relayer companion in bridges repo -\nhttps://github.com/paritytech/parity-bridges-common/pull/3022\n- [x] extract pruning stuff to separate PR\nhttps://github.com/paritytech/polkadot-sdk/pull/4944\n\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2976\nRelates to:\nhttps://github.com/paritytech/parity-bridges-common/issues/2451\n\n---------\n\nSigned-off-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Serban Iorga <serban@parity.io>\nCo-authored-by: Svyatoslav Nikolsky <svyatonik@gmail.com>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-12T08:20:56Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1f8e44831d0a743b116dfb5948ea9f4756955962"
        },
        "date": 1720778517569,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19755496653333335,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.161793651333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d31285a1562318959a7b21dbfec95c2fd6f06d7a",
          "message": "[statement-distribution] Add metrics for distributed statements in V2 (#4554)\n\nPart of https://github.com/paritytech/polkadot-sdk/issues/4334",
          "timestamp": "2024-07-12T10:43:48Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d31285a1562318959a7b21dbfec95c2fd6f06d7a"
        },
        "date": 1720787197203,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.039297010566665,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2167490387,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dharjeezy",
            "username": "dharjeezy",
            "email": "dharjeezy@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4aa29a41cf731b8181f03168240e8dedb2adfa7a",
          "message": "Try State Hook for Bounties (#4563)\n\nPart of: https://github.com/paritytech/polkadot-sdk/issues/239\n\nPolkadot address: 12GyGD3QhT4i2JJpNzvMf96sxxBLWymz4RdGCxRH5Rj5agKW\n\n---------\n\nSigned-off-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-12T11:51:33Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4aa29a41cf731b8181f03168240e8dedb2adfa7a"
        },
        "date": 1720794717267,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.095873097833334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24598976506666662,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d2dff5f1c3f705c5acdad040447822f92bb02891",
          "message": "network/tx: Ban peers with tx that fail to decode (#5002)\n\nA malicious peer can submit random bytes on transaction protocol.\nIn this case, the peer is not disconnected or reported back to the\npeerstore.\n\nThis PR ensures the peer's reputation is properly reported.\n\nDiscovered during testing:\n- https://github.com/paritytech/polkadot-sdk/pull/4977\n\n\ncc @paritytech/networking\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-15T08:31:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d2dff5f1c3f705c5acdad040447822f92bb02891"
        },
        "date": 1721038807098,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.969914110766663,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19434548913333335,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "291210aa0fafa97d9b924fe82d68c023bdb0a340",
          "message": "Use sp_runtime::traits::BadOrigin (#5011)\n\nIt says `Will be removed after July 2023` but that's not true 😃\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-15T10:45:49Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/291210aa0fafa97d9b924fe82d68c023bdb0a340"
        },
        "date": 1721046639432,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.943929003500006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20039562543333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "7ecf3f757a5d6f622309cea7f788e8a547a5dce8",
          "message": "Remove most all usage of `sp-std` (#5010)\n\nThis should remove nearly all usage of `sp-std` except:\n- bridge and bridge-hubs\n- a few of frames re-export `sp-std`, keep them for now\n- there is a usage of `sp_std::Writer`, I don't have an idea how to move\nit\n\nPlease review proc-macro carefully. I'm not sure I'm doing it the right\nway.\n\nNote: need `/bot fmt`\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-15T13:50:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/7ecf3f757a5d6f622309cea7f788e8a547a5dce8"
        },
        "date": 1721058506632,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.223482810499998,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17959959193333336,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Branislav Kontur",
            "username": "bkontur",
            "email": "bkontur@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "c05b0b970942c2bc298f61be62cc2e2b5d46af19",
          "message": "Updated substrate-relay version for tests (#5017)\n\n## Testing\n\nBoth Bridges zombienet tests passed, e.g.:\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6698640\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6698641\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6700072\nhttps://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6700073",
          "timestamp": "2024-07-15T21:10:03Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/c05b0b970942c2bc298f61be62cc2e2b5d46af19"
        },
        "date": 1721084041716,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.2400473834,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.21589520773333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Niklas Adolfsson",
            "username": "niklasad1",
            "email": "niklasadolfsson1@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "926c1b6adcaa767f4887b5774ef1fdde75156dd9",
          "message": "rpc: add back rpc logger (#4952)\n\nCo-authored-by: Bastian Köcher <git@kchr.de>",
          "timestamp": "2024-07-15T22:57:08Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/926c1b6adcaa767f4887b5774ef1fdde75156dd9"
        },
        "date": 1721090389243,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.19637519589999997,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.221900248933334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "79a3d6c294430a52f00563f8a5e59984680b889f",
          "message": "Adjust base value for statement-distribution regression tests (#5028)\n\nA baseline for the statement-distribution regression test was set only\nin the beginning and now we see that the actual values a bit lower.\n\n<img width=\"1001\" alt=\"image\"\nsrc=\"https://github.com/user-attachments/assets/40b06eec-e38f-43ad-b437-89eca502aa66\">\n\n\n[Source](https://paritytech.github.io/polkadot-sdk/bench/statement-distribution-regression-bench)",
          "timestamp": "2024-07-16T11:31:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/79a3d6c294430a52f00563f8a5e59984680b889f"
        },
        "date": 1721136038227,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.865076441700001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17509883266666665,
            "unit": "seconds"
          }
        ]
      },
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
          "id": "66baa2fb307fe72cb9ddc7c3be16ba57fcb2670a",
          "message": "[ci] Update forklift in CI image (#5032)\n\ncc https://github.com/paritytech/ci_cd/issues/939",
          "timestamp": "2024-07-16T14:48:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/66baa2fb307fe72cb9ddc7c3be16ba57fcb2670a"
        },
        "date": 1721146578226,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.865076441700001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.17509883266666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "975e04bbb59b643362f918e8521f0cde5c27fbc8",
          "message": "Send PeerViewChange with high priority (#4755)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/577\n\n### Changed\n- `orchestra` updated to 0.4.0\n- `PeerViewChange` sent with high priority and should be processed first\nin a queue.\n- To count them in tests added tracker to TestSender and TestOverseer.\nIt acts more like a smoke test though.\n\n### Testing on Versi\n\nThe changes were tested on Versi with two objectives:\n1. Make sure the node functionality does not change.\n2. See how the changes affect performance.\n\nTest setup:\n- 2.5 hours for each case\n- 100 validators\n- 50 parachains\n- validatorsPerCore = 2\n- neededApprovals = 100\n- nDelayTranches = 89\n- relayVrfModuloSamples = 50\n\nDuring the test period, all nodes ran without any crashes, which\nsatisfies the first objective.\n\nTo estimate the change in performance we used ToF charts. The graphs\nshow that there are no spikes in the top as before. This proves that our\nhypothesis is correct.\n\n### Normalized charts with ToF\n\n![image](https://github.com/user-attachments/assets/0d49d0db-8302-4a8c-a557-501856805ff5)\n[Before](https://grafana.teleport.parity.io/goto/ZoR53ClSg?orgId=1)\n\n\n![image](https://github.com/user-attachments/assets/9cc73784-7e45-49d9-8212-152373c05880)\n[After](https://grafana.teleport.parity.io/goto/6ux5qC_IR?orgId=1)\n\n### Conclusion\n\nThe prioritization of subsystem messages reduces the ToF of the\nnetworking subsystem, which helps faster propagation of gossip messages.",
          "timestamp": "2024-07-16T17:56:25Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/975e04bbb59b643362f918e8521f0cde5c27fbc8"
        },
        "date": 1721161231992,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.2040626749666667,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.87594991383333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alin Dima",
            "username": "alindima",
            "email": "alin@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "0db509263c18ff011dcb64af0b0e87f6f68a7c16",
          "message": "add elastic scaling MVP guide (#4663)\n\nResolves https://github.com/paritytech/polkadot-sdk/issues/4468\n\nGives instructions on how to enable elastic scaling MVP to parachain\nteams.\n\nStill a draft because it depends on further changes we make to the\nslot-based collator:\nhttps://github.com/paritytech/polkadot-sdk/pull/4097\n\nParachains cannot use this yet because the collator was not released and\nno relay chain network has been configured for elastic scaling yet",
          "timestamp": "2024-07-17T09:27:11Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/0db509263c18ff011dcb64af0b0e87f6f68a7c16"
        },
        "date": 1721214549788,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22698849216666672,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.108043464266668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Egor_P",
            "username": "EgorPopelyaev",
            "email": "egor@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "739951991f14279a7dc05d42c29ccf57d3740a4c",
          "message": "Adjust release flows to use those with the new branch model (#5015)\n\nThis PR contains adjustments of the node release pipelines so that it\nwill be possible to use those to trigger release actions based on the\n`stable` branch.\n\nPreviously the whole pipeline of the flows from [creation of the\n`rc-tag`](https://github.com/paritytech/polkadot-sdk/blob/master/.github/workflows/release-10_rc-automation.yml)\n(v1.15.0-rc1, v1.15.0-rc2, etc) till [the release draft\ncreation](https://github.com/paritytech/polkadot-sdk/blob/master/.github/workflows/release-30_publish_release_draft.yml)\nwas triggered on push to the node release branch. As we had the node\nrelease branch and the crates release branch separately, it worked fine.\n\nFrom now on, as we are switching to the one branch approach, for the\nfirst iteration I would like to keep things simple to see how the new\nrelease process will work with both parts (crates and node) made from\none branch.\n\nChanges made: \n\n- The first step in the pipeline (rc-tag creation) will be triggered\nmanually instead of the push to the branch\n- The tag version will be set manually from the input instead of to be\ntaken from the branch name\n- Docker image will be additionally tagged as `stable`\n\n\n\nCloses: https://github.com/paritytech/release-engineering/issues/214",
          "timestamp": "2024-07-17T11:28:58Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/739951991f14279a7dc05d42c29ccf57d3740a4c"
        },
        "date": 1721223950773,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.18727590923333332,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.799947688233333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Sebastian Kunert",
            "username": "skunert",
            "email": "skunert49@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "1b6292bf7c71d56b793e98b651799f41bb0ef76b",
          "message": "Do not crash on block gap in `displaced_leaves_after_finalizing` (#4997)\n\nAfter the merge of #4922 we saw failing zombienet tests with the\nfollowing error:\n```\n2024-07-09 10:30:09 Error applying finality to block (0xb9e1d3d9cb2047fe61667e28a0963e0634a7b29781895bc9ca40c898027b4c09, 56685): UnknownBlock: Header was not found in the database: 0x0000000000000000000000000000000000000000000000000000000000000000    \n2024-07-09 10:30:09 GRANDPA voter error: could not complete a round on disk: UnknownBlock: Header was not found in the database: 0x0000000000000000000000000000000000000000000000000000000000000000    \n```\n\n[Example](https://gitlab.parity.io/parity/mirrors/polkadot-sdk/-/jobs/6662262)\n\nThe crashing situation is warp-sync related. After warp syncing, it can\nhappen that there are gaps in block ancestry where we don't have the\nheader. At the same time, the genesis hash is in the set of leaves. In\n`displaced_leaves_after_finalizing` we then iterate from the finalized\nblock backwards until we hit an unknown block, crashing the node.\n\nThis PR makes the detection of displaced branches resilient against\nunknown block in the finalized block chain.\n\ncc @nazar-pc (github won't let me request a review from you)\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-17T15:41:26Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/1b6292bf7c71d56b793e98b651799f41bb0ef76b"
        },
        "date": 1721237236580,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.010660993733334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.20628284670000002,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "b862b181ec507e1510dff6d78335b184b395d9b2",
          "message": "fix: Update libp2p-websocket to v0.42.2 to fix panics (#5040)\n\nThis release includes: https://github.com/libp2p/rust-libp2p/pull/5482\n\nWhich fixes substrate node crashing with libp2p trace:\n\n```\n 0: sp_panic_handler::set::{{closure}}\n   1: std::panicking::rust_panic_with_hook\n   2: std::panicking::begin_panic::{{closure}}\n   3: std::sys_common::backtrace::__rust_end_short_backtrace\n   4: std::panicking::begin_panic\n   5: <quicksink::SinkImpl<S,F,T,A,E> as futures_sink::Sink<A>>::poll_ready\n   6: <rw_stream_sink::RwStreamSink<S> as futures_io::if_std::AsyncWrite>::poll_write\n   7: <libp2p_noise::io::framed::NoiseFramed<T,S> as futures_sink::Sink<&alloc::vec::Vec<u8>>>::poll_ready\n   8: <libp2p_noise::io::Output<T> as futures_io::if_std::AsyncWrite>::poll_write\n   9: <yamux::frame::io::Io<T> as futures_sink::Sink<yamux::frame::Frame<()>>>::poll_ready\n  10: yamux::connection::Connection<T>::poll_next_inbound\n  11: <libp2p_yamux::Muxer<C> as libp2p_core::muxing::StreamMuxer>::poll\n  12: <libp2p_core::muxing::boxed::Wrap<T> as libp2p_core::muxing::StreamMuxer>::poll\n  13: <libp2p_core::muxing::boxed::Wrap<T> as libp2p_core::muxing::StreamMuxer>::poll\n  14: libp2p_swarm::connection::pool::task::new_for_established_connection::{{closure}}\n  15: <sc_service::task_manager::prometheus_future::PrometheusFuture<T> as core::future::future::Future>::poll\n  16: <futures_util::future::select::Select<A,B> as core::future::future::Future>::poll\n  17: <tracing_futures::Instrumented<T> as core::future::future::Future>::poll\n  18: std::panicking::try\n  19: tokio::runtime::task::harness::Harness<T,S>::poll\n  20: tokio::runtime::scheduler::multi_thread::worker::Context::run_task\n  21: tokio::runtime::scheduler::multi_thread::worker::Context::run\n  22: tokio::runtime::context::set_scheduler\n  23: tokio::runtime::context::runtime::enter_runtime\n  24: tokio::runtime::scheduler::multi_thread::worker::run\n  25: tokio::runtime::task::core::Core<T,S>::poll\n  26: tokio::runtime::task::harness::Harness<T,S>::poll\n  27: std::sys_common::backtrace::__rust_begin_short_backtrace\n  28: core::ops::function::FnOnce::call_once{{vtable.shim}}\n  29: std::sys::pal::unix::thread::Thread::new::thread_start\n  30: <unknown>\n  31: <unknown>\n\n\nThread 'tokio-runtime-worker' panicked at 'SinkImpl::poll_ready called after error.', /home/ubuntu/.cargo/registry/src/index.crates.io-6f17d22bba15001f/quicksink-0.1.2/src/lib.rs:158\n```\n\nCloses: https://github.com/paritytech/polkadot-sdk/issues/4934\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-17T17:04:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/b862b181ec507e1510dff6d78335b184b395d9b2"
        },
        "date": 1721243565948,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.9441507065,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2009658755333333,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Tom",
            "username": "senseless",
            "email": "tsenseless@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "4bcdf8166972b63a70b618c90424b9f7d64b719b",
          "message": "Update the stake.plus bootnode addresses (#5039)\n\nUpdate the stake.plus bootnode addresses\n\n---------\n\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-17T21:36:51Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/4bcdf8166972b63a70b618c90424b9f7d64b719b"
        },
        "date": 1721258686093,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.9690499113,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.19701217403333332,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Jun Jiang",
            "username": "jasl",
            "email": "jasl9187@hotmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "25d9b59e6d0bf74637796a99d1adf84725468e0a",
          "message": "upgrade `wasm-bindgen` to 0.2.92 (#5056)\n\nThe rustc warns\n\n```\nThe package `wasm-bindgen v0.2.87` currently triggers the following future incompatibility lints:\n> warning: older versions of the `wasm-bindgen` crate will be incompatible with future versions of Rust; please update to `wasm-bindgen` v0.2.88\n>   |\n>   = warning: this was previously accepted by the compiler but is being phased out; it will become a hard error in a future release!\n>   = note: for more information, see issue #71871 <https://github.com/rust-lang/rust/issues/71871>\n>\n```",
          "timestamp": "2024-07-18T08:01:37Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/25d9b59e6d0bf74637796a99d1adf84725468e0a"
        },
        "date": 1721296962047,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.1657066704333333,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.716310839966667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d05cd9a55de64318f0ea16a47b21a0af4204c522",
          "message": "Bump enumn from 0.1.12 to 0.1.13 (#5061)\n\nBumps [enumn](https://github.com/dtolnay/enumn) from 0.1.12 to 0.1.13.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/dtolnay/enumn/releases\">enumn's\nreleases</a>.</em></p>\n<blockquote>\n<h2>0.1.13</h2>\n<ul>\n<li>Update proc-macro2 to fix caching issue when using a rustc-wrapper\nsuch as sccache</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/de964e3ce0463f01dabb94c168d507254facfb86\"><code>de964e3</code></a>\nRelease 0.1.13</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/52dcafcb2ee193be8839dc0f96bfa0e151888645\"><code>52dcafc</code></a>\nPull in proc-macro2 sccache fix</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/ba2e288a83c5e62d1e29b993523ccf0528043ab0\"><code>ba2e288</code></a>\nTest docs.rs documentation build in CI</li>\n<li><a\nhref=\"https://github.com/dtolnay/enumn/commit/6f5a37e5a9dcdb75987552b44d7ebdbd7f0a2a93\"><code>6f5a37e</code></a>\nUpdate actions/checkout@v3 -&gt; v4</li>\n<li>See full diff in <a\nhref=\"https://github.com/dtolnay/enumn/compare/0.1.12...0.1.13\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=enumn&package-manager=cargo&previous-version=0.1.12&new-version=0.1.13)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-18T14:06:02Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d05cd9a55de64318f0ea16a47b21a0af4204c522"
        },
        "date": 1721317645642,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.032316359866666,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.16829653216666665,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Juan Ignacio Rios",
            "username": "JuaniRios",
            "email": "54085674+JuaniRios@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ad9804f7b1e707d5144bcdc14b67c19db2975da8",
          "message": "Add `pub` to xcm::v4::PalletInfo (#4976)\n\nv3 PalletInfo had the fields public, but not v4. Any reason why?\nI need the PalletInfo fields public so I can read the values and do some\nlogic based on that at Polimec\n@franciscoaguirre \n\nIf this could be backported would be highly appreciated 🙏🏻\n\n---------\n\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: Oliver Tale-Yazdi <oliver.tale-yazdi@parity.io>",
          "timestamp": "2024-07-18T19:35:00Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ad9804f7b1e707d5144bcdc14b67c19db2975da8"
        },
        "date": 1721337485873,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.367816785500002,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.24675438656666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Özgün Özerk",
            "username": "ozgunozerk",
            "email": "ozgunozerk.elo@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f8f70b37562e3519401f8c1fada9a2c55589e0c6",
          "message": "relax `XcmFeeToAccount` trait bound on `AccountId` (#4959)\n\nFixes #4960 \n\nConfiguring `FeeManager` enforces the boundary `Into<[u8; 32]>` for the\n`AccountId` type.\n\nHere is how it works currently: \n\nConfiguration:\n```rust\n    type FeeManager = XcmFeeManagerFromComponents<\n        IsChildSystemParachain<primitives::Id>,\n        XcmFeeToAccount<Self::AssetTransactor, AccountId, TreasuryAccount>,\n    >;\n```\n\n`XcmToFeeAccount` struct:\n```rust\n/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain\n/// `ReceiverAccount`.\n///\n/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If\n/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be\n/// logged and the fee burned.\npub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(\n\tPhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,\n);\n\nimpl<\n\t\tAssetTransactor: TransactAsset,\n\t\tAccountId: Clone + Into<[u8; 32]>,\n\t\tReceiverAccount: Get<AccountId>,\n\t> HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>\n{\n\tfn handle_fee(fee: Assets, context: Option<&XcmContext>, _reason: FeeReason) -> Assets {\n\t\tdeposit_or_burn_fee::<AssetTransactor, _>(fee, context, ReceiverAccount::get());\n\n\t\tAssets::new()\n\t}\n}\n```\n\n`deposit_or_burn_fee()` function:\n```rust\n/// Try to deposit the given fee in the specified account.\n/// Burns the fee in case of a failure.\npub fn deposit_or_burn_fee<AssetTransactor: TransactAsset, AccountId: Clone + Into<[u8; 32]>>(\n\tfee: Assets,\n\tcontext: Option<&XcmContext>,\n\treceiver: AccountId,\n) {\n\tlet dest = AccountId32 { network: None, id: receiver.into() }.into();\n\tfor asset in fee.into_inner() {\n\t\tif let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {\n\t\t\tlog::trace!(\n\t\t\t\ttarget: \"xcm::fees\",\n\t\t\t\t\"`AssetTransactor::deposit_asset` returned error: {:?}. Burning fee: {:?}. \\\n\t\t\t\tThey might be burned.\",\n\t\t\t\te, asset,\n\t\t\t);\n\t\t}\n\t}\n}\n```\n\n---\n\nIn order to use **another** `AccountId` type (for example, 20 byte\naddresses for compatibility with Ethereum or Bitcoin), one has to\nduplicate the code as the following (roughly changing every `32` to\n`20`):\n```rust\n/// A `HandleFee` implementation that simply deposits the fees into a specific on-chain\n/// `ReceiverAccount`.\n///\n/// It reuses the `AssetTransactor` configured on the XCM executor to deposit fee assets. If\n/// the `AssetTransactor` returns an error while calling `deposit_asset`, then a warning will be\n/// logged and the fee burned.\npub struct XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>(\n    PhantomData<(AssetTransactor, AccountId, ReceiverAccount)>,\n);\nimpl<\n        AssetTransactor: TransactAsset,\n        AccountId: Clone + Into<[u8; 20]>,\n        ReceiverAccount: Get<AccountId>,\n    > HandleFee for XcmFeeToAccount<AssetTransactor, AccountId, ReceiverAccount>\n{\n    fn handle_fee(fee: XcmAssets, context: Option<&XcmContext>, _reason: FeeReason) -> XcmAssets {\n        deposit_or_burn_fee::<AssetTransactor, _>(fee, context, ReceiverAccount::get());\n\n        XcmAssets::new()\n    }\n}\n\npub fn deposit_or_burn_fee<AssetTransactor: TransactAsset, AccountId: Clone + Into<[u8; 20]>>(\n    fee: XcmAssets,\n    context: Option<&XcmContext>,\n    receiver: AccountId,\n) {\n    let dest = AccountKey20 { network: None, key: receiver.into() }.into();\n    for asset in fee.into_inner() {\n        if let Err(e) = AssetTransactor::deposit_asset(&asset, &dest, context) {\n            log::trace!(\n                target: \"xcm::fees\",\n                \"`AssetTransactor::deposit_asset` returned error: {:?}. Burning fee: {:?}. \\\n                They might be burned.\",\n                e, asset,\n            );\n        }\n    }\n}\n```\n\n---\n\nThis results in code duplication, which can be avoided simply by\nrelaxing the trait enforced by `XcmFeeToAccount`.\n\nIn this PR, I propose to introduce a new trait called `IntoLocation` to\nbe able to express both `Into<[u8; 32]>` and `Into<[u8; 20]>` should be\naccepted (and every other `AccountId` type as long as they implement\nthis trait).\n\nCurrently, `deposit_or_burn_fee()` function converts the `receiver:\nAccountId` to a location. I think converting an account to `Location`\nshould not be the responsibility of `deposit_or_burn_fee()` function.\n\nThis trait also decouples the conversion of `AccountId` to `Location`,\nfrom `deposit_or_burn_fee()` function. And exposes `IntoLocation` trait.\nThus, allowing everyone to come up with their `AccountId` type and make\nit compatible for configuring `FeeManager`.\n\n---\n\nNote 1: if there is a better file/location to put `IntoLocation`, I'm\nall ears\n\nNote 2: making `deposit_or_burn_fee` or `XcmToFeeAccount` generic was\nnot possible from what I understood, due to Rust currently do not\nsupport a way to express the generic should implement either `trait A`\nor `trait B` (since the compiler cannot guarantee they won't overlap).\nIn this case, they are `Into<[u8; 32]>` and `Into<[u8; 20]>`.\nSee [this](https://github.com/rust-lang/rust/issues/20400) and\n[this](https://github.com/rust-lang/rfcs/pull/1672#issuecomment-262152934).\n\nNote 3: I should also submit a PR to `frontier` that implements\n`IntoLocation` for `AccountId20` if this PR gets accepted.\n\n\n### Summary \nthis new trait:\n- decouples the conversion of `AccountId` to `Location`, from\n`deposit_or_burn_fee()` function\n- makes `XcmFeeToAccount` accept every possible `AccountId` type as long\nas they they implement `IntoLocation`\n- backwards compatible\n- keeps the API simple and clean while making it less restrictive\n\n\n@franciscoaguirre and @gupnik are already aware of the issue, so tagging\nthem here for visibility.\n\n---------\n\nCo-authored-by: Francisco Aguirre <franciscoaguirreperez@gmail.com>\nCo-authored-by: Branislav Kontur <bkontur@gmail.com>\nCo-authored-by: Adrian Catangiu <adrian@parity.io>\nCo-authored-by: command-bot <>",
          "timestamp": "2024-07-19T11:09:44Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f8f70b37562e3519401f8c1fada9a2c55589e0c6"
        },
        "date": 1721393528160,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.039880686933335,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1830517192666667,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Ankan",
            "username": "Ank4n",
            "email": "10196091+Ank4n@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "394ea70d2ad8d37b1a41854659d989e750758705",
          "message": "[NPoS] Some simple refactors to Delegate Staking (#4981)\n\n## Changes\n- `fn update_payee` is renamed to `fn set_payee` in the trait\n`StakingInterface` since there is also a call `Staking::update_payee`\nwhich does something different, ie used for migrating deprecated\n`Controller` accounts.\n- `set_payee` does not re-dispatch, only mutates ledger.\n- Fix rustdocs for `NominationPools::join`.\n- Add an implementation note about why we cannot allow existing stakers\nto join/bond_extra into the pool.",
          "timestamp": "2024-07-19T16:32:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/394ea70d2ad8d37b1a41854659d989e750758705"
        },
        "date": 1721413768990,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.111055174866664,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1732824341666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Bastian Köcher",
            "username": "bkchr",
            "email": "git@kchr.de"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d649746e840ead01898957329b5f63ddad6e032c",
          "message": "Implements `PoV` export and local validation (#4640)\n\nThis pull requests adds a new CLI flag to `polkadot-parachains`\n`--export-pov-to-path`. This CLI flag will instruct the node to export\nany `PoV` that it build locally to export to the given folder. Then\nthese `PoV` files can be validated using the introduced\n`cumulus-pov-validator`. The combination of export and validation can be\nused for debugging parachain validation issues that may happen on the\nrelay chain.",
          "timestamp": "2024-07-19T21:23:06Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d649746e840ead01898957329b5f63ddad6e032c"
        },
        "date": 1721431136770,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.967191741099999,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.2040535094333334,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Parth Mittal",
            "username": "mittal-parth",
            "email": "76661350+mittal-parth@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "59e3315f7fd74b95e08e6409d1b3f53f6c2123f4",
          "message": "Balances Pallet: Emit events when TI is updated in currency impl (#4936)\n\n# Description\n\nPreviously, in the `Currency` impl, the implementation of\n`pallet_balances` was not emitting any instances of `Issued` and\n`Rescinded` events, even though the `Fungible` equivalent was.\n\nThis PR adds the `Issued` and `Rescinded` events in appropriate places\nin `impl_currency` along with tests.\n\nCloses #4028 \n\npolkadot address: 5GsLutpKjbzsbTphebs9Uy4YK6gTN47MAaz6njPktidjR5cp\n\n---------\n\nCo-authored-by: Bastian Köcher <git@kchr.de>\nCo-authored-by: Bastian Köcher <info@kchr.de>",
          "timestamp": "2024-07-19T23:13:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/59e3315f7fd74b95e08e6409d1b3f53f6c2123f4"
        },
        "date": 1721437108787,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.860193946233332,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1708109938666666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Alexandru Vasile",
            "username": "lexnv",
            "email": "60601340+lexnv@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "9dcf6f12e87c479f7d32ff6ac7869b375ca06de3",
          "message": "beefy: Increment metric and add extra log details (#5075)\n\nThis PR increments the beefy metric wrt no peers to query justification\nfrom.\nThe metric is incremented when we submit a request to a known peer,\nhowever that peer failed to provide a valid response, and there are no\nfurther peers to query.\n\nWhile at it, add a few extra details to identify the number of active\npeers and cached peers, together with the request error\n\nPart of:\n- https://github.com/paritytech/polkadot-sdk/issues/4985\n- https://github.com/paritytech/polkadot-sdk/issues/4925\n\n---------\n\nSigned-off-by: Alexandru Vasile <alexandru.vasile@parity.io>",
          "timestamp": "2024-07-20T14:37:14Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/9dcf6f12e87c479f7d32ff6ac7869b375ca06de3"
        },
        "date": 1721492474629,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.22041037196666666,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 13.134680832966666,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d1979d4be9c44d06c62aeea44d30f5f201d67b5e",
          "message": "Bump assert_cmd from 2.0.12 to 2.0.14 (#5070)\n\nBumps [assert_cmd](https://github.com/assert-rs/assert_cmd) from 2.0.12\nto 2.0.14.\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/assert-rs/assert_cmd/blob/master/CHANGELOG.md\">assert_cmd's\nchangelog</a>.</em></p>\n<blockquote>\n<h2>[2.0.14] - 2024-02-19</h2>\n<h3>Compatibility</h3>\n<ul>\n<li>MSRV is now 1.73.0</li>\n</ul>\n<h3>Features</h3>\n<ul>\n<li>Run using the cargo target runner</li>\n</ul>\n<h2>[2.0.13] - 2024-01-12</h2>\n<h3>Internal</h3>\n<ul>\n<li>Dependency update</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/9ebfc0b140847e926374a8cd0ad235ba93f119f9\"><code>9ebfc0b</code></a>\nchore: Release assert_cmd version 2.0.14</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/025c5f6dcb1ca3b0dfc24983e48aab123d093895\"><code>025c5f6</code></a>\ndocs: Update changelog</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/82b99c139882bdbc3d613094fb0eea944b05419c\"><code>82b99c1</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/193\">#193</a>\nfrom glehmann/cross</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/b3a290ce81873bb266457c6ea7f6d94124ba8ed5\"><code>b3a290c</code></a>\nfeat: add cargo runner support in order to work with cross</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/132db496f6e89454e33b13269b4bb9d42324ce7d\"><code>132db49</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/194\">#194</a>\nfrom assert-rs/renovate/rust-1.x</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/f1308abaf458e22511548bc7f3ddecc2bde579ed\"><code>f1308ab</code></a>\nchore(deps): update msrv to v1.73</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/9b0f20acd4868a00544b5e28a0fcbcad6689afdf\"><code>9b0f20a</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/assert-rs/assert_cmd/issues/192\">#192</a>\nfrom assert-rs/renovate/rust-1.x</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/07f4cdee717ea3b6f96ac7eb1eaeb4ed2253d6af\"><code>07f4cde</code></a>\nchore(deps): update msrv to v1.72</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/19da72b81c789f9c06817b99c1ecebfe7083dbfb\"><code>19da72b</code></a>\nchore: Release assert_cmd version 2.0.13</li>\n<li><a\nhref=\"https://github.com/assert-rs/assert_cmd/commit/db5ee325aafffb5b8e042cda1cc946e36079302a\"><code>db5ee32</code></a>\ndocs: Update changelog</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/assert-rs/assert_cmd/compare/v2.0.12...v2.0.14\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=assert_cmd&package-manager=cargo&previous-version=2.0.12&new-version=2.0.14)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-20T23:26:46Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d1979d4be9c44d06c62aeea44d30f5f201d67b5e"
        },
        "date": 1721525582096,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.817842762033331,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.15195324816666672,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "gupnik",
            "username": "gupnik",
            "email": "mail.guptanikhil@gmail.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "d0d8e29197a783f3ea300569afc50244a280cafa",
          "message": "Fixes doc links for `procedural` crate (#5023)\n\nThis PR fixes the documentation for FRAME Macros when pointed from\n`polkadot_sdk_docs` crate. This is achieved by referring to the examples\nin the `procedural` crate, embedded via `docify`.\n\n---------\n\nCo-authored-by: Kian Paimani <5588131+kianenigma@users.noreply.github.com>",
          "timestamp": "2024-07-22T10:13:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/d0d8e29197a783f3ea300569afc50244a280cafa"
        },
        "date": 1721649566300,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "test-environment",
            "value": 0.15596626180000003,
            "unit": "seconds"
          },
          {
            "name": "availability-recovery",
            "value": 12.820185291266672,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Lulu",
            "username": "Morganamilo",
            "email": "morgan@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "f13ed8de69bcfcccecf208211998b8af2ef882a2",
          "message": "Update parity publish (#5105)",
          "timestamp": "2024-07-22T15:12:59Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/f13ed8de69bcfcccecf208211998b8af2ef882a2"
        },
        "date": 1721667625663,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.072283808333328,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.1886168326,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Andrei Eres",
            "username": "AndreiEres",
            "email": "eresav@me.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "612c1bd3d51c7638fd078b632c57d6bb177e04c6",
          "message": "Prepare PVFs if node is a validator in the next session (#4791)\n\nCloses https://github.com/paritytech/polkadot-sdk/issues/4324\n- On every active leaf candidate-validation subsystem checks if the node\nis the next session authority.\n- If it is, it fetches backed candidates and prepares unknown PVFs.\n- We limit number of PVFs per block to not overload subsystem.",
          "timestamp": "2024-07-22T16:35:05Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/612c1bd3d51c7638fd078b632c57d6bb177e04c6"
        },
        "date": 1721672644850,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 14.242469616866668,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.23862815273333338,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "ac98bc3fa158aa01faa3c0f95d10c8932affbcc2",
          "message": "Bump slotmap from 1.0.6 to 1.0.7 (#5096)\n\nBumps [slotmap](https://github.com/orlp/slotmap) from 1.0.6 to 1.0.7.\n<details>\n<summary>Changelog</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/orlp/slotmap/blob/master/RELEASES.md\">slotmap's\nchangelog</a>.</em></p>\n<blockquote>\n<h1>Version 1.0.7</h1>\n<ul>\n<li>Added <code>clone_from</code> implementations for all slot\nmaps.</li>\n<li>Added <code>try_insert_with_key</code> methods that accept a\nfallible closure.</li>\n<li>Improved performance of insertion and key hashing.</li>\n<li>Made <code>new_key_type</code> resistant to shadowing.</li>\n<li>Made iterators clonable regardless of item type clonability.</li>\n</ul>\n</blockquote>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/c905b6ced490551476cb7c37778eb8128bdea7ba\"><code>c905b6c</code></a>\nRelease 1.0.7.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/cdee6974d5d57fef62f862ffad9ffe668652d26f\"><code>cdee697</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/orlp/slotmap/issues/107\">#107</a> from\nwaywardmonkeys/minor-doc-tweaks</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/4456784a9bce360ec38005c9f4cbf4b4a6f92162\"><code>4456784</code></a>\nFix a typo and add some backticks.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/a7287a2caa80bb4a6d7a380a92a49d48535357a5\"><code>a7287a2</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/orlp/slotmap/issues/99\">#99</a> from\nchloekek/hash-u64</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/eeaf92e5b3627ac5a2035742c6e2818999ac3d0c\"><code>eeaf92e</code></a>\nProvide explicit impl Hash for KeyData</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/ce6e1e02bb2c2074d8d581e87ad9c2f72ce495c3\"><code>ce6e1e0</code></a>\nLint invalid_html_tags has been renamed, but not really necessary.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/941c39301211a03385e8d7915d0d31b6f6f5ecd5\"><code>941c393</code></a>\nFixed remaining references to global namespace in new_key_type\nmacro.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/cf7e44c05d777440687cfa0d439a31fdec50cc3a\"><code>cf7e44c</code></a>\nAdded utility module.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/5575afe1a31c634d5ab15d273ad8793f6711f8b1\"><code>5575afe</code></a>\nEnsure insert always has fast path.</li>\n<li><a\nhref=\"https://github.com/orlp/slotmap/commit/7220adc6fa9defb356699f3d96af736e9ef477b5\"><code>7220adc</code></a>\nCargo fmt and added test case for cloneable iterators.</li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/orlp/slotmap/compare/v1.0.6...v1.0.7\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=slotmap&package-manager=cargo&previous-version=1.0.6&new-version=1.0.7)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-22T22:25:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/ac98bc3fa158aa01faa3c0f95d10c8932affbcc2"
        },
        "date": 1721693333116,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.05306050176667,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.167661199,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "Serban Iorga",
            "username": "serban300",
            "email": "serban@parity.io"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "216e8fa126df9ee819d524834e75a82881681e02",
          "message": "Beefy equivocation: add runtime API methods (#4993)\n\nRelated to https://github.com/paritytech/polkadot-sdk/issues/4523\n\nAdd runtime API methods for:\n- generating the ancestry proof\n- submiting a fork voting report\n- submitting a future voting report",
          "timestamp": "2024-07-23T14:27:42Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/216e8fa126df9ee819d524834e75a82881681e02"
        },
        "date": 1721752368977,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 13.110616988333334,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.18511451006666668,
            "unit": "seconds"
          }
        ]
      },
      {
        "commit": {
          "author": {
            "name": "dependabot[bot]",
            "username": "dependabot[bot]",
            "email": "49699333+dependabot[bot]@users.noreply.github.com"
          },
          "committer": {
            "name": "GitHub",
            "username": "web-flow",
            "email": "noreply@github.com"
          },
          "id": "11dd10b46511647d8663afe063015b16a8c1e124",
          "message": "Bump backtrace from 0.3.69 to 0.3.71 (#5110)\n\nBumps [backtrace](https://github.com/rust-lang/backtrace-rs) from 0.3.69\nto 0.3.71.\n<details>\n<summary>Release notes</summary>\n<p><em>Sourced from <a\nhref=\"https://github.com/rust-lang/backtrace-rs/releases\">backtrace's\nreleases</a>.</em></p>\n<blockquote>\n<h2>0.3.71</h2>\n<p>This is mostly CI changes, with a very mild bump to our effective cc\ncrate version recorded, and a small modification to a previous changeset\nto allow backtrace to run at its current checked-in MSRV on Windows.\nSorry about that! We will be getting 0.3.70 yanked shortly.</p>\n<h2>What's Changed</h2>\n<ul>\n<li>Make sgx functions exist with cfg(miri) by <a\nhref=\"https://github.com/saethlin\"><code>@​saethlin</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/591\">rust-lang/backtrace-rs#591</a></li>\n<li>Update version of cc crate by <a\nhref=\"https://github.com/jfgoog\"><code>@​jfgoog</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/592\">rust-lang/backtrace-rs#592</a></li>\n<li>Pull back MSRV on Windows by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/598\">rust-lang/backtrace-rs#598</a></li>\n<li>Force frame pointers on all i686 tests by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/601\">rust-lang/backtrace-rs#601</a></li>\n<li>Use rustc from stage0 instead of stage0-sysroot by <a\nhref=\"https://github.com/Nilstrieb\"><code>@​Nilstrieb</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/602\">rust-lang/backtrace-rs#602</a></li>\n<li>Cut backtrace 0.3.71 by <a\nhref=\"https://github.com/workingjubilee\"><code>@​workingjubilee</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/599\">rust-lang/backtrace-rs#599</a></li>\n</ul>\n<h2>New Contributors</h2>\n<ul>\n<li><a href=\"https://github.com/jfgoog\"><code>@​jfgoog</code></a> made\ntheir first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/592\">rust-lang/backtrace-rs#592</a></li>\n<li><a href=\"https://github.com/Nilstrieb\"><code>@​Nilstrieb</code></a>\nmade their first contribution in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/602\">rust-lang/backtrace-rs#602</a></li>\n</ul>\n<p><strong>Full Changelog</strong>: <a\nhref=\"https://github.com/rust-lang/backtrace-rs/compare/0.3.70...0.3.71\">https://github.com/rust-lang/backtrace-rs/compare/0.3.70...0.3.71</a></p>\n<h2>0.3.70</h2>\n<h2>New API</h2>\n<ul>\n<li>A <code>BacktraceFrame</code> can now have <code>resolve(&amp;mut\nself)</code> called on it thanks to <a\nhref=\"https://github.com/fraillt\"><code>@​fraillt</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/526\">rust-lang/backtrace-rs#526</a></li>\n</ul>\n<h2>Platform Support</h2>\n<p>We added support for new platforms in this release!</p>\n<ul>\n<li>Thanks to <a href=\"https://github.com/bzEq\"><code>@​bzEq</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/508\">rust-lang/backtrace-rs#508</a>\nwe now have AIX support!</li>\n<li>Thanks to <a\nhref=\"https://github.com/sthibaul\"><code>@​sthibaul</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/567\">rust-lang/backtrace-rs#567</a>\nwe now have GNU/Hurd support!</li>\n<li>Thanks to <a\nhref=\"https://github.com/dpaoliello\"><code>@​dpaoliello</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/587\">rust-lang/backtrace-rs#587</a>\nwe now support &quot;emulation-compatible&quot; AArch64 Windows (aka\narm64ec)</li>\n</ul>\n<h3>Windows</h3>\n<ul>\n<li>Rewrite msvc backtrace support to be much faster on 64-bit platforms\nby <a\nhref=\"https://github.com/wesleywiser\"><code>@​wesleywiser</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/569\">rust-lang/backtrace-rs#569</a></li>\n<li>Fix i686-pc-windows-gnu missing dbghelp module by <a\nhref=\"https://github.com/wesleywiser\"><code>@​wesleywiser</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/571\">rust-lang/backtrace-rs#571</a></li>\n<li>Fix build errors on <code>thumbv7a-*-windows-msvc</code> targets by\n<a href=\"https://github.com/kleisauke\"><code>@​kleisauke</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/573\">rust-lang/backtrace-rs#573</a></li>\n<li>Fix panic in backtrace symbolication on win7 by <a\nhref=\"https://github.com/roblabla\"><code>@​roblabla</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/578\">rust-lang/backtrace-rs#578</a></li>\n<li>remove few unused windows ffi fn by <a\nhref=\"https://github.com/klensy\"><code>@​klensy</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/576\">rust-lang/backtrace-rs#576</a></li>\n<li>Make dbghelp look for PDBs next to their exe/dll. by <a\nhref=\"https://github.com/michaelwoerister\"><code>@​michaelwoerister</code></a>\nin <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/584\">rust-lang/backtrace-rs#584</a></li>\n<li>Revert 32-bit dbghelp to a version WINE (presumably) likes by <a\nhref=\"https://github.com/ChrisDenton\"><code>@​ChrisDenton</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/588\">rust-lang/backtrace-rs#588</a></li>\n<li>Update for Win10+ by <a\nhref=\"https://github.com/ChrisDenton\"><code>@​ChrisDenton</code></a> in\n<a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/589\">rust-lang/backtrace-rs#589</a></li>\n</ul>\n<h3>SGX</h3>\n<p>Thanks to</p>\n<ul>\n<li>Adjust frame IP in SGX relative to image base by <a\nhref=\"https://github.com/mzohreva\"><code>@​mzohreva</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/566\">rust-lang/backtrace-rs#566</a></li>\n</ul>\n<h2>Internals</h2>\n<p>We did a bunch more work on our CI and internal cleanups</p>\n<ul>\n<li>Modularise CI workflow and validate outputs for binary size checks.\nby <a href=\"https://github.com/detly\"><code>@​detly</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/549\">rust-lang/backtrace-rs#549</a></li>\n<li>Commit Cargo.lock by <a\nhref=\"https://github.com/bjorn3\"><code>@​bjorn3</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/562\">rust-lang/backtrace-rs#562</a></li>\n<li>Enable calling build.rs externally v2 by <a\nhref=\"https://github.com/pitaj\"><code>@​pitaj</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/568\">rust-lang/backtrace-rs#568</a></li>\n<li>Upgrade to 2021 ed and inline panics by <a\nhref=\"https://github.com/nyurik\"><code>@​nyurik</code></a> in <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/pull/538\">rust-lang/backtrace-rs#538</a></li>\n</ul>\n<!-- raw HTML omitted -->\n</blockquote>\n<p>... (truncated)</p>\n</details>\n<details>\n<summary>Commits</summary>\n<ul>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/7be8953188582ea83f1e88622ccdfcab1a49461c\"><code>7be8953</code></a><code>rust-lang/backtrace-rs#599</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/c31ea5ba7ac52f5c15c65cfec7d7b5d0bcf00eed\"><code>c31ea5b</code></a><code>rust-lang/backtrace-rs#602</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/193125abc094b433859c4fdb2e672d391a6bdf8d\"><code>193125a</code></a><code>rust-lang/backtrace-rs#601</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/bdc8b8241b16bb20124a3cec86e1b339e4c008a1\"><code>bdc8b82</code></a>\nMerge pull request <a\nhref=\"https://redirect.github.com/rust-lang/backtrace-rs/issues/598\">#598</a>\nfrom workingjubilee/pull-back-msrv</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/edc9f5cae874bf008e52558e4b2c6c86847c9575\"><code>edc9f5c</code></a>\nhack out binary size checks</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/4c8fe973eb39f4cd31c3d6dfd74c6b670de6911a\"><code>4c8fe97</code></a>\nadd Windows to MSRV tests</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/84dfe2472456a000d7cced566b06f3bada898f8e\"><code>84dfe24</code></a>\nhack CI</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/3f08ec085fb5bb4edfb084cf9e3170e953a44107\"><code>3f08ec0</code></a>\nPull back MSRV-breaking ptr::from_ref</li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/6fa4b85b9962c3e1be8c2e5cc605cd078134152b\"><code>6fa4b85</code></a><code>rust-lang/backtrace-rs#592</code></li>\n<li><a\nhref=\"https://github.com/rust-lang/backtrace-rs/commit/ea7dc8e964d0046d92382e40308876130e5301ba\"><code>ea7dc8e</code></a><code>rust-lang/backtrace-rs#591</code></li>\n<li>Additional commits viewable in <a\nhref=\"https://github.com/rust-lang/backtrace-rs/compare/0.3.69...0.3.71\">compare\nview</a></li>\n</ul>\n</details>\n<br />\n\n\n[![Dependabot compatibility\nscore](https://dependabot-badges.githubapp.com/badges/compatibility_score?dependency-name=backtrace&package-manager=cargo&previous-version=0.3.69&new-version=0.3.71)](https://docs.github.com/en/github/managing-security-vulnerabilities/about-dependabot-security-updates#about-compatibility-scores)\n\nDependabot will resolve any conflicts with this PR as long as you don't\nalter it yourself. You can also trigger a rebase manually by commenting\n`@dependabot rebase`.\n\n[//]: # (dependabot-automerge-start)\n[//]: # (dependabot-automerge-end)\n\n---\n\n<details>\n<summary>Dependabot commands and options</summary>\n<br />\n\nYou can trigger Dependabot actions by commenting on this PR:\n- `@dependabot rebase` will rebase this PR\n- `@dependabot recreate` will recreate this PR, overwriting any edits\nthat have been made to it\n- `@dependabot merge` will merge this PR after your CI passes on it\n- `@dependabot squash and merge` will squash and merge this PR after\nyour CI passes on it\n- `@dependabot cancel merge` will cancel a previously requested merge\nand block automerging\n- `@dependabot reopen` will reopen this PR if it is closed\n- `@dependabot close` will close this PR and stop Dependabot recreating\nit. You can achieve the same result by closing it manually\n- `@dependabot show <dependency name> ignore conditions` will show all\nof the ignore conditions of the specified dependency\n- `@dependabot ignore this major version` will close this PR and stop\nDependabot creating any more for this major version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this minor version` will close this PR and stop\nDependabot creating any more for this minor version (unless you reopen\nthe PR or upgrade to it yourself)\n- `@dependabot ignore this dependency` will close this PR and stop\nDependabot creating any more for this dependency (unless you reopen the\nPR or upgrade to it yourself)\n\n\n</details>\n\nSigned-off-by: dependabot[bot] <support@github.com>\nCo-authored-by: dependabot[bot] <49699333+dependabot[bot]@users.noreply.github.com>",
          "timestamp": "2024-07-23T16:38:21Z",
          "url": "https://github.com/paritytech/polkadot-sdk/commit/11dd10b46511647d8663afe063015b16a8c1e124"
        },
        "date": 1721759304371,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 307203,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 1.6666666666666665,
            "unit": "KiB"
          },
          {
            "name": "availability-recovery",
            "value": 12.747675927100001,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 0.180799625,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}