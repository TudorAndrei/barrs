---
id: DOC-SCHEDULER-DATA-LAYOUT
kind: doc
tags:
  - performance
  - runtime
---

# Scheduler data layout

## Scope and input

This change applies data-oriented design to the refresh scan in `src/daemon.rs`.
The daemon runs on macOS with a current-thread Tokio runtime. The scan runs
every 250 ms. It reads deadlines and returns two owned ID lists: Lua updates
and other source updates. Source queries and rendering are outside this scan.

The supplied `barrs.lua` has six items. It includes one Rift item and five
system items. Its explicit intervals range from 1 to 1800 seconds. A Mach
Rift item has no polling deadline; the CLI backend can require polling.
The tests also use Lua items and fixed labels.

ASSUMPTION: the supplied six-item configuration represents a small real use
case. The active user configuration and its item-count distribution were not
available. The benchmark therefore also checks empty lists, 64 items, and
1024 items. These larger lists are synthetic workloads.

## Transform and cost

Before this change, deadlines used a `HashMap<String, Instant>`. Each scheduled
item owned a second copy of its ID. A tick scanned the table, copied due IDs
into a temporary vector, then searched the configuration for each due ID to
classify it. With N configured items and D due items, classification could
require O(D × N) string comparisons.

The new array contains only scheduled items. Each row holds a deadline and an
index into `config.items`. Rows follow configuration order. The scan reads
rows in sequence. It reads configuration data only for due rows and appends
their IDs directly to one of the two output lists. The scan costs O(S + D),
where S is the number of scheduled items. It has no intermediate ID list.
Unscheduled labels add no rows. An idle tick allocates no output storage.

The configuration and its schedule have the same owner and lifetime. Startup
builds the schedule; successful reload replaces it with the configuration.
Old-epoch results are discarded before they can advance deadlines. Deadline
updates resolve an ID to its configuration index and use binary search in the
sorted schedule. This update path still searches configuration IDs; this change
does not claim to remove all string lookup from the daemon.

A missing interval creates no row. An overflowing initial deadline creates no
row. An overflowing later deadline keeps the previous deadline. Unknown IDs
are ignored. The existing interval clamp, immediate first Lua update, failed
snapshot retry interval, and refresh coalescing remain in place.

## Design choices

A flat array and configuration indices remove owned string keys and repeated
configuration searches from the scan. Separate deadline storage also keeps
labels, icons, handlers, and other configuration data out of the idle scan.
The compact rows keep their deadline and index together; separate arrays for
these two fields would add another length invariant without removing a scan.

No cache is added. The Rift snapshot is retained because Rift events modify
it and a failed query can use it. Removing that state would change current
behavior and require more external queries. No packed index bits, bitset,
inline-capacity dependency, or native renderer rewrite is needed for this
bounded scheduler change.

## Validation

The existing scheduler transition test covers due and future deadlines,
Lua's first update, unscheduled labels, failed snapshot attempts, unknown IDs,
and pending refresh and epoch decisions. The existing daemon tests cover reload
and stale results. The full locked test suite passes with local socket access:
113 unit tests and 23 Cucumber scenarios. The manual benchmark is ignored in
the normal test suite.

Formatting, Clippy with warnings denied, the locked all-target check, Rust
documentation, and criv validation are checked with the repository toolchain.
There are no dependency, public API, or protocol changes. Native visual behavior
and whole-daemon CPU or energy use were not measured.

## Scan benchmark

Run:

```sh
RUSTUP_TOOLCHAIN=1.97.1 cargo test --release --locked --lib \
  scheduler_scan_cost -- --ignored --nocapture
```

`src/daemon/benchmarks.rs` retains the old production scan and classification
as the reference. Both paths receive the same configuration, deadlines, and
fixed time. The benchmark compares their due-item sets before timing them.
It alternates measurement order across seven rounds and reports the median
and minimum/maximum time per scan. Setup, result sorting, and validation are
outside the timed region; output allocation and destruction are included.

The build uses Rust 1.97.1 on macOS arm64 and the unchanged release profile.
The reference runs in the same binary as the candidate, rather than in a
separate baseline checkout. This is a deliberate benchmark-contract exception:
it holds the inputs, build, and output checks together without adding a public
benchmark API. The measured scope is due-item selection and classification,
not schedule construction, deadline updates, source queries, or full refresh
latency. Timings on this shared machine are observations, not a CI speed gate.

Measured on 2026-11-02. Values are median [minimum, maximum] nanoseconds per scan.
`map` is the reference and `array` is the production path.

```text
0 idle
  map   12.1 [10.5,18.0]
  array 9.9 [8.6,11.0]
0 sparse
  map   10.6 [10.5,15.9]
  array 8.6 [8.5,11.5]
0 all
  map   11.3 [11.2,14.6]
  array 9.1 [9.1,9.8]
0 mostly_labels
  map   11.2 [10.6,11.9]
  array 9.5 [8.6,15.0]
0 labels_only
  map   10.8 [10.5,13.1]
  array 8.6 [8.5,9.5]
6 idle
  map   20.4 [19.4,21.1]
  array 12.8 [12.1,14.9]
6 sparse
  map   81.5 [79.5,93.6]
  array 50.3 [47.2,51.4]
6 all
  map   301.5 [284.6,325.1]
  array 139.2 [124.9,149.6]
6 mostly_labels
  map   59.6 [55.9,63.3]
  array 36.6 [36.1,42.2]
6 labels_only
  map   8.6 [8.5,10.1]
  array 6.9 [6.9,7.1]
64 idle
  map   125.1 [122.5,126.0]
  array 41.3 [41.1,44.6]
64 sparse
  map   1133.0 [1113.8,1152.0]
  array 235.3 [224.2,247.0]
64 all
  map   8771.6 [8674.3,8900.6]
  array 1300.5 [1281.5,1340.0]
64 mostly_labels
  map   1031.8 [1010.7,1051.2]
  array 189.1 [183.9,193.6]
64 labels_only
  map   8.7 [8.5,9.6]
  array 6.9 [6.9,7.2]
1024 idle
  map   2405.7 [2321.5,2459.9]
  array 499.2 [455.8,514.1]
1024 sparse
  map   147663.8 [143776.5,157416.4]
  array 2800.3 [2730.9,3008.2]
1024 all
  map   1192767.3 [1168942.0,1217984.3]
  array 16605.2 [15942.0,16665.8]
1024 mostly_labels
  map   166611.0 [151085.9,188890.5]
  array 2468.6 [2304.8,3821.7]
1024 labels_only
  map   8.8 [8.8,25.9]
  array 7.2 [7.1,68.3]
```

The repeated run kept lower median scan times for every workload. The small
idle case saves only nanoseconds at four scans per second. These results do
not establish a visible speed change or an energy saving for the six-item bar.
The large fully due case removes the repeated configuration search; it is a
synthetic stress case, not a measured user workload. Peak memory and allocator
counts were not measured. The stored schedule no longer owns ID strings.
