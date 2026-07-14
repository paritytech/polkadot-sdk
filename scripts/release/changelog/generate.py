#!/usr/bin/env python3
"""Generate the release changelog outputs from a folder of prdoc files.

Produces three files in --output-dir:
  changelog_body.md  Markdown fragment placed into the GitHub release body by
                     build-changelogs.sh (substituted into the rendered draft AFTER
                     tera runs, so prdoc content is never parsed as a template).
                     Kept under --max-body-chars via a deterministic degradation
                     ladder; the driving script computes that budget from the size
                     of the surrounding document so the final body stays under
                     GitHub's 125000-char cap. Named distinctly from CHANGELOG.md
                     so the two can coexist on case-insensitive filesystems (macOS).
  CHANGELOG.md       The complete, untruncated changelog, attached as a release asset:
                     same topic structure plus a per-audience appendix.
  changelog.json     Machine-readable changelog, attached as a release asset. The format
                     is described by schema.json next to this script.

Topics are resolved per prdoc: an explicit `topic:` field (matched case-insensitively
against an id or label from --topics) wins; otherwise the topic is derived from the
affected crates via the ordered glob patterns in --topics; otherwise the last topic
(Misc) is used.

Malformed prdocs (empty files, non-mapping YAML, unknown audiences) are tolerated with
a warning so a single bad file cannot abort a release; --check turns warnings into a
non-zero exit for CI.

Only needs python3 + PyYAML. No network access, no containers.
"""

import argparse
import fnmatch
import hashlib
import json
import re
import sys
from pathlib import Path
from urllib.parse import quote

import yaml

REPO_URL = "https://github.com/paritytech/polkadot-sdk"
SCHEMA_VERSION = 1
CANONICAL_AUDIENCES = ("Node Dev", "Runtime Dev", "Node Operator", "Runtime User")
AUDIENCE_ALIASES = {re.sub(r"[\W_]", "", a).lower(): a for a in CANONICAL_AUDIENCES}
# Description truncation limits tried in order until the body fits the budget; the
# final (0, drop-all) rung in main() removes descriptions entirely and always converges.
TRUNCATION_LADDER = (700, 550, 450, 400, 350, 300, 250)
BUMP_RANK = {"major": 3, "minor": 2, "patch": 1, "none": 0}
CRATE_LIST_CAP = 6
# Colored audience badges: shields.io images via markdown reference syntax, so each
# use costs only `![Audience][ref]` and the URL is defined once per document.
AUDIENCE_BADGES = {
    "Node Dev": ("nd", "a78bfa"),
    "Runtime Dev": ("rd", "60a5fa"),
    "Node Operator": ("no", "fbbf24"),
    "Runtime User": ("ru", "34d399"),
}
DEFAULT_BADGE_COLOR = "9ca3af"
# Legacy prdocs escaped tera-significant text for the old pipeline ({{ "{{" }}closure...);
# the new pipeline never tera-processes prdoc content, so unescape it back.
TERA_ESCAPE_RE = re.compile(r'\{\{\s*"(\{\{|\}\})"\s*\}\}')
FENCE_RE = re.compile(r"^ {0,3}(`{3,}|~{3,})")


def log(msg):
    print(msg, file=sys.stderr)


def text_or_none(value):
    """YAML scalars can parse as dates/bools/ints; the JSON model wants strings."""
    return None if value is None else str(value)


def one_line(value):
    """Titles are interpolated into single-line markdown constructs (headings, index
    bullets); collapse any whitespace, including newlines from block scalars."""
    return " ".join(str(value).split())


def clean_description(value):
    return TERA_ESCAPE_RE.sub(r"\1", str(value or "")).strip()


def normalize_audiences(value, pr, warnings):
    if value is None or value == "":
        warnings.append(f"pr_{pr}: empty audience")
        return ["Unknown"]
    values = value if isinstance(value, list) else [value]
    seen = []
    for v in values:
        key = re.sub(r"[\W_]", "", str(v)).lower()
        aud = AUDIENCE_ALIASES.get(key)
        if aud is None:
            aud = one_line(v) or "Unknown"
            warnings.append(f"pr_{pr}: unknown audience {str(v)!r}")
        if aud not in seen:
            seen.append(aud)
    return seen or ["Unknown"]


