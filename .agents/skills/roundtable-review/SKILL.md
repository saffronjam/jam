---
name: roundtable-review
description: Run an adversarial multi-expert round table review of a merge request, pull request, branch diff, or working tree. Independent expert reviewers hunt show-stopper defects in parallel under distinct lenses, their findings are pooled and deduplicated, every expert votes on every finding, and only findings clearing a consensus threshold get reported. Use this whenever the user asks for a round table, expert panel, jury, tribunal, multi-agent or multi-reviewer review, says "N experts", asks to have a change reviewed from several perspectives, wants findings voted on, debated, or filtered by consensus, or asks for a deep, serious, or high-stakes review of an MR/PR where a single reviewer pass would miss things. Prefer this over a single-pass review whenever the user names a number of reviewers or asks for consensus, voting, or debate.
---

# Round table review

A panel of independent reviewers, each with a different lens, hunting only for defects
that genuinely matter — then made to defend those findings in front of each other.

The value is not "more reviewers". It is that **findings are challenged before they reach the
user**. A single reviewer produces a mix of real bugs, misreadings, and nitpicks, and the user has
to sort them. Here the panel does that sorting, and the vote is what separates a real defect from
a confident-sounding misreading.

You are the **moderator**. You do not hunt for bugs yourself — you define the target, assign the
lenses, adjudicate disputes, and decide what gets reported. Your judgment is the last filter.

## Parameters

Default to **5 experts** and a **4/5 reporting threshold**. If the user names a number ("3 experts",
"a panel of 7"), use it, and scale the threshold to the same proportion (roughly 80%: 3/3, 4/5, 6/7).
State the threshold before round 1 so it is not tuned after seeing the findings.

## Round 0 — Pin the target (before any expert runs)

Most round table failures are not bad reviewing. They are experts reviewing *different things*.
Do this work yourself, and do not skip it.

**Resolve what "the MR" actually means.** A branch name is not a review target. Find the real
merge request (`glab mr list --source-branch <branch>`, `gh pr list --head <branch>`). Check whether
the user's working tree matches the pushed branch — worktrees are often stacked on unmerged commits,
or carry uncommitted work, or another session is editing them concurrently.

**Pin an immutable commit SHA and give that SHA to every expert.** Branches move during a review.
When they do, experts reach opposite conclusions from the same evidence and the vote becomes noise.
Write the SHA into the brief and have experts diff against it explicitly.

**Get the complete file list — never truncate it.** Piping a `--stat` through `tail` or `head` to
save space will silently hide files, and a hidden file is often the most important one (a migration,
a generated artifact, a config default). If the output is large, write it to a file and read it.

**Establish the pre-existing failure baseline.** Run the affected test suites on the *merge base*
before reviewing, or at minimum note which failures are already present. Without this, experts report
unrelated broken tests as defects of the change, and you cannot tell a real regression from
background noise.

**Read the diff yourself.** You cannot moderate a dispute between two experts about code you have
not read, and adjudicating disputes is the part of this job only you can do.

## Round 1 — Independent hunt

Write one shared brief (see `references/brief-template.md`) and give every expert the same one, plus
a lens that is theirs alone. Schedule experts concurrently within available capacity, using waves when necessary. Keep the same immutable target for every wave.

**Lenses must be derived from the actual diff, not chosen from a generic list.** Read what the change
does, then carve it into non-overlapping territories and tell each expert to stay inside theirs.
`references/lenses.md` has archetypes to adapt, not a menu to copy. Give each expert a handful of
concrete, pointed questions about their territory — the specific joins, the specific casts, the
specific interleavings. Vague lenses produce vague findings.

Two rules do the heavy lifting in the brief:

**The bar.** Only defects that corrupt or mis-attribute data, break production on deploy, silently
produce wrong results, damage an existing database, or create a real security or availability
hazard. Spell out what is out of scope — nits, style, naming, comment wording, doc typos, missing
tests for their own sake, refactoring opportunities, minor efficiency. Without an explicit exclusion
list you get a pile of style notes wearing a severity label.

