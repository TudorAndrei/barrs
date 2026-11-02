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
