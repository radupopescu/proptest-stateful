# proptest-stateful - property-based testing for stateful systems

[![Rust](https://github.com/radupopescu/proptest-stateful/actions/workflows/rust.yml/badge.svg)](https://github.com/radupopescu/proptest-stateful/actions/workflows/rust.yml)

## Overview

**proptest-stateful** builds upon the [proptest](https://crates.io/crates/proptest) library to implement model property-based testing for stateful
systems, similarly to the [proper](https://github.com/proper-testing/proper) library described in the book [Property-based testing with Proper, Erlang and Elixir](https://propertesting.com/).

The approach involves defining a simplified model of the system-under-test
(SUT), generating a random sequence of commands that both the SUT and the model
understand, executing the commands on both systems and checking that the
internal state of the two systems remains the same. Please see the [user
guide](https://github.com/radupopescu/proptest-stateful/blob/master/doc/user_guide.md) for detailed instructions.

## License and authorship

The contributors are listed in AUTHORS. This project uses the MPL v2 license, see LICENSE.

## Issues

To report an issue, use the [proptest-stateful issue tracker](https://github.com/radupopescu/proptest-stateful/issues) on GitHub.


