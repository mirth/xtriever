# Feature Specification: The iOS Wikipedia Demo App

**Feature Branch**: `009-ios-wiki-demo`

**Created**: 2026-09-14

**Status**: Draft

**Input**: User description: "Let's proceed with the demo app" — the third and last of the three features toward the iOS Wikipedia demo: a SwiftUI application at `apps/ios-wiki-demo/` that ships the 008 index and both models, opens them in place through the 007 package, and lets a person search all of Simple English Wikipedia on a phone while *seeing* the pipeline work — lexical, dense, fusion, re-rank — with attribution, the corpus's identity and the measured costs on screen.

## Why This Spec Reads Technically

The user is finally a person holding a phone, and the deliverable is the thing 001–008 were
for: proof, on a device, that the whole pipeline runs offline over a real corpus at a cost a
person will wait for. The app is deliberately thin — it owns no retrieval logic; every number it
shows comes from the engine's own stage report and explanations — so that what it demonstrates
is the engine, not the app. The requirements therefore name the engine's facts (stages,
budgets, explanations, degradation, attribution) rather than app internals. What 008 measured
sets the design: a first search pays ~2 s to page the vectors in, a fused answer takes ~0.35 s,
a re-ranked one ~2.3 s (default threads), and the app holds ~0.5 GB resident — the interaction
must be built around those numbers, not around wishful ones.

## User Scenarios & Testing *(mandatory)*

### User Story 1 - A person searches Wikipedia on the phone and gets passages (Priority: P1)

The app opens, prepares the index once (visibly, with what it is doing), and offers a search
field. The person types a question, submits, and gets a ranked list of passages, each showing
its article title, the passage text, and a link to the article. Everything works in airplane
mode.

**Why this priority**: This is the demo. Without it nothing else matters.

**Independent Test**: On a device in airplane mode, submit "why is the sky blue" and receive
passages with titles, text and working article links within the measured latency.

**Acceptance Scenarios**:

1. **Given** the app launched for the first time, **When** the index is being prepared,
   **Then** the screen says so (models loading, index opening, first-query warm-up) and the
   search field becomes active only when a search can succeed.
2. **Given** an open index and a question, **When** the person submits it, **Then** a ranked
   list of passages appears, each with the article title, the passage body, its rank, and a
   way to open the article's Wikipedia page.
3. **Given** the phone is offline, **When** the person searches, **Then** results arrive
   exactly as when online — nothing in the app needs a network.
4. **Given** a query with no hits (an empty string, or only stop-words), **When** submitted,
   **Then** the app says so plainly; it never shows a spinner forever or an error dialog for
   an ordinary empty result.
5. **Given** a passage in the list, **When** tapped, **Then** the full passage is shown with
   its title, article link, position in the article ("passage 3 of the article"), and the
   engine's explanation for the hit (US2).

---

### User Story 2 - The person sees the pipeline work (Priority: P1)

A search does not just return a list; it shows the stages. The fused (lexical + dense) list
arrives first, fast; the re-ranked order then replaces it, with the change visible. Each hit
can explain itself — its lexical score and rank, its dense score and rank, its fused score, its
re-rank score and rank — in the engine's own names. The stage report is on screen: how many
candidates each stage produced, whether a stage was skipped or degraded and why, how many pairs
the re-ranker scored, and the elapsed time.

**Why this priority**: "Demonstrates the whole pipeline functionality" is the owner's brief.
A list alone demonstrates a search box.

**Independent Test**: Submit a query; observe the fused list appear, then the re-ranked list;
open a hit's explanation and see all seven features; read the stage report's counts and time.

**Acceptance Scenarios**:

1. **Given** a submitted query, **When** the fused result is available, **Then** it is shown
   immediately (before re-ranking finishes), labelled as the fused stage.
2. **Given** the fused list on screen, **When** the re-ranked result arrives, **Then** the list
   updates to the re-ranked order, labelled as such, and the hits whose position changed are
   visibly marked (moved up / moved down / new / dropped).
3. **Given** any hit, **When** its explanation is opened, **Then** the seven pipeline features
   are listed under the engine's names, with "not seen by this stage" where a stage did not
   retrieve the hit.
4. **Given** any completed search, **When** the stage report is read, **Then** it shows
   lexical candidates, dense candidates (or that the dense stage was skipped and why), re-rank
   candidates and scored count (or that re-ranking was skipped and why), whether a time limit
   was ignored, and the elapsed milliseconds — all from the engine's report, none computed by
   the app.
5. **Given** the person changes the re-rank depth or the time budget in settings, **When** the
   next search runs, **Then** the stage report reflects it (fewer pairs scored; a budget
   exhausted shows as degradation, not as an error).

---

### User Story 3 - The person sees what they are searching and on what terms (Priority: P2)

