#!/usr/bin/env bash

RUST_BACKTRACE=1 cargo run -- -vv -p ~/code/poe-files/ "$(< examples/mods.pql)"
