#!/bin/sh

# TODO: apollo-rs does not support comments, need to strip them out
# https://github.com/apollographql/apollo-rs/issues/174

curl -Ls https://github.com/poe-tool-dev/dat-schema/archive/refs/heads/main.zip --output dat_schema.zip
unzip -jo dat_schema.zip dat-schema-main/dat-schema/* -d dat-schema
rm dat_schema.zip
