---
id: DOC-DATA-LAYOUT-MEASUREMENTS
kind: doc
tags:
  - performance
  - runtime
---

# Data-layout measurements

## Contract

This experiment measures the five candidates found after the scheduler change
at commit `0bed82a`. It does not change production behavior. Prototypes and
measurement helpers compile only in the test build. No public test API,
dependency, release profile, or toolchain change is required.

The platform is macOS arm64 with Rust 1.97.1 and the existing release profile.
The real supplied configuration has six items. The fixtures use six items and
larger synthetic lists. They use stable IDs, text, icons, and left placement.
ASSUMPTION: these fixtures represent the storage and layout costs of a small
bar. They do not represent its system-query latency or real content changes.

The primary metric is nanoseconds per operation. Both paths receive the same
input and preserve the final data result. Setup and output checks are outside
the timed region. Timed outputs pass through `black_box`. Each comparison has
16 warmup operations and nine rounds, with alternating measurement order.
The report gives median, minimum, and maximum round means. These are not
individual-request latency percentiles. Tests run one at a time. A second
process run checks repeatability. The shared machine has no CPU isolation.

The current path and prototype run in the same release test binary. This is
a deliberate exception to separate-checkout benchmarking: no production
candidate is being shipped, and the comparison needs access to private data
without a public benchmark interface. The producer-selection and pointer
references copy the current expressions; the renderer, shown-item update,
JSON output, and producer worker use current production code directly.

## Hypotheses and workloads

1. Renderer batching can remove repeated layout and scene publication. Measure
   one update, three updates, and a full batch at 1, 6, 64, and 256 items.
   Both paths start
   with the same scene and alternate two text values. The prototype uses the same
   remove-and-append item replacement, lays out once, and publishes once.
   Compare the final surface
   state and host plan. The current path uses `render_item` and the real mock
   host. Native AppKit drawing and intermediate-frame visibility are excluded.
   Errors, reloads, additions, and removals need more checks before adoption.
2. Lua dispatch can replace two string sets with one indexed state array.
   Measure a full trace: start, two repeat requests, completion, one repeat,
   and final completion. Counts are 1, 6, 64, and 1024. Compare repeat counts
   and final idle state. This is storage-only work. Lua execution, channels,
   executor scheduling, epoch rejection, and ID resolution are excluded.
   No claim about full dispatch latency follows from this measurement.
3. Producer requests can select items by index. At 1, 6, 64, and 1024 items,
   request one item or all items. Measure selection alone, selection including
   ID resolution, and full worker completion. The last case uses the actual
   blocking job and completion channel. Labels avoid external queries. Compare
   produced snapshots. Candidate indices are sorted, unique, and valid for the
   current epoch. The prototype does not cover Rift fetches or invalid input.
4. Pointer hit tests can scan a compact bounds array. At 1, 6, 64, and 1024
   items, cycle through item centres, two misses, and a boundary point. Both
   paths use the production frame predicate and copy the matched string ID.
   Compare every hit result before timing. The prototype builds bounds once;
   layout-change maintenance, AppKit input reads, and event creation are not
   timed. This measures the hit scan alone, with warm data.
5. Shown snapshots can use indexed storage. At 1, 6, 64, and 1024 items,
   measure updates of all items and JSON-object construction. The current
   update uses `Bar::show` with a noop renderer. The candidate replaces a
   retained array slot after the same noop render. Both clone and destroy
   snapshots. Compare the complete JSON values. The candidate serializes a
   borrowed array as the current ID-keyed object. Reload, unknown IDs, and
   failed renders need more checks before adoption.

None of these changes adds a cache. Peak memory and allocation counts are not
measured. No speed threshold is imposed on this exploratory measurement.
A candidate can be slower or too small to justify a change. Absolute cost on
the six-item workload matters more than ratios on synthetic large lists.

## Reproduction

```sh
RUSTUP_TOOLCHAIN=1.97.1 cargo test --release --locked --lib \
  measure_ -- --ignored --nocapture --test-threads=1
```

