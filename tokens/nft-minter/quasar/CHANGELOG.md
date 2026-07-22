# Changelog


## [2026-07-22]

### Changed

- Deliberately NOT migrated to Quasar 0.1.0: this example depends on
  `quasar-metadata`, which was removed upstream before the 0.1.0 release with
  no replacement. It stays on the pre-0.1.0 pins (quasar `623bb70` /
  quasar-svm `cb7565d`) and builds in the `legacy-metadata-examples` CI job
  with the older quasar CLI. Migrate once upstream ships a metadata story for
  0.1.x.

## 2026-07-07

Added this changelog. Changes prior to this date were tracked in git history only.