def ordered_audience_union(docs):
    all_auds = [a for d in docs for a in d["audiences"]]
    ordered = [a for a in CANONICAL_AUDIENCES if a in all_auds]
    ordered += [a for a in all_auds if a not in ordered]
    return list(dict.fromkeys(ordered))


def load_topics(path):
    data = yaml.safe_load(Path(path).read_text(encoding="utf-8"))
    topics = data.get("topics") if isinstance(data, dict) else None
    if not topics:
        sys.exit(f"error: no topics found in {path}")
    for t in topics:
        if not t.get("id") or not t.get("label"):
            sys.exit(f"error: topic without id/label in {path}: {t}")
        t.setdefault("patterns", [])
    return topics


def resolve_topic(explicit, crates, topics, pr, warnings):
    fallback = topics[-1]
    if explicit:
        wanted = str(explicit).strip().lower()
        for t in topics:
            if wanted in (t["id"].lower(), t["label"].lower()):
                return t, "explicit"
        warnings.append(f"pr_{pr}: unknown topic {explicit!r}, falling back to derivation")
    best = None
    for crate in crates:
        for idx, t in enumerate(topics):
            if any(fnmatch.fnmatch(crate["name"], p) for p in t["patterns"]):
                if best is None or idx < best:
                    best = idx
                break
    # The catch-all last topic counts as fallback, not a real derivation.
    if best is not None and topics[best] is not fallback:
        return topics[best], "derived"
    return fallback, "fallback"


def max_bump_rank(crates):
    return max((BUMP_RANK.get(c["bump"], 0) for c in crates), default=0)


def pr_number_from_filename(name):
    """PR number from pr_NNNN.prdoc; legacy files may carry a suffix (pr_1234-some-title.prdoc)."""
    m = re.match(r"pr_(\d+)", name)
    return int(m.group(1)) if m else None


def load_prdocs(prdoc_dir, topics, warnings):
    """Returns (entries, expected_prs): entries for every parseable prdoc, and the PR
    numbers that made it in (skipped files are warned about, not silently dropped)."""
    entries = []
    files = []
    for f in Path(prdoc_dir).glob("pr_*.prdoc"):
        pr = pr_number_from_filename(f.name)
        if pr is not None:
            files.append((pr, f))
        else:
            warnings.append(f"skipping unrecognized file name: {f.name}")
    for pr, f in sorted(files):
        try:
            content = yaml.safe_load(f.read_text(encoding="utf-8"))
        except yaml.YAMLError as e:
            warnings.append(f"skipping {f.name}: YAML parse error: {e}")
            continue
        if not isinstance(content, dict):
            warnings.append(f"skipping {f.name}: not a YAML mapping (empty file?)")
            continue

        docs = []
        for item in content.get("doc") or []:
            if not isinstance(item, dict):
                continue
            title = item.get("title")
            docs.append({
                "audiences": normalize_audiences(item.get("audience"), pr, warnings),
                "title": one_line(title) if title is not None else None,
                "description": clean_description(item.get("description")),
            })

        crates = []
        for c in content.get("crates") or []:
            if isinstance(c, str):
                crates.append({"name": c, "bump": None, "note": None})
            elif isinstance(c, dict) and c.get("name"):
                bump = c.get("bump")
                crates.append({
                    "name": str(c["name"]),
                    "bump": str(bump).strip().lower() if bump else None,
                    "note": text_or_none(c.get("note")),
                })

        raw_migrations = content.get("migrations") or {}
        migrations = {
            "db": [
                {"name": text_or_none(m.get("name")) or "",
                 "description": clean_description(m.get("description"))}
                for m in (raw_migrations.get("db") or []) if isinstance(m, dict)
            ],
            "runtime": [
                {"reference": text_or_none(m.get("reference")),
                 "description": clean_description(m.get("description"))}
                for m in (raw_migrations.get("runtime") or []) if isinstance(m, dict)
            ],
        }

        host_functions = []
        for hf in content.get("host_functions") or []:
            if isinstance(hf, str):
                host_functions.append({"name": hf, "description": None})
            elif isinstance(hf, dict) and hf.get("name"):
                host_functions.append({
                    "name": str(hf["name"]),
                    "description": text_or_none(hf.get("description")),
                })

        topic, source = resolve_topic(content.get("topic"), crates, topics, pr, warnings)
        title = content.get("title")
        entries.append({
            "pr": pr,
            "title": one_line(title) if title is not None else f"PR #{pr}",
            "url": f"{REPO_URL}/pull/{pr}",
            "author": text_or_none(content.get("author")),
            "audiences": ordered_audience_union(docs) if docs else [],
            "topic": {"id": topic["id"], "label": topic["label"], "source": source},
            "breaking": any(c["bump"] == "major" for c in crates),
            "docs": docs,
            "crates": crates,
            "migrations": migrations,
            "host_functions": host_functions,
        })
    return entries, sorted(e["pr"] for e in entries)