**Verification over diff-reading.** Require experts to read the actual schema, the actual column
types, the surrounding code — and to run things where they can. Findings that were *executed* (a
test actually failed, a query actually returned the wrong rows) survive the vote and are believed by
the user; findings inferred from a diff usually do not and should not.

Require a **"Cleared" section**: suspicious things the expert checked that turned out to be fine.
This is worth as much as the findings. It shows coverage, stops the panel from re-litigating settled
questions, and an expert who clears something another expert flags is a signal you need to chase.

Tell experts that **zero findings is a valid and respectable answer**. Otherwise they invent
findings to look useful, and a padded round 1 poisons every round after it.

## Round 2 — Assemble

Pool everything and merge duplicates into one canonical finding per defect. For each, record:

- a one-line claim, the file:line, and the concrete failure scenario
- the evidence that supports it, especially anything actually executed
- **who raised it** — corroboration by two experts under different lenses is strong signal, and
  single-source findings deserve more scrutiny in the vote

Keep the canonical wording faithful to the evidence. If an expert overstated ("100% of the time,
permanently"), do not quietly fix it — leave it and let the vote correct it, which it will.

Write the assembled list to a file. Experts need to read it in round 3, and several of them will
re-verify claims from it.

## Round 3 — Vote

Send every expert the full assembled list, including findings they have not seen, and ask for a
FOR/AGAINST vote on **every** finding — no abstentions. FOR means it clears the bar and belongs in
the report; AGAINST means do not report it, whether because the code refutes it, it is speculative,
it is below the bar, it is pre-existing, or it is already mitigated elsewhere.

Three things make the vote work:

- **Explicit permission to defect.** Tell experts to vote against their own round 1 finding if the
  assembly or another expert's evidence changed their mind, and that doing so is a good outcome.
- **A CORRECTIONS field.** Ask for one line on any finding they vote FOR that is real but overstated
  or misattributed. This is where the panel earns its keep — it converts alarming-but-sloppy claims
  into precise ones, and it is usually the highest-value output of the whole exercise.
- **Re-verification.** Tell experts to check anything outside their round 1 lens before voting FOR,
  rather than taking a colleague's word for it.

Use `references/ballot-template.md`. Resume the same expert agents so they vote with their round 1
context intact — do not spawn fresh ones.

## Round 4 — Report

Report **only** findings at or above the threshold. Show the tally for every finding, including the
ones that failed, with one line on why they failed — the user learns as much from what the panel
rejected as from what it accepted.

**Apply the corrections.** A finding reported with its original overstated wording after the panel
narrowed it is a moderator failure.

**Adjudicate factual disputes yourself.** When experts disagree about a fact — not a judgment, a
fact — go and check it personally before reporting, and say what you found. In practice the usual
cause is stale state: one expert read the working tree, another read the pushed branch, and the
branch moved. Resolve it with evidence, and tell the user which one was right and why.

Order by production severity, not by vote count or by the order they were found. Close with the
shape of the problem — which findings share a root cause, what the panel cleared, and what a
reasonable next step is. A list of six unrelated-looking bugs is much less useful than "these four
are all consequences of one cadence change".

## Moderator failure modes

These are the ones that actually happen:

- **Truncating output while establishing scope.** Hides files, produces a confidently wrong summary.
- **Letting the target drift.** Re-fetch before the vote and before reporting; if the branch moved,
  say so and re-pin.
- **Tallying instead of judging.** The threshold filters noise; it does not absolve you of checking
  a disputed fact or of noticing that a 5/5 finding is overstated.
- **Reporting the panel's process instead of its conclusions.** The user wants the defects. Keep the
  scaffolding to a tally table.
- **Padding.** If nothing clears the threshold, say so plainly. That is a real and useful result.

## After the report

The user will often want fixes. Treat each accepted finding as its own task, fix the highest-severity
first, and re-run the affected suites against the baseline from round 0 so you can tell a new failure
from a pre-existing one. If a fix breaks a test, read the test before changing it — in a good
codebase the test is usually encoding a requirement the fix violated, not an obstacle.
