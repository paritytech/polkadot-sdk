# How to use

```yml
  set-image:
    runs-on: macos-latest
    steps:
      - name: Checkout
        uses: actions/checkout@v4
      - id: set_image
        run: cat .github/env >> $GITHUB_OUTPUT
      - name: Install dependencies
        uses: ./.github/actions/set-up-mac
        with:
          IMAGE: ${{ steps.set-image.outputs.IMAGE }}
```
