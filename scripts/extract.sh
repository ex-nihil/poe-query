#!/usr/bin/env bash

cargo build --release
../target/release/poe_query -p "/home/nihil/Games/path-of-exile/drive_c/Program Files (x86)/Grinding Gear Games/Path of Exile/" "$(< mods.pql)"
