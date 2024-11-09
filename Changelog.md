# Changelog

## 0.2.0

New mpmc algorithm. Previous algorithm was based on block write counters - block's
len is updated when counter reaches 0. New one is based on bitblocks - each element
in block, have corresponding bit, and it is raised when write finished. The Number 
of continuously raised bits is block's len. This ensures that readers will see their
messages ASAP, while maintaining message order. This requires less atomic stores
on the writer's side as well, so it is almost 30% faster!

See [how it works](doc/how_it_works.md#atomic-bitblocks-v020).

# Changed

- `spmc` and `mpmc` now have separate `Reader`s.
- `LendingItereator` replaced with non-GAT `LendingReader`.

# Added

- Readers now `Clone`able.
- MIRI-friendly fuzzy testing. 

## 0.1.1

The library is MIRI friendly now.

## 0.1.0

Initial release.