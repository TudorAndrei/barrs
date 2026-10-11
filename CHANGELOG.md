# Changelog

All notable changes to this project will be documented in this file.

See [Conventional Commits](https://www.conventionalcommits.org/) for commit guidelines.

- - -
## [0.3.0](https://github.com/TudorAndrei/barrs/releases/tag/v0.3.0) - 2026-10-11

### Features

- (**config**) add the lua plugin kind with an update function - ([82c74a8](https://github.com/TudorAndrei/barrs/commit/82c74a87b4ae44327ef5951b618654db6652bdcd)) - TudorAndrei
- (**daemon**) run Lua update functions on the executor - ([3de3bed](https://github.com/TudorAndrei/barrs/commit/3de3bed54c7e9341c45fb99ba2eb11ea65b7033c)) - TudorAndrei
- (**daemon**) stop Lua event handlers that run longer than one second - ([ca32842](https://github.com/TudorAndrei/barrs/commit/ca32842512cc7198d237c28e3309e7eb0224782f)) - TudorAndrei

### Bug Fixes

- (**daemon**) send pending replies before the daemon exits on stop - ([88a079a](https://github.com/TudorAndrei/barrs/commit/88a079a7c16da880d1f0e41fa6dad051b3b34543)) - TudorAndrei

### Documentation

- (**lua**) document Lua update functions and record ADR-0020 - ([189d620](https://github.com/TudorAndrei/barrs/commit/189d620545d6a8c0b44806df5ca07d274091aefe)) - TudorAndrei

### Build system

- (**deps**) update mlua to v0.12 - ([fd5acf3](https://github.com/TudorAndrei/barrs/commit/fd5acf316dbeb9e93a654acd31cf653b5ccb6bf9)) - TudorAndrei
- (**deps**) update usage-rs to v7 - ([73665ad](https://github.com/TudorAndrei/barrs/commit/73665ad9919c3dd741dd3e26f01294863bdb2851)) - TudorAndrei
- (**lints**) deny undocumented and multi-operation unsafe blocks - ([0c0e01a](https://github.com/TudorAndrei/barrs/commit/0c0e01a64acc8b6e6ecb705a06423b5ff65b49df)) - TudorAndrei
- (**lints**) deny unsafe_op_in_unsafe_fn - ([67c8617](https://github.com/TudorAndrei/barrs/commit/67c86179baecaa42e93c837c183ad859388e73d0)) - TudorAndrei
- (**mbx**) update mr-boxington to 1.23.0 - ([8e506d1](https://github.com/TudorAndrei/barrs/commit/8e506d101a335dce842d052c851d869dc43e7dc0)) - TudorAndrei
- (**mise**) add cargo-audit and an audit task - ([0f7cb11](https://github.com/TudorAndrei/barrs/commit/0f7cb11a8ac5361e10f35a029dd1611e4c1545dd)) - TudorAndrei

### Refactoring

- (**daemon**) detect stop from the request and share the readiness line - ([2aafe5d](https://github.com/TudorAndrei/barrs/commit/2aafe5db2898cc152930b47d6e59b503d817e434)) - TudorAndrei
- (**lua**) own the Lua runtime on a dedicated executor thread - ([4edd5d0](https://github.com/TudorAndrei/barrs/commit/4edd5d02ea9f0cbd9ea4257de0f7d9d778c7281a)) - TudorAndrei
- (**render**) drop unread noop renderer state and gate test-only accessors - ([810b7ed](https://github.com/TudorAndrei/barrs/commit/810b7ed7558178445be0b9efcd0fb1058ecabdb9)) - TudorAndrei
- (**rift**) read mach_task_self_ instead of the deprecated libc wrapper - ([ef60172](https://github.com/TudorAndrei/barrs/commit/ef6017257ed39436f93cc360b3703fcea127730c)) - TudorAndrei
- (**rift**) use expect with a reason for the deprecated mach_task_self - ([ee4b2ad](https://github.com/TudorAndrei/barrs/commit/ee4b2ad568e2d74965eec7619509bb43e7894683)) - TudorAndrei
- (**unsafe**) document each unsafe block and keep one operation per block - ([2f35cdf](https://github.com/TudorAndrei/barrs/commit/2f35cdfe3e7f1dfdc61ec91587eabf8a5709af3a)) - TudorAndrei

- - -

## [0.2.7](https://github.com/TudorAndrei/barrs/releases/tag/v0.2.7) - 2026-09-28

### Documentation

- (**release**) record the cog bump release flow - ([92d73aa](https://github.com/TudorAndrei/barrs/commit/92d73aaf57ff95da2fa26e52324c449f74a98135)) - TudorAndrei

### Build system

- (**mise**) route cargo through mbx shared build cache - ([8d2c1dc](https://github.com/TudorAndrei/barrs/commit/8d2c1dcd8fc365e89b255b5cbf47abc592b8358b)) - TudorAndrei

### Continuous Integration

- (**release**) release with cog bump hooks - ([60e5397](https://github.com/TudorAndrei/barrs/commit/60e539719b404bec010a6e13a6398b7a16b46a58)) - TudorAndrei
- cache rust builds with mr-boxington - ([e8544fd](https://github.com/TudorAndrei/barrs/commit/e8544fd16fe4b43d4a5074f561ddb788ce745f20)) - TudorAndrei

### Refactoring

- (**app**) satisfy strict clippy lints - ([8295338](https://github.com/TudorAndrei/barrs/commit/8295338bfd8f55de0d060f40062af65bde4db5e9)) - TudorAndrei
- (**cli**) replace clap with usage-rs - ([505b8b2](https://github.com/TudorAndrei/barrs/commit/505b8b28f90d3490e4160a9e4f137f12f0efd404)) - TudorAndrei
- (**config**) satisfy strict clippy lints - ([e083a8b](https://github.com/TudorAndrei/barrs/commit/e083a8b360e50a9808d01964cf600ae3c1b03035)) - TudorAndrei
- (**daemon**) satisfy strict clippy lints - ([ef29875](https://github.com/TudorAndrei/barrs/commit/ef29875d7f533318ec9271f7a3332c0b97df6a8c)) - TudorAndrei
- (**ipc**) satisfy strict clippy lints - ([dc48bad](https://github.com/TudorAndrei/barrs/commit/dc48bad8aa24c2b3828b4bbfc9a558d9c7c1d303)) - TudorAndrei
- (**plugin**) satisfy strict clippy lints - ([5133c10](https://github.com/TudorAndrei/barrs/commit/5133c106d324d9f0fc75b4745882f91435aa7a46)) - TudorAndrei
- (**render**) satisfy strict clippy lints - ([f0f745f](https://github.com/TudorAndrei/barrs/commit/f0f745f280d113e83ffc64e2651c24d8dcf2ef4b)) - TudorAndrei
- (**rift**) satisfy strict clippy lints - ([c1c9fa6](https://github.com/TudorAndrei/barrs/commit/c1c9fa654b66aeccba10eaa6204c29bd3316740a)) - TudorAndrei

- - -


## [0.2.6](https://github.com/TudorAndrei/barrs/compare/v0.2.5...v0.2.6) - 2026-08-04

### Bug Fixes

- (**release**) pass tag to release workflow - ([7aa7c9d](https://github.com/TudorAndrei/barrs/commit/7aa7c9dd3435bc371893da160664f5fd1d16bb33)) - TudorAndrei

## [0.2.5](https://github.com/TudorAndrei/barrs/compare/v0.2.4...v0.2.5) - 2026-08-04

### Bug Fixes

- (**render**) fill top screen inset - ([3453c14](https://github.com/TudorAndrei/barrs/commit/3453c1473cdfa8c48c1370c923b2417da59c92fc)) - TudorAndrei

## [0.2.4](https://github.com/TudorAndrei/barrs/compare/v0.2.3...v0.2.4) - 2026-07-31

### Bug Fixes

- (**cli**) propagate daemon response errors - ([b01777a](https://github.com/TudorAndrei/barrs/commit/b01777ac9f02624d7c52116df12caffb29ada6c5)) - TudorAndrei
- (**config**) evaluate lua once per load - ([ec570eb](https://github.com/TudorAndrei/barrs/commit/ec570eb4c51223ad929e0a91197935eb96fd8a47)) - TudorAndrei
- (**daemon**) enforce single-instance ready startup - ([115a4db](https://github.com/TudorAndrei/barrs/commit/115a4dba13b03d6c7499c87f923da31c9741dbd1)) - TudorAndrei
- (**ipc**) bound frames and isolate slow clients - ([b2b7212](https://github.com/TudorAndrei/barrs/commit/b2b7212ecbe07b76f6fb7ea888cef57cd20e434d)) - TudorAndrei
- (**render**) honor synthetic hover item targets - ([b097202](https://github.com/TudorAndrei/barrs/commit/b097202a9951d0cb5a1db6058b43b1a9a28b3f98)) - TudorAndrei
- (**render**) reconcile items during reload - ([89320ca](https://github.com/TudorAndrei/barrs/commit/89320ca59f494c4aa229bcb11c3d94df83dbbc06)) - TudorAndrei
- (**rift**) preserve workspace window counts -
  ([80960f0](https://github.com/TudorAndrei/barrs/commit/80960f0cf81be6010d4bd292460d31c61c0c13d7)) - TudorAndrei
- (**rift**) finish no-op debounce cycles - ([52c37da](https://github.com/TudorAndrei/barrs/commit/52c37daa2820db57a44e79020ef43d3fb43ee69c)) - TudorAndrei
- add notch clearance - ([ff4d7df](https://github.com/TudorAndrei/barrs/commit/ff4d7dff1898580981e2e74a09df21fffe22168d)) - TudorAndrei

### Performance Improvements

- (**render**) skip unchanged hover publications - ([377ad2c](https://github.com/TudorAndrei/barrs/commit/377ad2c46a4d82d9914d747b00be1578a3f919f2)) - TudorAndrei

### Documentation

- (**config**) define lua handler contract - ([4d4cbf1](https://github.com/TudorAndrei/barrs/commit/4d4cbf13aaacb7734a539b3a87cdf28909410e10)) - TudorAndrei
- (**design**) specify doctor diagnostics - ([e93f2a1](https://github.com/TudorAndrei/barrs/commit/e93f2a124f52739d2b51f4ab875b0e9614b539b7)) - TudorAndrei
- (**design**) specify display targeting - ([a0d60c3](https://github.com/TudorAndrei/barrs/commit/a0d60c37a20778dd25727ed3c3085cce42180ee8)) - TudorAndrei
- (**design**) specify lua snapshot providers - ([7ee9aac](https://github.com/TudorAndrei/barrs/commit/7ee9aac9b1353185d8b2c26d7bd8e0dda60bf4ab)) - TudorAndrei
- (**release**) align canonical release workflow - ([03275ee](https://github.com/TudorAndrei/barrs/commit/03275ee0c464a8bcdfee5ef001c35d3cec531e3a)) - TudorAndrei
- record historical architecture decisions - ([7a706cc](https://github.com/TudorAndrei/barrs/commit/7a706cc484b00b6157fd2190a90398ca2e8d4ad5)) - TudorAndrei
- initialize criv knowledge graph - ([8863dcc](https://github.com/TudorAndrei/barrs/commit/8863dcc1af23cf1e6bdb622bdc13c7e0b25c3787)) - TudorAndrei

### Tests

- (**daemon**) record lifecycle smoke coverage - ([4260646](https://github.com/TudorAndrei/barrs/commit/4260646917fae9c4a49f41af8f0b69c215c70277)) - TudorAndrei
- (**daemon**) characterize refresh and rift state transitions -
  ([814fdf7](https://github.com/TudorAndrei/barrs/commit/814fdf758a24c445339f2b9df05a6c1c564877aa)) - TudorAndrei

### Refactoring

- (**render**) confine appkit host to main thread - ([d44d932](https://github.com/TudorAndrei/barrs/commit/d44d9328cb85f581dfda28fea3da8f35e365d277)) - TudorAndrei

### Miscellaneous Chores

- (**ci**) enforce rust quality gates - ([13167fd](https://github.com/TudorAndrei/barrs/commit/13167fd5c71636cff4c84844a6c9cf41a1806339)) - TudorAndrei
- (**plans**) record verification audit - ([03f1f8c](https://github.com/TudorAndrei/barrs/commit/03f1f8c49df22570057e0c1745c986597cb84c8f)) - TudorAndrei

## [0.2.0](https://github.com/TudorAndrei/barrs/compare/v0.1.13...v0.2.0) - 2026-06-19

### Features

- (**release**) automate conventional releases - ([b0d6d0a](https://github.com/TudorAndrei/barrs/commit/b0d6d0a0ea729cbcc1cff8dd044cd4aa4e239e16)) - TudorAndrei
