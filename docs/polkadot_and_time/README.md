# Polkadot and Time

Slides explaining why slot semantics matter for Polkadot, and why the
v3 elastic-scaling timing model (#12063) is the right fix — not just
cleanup or over-engineering.

## Files

- `slides.md` — Marp-flavored slide deck.
- `notes.md` — raw notes / source material.
- `shell.nix` — drop into `nix-shell` to get `marp-cli`.

## Building

```sh
nix-shell
marp slides.md -o slides.html               # html (live-reloadable with -p)
marp slides.md -o slides.pdf --pdf          # pdf
marp slides.md -o slides.pptx --pptx        # powerpoint
```

Preview while editing:

```sh
nix-shell --run "marp -p slides.md"
```

## Audience

Internal Parity engineering — specifically the elastic-scaling /
collator-protocol / statement-distribution group.

## Issues referenced

- `#12063` — the proposal: enforce minimum claim-queue offset.
- `#12028` — statement-distribution latency (complementary work).
- `#10921` — the data: 14× MVR for non-EU validators.
- `#11903` — v4 collator protocol for resubmissions (unblocked by this).
- `#11621`, `#11453` — historical slot-offset bugs.
- `#8893`, `#9428` — claim-queue offset / core selector context.