def load_audience_descriptions(schema_path):
    """Audience descriptions from the local prdoc schema (JSON with // comment lines)."""
    try:
        raw = Path(schema_path).read_text(encoding="utf-8")
        schema = json.loads(re.sub(r"^\s*//.*$", "", raw, flags=re.MULTILINE))
        return {
            item["const"]: item["description"]
            for item in schema["$defs"]["audience_id"]["oneOf"]
            if "const" in item and "description" in item
        }
    except (OSError, KeyError, json.JSONDecodeError):
        return {}


def scan_fences(lines):
    """Yield (line, in_code) tracking ```/~~~ fenced blocks CommonMark-ish: a fence
    opens with 3+ backticks or tildes at <=3-space indent (info string allowed) and
    closes only on a matching-char run at least as long with nothing else on the line.
    Both fence delimiter lines report in_code=True."""
    fence = None  # (char, run length) of the open fence
    for line in lines:
        m = FENCE_RE.match(line)
        if fence is None:
            if m:
                fence = (m.group(1)[0], len(m.group(1)))
                yield line, True
            else:
                yield line, False
        else:
            yield line, True
            stripped = line.strip()
            if (m and m.group(1)[0] == fence[0] and set(stripped) == {fence[0]}
                    and len(stripped) >= fence[1]):
                fence = None


def demote_headings(text, min_level=5):
    """Demote markdown headings inside prdoc descriptions below our entry headings (####),
    so a description's own `# Description` doesn't outrank the changelog structure."""
    out = []
    for line, in_code in scan_fences(text.splitlines()):
        if not in_code:
            m = re.match(r"^(#{1,6})\s", line)
            if m and len(m.group(1)) < min_level:
                line = "#" * min_level + line[len(m.group(1)):]
        out.append(line)
    return "\n".join(out)


UNSAFE_PREFIX_RE = re.compile(r"```|~~~|<!--|<details\b", re.IGNORECASE)


def truncate_markdown(text, limit):
    """Cut at the last paragraph boundary within `limit` that is *safe*: outside code
    fences, HTML comments, and <details> blocks — a cut must never corrupt the markdown
    that follows the entry. Returns (snippet, truncated); the snippet is empty when no
    safe cut exists (e.g. the text opens with a huge code block)."""
    if len(text) <= limit:
        return text, False
    lines = text.splitlines()
    fence_states = [in_code for _, in_code in scan_fences(lines)]
    safe_end = 0
    pos = 0
    comment_depth = 0
    details_depth = 0
    for i, line in enumerate(lines):
        end = pos + len(line)
        if end > limit:
            break
        if not fence_states[i]:
            comment_depth += line.count("<!--") - line.count("-->")
            details_depth += (len(re.findall(r"<details\b", line, re.IGNORECASE))
                              - line.lower().count("</details>"))
        clean = not fence_states[i] and comment_depth <= 0 and details_depth <= 0
        at_paragraph_end = line.strip() and (i + 1 == len(lines) or not lines[i + 1].strip())
        if clean and at_paragraph_end:
            safe_end = end
        pos = end + 1
    if safe_end:
        return text[:safe_end].rstrip(), True
    # No complete paragraph fits: cut mid-paragraph at a sentence/word boundary, but
    # only when the prefix holds none of the constructs a cut could break.
    prefix = text[:limit]
    if UNSAFE_PREFIX_RE.search(prefix):
        return "", True
    cut = prefix.rfind(". ")
    if cut >= limit * 0.4:
        return prefix[: cut + 1].rstrip(), True
    cut = prefix.rfind(" ")
    return prefix[: cut if cut > 0 else limit - 1].rstrip(), True