The command runs five ignored tests. The producer and shown-snapshot tests
report multiple operation boundaries. `MEASURE` lines contain JSON records
with all nine round means and their summary. Normal tests skip these timing
experiments. Each experiment checks its stated data contract when run.

## Results

Measured on 2026-11-02 at benchmark commit `8534bfb`. Both process runs
passed all five output checks. The table shows run 2 medians for six items.
Times are microseconds per named operation. The raw file contains both runs,
all 47 workloads per run, and all nine round means for each path.

| Operation | Current | Prototype | Current / prototype |
| --- | ---: | ---: | ---: |
| Renderer, one update | 2.736 | 2.726 | 1.00x |
| Renderer, three updates | 8.028 | 3.360 | 2.39x |
| Renderer, six updates | 15.284 | 3.924 | 3.89x |
| Lua state, full six-item trace | 1.521 | 0.013 | 115.15x |
| Producer selection, all six | 0.741 | 0.540 | 1.37x |
| Producer with ID resolution | 0.756 | 0.713 | 1.06x |
| Producer worker, all six | 8.762 | 8.345 | 1.05x |
| Pointer, one poll | 0.020 | 0.019 | 1.06x |
| Shown snapshots, six updates | 0.583 | 0.482 | 1.21x |
| Dump, complete JSON object | 4.554 | 4.529 | 1.01x |

Read the [interactive comparison](data-layout-results.html) and the
[raw samples](data-layout-results.json) for ranges and larger workloads.
The HTML selector switches between six and 64 items and between process runs.
Ratios describe only the named operation, not full daemon speed.

## Decisions from these measurements

- Renderer batching is the first implementation candidate. For six items,
  three updates took 7.81 vs 3.29 microseconds in run 1 and 8.03 vs 3.36 in
  run 2. One update had no useful gain. Six updates took 15.28 vs 3.92 in
  run 2. A three-item batch is closer to a partial scheduled refresh than a
  full update. The benefit still depends on how often updates complete as a
  group. AppKit drawing and screen latency were not measured.
- Indexed Lua state removes much of this isolated storage trace: 1.52 vs
  0.013 microseconds for six items in run 2. Run 1 gave 1.59 vs 0.017.
  That is about 1.5 microseconds saved across the whole six-item trace.
  The large ratio must not be read as a Lua execution or dispatch speedup.
  Carrying epoch-safe indices to this boundary remains implementation work.
- Producer indices help selection when the caller already has indices.
  Resolving all IDs on each request removes most of that gain. At 64 items,
  the full label worker took 33.70 vs 25.25 microseconds in run 2; at six
  items it took 8.76 vs 8.35, with overlapping ranges. At 1024 items, a
  single-item worker was slower with the prototype in both runs. The
  selection-only result does not justify a blanket worker speed claim.
- Flat shown-item storage saved about 0.10 microseconds across six updates
  in run 2, compared with 0.16 in run 1. JSON-object construction had no
  clear gain. This change has low priority for a six-item bar.
- Compact pointer bounds saved about one nanosecond per six-item poll.
  At 1024 items, run 2 gave 0.628 vs 0.507 microseconds per poll. That
  excludes construction and maintenance of the extra array. Defer this
  change for the small bar.

The two process runs varied in absolute time. The stronger renderer results
and Lua storage result persisted; small worker and JSON differences overlap
measurement noise. No formal confidence interval or CI threshold is claimed.
No production optimization was adopted in this measurement task.

## Verification and limits

The five benchmark tests pass in both final process runs. Normal unit and
Cucumber tests, formatting, locked all-target checks, strict Clippy, Rust
documentation, and criv checks are run before completion. The benchmark
modules add no production execution path.

The initial exploratory renderer prototype also used indexed replacement.
It was replaced before commit `8534bfb` by the current remove-and-append
method so that the final benchmark isolates batching. Exploratory samples
are not included in the final result file. The three-update workload was
also fixed before the two final runs.

The fixture text is short and does not include workspace segments or hover
content. Larger fixture sizes are stress cases, not observed user workloads.
There is no measurement of peak memory, allocations, energy, live system
queries, native AppKit calls, or whole-daemon CPU. The next renderer change
must verify failure behavior and reload before it can ship.
