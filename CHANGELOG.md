# Changelog

All notable changes to this project will be documented in this file.

See [Conventional Commits](https://www.conventionalcommits.org/) for commit guidelines.

## [0.2.5](https://github.com/TudorAndrei/barrs/compare/v0.2.4...v0.2.5) - 2026-08-04

#### Bug Fixes
- (**render**) fill top screen inset - ([3453c14](https://github.com/TudorAndrei/barrs/commit/3453c1473cdfa8c48c1370c923b2417da59c92fc)) - TudorAndrei



## [0.2.4](https://github.com/TudorAndrei/barrs/compare/v0.2.3...v0.2.4) - 2026-07-31

#### Bug Fixes
- (**cli**) propagate daemon response errors - ([b01777a](https://github.com/TudorAndrei/barrs/commit/b01777ac9f02624d7c52116df12caffb29ada6c5)) - TudorAndrei
- (**config**) evaluate lua once per load - ([ec570eb](https://github.com/TudorAndrei/barrs/commit/ec570eb4c51223ad929e0a91197935eb96fd8a47)) - TudorAndrei
- (**daemon**) enforce single-instance ready startup - ([115a4db](https://github.com/TudorAndrei/barrs/commit/115a4dba13b03d6c7499c87f923da31c9741dbd1)) - TudorAndrei
- (**ipc**) bound frames and isolate slow clients - ([b2b7212](https://github.com/TudorAndrei/barrs/commit/b2b7212ecbe07b76f6fb7ea888cef57cd20e434d)) - TudorAndrei
- (**render**) honor synthetic hover item targets - ([b097202](https://github.com/TudorAndrei/barrs/commit/b097202a9951d0cb5a1db6058b43b1a9a28b3f98)) - TudorAndrei
- (**render**) reconcile items during reload - ([89320ca](https://github.com/TudorAndrei/barrs/commit/89320ca59f494c4aa229bcb11c3d94df83dbbc06)) - TudorAndrei
- (**rift**) preserve workspace window counts - ([80960f0](https://github.com/TudorAndrei/barrs/commit/80960f0cf81be6010d4bd292460d31c61c0c13d7)) - TudorAndrei
- (**rift**) finish no-op debounce cycles - ([52c37da](https://github.com/TudorAndrei/barrs/commit/52c37daa2820db57a44e79020ef43d3fb43ee69c)) - TudorAndrei
- add notch clearance - ([ff4d7df](https://github.com/TudorAndrei/barrs/commit/ff4d7dff1898580981e2e74a09df21fffe22168d)) - TudorAndrei
#### Performance Improvements
- (**render**) skip unchanged hover publications - ([377ad2c](https://github.com/TudorAndrei/barrs/commit/377ad2c46a4d82d9914d747b00be1578a3f919f2)) - TudorAndrei
#### Documentation
- (**config**) define lua handler contract - ([4d4cbf1](https://github.com/TudorAndrei/barrs/commit/4d4cbf13aaacb7734a539b3a87cdf28909410e10)) - TudorAndrei
- (**design**) specify doctor diagnostics - ([e93f2a1](https://github.com/TudorAndrei/barrs/commit/e93f2a124f52739d2b51f4ab875b0e9614b539b7)) - TudorAndrei
- (**design**) specify display targeting - ([a0d60c3](https://github.com/TudorAndrei/barrs/commit/a0d60c37a20778dd25727ed3c3085cce42180ee8)) - TudorAndrei
- (**design**) specify lua snapshot providers - ([7ee9aac](https://github.com/TudorAndrei/barrs/commit/7ee9aac9b1353185d8b2c26d7bd8e0dda60bf4ab)) - TudorAndrei
- (**release**) align canonical release workflow - ([03275ee](https://github.com/TudorAndrei/barrs/commit/03275ee0c464a8bcdfee5ef001c35d3cec531e3a)) - TudorAndrei
- record historical architecture decisions - ([7a706cc](https://github.com/TudorAndrei/barrs/commit/7a706cc484b00b6157fd2190a90398ca2e8d4ad5)) - TudorAndrei
- initialize criv knowledge graph - ([8863dcc](https://github.com/TudorAndrei/barrs/commit/8863dcc1af23cf1e6bdb622bdc13c7e0b25c3787)) - TudorAndrei
#### Tests
- (**daemon**) record lifecycle smoke coverage - ([4260646](https://github.com/TudorAndrei/barrs/commit/4260646917fae9c4a49f41af8f0b69c215c70277)) - TudorAndrei
- (**daemon**) characterize refresh and rift state transitions - ([814fdf7](https://github.com/TudorAndrei/barrs/commit/814fdf758a24c445339f2b9df05a6c1c564877aa)) - TudorAndrei
#### Refactoring
- (**render**) confine appkit host to main thread - ([d44d932](https://github.com/TudorAndrei/barrs/commit/d44d9328cb85f581dfda28fea3da8f35e365d277)) - TudorAndrei
#### Miscellaneous Chores
- (**ci**) enforce rust quality gates - ([13167fd](https://github.com/TudorAndrei/barrs/commit/13167fd5c71636cff4c84844a6c9cf41a1806339)) - TudorAndrei
- (**plans**) record verification audit - ([03f1f8c](https://github.com/TudorAndrei/barrs/commit/03f1f8c49df22570057e0c1745c986597cb84c8f)) - TudorAndrei



## [0.2.0](https://github.com/TudorAndrei/barrs/compare/v0.1.13...v0.2.0) - 2026-06-19

### Features

- (**release**) automate conventional releases - ([b0d6d0a](https://github.com/TudorAndrei/barrs/commit/b0d6d0a0ea729cbcc1cff8dd044cd4aa4e239e16)) - TudorAndrei
