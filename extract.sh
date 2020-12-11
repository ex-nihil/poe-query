#!/usr/bin/env bash

cargo build --release
./target/release/poe_query -v -p ~/code/poe-files/ "$(< examples/mods.pql)"
