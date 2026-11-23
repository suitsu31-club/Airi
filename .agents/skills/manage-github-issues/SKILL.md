---
name: manage-github-issues
description: File, label, triage, and close GitHub issues and PRs with the prefixed label scheme (C:/D:/F:/M:/T:); create or sync those labels.
---

# Manage GitHub Issues

All GitHub work goes through the `gh` CLI, run from the repository root so it
targets this repository. Read the live label set before labelling anything;
`gh label list` is the source of truth, and the table below describes the
labels the template ships with.

```sh
gh label list --limit 200
```

## Label scheme

Every label is `<prefix>:<name>`. Only labels with a prefix below are part of
the scheme; never invent a new prefix.

### `T:` — issue type (pick at most one)

| Label | Use for |
|---|---|
| `T:Bug` | Existing behaviour is wrong: a crash, a wrong result, an error that should not happen. |
| `T:Feature` | A capability that does not exist yet. |
| `T:enhancement` | Improving something that already works: refactors, redesigns, better limits. |
| `T:Documentation` | Docs, `AGENTS.md`, skills, code comments. |
| `T:Deployment` | Rolling out, configuring, or migrating a running environment. |

Reviews, audits, and open questions may carry no `T:` label (use `F:question`).

### `C:` — component (where the work happens)

| Label | Covers |
|---|---|
| `C:Backend` | Rust server code: `bin/app-server` and the runtime behaviour of `modules/*`. |
| `C:Frontend` | Everything under `typescript/` except generated code. |
| `C:Framework` | Cross-cutting plumbing: `lib/` (incl. `app_protobuf`), `proto/`, the wakuwaku / kanau integration, `base::Error`, worker wiring. |
| `C:Infrastructure` | Deployment, containers, CI (`.github/workflows`), databases, Redis, Iggy provisioning. |
| `C:Internal Tool` | `bin/manage-tool` and other operator/admin tooling. |

### `M:` — module (one label per crate under `modules/`)

`M:<crate dir name>`, e.g. `M:base`. Description: `Common Module` for modules
meant to be reused across projects (like `base`), `<Project> Module` for
project-specific ones. Add the `M:` label of every module the fix touches.

When a module is added under `modules/`, create its label in the same change:

```sh
gh label create "M:<name>" --color 4cb782 --description "Common Module"
```

### `D:` — detail (what kind of problem; any number)

| Label | Meaning |
|---|---|
| `D:Correctness` | Wrong data or wrong results; data may already be damaged. |
| `D:Performance` | Latency, throughput, memory, DB/Redis/Iggy load, storage growth. |
| `D:Security` | Authn/authz, secrets, abuse, data exposure. |
| `D:Quality` | Code that works but breaks the layering rules in `AGENTS.md`, or is hard to maintain. |
| `D:Abstract` | The design or abstraction itself is wrong (e.g. events kept as DB rows instead of Iggy messages). |
| `D:Consistence` | Parts disagree: naming, behaviour across modules, API vs docs, proto vs implementation. |
| `D:UX` | The product works but is confusing or awkward for its user. |
| `D:User Feedback` | Raised by a real user or operator, not found by the team. |

### `F:` — feedback (on issues and pull requests)

| Label | Meaning and required action |
|---|---|
| `F:question` | More information or a decision is needed. State the question in the body or a comment. |
| `F:Suspicious` | Needs further review before anyone acts on it. Say what looks wrong. |
| `F:need-revert` | A **merged** PR should be reverted. Comment with the reason and the evidence. |
| `F:duplicate` | Already reported. Comment `Duplicate of #N`, then close as not planned. |
| `F:invalid` | Not a real problem (misreading, wrong environment). Explain, then close as not planned. |
| `F:wontfix` | Real, but will not be worked on. Give the reason, then close as not planned. |

### Minimum labelling

- One `T:` label when the type is clear.
- At least one `C:` **or** `M:` label so the issue has an owner area. Use both
  when a module bug surfaces in one component (e.g. `C:Frontend` + `M:base`).
- `D:` labels whenever the nature of the problem is known.

## Filing an issue

1. **Search first.** `gh issue list --state all --search "<keywords>"`. If it
   exists, comment there instead of opening a new one.
