# Waves

Work on freeQRent happens in **waves**. The format is adapted from labyrinth.

## Two levels

- **M waves** are the milestones in `SPEC.md` §12: `M0` (encoder) through `M9` (bindings release). Each one has a fixed scope and a done-when gate, and it is finished when every part of its scope has landed.
- **W waves** are the small units of work inside an M wave. Each W wave has one topic, one plan, and usually one branch. An M wave is made of as many W waves as it takes.

## Naming W waves

A W wave is named by the time its plan was created, to the second, in local time: `YYYYMMDDHHMMSS`, e.g. `20261025203745`. Timestamps never collide, and they sort in order.

- Take the id when you create the plan file: `date +%Y%m%d%H%M%S`. Never change it afterwards, even if the plan is rewritten.
- The plan is `docs/waves/notes/WAVE_<id>.md`, titled `# Wave <id> (M<n>): <topic>`.
- Packages inside a wave are `<id>-0`, `<id>-1`, … Use `-0` for any spec or design work. A package's working notes are `docs/waves/notes/<id>-<n>.md`. Within one plan, `-2` alone is enough.
- The branch is `w/<id>`.

## What is public and what is not

| Path | Tracked in git | Contents |
|---|---|---|
| `docs/waves/README.md` | yes | This protocol |
| `docs/waves/LOG.md` | yes | **A short entry per W wave**: what landed, decisions, follow-ups |
| `ROADMAP.md` | yes | Ideas for future waves that **haven't landed in `SPEC.md` yet** |
| `docs/waves/notes/` | **no** (gitignored) | Detailed plans, package notes, scratch, investigation logs |

The detailed notes are working material. Anything a future contributor needs to know must reach a tracked file: the log entry, `SPEC.md`, code comments, or the roadmap.

## Lifecycle of a W wave

1. **Plan.** Create `docs/waves/notes/WAVE_<id>.md` with:
   - a status comment as its first line: `<!-- Status: PLANNED -->`
   - the M wave it belongs to and the spec sections it implements
   - context: why now, and what exists already
   - decisions taken, with the reasons
   - packages `-0`, `-1`, … with each package's owned paths and its done-when check
2. **Build.** Work on `w/<id>`. Every package appends to its own notes file: deviations from the plan and why, workarounds (crate + version), and anything a later wave would otherwise rediscover.
3. **Close.** Update the plan's status line (`<!-- Status: BUILT <date>. <commits>. <checks>. -->`), then:
   - add the wave's entry to `LOG.md`
   - fold any spec changes into `SPEC.md` (spec revisions are tagged with the wave id)
   - move ideas that came up but weren't built into `ROADMAP.md`
   - merge
4. **Close the M wave** when its scope is complete. Add an `M<n> complete` line to `LOG.md`.

## Log entry format

Keep it short: five to ten lines. The log is a history, not a journal.

```markdown
## 20261025203745 · M0 · Encoder: segmentation and masks

- Landed: byte/numeric/alphanumeric modes, minimal-length segmentation, all 8 masks with penalty scoring.
- Decision: ECI for UTF-8 only emitted when the text isn't Latin-1 (keeps codes smaller).
- Checks: 412 tests, `rqrr` round-trip on v1–40 × L/M/Q/H green.
- Follow-up: Kanji mode moved to its own wave.
```

## Before starting a wave

Read `SPEC.md`, `LOG.md`, `ROADMAP.md`, and the plan and notes of any earlier wave in the same M wave. If you're working in a fresh clone, the gitignored notes won't exist, so `LOG.md` is the source of truth.