def fmt_crate(crate):
    bump = f" ({crate['bump']})" if crate["bump"] else ""
    return f"`{crate['name']}`{bump}"


def fmt_crate_list(crates, cap=CRATE_LIST_CAP):
    shown = ", ".join(fmt_crate(c) for c in crates[:cap])
    extra = len(crates) - cap
    return shown + (f" and {extra} more" if extra > 0 else "")


def migration_flags(entry):
    flags = []
    if entry["migrations"]["runtime"]:
        flags.append("🗄️ *runtime migration*")
    if entry["migrations"]["db"]:
        flags.append("🗄️ *db migration*")
    return flags


def first_line(text, limit=200):
    line = text.strip().splitlines()[0] if text.strip() else ""
    return line if len(line) <= limit else line[: limit - 1].rstrip() + "…"


def badge_ref(audience):
    if audience in AUDIENCE_BADGES:
        return AUDIENCE_BADGES[audience][0]
    slug = re.sub(r"[^a-z0-9]+", "-", audience.lower()).strip("-")
    if not slug:
        # Symbol-only audiences (legacy placeholders like "...") must not collide
        # on one shared reference; derive a stable suffix from the content.
        slug = hashlib.md5(audience.encode("utf-8")).hexdigest()[:6]
    return "b-" + slug


def audience_badges(entry):
    return " ".join(f"![{a}][{badge_ref(a)}]" for a in entry["audiences"])


def badge_definitions(entries):
    """Reference definitions for every audience badge used in the document."""
    used = {a for e in entries for a in e["audiences"]}
    ordered = [a for a in CANONICAL_AUDIENCES if a in used]
    ordered += sorted(used - set(CANONICAL_AUDIENCES))
    lines = []
    for a in ordered:
        color = AUDIENCE_BADGES.get(a, (None, DEFAULT_BADGE_COLOR))[1]
        label = quote(a.replace("-", "--").replace("_", "__").replace(" ", "_"), safe="_-.")
        lines.append(f"[{badge_ref(a)}]: https://img.shields.io/badge/{label}-{color}")
    return lines


def asset_note(tag):
    if tag:
        base = f"{REPO_URL}/releases/download/{tag}"
        changelog, data = f"{base}/CHANGELOG.md", f"{base}/changelog.json"
        # Plain link text on purpose: code spans nested in link text render fine on
        # GitHub but are mangled by some editors/formatters and simpler renderers.
        return (
            f"> 📦 The complete changelog with full descriptions is attached to this release as "
            f"[CHANGELOG.md]({changelog}), and in machine-readable form as "
            f"[changelog.json]({data})."
        )
    return (
        "> 📦 The complete changelog with full descriptions is attached to this release as "
        "`CHANGELOG.md`, and in machine-readable form as `changelog.json`."
    )


def render_breaking_index(entries, cap=4):
    lines = []
    for e in (e for e in entries if e["breaking"]):
        names = [f"`{c['name']}`" for c in e["crates"] if c["bump"] == "major"]
        shown = ", ".join(names[:cap] if cap else names)
        extra = len(names) - cap if cap else 0
        if extra > 0:
            shown += f" and {extra} more"
        flags = " ".join(migration_flags(e))
        lines.append(
            f"- **{e['topic']['label']}** — [#{e['pr']}]({e['url']}) {e['title']} — {shown}"
            + (f" {flags}" if flags else "")
        )
    return lines