An "About" screen shows the corpus: Simple English Wikipedia, the snapshot date, the article
and passage counts, the corpus identity, the two models' identities, the index's format
version, and the licence attribution the corpus requires — the text 008 ships beside the
index. It also shows what the app measured on this device: open time and the model load times
of this session.

**Why this priority**: Attribution is a licence obligation (CC BY-SA); the identities are what
make the demo a *measured* claim rather than a magic trick.

**Independent Test**: Open About; every item listed above is present and the attribution text
equals the shipped `ATTRIBUTION.txt`.

**Acceptance Scenarios**:

1. **Given** the About screen, **When** read, **Then** it shows the attribution text verbatim,
   with the licence link openable.
2. **Given** the About screen, **When** read, **Then** the corpus identity, passage count,
   embedder and re-ranker identities and format version match what the engine reports for
   the open index — not constants typed into the app.

---

### User Story 4 - The app is honest about cost and never hangs (Priority: P2)

Every search is cancellable: typing a new query and submitting cancels the previous one's
result (the engine finishes its work, the app discards it). The first search after launch is
warned about ("first search warms the index"). A search runs under a time budget by default
so the person never waits more than a stated bound; when the budget cuts a stage short the app
says which stage and why, from the engine's report. Memory use and elapsed time of the last
search are visible for the curious.

**Why this priority**: 008 measured a 2 s cold first query and 4.4 s re-ranked queries at one
thread; a demo that appears to hang on a stage is worse than one that says "re-ranking, 12 of
20 pairs so far".

**Independent Test**: Submit a query, immediately submit another; only the second's results
appear and the app stays responsive throughout. Set a 300 ms budget; the report shows the
re-rank stage (or the dense stage) cut short and the list still arrives.

**Acceptance Scenarios**:

1. **Given** a search in progress, **When** another is submitted, **Then** the first's result
   never appears, the second's does, and the UI never blocks (the search field keeps
   accepting input).
2. **Given** the default settings, **When** any search runs, **Then** it completes within the
   default time budget or the engine's report says which stage was cut and why.
3. **Given** the last search, **When** the person looks, **Then** its elapsed time and the
   process footprint at that moment are shown.

---

### Edge Cases

- The bundled resources are missing or incomplete (a build without `--with-wiki`): the app
  says exactly which resource is missing and how it is staged; it does not crash or show an
  empty search.