2. **Gather evidence before writing.** Read the code, run the repro, query the
   data. Cite paths with line ranges (`modules/base/src/services/foo.rs:40-62`)
   and pin the commit (`git rev-parse --short HEAD`) for reviews. Never invent
   numbers or output; mark anything not observed as an inference.
3. **Title** states the observable problem and its consequence, with the
   concrete symbol, table, or endpoint. An optional lowercase module prefix
   helps scanning:
   - `base: CreateUser answers INTERNAL for a duplicate email instead of ALREADY_EXISTS`
   - not `Bug in user creation`
4. **Body** — write it to a temp file and pass `--body-file` (no shell quoting
   problems). Use the sections that apply, in this order:

   ```markdown
   ## Summary
   One paragraph: what happens, where, and why it matters.

   ## Cause
   The code path, with file:line references. Explain the mechanism.

   ## Evidence
   Logs, query results, measurements (with environment and time window).

   ## Reproduce
   1. Numbered, copy-pasteable steps.
   2. … → the wrong result.

   ## Expected
   What should happen instead.

   ## Proposed fix
   Optional. The direction, which layer it belongs in (entities / services /
   hooks / rpc), and the trade-offs.
   ```

   For design or review issues, replace Cause/Reproduce with **Scope and
   method**, **Findings**, and **Decisions needed** (a numbered list).
5. **Create and label in one call:**

   ```sh
   gh issue create --title "<title>" --body-file /tmp/issue.md \
     --label "T:Bug" --label "C:Backend" --label "M:base" --label "D:Correctness"
   ```

6. Report the issue URL that `gh` prints.

One problem per issue. Split unrelated findings into separate issues and link
them (`see #N`) rather than bundling them.

## Triage

```sh
gh issue list --limit 100 --json number,title,labels \
  --jq '.[] | select([.labels[].name | select(startswith("T:") or startswith("C:") or startswith("M:"))] | length == 0) | "#\(.number) \(.title)"'
gh issue view <N> --comments
gh issue edit <N> --add-label "T:Bug,M:base" --remove-label "F:question"
```

- Read the issue and its comments before relabelling; do not drop labels a
  human added without saying why in a comment.
- Remove `F:question` / `F:Suspicious` once the question is answered or the
  review is done.

## Closing

- Fixed: let the PR close it (`Fixes #N` in the PR body), or
  `gh issue close <N> --reason completed --comment "Fixed in <sha or #PR>"`.
- Duplicate / invalid / won't fix: add the `F:` label, then
  `gh issue close <N> --reason "not planned" --comment "<reason>"`.

## Pull requests

- Link the issue with `Fixes #N` / `Refs #N` in the body.
- Apply the same `T:` / `C:` / `M:` labels as the issue it resolves:
  `gh pr edit <N> --add-label "T:Bug,M:base"`.
- To request a revert of a merged PR: `gh pr edit <N> --add-label F:need-revert`
  and comment with what broke and the evidence.

## Syncing labels to a new repository

A project created from this template gets no labels from it (GitHub copies
files, not labels). Recreate the scheme with:

```sh
mk() { gh label create "$1" --color "$2" --description "$3" --force; }
for t in Bug:EB5757 Documentation:4cb782 Feature:BB87FC Deployment:f7c8c1; do mk "T:${t%%:*}" "${t##*:}" "Issue Type"; done
mk "T:enhancement" a2eeef "New feature or request"
for c in Backend Framework Frontend Infrastructure "Internal Tool"; do mk "C:$c" f2c94c Component; done
for d in Correctness Performance Quality Security Consistence UX Abstract "User Feedback"; do mk "D:$d" f2994a "Issue Detail"; done
mk "F:duplicate" cfd3d7 "Feedback: This issue or pull request already exists"
mk "F:invalid" e4e669 "Feedback: This doesn't seem right"
mk "F:question" e4e669 "Feedback: Further information is requested"
mk "F:wontfix" ffffff "Feedback: This will not be worked on"
mk "F:need-revert" EB5757 "Feedback: Merged pull request should be reverted"
mk "F:Suspicious" e4e669 "Feedback: Need further review"
for m in modules/*/; do m=${m%/}; mk "M:${m#modules/}" 4cb782 "Common Module"; done
```

Ask before deleting GitHub's default labels (`bug`, `enhancement`, …) in a
repository that already has issues; prefer `gh label edit <old> --name <new>`
so existing issues keep their label.
