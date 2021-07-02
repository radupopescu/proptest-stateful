//
// Copyright 2021 Radu Popescu <mail@radupopescu.net>
//
// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

use proptest::prelude::ProptestConfig;

/// Configuration object for a test run
pub struct Config {
    /// Minimum number of commands in the generated command sequence
    /// (default: 1)
    pub min_sequence_size: usize,

    /// Maximum number of commands in the generated command sequence
    /// (default: 100)
    pub max_sequence_size: usize,

    /// Once the minimal command sequence has been found, also attempt
    /// to simplify the individual commands (default: false)
    pub shrink_commands: bool,

    /// Parameters for the underlying proptest library
    pub proptest: ProptestConfig,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            min_sequence_size: 1,
            max_sequence_size: 100,
            shrink_commands: false,
            proptest: ProptestConfig::default(),
        }
    }
}