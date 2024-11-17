# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

**Note:** Version 0 of Semantic Versioning is handled differently from version 1 and above.
The minor version will be incremented upon a breaking change and the patch version will be incremented for features.

## [Unreleased]

### Fixes

### Features

### Breaking

## [2.0.0] - 2024-11-17

- api: add getLeaderSchedule ([#1](https://github.com/solana-stream-solutions/solfees/pull/1))
- rpc: run multiple handlers ([#2](https://github.com/solana-stream-solutions/solfees/pull/2))
- api: fix getLeaderSchedule for frontend ([#3](https://github.com/solana-stream-solutions/solfees/pull/3))
- rust: bump to 1.82.0 ([#4](https://github.com/solana-stream-solutions/solfees/pull/4))
- api: use separate thread for WebSocket ([#5](https://github.com/solana-stream-solutions/solfees/pull/5))
- api: fix getLatestBlockhash for rollback / lastValidBlockHeight ([#6](https://github.com/solana-stream-solutions/solfees/pull/6))
- metrics: add requests queue size ([#7](https://github.com/solana-stream-solutions/solfees/pull/7))
- api: optimize getLeaderSchedule ([#9](https://github.com/solana-stream-solutions/solfees/pull/9))
- geyser: wait all transactions before process block ([#10](https://github.com/solana-stream-solutions/solfees/pull/10))
- frontend: init ([#8](https://github.com/solana-stream-solutions/solfees/pull/8))
- geyser: use process_compute_budget_instructions ([#15](https://github.com/solana-stream-solutions/solfees/pull/15))
- backend: improve parallelism ([#16](https://github.com/solana-stream-solutions/solfees/pull/16))
- geyser: do not stream outdated data ([#17](https://github.com/solana-stream-solutions/solfees/pull/17))
- backend: add metrics of used resources ([#18](https://github.com/solana-stream-solutions/solfees/pull/18))
- backend: fix ws stream shutdown ([#19](https://github.com/solana-stream-solutions/solfees/pull/19))
- api: add `skipZeros` to `gRPF` solfees ([#20](https://github.com/solana-stream-solutions/solfees/pull/20))
- api: fix context slot ([#21](https://github.com/solana-stream-solutions/solfees/pull/21))
