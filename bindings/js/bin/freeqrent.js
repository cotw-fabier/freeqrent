#!/usr/bin/env node
// The npm `freeqrent` command: the Rust CLI's flags over the WebAssembly
// build (SPEC §10.5). See bin/cli.cjs.
'use strict';

process.exitCode = require('./cli.cjs').main(process.argv.slice(2));