def render_breaking_section(entries, cap=4):
    breaking_count = sum(1 for e in entries if e["breaking"])
    lines = ["## 💥 Breaking Changes", ""]
    if breaking_count:
        lines += [
            f"{breaking_count} change{'s' if breaking_count != 1 else ''} contain at least one "
            "`major` crate bump. Details in the 💥-marked entries below.",
            "",
        ]
        lines += render_breaking_index(entries, cap=cap) + [""]
    else:
        lines += ["None in this release.", ""]
    return lines


def render_migrations_section(entries, full=False):
    runtime, db = [], []
    for e in entries:
        for m in e["migrations"]["runtime"]:
            desc = m["description"] if full else first_line(m["description"])
            ref = f" ({m['reference']})" if m["reference"] else ""
            runtime.append(f"- [#{e['pr']}]({e['url']}){ref}: {desc}")
        for m in e["migrations"]["db"]:
            desc = m["description"] if full else first_line(m["description"])
            name = f"**{m['name']}** — " if m["name"] else ""
            db.append(f"- [#{e['pr']}]({e['url']}): {name}{desc}")
    if not runtime and not db:
        return []
    lines = ["### 🗄️ Migrations", ""]
    if runtime:
        lines += ["**Runtime:**", ""] + runtime + [""]
    if db:
        lines += ["**DB:**", ""] + db + [""]
    return lines


def render_entry_body(entry, limit, drop_low):
    lines = [f"#### {'💥 ' if entry['breaking'] else ''}[#{entry['pr']}]({entry['url']}): {entry['title']}"]
    if entry["audiences"]:
        lines.append(audience_badges(entry))
    lines.append("")

    description = entry["docs"][0]["description"] if entry["docs"] else ""
    if (limit > 0 and description
            and not (drop_low and max_bump_rank(entry["crates"]) <= BUMP_RANK["patch"])):
        text, truncated = truncate_markdown(demote_headings(description), limit)
        if text:
            lines += [text, ""]
        if truncated:
            # Own paragraph: appending to the last line could corrupt a closing fence.
            lines += ["[…]", ""]

    breaking_crates = [c for c in entry["crates"] if c["bump"] == "major"]
    other_crates = [c for c in entry["crates"] if c["bump"] != "major"]
    if breaking_crates:
        lines.append(f"**Breaking**: {fmt_crate_list(breaking_crates)}")
    for m in entry["migrations"]["runtime"]:
        lines.append(f"🗄️ **Runtime migration**: {first_line(m['description'])}")
    for m in entry["migrations"]["db"]:
        lines.append(f"🗄️ **DB migration**: {first_line(m['description'] or m['name'])}")
    if entry["host_functions"]:
        names = ", ".join(f"`{hf['name']}`" for hf in entry["host_functions"])
        lines.append(f"⚙️ **Host functions**: {names}")
    if other_crates:
        label = "Other crates" if breaking_crates else "Crates"
        lines.append(f"<sub>{label}: {fmt_crate_list(other_crates)}</sub>")
    lines.append("")
    return lines


def group_by_topic(entries, topics):
    groups = []
    for t in topics:
        group = [e for e in entries if e["topic"]["id"] == t["id"]]
        if group:
            groups.append((t, group))
    return groups


def render_body_fragment(entries, topics, tag, limit, drop_low):
    lines = [asset_note(tag), ""]
    lines += render_breaking_section(entries)
    # Migrations are orthogonal to breaking changes (they routinely ship under
    # minor/patch bumps), so the index renders regardless of the breaking count.
    lines += render_migrations_section(entries)
    lines += ["## Changelog", ""]
    for topic, group in group_by_topic(entries, topics):
        lines += [f"### {topic['label']}", ""]
        for e in group:
            lines += render_entry_body(e, limit, drop_low)
    lines += badge_definitions(entries)
    return "\n".join(lines).rstrip() + "\n"


