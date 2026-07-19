# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](http://keepachangelog.com/en/1.0.0/)
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## Unreleased

### Added
- Exponential operator, inverse modifier and logarithm using exponential's inverse.
- Raise to power operator, reciprocal operator
- Different display formats for cells, inferred from value syntax

### Changed
- Formula marker is now `<` instead of `,`.
- Removed ubiquitous subscript indexing, now there's a dedicated `.` operator for rearranging the stack.
- Scientific notation in output is triggered by display format specification, not by its presence in neighboring cells

### Fixed
- Column width determination was using byte counts instead of character counts and would break with UTF-8 multi-byte characters.
- Number literals in formulas must start with a digit.

## [0.1.0] - 2026-07-05
Initial release