- The engine refuses the index at open (format version, fingerprint, interrupted commit —
  the pipeline's own refusals): the app shows the engine's message.
- Memory pressure: iOS may terminate the app in the background; on return it re-opens the
  index rather than assuming state.
- Very long passages or titles, right-to-left text, CJK: displayed, not truncated silently.
- A query longer than the embedder's window: the engine truncates it; the app does not need
  to know, but the length is not restricted by the app either.
- Rotation and Dynamic Type: the layout survives both.

## Requirements *(mandatory)*

### Functional Requirements

**The app**

- **FR-001**: The app MUST live at `apps/ios-wiki-demo/`, consume the Swift package at
  `swift/Xtriever/` by path, and bundle the 008 index, both models and the attribution file
  staged by the existing build script; it MUST NOT contain retrieval logic of its own or any
  network access.
- **FR-002**: The app MUST open the bundled index in place (no copy-out), with both models
  memory-mapped, off the main thread, showing what it is doing; the search field MUST become
  active only once the index is open.
- **FR-003**: The app MUST work offline entirely; the only outbound action is opening an
  article's Wikipedia URL in the system browser at the person's request.

**Search and results**

- **FR-004**: A submitted query MUST run through the full pipeline (lexical → dense → fusion →
  re-rank) with explanations requested, under the settings' re-rank depth and time budget.
- **FR-005**: The fused result MUST be shown as soon as it is available and the re-ranked
  result MUST replace it when it arrives, each labelled with its stage; position changes
  between the two MUST be visible per hit.
- **FR-006**: Each hit MUST show the article title, the passage body, its rank, and MUST offer
  the article's URL — all derived from the engine's hit (`titleAndPassage`, `wikipediaURL`,
  chunk provenance), never from a lookup the app maintains.
- **FR-007**: Each hit MUST be able to show its explanation: the seven pipeline features under
  the engine's names, absent stages marked as such.
- **FR-008**: The stage report MUST be shown for every completed search: lexical and dense
  candidate counts, degradation with its reason, the re-rank report (candidates, scored,
  skipped reason), whether a time limit was ignored, elapsed milliseconds — as reported by
  the engine.
- **FR-009**: An empty result MUST be stated as such; an engine error MUST be shown with the
  engine's message; neither MUST leave a spinner on screen.

**Responsiveness**

- **FR-010**: Searches MUST never block the main thread; a newly submitted search MUST cancel
  the previous one's delivery (cooperative — the engine finishes; the app drops the result).
- **FR-011**: Searches MUST run under a default time budget stated in the plan (derived from
  008's device numbers) and the person MUST be able to change it and the re-rank depth in a
  settings screen; a budget-cut stage MUST appear as degradation in the report, never as an
  error, unless strict mode is switched on in settings.
- **FR-012**: The first search after open MUST be labelled as the warm-up it is (008: ~2 s
  to page the vectors in); the app MAY warm the index itself with a fixed query during
  preparation, in which case it says so.

**About**

- **FR-013**: An About screen MUST show, from the engine's report and the shipped files: the
  corpus edition and snapshot date, article and passage counts, corpus identity, embedder and
  re-ranker identities, index format version, this session's open and model load times, and
  the attribution text verbatim with its licence link.

**Discipline**

- **FR-014**: The app MUST be built by the existing package build script plus one project
  generation step (as the harness app is), never by a committed Xcode project; the resources
  MUST be staged, never committed.
- **FR-015**: The app's own logic (state machine of preparation → ready → searching → results;
  the fused-then-reranked merge; the change marking; settings) MUST be covered by tests that
  run on the simulator against the 007 fixture index, so the app's tests need no 1.3 GB
  bundle; a device run of the full app MUST be recorded once, with the same footprint and
  latency instrumentation as the harness, under the 600 MB ceiling (ADR-0010).
- **FR-016**: No change under `crates/` or `swift/Xtriever/Sources/` except what the plan
  names; the FFI wire is unchanged; CI builds nothing for the app (no simulator or device job
  — standing rule).

### Key Entities

- **Preparation state**: idle → loading models / opening index → warming → ready, with the
  engine's timings; or failed with the engine's message.
- **Search**: the query text, the options used (re-rank depth, budget, strict), the fused
  response, the re-ranked response, elapsed times; cancellation token.
- **Displayed hit**: title, passage, article URL, provenance (parent id, ordinal), the seven
  features, the fused rank, the re-ranked rank, the change mark.
- **Stage report view**: the engine's `StageReport` rendered — counts, degradation, re-rank
  report, time-limit flag, elapsed.
- **Settings**: re-rank depth (0 / 5 / 20), time budget in ms (or none), strict mode.
- **About**: `IndexInfo`, the corpus sidecar's fields, the attribution text.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: On the reference device in airplane mode, a submitted question returns a fused
  list within 1 s and the re-ranked list within 3 s after the first (warm-up) search, at the
  default settings — consistent with 008's measured 0.35 s / 2.3 s.
- **SC-002**: The main thread never stalls for more than 100 ms during a search (measured by
  the same timer-cadence method as 007 SC-002, in an app test on the simulator).
- **SC-003**: Every hit's title, passage, URL and seven features on screen equal the engine's
  values for that hit — verified by an app test against the 007 fixture goldens.
- **SC-004**: The About screen's identities and counts equal `IndexInfo` and the corpus
  sidecar; the attribution text equals `ATTRIBUTION.txt` byte for byte.
- **SC-005**: A device run record of the full app shows peak footprint under 600 MB and
  the latencies of SC-001; committed verbatim.
- **SC-006**: Submitting a second query while the first runs shows only the second's results
  in 100 % of 20 app-test iterations.

## Assumptions

- **Interaction**: search on submit (return key), not as-you-type — 008's latencies make live
  search a queue of stale work; the fused-then-reranked progression is the responsiveness.
  *(Owner decision 2026-09-14, Q1 = A: submit only.)*
- **Default budget**: 3,000 ms and re-rank depth 20 at the pipeline defaults — the plan sets
  the number from the 008 records so that the default never cuts a stage on the reference
  device. **Thread count**: the process default — as many threads as the inference library
  takes on the device, bounded by common sense (004 F-005: no gain beyond ~2–4 cores; the
  plan states what the device actually uses) — 008 run 2's numbers (97 ms per pair, fused
  0.35 s, re-ranked 2.3 s) are the demo's truth. *(Owner decision 2026-09-14, Q2 = A.)*
- **Presentation of stages**: one list that transitions from fused to re-ranked with change
  marks, plus a per-hit explanation sheet and a stage-report panel — rather than three
  side-by-side lists (which would cost three searches per query).
- **Minimum iOS**: whatever the 007 package requires (iOS 16); SwiftUI; the harness app's
  project layout (xcodegen `project.yml`, gitignored `.xcodeproj`) reused.
- **Distribution**: sideload only (1.26 GB of resources); no App Store considerations.
- **Quality**: the corpus's retrieval quality is unmeasured (008, owner decision Q2); the app
  makes no claim about it and the About screen does not pretend otherwise.
- **The 007 fixture index** serves the app's own tests on the simulator (40 documents, with
  goldens); the full index is for the device and for people.
