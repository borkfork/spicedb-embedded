# Changelog

## [1.0.0](https://github.com/borkfork/spicedb-embedded/compare/spicedb-grpc-tonic-v0.2.0...spicedb-grpc-tonic-v1.0.0) (2026-02-25)


### ⚠ BREAKING CHANGES

* **deps:** `trait Debug` was a supertrait of `trait Message`. This is no longer required by `prost`. If your code relies on `trait Debug` being implemented for every `impl Message`, you must now explicitly state that you require both Debug and Message. For example: `where M: Debug + Message`

### Features

* Add generated code to published spicedb-grpc-tonic crate ([#45](https://github.com/borkfork/spicedb-embedded/issues/45)) ([9e70efe](https://github.com/borkfork/spicedb-embedded/commit/9e70efe5ab48c50e888c91c950882b750d344086))


### Bug Fixes

* **deps:** update rust crates ([#68](https://github.com/borkfork/spicedb-embedded/issues/68)) ([d8aeb92](https://github.com/borkfork/spicedb-embedded/commit/d8aeb925c8f4b64ca9ece763b7b45ef3502555ba))
* Fix building from go source ([#46](https://github.com/borkfork/spicedb-embedded/issues/46)) ([6f36e93](https://github.com/borkfork/spicedb-embedded/commit/6f36e93f244b4c2a2cfb533cb362ed8fc747174c))
