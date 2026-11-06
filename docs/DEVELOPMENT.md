# Development

Building, testing and releasing ProjectFCG, and where the code lives. See also [DECISIONS.md](DECISIONS.md) and [CHANGELOG.md](CHANGELOG.md).

## Building from source

```sh
cargo test                          # 418 tests: engine, analysis, resources, board, dependencies, tracking, epics/Jira, MS Project XML, recurring and inactive tasks, filters and groups, releases, printing, task calendars, custom fields, risk register, invariants, UI (headless)
cargo test --release --test perf -- --ignored --nocapture   # timings on 500–5,000-task projects
cargo run --release -- --sample
./packaging/build-appimage.sh       # → dist/ProjectFCG-x86_64.AppImage
```
`build-appimage.sh` expects `appimagetool` and the static `runtime-x86_64` from
the AppImage project in `../tools` (or set `APPIMAGETOOL` / `APPIMAGE_RUNTIME`).

The binary has no system dependencies beyond libc at build time; X11, Wayland, EGL/OpenGL and xkbcommon are loaded from the host at runtime. AppImages built locally need the build machine's glibc; the ones GitHub builds on Ubuntu 22.04 need glibc ≥ 2.35. Locally built AppImages carry no update address unless `UPDATE_INFO` is set.

## Releases (GitHub Actions)

`.github/workflows/release.yml` builds all three platforms when a release is
published on GitHub, and attaches the files to it:

1. Bump `version` in `Cargo.toml`, commit and push.
2. **Releases ▸ Draft a new release**, create a tag such as `v0.7.0`, **Publish**.
3. The workflow runs the tests, then builds three files in parallel, each
   smoke-tested with `--self-test`:
   * `ProjectFCG-v0.7.0-linux-x86_64.AppImage`: built on Ubuntu 22.04, so it
     runs on any distribution from 2022 on. It carries its update
     information (`gh-releases-zsync|AnonyMap|ProjectFCG|latest|ProjectFCG-*-linux-x86_64.AppImage.zsync`),
     and a `.zsync` file is uploaded next to it. Gear Lever compares that
     file's SHA-1 with the installed AppImage to see whether there's an update;
     AppImageUpdate can use it to download only what changed. The build fails
     if the update information is missing.
   * `ProjectFCG-v0.7.0-windows-x86_64.exe`: a single file with the app icon.
     There's no installer and nothing to unpack.
   * `ProjectFCG-v0.7.0-macos-universal.dmg`: one app for Apple Silicon and
     Intel Macs, macOS 11 or later. Drag it to Applications.

If a build fails, fix it and run **Actions ▸ Release builds ▸ Run workflow**
with the same tag; files already on the release are replaced. A tag that
doesn't match `Cargo.toml` gets a warning, because the app reports the
Cargo version.

**Unsigned builds.** The Windows and Mac files aren't code-signed, which costs
money: a Windows code-signing certificate, and Apple's $99/year Developer ID
plus notarization. What that means for whoever downloads them:

* Windows SmartScreen says "Windows protected your PC". Click **More info ▸ Run anyway**.
* macOS says it "can't be opened because Apple cannot check it for malicious
  software". On macOS 15, open **System Settings ▸ Privacy & Security** and
  click **Open Anyway** (on older versions, right-click the app and choose
  **Open**). Or run `xattr -dr com.apple.quarantine /Applications/ProjectFCG.app`.

Both can be signed later by adding certificate secrets and a signing step to
the workflow.

**On Windows and macOS:** preferences are kept in `%APPDATA%\ProjectFCG`
and `~/Library/Application Support/ProjectFCG`, and light/dark follows the
system setting. Shortcuts written as Ctrl in the README are Cmd on a Mac.
Double-clicking a `.pfcg` file doesn't open it there yet; use File ▸ Open.

## Web build

The same code compiles to WebAssembly (`cargo build --release --target wasm32-unknown-unknown`).

