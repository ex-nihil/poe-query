#!/usr/bin/env bash

RUST_BACKTRACE=1 cargo run -- -v -p ~/code/poe-files/ "$(< examples/mods.pql)"