def render_full_entry(entry):
    lines = [f"#### {'💥 ' if entry['breaking'] else ''}[#{entry['pr']}]({entry['url']}): {entry['title']}"]
    if entry["audiences"]:
        lines.append(audience_badges(entry))
    lines.append("")
    multi = len(entry["docs"]) > 1
    for doc in entry["docs"]:
        if multi or doc["title"]:
            heading = doc["title"] or entry["title"]
            lines += [f"**For {' '.join(f'`{a}`' for a in doc['audiences'])}** — {heading}", ""]
        if doc["description"]:
            lines += [demote_headings(doc["description"]), ""]
    if entry["crates"]:
        crate_lines = []
        for c in entry["crates"]:
            crate_lines.append(f"  - {fmt_crate(c)}" + (f" — {c['note']}" if c["note"] else ""))
        lines += ["**Crates**:"] + crate_lines + [""]
    for m in entry["migrations"]["runtime"]:
        ref = f" ({m['reference']})" if m["reference"] else ""
        lines += [f"🗄️ **Runtime migration**{ref}: {m['description']}", ""]
    for m in entry["migrations"]["db"]:
        name = f"**{m['name']}** — " if m["name"] else ""
        lines += [f"🗄️ **DB migration**: {name}{m['description']}", ""]
    for hf in entry["host_functions"]:
        desc = f": {hf['description']}" if hf["description"] else ""
        lines += [f"⚙️ **Host function** `{hf['name']}`{desc}", ""]
    return lines


def render_full_changelog(entries, topics, tag, previous_tag, audience_descriptions):
    title_ref = tag or "unreleased"
    lines = [f"# Polkadot SDK {title_ref} — Complete Changelog", ""]
    if tag and previous_tag:
        lines += [f"This release contains the changes from `{previous_tag}` to `{tag}`.", ""]
    lines += [
        "The release page shows a condensed version of this changelog. A machine-readable",
        "version is attached to the release as `changelog.json`.",
        "",
    ]
    lines += render_breaking_section(entries, cap=0)
    lines += render_migrations_section(entries, full=True)
    lines += ["## Changes by Topic", ""]
    for topic, group in group_by_topic(entries, topics):
        lines += [f"### {topic['label']}", ""]
        for e in group:
            lines += render_full_entry(e)
    lines += ["## Appendix: Changes by Audience", ""]
    audiences = [a for a in CANONICAL_AUDIENCES if any(a in e["audiences"] for e in entries)]
    audiences += sorted(
        {a for e in entries for a in e["audiences"]} - set(CANONICAL_AUDIENCES)
    )
    for audience in audiences:
        lines += [f"### Changelog for `{audience}`", ""]
        if audience in audience_descriptions:
            lines += [f"ℹ️ These changes are relevant to: {audience_descriptions[audience]}", ""]
        for e in sorted(entries, key=lambda e: e["pr"]):
            for doc in e["docs"]:
                if audience in doc["audiences"]:
                    heading = doc["title"] or e["title"]
                    lines += [f"#### [#{e['pr']}]({e['url']}): {heading}", ""]
                    if doc["description"]:
                        lines += [demote_headings(doc["description"]), ""]
    lines += badge_definitions(entries)
    return "\n".join(lines).rstrip() + "\n"


