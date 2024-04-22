window.BENCHMARK_DATA = {
  "lastUpdate": 1713807778772,
  "repoUrl": "https://github.com/paritytech/polkadot-sdk",
  "entries": {
    "approval-voting-regression-bench": [
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
        "date": 1713807753741,
        "tool": "customSmallerIsBetter",
        "benches": [
          {
            "name": "Received from peers",
            "value": 52940.5,
            "unit": "KiB"
          },
          {
            "name": "Sent to peers",
            "value": 63548.46,
            "unit": "KiB"
          },
          {
            "name": "approval-distribution",
            "value": 7.115701152709936,
            "unit": "seconds"
          },
          {
            "name": "approval-voting",
            "value": 9.713580235960006,
            "unit": "seconds"
          },
          {
            "name": "test-environment",
            "value": 3.0335909815501774,
            "unit": "seconds"
          }
        ]
      }
    ]
  }
}