- **Bundling:** `web/bundle.py` packs the result, wasm-bindgen's `no-modules` loader and the browser shim `web/pfcg.js` into one HTML file that runs from disk.
- **Browser-only code** lives in `src/app/web.rs`, behind `#[cfg(target_arch = "wasm32")]`. That covers files, storage, AutoRecover, the tab title and the unsaved-changes warning.
- **Builds:** `.github/workflows/web.yml` builds it on every push to `main` and keeps the latest on the `web-build` branch, and the release workflow attaches it to each release.

See [web/README.md](../web/README.md) for how saving works and what differs from the desktop app.

## Roadmap (not yet implemented)

| Milestone | Scope |
|---|---|
| ~~Agile~~ | Board, sprints, backlog, burndown, velocity (0.5); epics, issue types, sub-tasks, links, labels, filters, Jira CSV (0.6); board ↔ Gantt dependencies, sprint lifecycle, columns, WIP limits, swimlanes (0.7); releases, cumulative flow (0.8). Still open: Jira REST sync, control chart / cycle time |
| ~~Resources~~ | Done in 0.3. Still open: work contours, overtime, time-phased cost accrual, resource pools, Team Planner, generic/budget resources, leveling that splits tasks |
| ~~Interop~~ | MSPDI XML import/export done in 0.6; task calendars and custom fields in 0.9; print/PDF in 0.9. Still open: `.mpp` reading, splits and outline codes in XML |
| ~~Tracking~~ | Done in 0.6; slippage bars in 0.8; physical % complete in 0.10. Still open: actual work entered per assignment, time-phased actuals |
| ~~Views~~ | Network Diagram and Calendar in 0.10. Still open: Team Planner, dragging boxes in the Network Diagram, a week/day Calendar. Risk register in 0.10; still open: Monte Carlo schedule risk, risk import from Excel |
| Later | Drawings, custom field formulas and value lists, outline codes, multi-level groups, AutoFilter on column headers, split tasks, work weeks in calendars, a high-contrast theme, embedded fonts in PDFs |

## Tests