def build_json(entries, topics, tag, previous_tag, version, generated_at):
    return {
        "schema_version": SCHEMA_VERSION,
        "release": {
            "tag": tag,
            "previous_tag": previous_tag,
            "version": version,
            "generated_at": generated_at,
        },
        "topics": [{"id": t["id"], "label": t["label"]} for t in topics],
        "changes": entries,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--prdoc-dir", required=True, help="folder containing pr_*.prdoc files")
    parser.add_argument("--topics", required=True, help="path to the topic taxonomy (topics.yaml)")
    parser.add_argument("--output-dir", required=True)
    parser.add_argument("--tag", default=None,
                        help="FINAL release tag (no -rcN suffix), e.g. polkadot-stable2606; "
                             "used for asset download URLs and the changelog.json identity")
    parser.add_argument("--previous-tag", default=None)
    parser.add_argument("--version", default=None, help="release version, e.g. stable2606")
    parser.add_argument("--schema", default=None,
                        help="path to prdoc schema_user.json (for audience descriptions); "
                             "defaults to schema_user.json next to --prdoc-dir's parent")
    parser.add_argument("--generated-at", default=None,
                        help="ISO timestamp recorded in changelog.json (omitted -> null, "
                             "keeping reruns byte-identical)")
    parser.add_argument("--max-body-chars", type=int, default=115000,
                        help="budget for changelog_body.md; build-changelogs.sh passes the "
                             "exact budget measured from the rendered outer document")
    parser.add_argument("--check", action="store_true",
                        help="fail (exit 1) on any warning-level problem, e.g. a skipped "
                             "prdoc, unknown topic/audience, or body over budget")
    args = parser.parse_args()

    prdoc_dir = Path(args.prdoc_dir)
    if not prdoc_dir.is_dir():
        sys.exit(f"error: {prdoc_dir} is not a directory")

    warnings = []
    topics = load_topics(args.topics)
    entries, expected_prs = load_prdocs(prdoc_dir, topics, warnings)
    if not entries:
        sys.exit(f"error: no usable pr_*.prdoc files found in {prdoc_dir}")

    topic_order = {t["id"]: i for i, t in enumerate(topics)}
    entries.sort(key=lambda e: (topic_order[e["topic"]["id"]], not e["breaking"], e["pr"]))

    schema_path = args.schema or (prdoc_dir.parent / "schema_user.json"
                                  if (prdoc_dir.parent / "schema_user.json").is_file()
                                  else prdoc_dir / "schema_user.json")
    audience_descriptions = load_audience_descriptions(schema_path)

    # Degradation ladder: shrink descriptions until the fragment fits the budget, then
    # drop descriptions of entries without any major/minor bump, and as the guaranteed
    # terminal rung drop all descriptions (titles/badges/crates always fit in practice).
    steps = [(limit, False) for limit in TRUNCATION_LADDER]
    steps += [(TRUNCATION_LADDER[-1], True), (0, True)]
    body = None
    for step, (limit, drop_low) in enumerate(steps):
        body = render_body_fragment(entries, topics, args.tag, limit, drop_low)
        if len(body) <= args.max_body_chars:
            if step > 0:
                log(f"warning: body over budget, degraded to step {step} "
                    f"(limit={limit}, drop_low={drop_low}), {len(body)} chars")
            break
    else:
        warnings.append(
            f"body fragment still over budget after full degradation: "
            f"{len(body)} > {args.max_body_chars} chars"
        )

    full = render_full_changelog(entries, topics, args.tag, args.previous_tag,
                                 audience_descriptions)
    data = build_json(entries, topics, args.tag, args.previous_tag, args.version,
                      args.generated_at)

    # Completeness invariants; a violation here is a generator bug, never ship it.
    output_prs = sorted(c["pr"] for c in data["changes"])
    if expected_prs != output_prs:
        sys.exit(f"error: loaded/output PR mismatch: {set(expected_prs) ^ set(output_prs)}")
    missing_in_body = [pr for pr in expected_prs if f"[#{pr}]" not in body]
    if missing_in_body:
        sys.exit(f"error: entries missing from the body fragment: {missing_in_body}")
    json.loads(json.dumps(data))  # round-trip sanity

    out = Path(args.output_dir)
    out.mkdir(parents=True, exist_ok=True)
    (out / "changelog_body.md").write_text(body, encoding="utf-8")
    (out / "CHANGELOG.md").write_text(full, encoding="utf-8")
    (out / "changelog.json").write_text(
        json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8"
    )

    by_source = {}
    by_topic = {}
    for e in entries:
        by_source[e["topic"]["source"]] = by_source.get(e["topic"]["source"], 0) + 1
        by_topic[e["topic"]["label"]] = by_topic.get(e["topic"]["label"], 0) + 1
    log(f"{len(entries)} changes | breaking: {sum(1 for e in entries if e['breaking'])} | "
        f"body: {len(body)} chars (budget {args.max_body_chars})")
    log(f"topic sources: {by_source}")
    log("entries per topic: " + ", ".join(f"{k}: {v}" for k, v in by_topic.items()))
    for w in warnings:
        log(f"warning: {w}")

    if warnings and args.check:
        sys.exit(1)


if __name__ == "__main__":
    main()
