window.BENCHMARK_DATA = {
  "lastUpdate": 1712671141039,
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
      }
    ]
  }
}