* `src/*` unit tests (57): CSV parsing (quotes, embedded newlines, BOM, `;` files), Jira dates and headers, ISO 8601 durations, MSPDI duration/lag format codes and day types, calendar arithmetic (including fast paths checked against a day-by-day reference), duration/lag parsing and formatting, date parsing, shortcut map, timescale presets, working-time parsing, sample project, risk matrices for each standard, theme contrast (every accent × mode against WCAG targets), dark-mode colour adaptation, desktop theme detection replies, preference migration.
* `tests/invariants.rs`: 11 seeded randomized tests (the newest: 120 random projects with sprints, stories, board dependencies with % and day lags and inactive tasks, where the fast leveler must equal the reference, scheduling must be idempotent, and every task must start after the sprint it waits on) on generated projects (up to 200 tasks, all link types, lags, constraints, manual tasks, vacations, mixed task types). They check that links are honoured, slack matches critical, summaries roll up dates, work and cost, Work = Units × span, saving and loading is exact, scheduling is idempotent, 25 random edits undo and redo exactly, the fast leveler equals the reference on 80 projects (including selection-only leveling), and leveling clears every overallocation that is possible to clear.
* `tests/scheduling.rs`: 46 behaviour tests whose expected dates were derived by hand from Project's rules (Standard calendar, project start Mon 1/5/2026). They cover all link types, lags and leads, constraints and conflicts, ALAP, deadlines, holidays, elapsed time, manual tasks, summaries, slack, editing operations, undo, file round-trip, and a 2,000-task performance check.
* `tests/analysis.rs`: 12 tests for start drivers, multiple critical paths, start/finish slack, Task Path sets (including through summary links), Status/Late rollups, baselines, and loading 0.1.0 files.
* `tests/resources.rs`: 23 tests for task types, effort-driven, Resource Names notation, resource calendars, costs, overallocation, usage, leveling (order, priority, within-slack, chains, selection), file round-trip, and 300 tasks leveled in under a second.
* `tests/print.rs`: 4 tests: bar labels outside the printed dates, the sample on one page with its tasks, a 300-task plan on Letter, A4 and Tabloid with every row once and the page count right, and printing what the view shows (filter, groups, date range). Each PDF is checked with `qpdf --check` and read back with `pdftotext` where they're installed.
* `tests/calendars.rs`: 12 tests: the same calendar for actual finish, % scheduled, assignments and Resource Usage, typed finishes on night shifts, ten-year exceptions intersected as ranges, pasting a task whose calendar was deleted, 24 Hours and Night Shift tasks, task ∩ resource calendars and ignoring resource calendars, exceptions of either calendar, names, deletes, summaries, repair, files, Project XML and undo, and the Change Working Time dialog's create / switch / delete.
* `tests/custom_fields.rs`: 6 tests: full precision through Fill Down and Project XML, typed values and how they show, refusals, aliases, filters and groups on custom fields, files, undo, repair, and Project XML with Project's field IDs (and fields we don't hold reported).
* `tests/core_regressions.rs`: 13 tests, one per finding of the 0.9 bug hunt.
* `tests/safety.rs`: 5 tests: an edit that panics is undone and reported (the report never holds the plan); a frame that panics closes dialogs and the app keeps running; repeated panics go back to the last plan that drew, and give up after four; AutoRecover copies unsaved work in the background, removes it once saved, never offers a running window's copy, and the next start opens it as an unsaved plan; a normal close leaves nothing behind.
* `tests/review_10.rs`: 9 tests, one per finding of the 0.10 review (recovery asking first and keeping its copy, rolling back a dialog's edit, concurrent and symlinked saves, Replace on dates, Move Task on finish constraints, typed dates in finish-scheduled plans, Fill Down when sorted, a woken window's copy).
* `tests/fuzz_stability.rs`: four seeded, bounded fuzzing campaigns: random edits with extreme values, every app command plus random clicks and keys in every view (the Risk Register included), thousands of mutated `.pfcg`, Project XML and Jira CSV files, and calendar math. Nothing may panic, and no step may take over 20 s. Set `FUZZ_SCALE=20` for a long exploration run. `tests/fuzz_repros.rs` has one minimal test per crash the campaign found.
* `tests/jobs.rs`: 2 tests: a large plan levels in the background with the same result as leveling in place, nothing else can change the plan meanwhile, and Cancel changes nothing.
* `tests/project_tools.rs`: 6 tests: Sort with and without the outline, built-in sorts and back to ID; renumbering moving whole summaries with links intact; Find in any field both ways, opening collapsed summaries; Replace and Replace All (one undo step, all or nothing, Resource Names reassigning); Move Task on auto, manual, constrained, summary and started tasks; Physical % Complete in earned value, the default method, and Project XML.
* `tests/from_finish.rs`: 4 tests: tasks hanging from the finish date with the start worked out, ASAP tasks starting with the project, negative slack when a task can't make the date, files, Project XML and undo.
* `tests/risks.rs`: 12 tests: each standard's levels and scores for the same cell (DoD matrix, PMI P × I, L × I, issues), the worst dimension and per-risk standards, custom matrix colours, statements and EMV, links, IDs never reused and repair, files (and files from before the register), CSV, the sample's register, and the Risk Register view: looking at every risk under every standard changes nothing, edits are one undo step that Undo reloads, Insert and Delete act on risks, and Add Risk for Task.
* `tests/task_views.rs`: 4 tests: Network Diagram columns following links and summaries (and surviving a loop), Calendar weeks, spans across week rows and lanes, and both views selecting a task on click and opening Task Information on double-click.
* `tests/views.rs`: 14 tests: custom filters comparing durations, dates, money and text by type, And/Or precedence, Within, related summary rows, Date Range and Using Resource, Group By (built-in and by field, rollups, collapsing, with filters), saving, undo and validation, and the 0.8 review's findings.
* `tests/releases.rs`: 10 tests: progress, projection and risk as stories move between sprints, what can carry a release, a milestone following its release (and the tasks linked to it), loops reported, releasing with carry-over, repairing files, Fix Version/s in Jira CSV, and the 0.8 review's findings.
* `tests/board.rs`: 17 tests (including the sample's current sprint on every day of the week, and cumulative flow from recorded moves): starting and completing sprints (commitment, carry-over with sub-tasks, frozen velocity, reopening), editing columns (remapping, Done follows the last column); new sprints, ticket moves and ordering, done stamping, progress by points and by count, unestimated fallback, unmarking and deleting sprints, burndown, velocity and its average, assignees booking no work, file round-trip and loading 0.4 files, and the agile sample pinned to a fixed date.
* `tests/tracking.rs`: 13 tests: % complete stamping actuals, an actual start pinning the task, actual finish, actual/remaining duration, Update Project through a date, Mark on Track at the status date, earned value worked by hand and rolled up, the three accrual rules, baselines 0–10, leveling leaving started tasks alone, progress-line points, and saving tracking data while loading 0.5 baselines.
* `tests/epics.rs`: 14 tests: 60 seeded random sequences of board operations (add, move, retype, re-parent, link, unlink, delete, epic→task), checking after every step that the hierarchy rules hold and that undo/redo is exact, then a Jira export → import of the result; repairing hand-edited boards on load; epics outside sprints and their progress, sub-tasks following their parent and counting nothing, cascading deletes, links in both directions with cycle and duplicate checks, an epic driving a Gantt task, keys, and Jira CSV import (a realistic export with repeated Labels/Sprint columns, links and sub-tasks), sprint creation in natural order, renumbering on clashes, and a full export → import round trip.
* `tests/mspdi.rs`: 10 tests: 4,500 randomly damaged XML files (bad numbers, dates, durations, truncation) and 200 damaged Jira CSVs, none of which may panic; a rich plan (all resource types, % and elapsed lags, manual task, baselines 0 and 3, progress, resource vacation) round-trips with identical dates, costs and assignments, and the second trip is byte-identical; units as Project writes them; **every element's order checked against the schema's sequences**; a Project-style file with the project summary task, yearly and "last Monday" recurring holidays, a cross-project link, a midnight shift and placeholder rows; and files written by a second MSPDI writer (MPXJ 16) from our samples.
* `tests/dependencies.rs`: 12 tests: a task following its story's sprint (and moving with it), released at the done time, unplanned stories reported, epics, keys in the Predecessors text, loops refused or reported, stories waiting on tasks and planning conflicts, blockers planned later, deletes, files and undo, leveling, and the 0.7 review's findings.
* `tests/recurring.rs`: 5 tests: weekly series on chosen days, editing in place (links and extra tasks survive), holidays, month-end and yearly dates, daily series over weekends, refusals, files, undo and XML.
* `tests/inactive.rs`: 3 tests: no effect on successors, rollups, totals, resources or leveling; whole inactive phases; files and XML.
* `tests/ui.rs`: 77 end-to-end tests (new in 0.9: Delete after clicking a bar) that drive the real app through egui's input pipeline (keystrokes, typing, clicks, double-clicks, bar and card drags) without a window. They include theme switching; every tab, view and dialog drawn in every theme; tracking tables and dialogs; real mouse drags of cards between columns, into and out of the backlog, onto an epic, and to a precise position; the filter bar; the ticket dialog with sub-tasks and links in one undo step; the Jira import dialog and export; Save as XML and opening XML; and a check that every symbol in the UI exists in the bundled fonts.
* A third independent review, of the 0.8 code, found 11 problems, among them: undoing a filter's deletion applied a different filter, Shift+arrow selected the wrong tasks in a grouped view, a release's old milestone kept its deadline, and the flow chart was empty for imported tickets. All are fixed and pinned by `review_*` tests in `tests/views.rs` and `tests/releases.rs`.
* Rendering the README screenshots turned up four more: the burndown's legend overlapped its axis, the status bar named a custom filter "Custom Filter", grouped views listed summary tasks under any filter, and over a weekend the flow chart never showed where tickets were.
* A second independent review, of the 0.7 scheduler, board and recurring/inactive code, found 18 more. Among them: typing a key followed by non-ASCII text could crash, a sprint could be held up by its own finished story, leveling miscounted inactive tasks and board loops, and editing a recurring task dropped links to its occurrences. All are fixed and pinned by `review_regressions_0_7_*` tests and the randomized leveler check.
* An independent review pass of the 0.6 import/export and board code found eight bugs (among them: Add Sub-task from a card failed, negated Jira statuses like "Not Done" landed in Done, derived MSPDI calendars lost to their base's exceptions, a sprint kept its % after its tickets left). All are fixed and pinned by `review_regressions_*` tests. The randomized board test caught a ninth (an epic could be retyped with itself as parent).
* The AppImage was also exercised by hand under X11 (Xvfb + xdotool) and Wayland (headless Weston).

## Layout

```
src/calendar.rs   working-time calendars & arithmetic
src/duration.rs   durations, lags, Project text formats
src/model.rs      tasks, links, constraints, project, date parsing
src/schedule.rs   CPM scheduler (forward/backward pass, slack, rollups, start drivers, status)
src/resources.rs  resources, assignments, task-type rules, costs, overallocation, usage
src/leveling.rs   resource leveling (dependency-ordered, lazy forward pass)
src/board.rs      Kanban tickets, issue types, epics, sub-tasks, links, sprints, progress, burndown, velocity, cumulative flow
src/releases.rs   releases (fix versions): progress, projection, milestones
src/risks.rs      risk register: risks/opportunities/issues, DoD and PMI scales, levels, matrix, CSV
src/views.rs      custom filter definitions (saved in the file)
src/calendars.rs  base calendars for tasks
src/custom_fields.rs  Text/Number/Flag/Date/Cost custom fields
src/pdf.rs        a small PDF writer (vector drawing, base-14 fonts)
src/recovery.rs   atomic writes, AutoRecover files, crash reports (no window needed)
src/sanitize.rs   values from files into range before scheduling
src/recurring.rs  recurring task patterns and occurrences
src/tracking.rs   actuals, Update Project, Mark on Track, earned value
src/jira.rs       Jira CSV import/export (CSV parser, field mapping)
src/mspdi.rs      Microsoft Project XML (MSPDI) import/export
src/analysis.rs   task paths, critical tasks, baselines
src/format.rs     bar styles, text styles, gridlines, layout, timescale (saved in file)
src/ops.rs        editing operations (insert, indent, link, cell edits …)
src/history.rs    undo/redo
src/io.rs         .pfcg JSON and CSV export
src/app/          egui front end (ribbon, table, gantt, bars, timeline, inspector, dialogs,
                  resource sheet, usage views, resource dialogs, themes, system theme detection,
                  board view, tracking and import dialogs, filters and groups, release dialogs, print,
                  safety (caught panics, AutoRecover), jobs (background leveling), sort, find/replace,
                  move tasks, network diagram, calendar view, risk register)
examples/xmldump.rs  writes the samples as MS Project XML (for checking with other tools)
examples/screens.rs  renders the README screenshots (`xvfb-run cargo run --release --example screens -- docs board`)
examples/profile.rs  profiling driver (callgrind: `cargo build --profile profiling --example profile`)
packaging/        icons (.png/.svg/.ico/.icns), desktop entry, AppRun, AppImage build script, macOS Info.plist
.github/workflows/release.yml  builds the AppImage, .exe and .dmg for each GitHub release
build.rs          embeds the icon in the Windows .exe
```
