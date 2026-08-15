# Path of Exile Query

[![latest release](https://img.shields.io/github/v/release/ex-nihil/poe-query?label=latest%20release "latest release")](https://github.com/ex-nihil/poe-query/releases/latest)

Perform queries directly against a Path of Exile installation with a [jq](https://stedolan.github.io/jq/) like query format.

The program depends on specifications from this repository [github.com/poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema/tree/main/dat-schema).  
Releases are bundled with the latest at the time the release was made, but you might need to update it if there has not been a release for some time.  
The spec should be placed in a `dat-schema` folder in the same directory as the `poe_query` binary.

## Usage / Examples

The binary is organized into subcommands:

```sh
poe_query query "<query>"     # run a query against the game data
poe_query tables              # list every table in the schema (no install needed)
poe_query describe <Table>    # show a table's columns, types, references, enums
poe_query translate <id=val>  # render stat ids and values as in-game text
poe_query serve               # answer NDJSON requests over stdio
```

Shared flags: `-p <INSTALL_DIR>`, `-l <LANGUAGE>`, `-s <SCHEMA_DIR>` (defaults to
`dat-schema` next to the binary), `-v` (repeat for more logging).

Errors are reported on stderr with distinct exit codes: `1` setup, `2` parse
error, `3` evaluation error. Unknown tables and columns are hard errors with a
"did you mean" suggestion instead of silently returning `null`.

Query the first `Id` field in `Mods.dat`
```sh 
$ poe_query query .Mods[1].Id
"Strength1"
```

Traverse through a foreign key. (`Name` taken from `ModType[364]`)
```sh
$ poe_query query .Mods[0].ModTypeKey.Name
"Strength"
```

Get `Id` from the first rows in `Mods.dat` and `Stats.dat`
```sh
$ poe_query query '.Mods[0].Id, .Stats[0].Id'
[
  "Strength1",
  "level"
]
```

Construct a JSON object from the wanted fields in the first row of `Mods.dat`
```sh
$ poe_query query '.Mods[0] | { foo: .Id, bar: .GenerationType }'
{
  "foo": "Strength1",
  "bar": "SUFFIX"
}
```

Transforming data with `transpose`, `map`, `reduce`.
```sh
$ poe_query query '.Mods[0] | [.SpawnWeight_TagsKeys[].Id, .SpawnWeight_Values]'
[
  [
    "ring",
    "default",
    "amulet",
    "default",
    "belt",
    "default",
    "str_armour",
    "default",
    "str_dex_armour",
    "default",
    "str_int_armour",
    "default",
    "str_dex_int_armour"
  ],
  [
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    1000,
    0
  ]
]

$ poe_query query '.Mods[0] | [.SpawnWeight_TagsKeys[].Id, .SpawnWeight_Values] | transpose | map({([0]): [1]}) | reduce .[] as $item ({}; . + $item)
 {
  "ring": 1000,
  "default": 1000,
  "amulet": 1000,
  "default": 1000,
  "belt": 1000,
  "default": 1000,
  "str_armour": 1000,
  "default": 1000,
  "str_dex_armour": 1000,
  "default": 1000,
  "str_int_armour": 1000,
  "default": 1000,
  "str_dex_int_armour": 0
}
```
There's an alias for the map/reduce operation above named `zip_to_obj` that can be used instead.

## Stat translation

`translate` renders raw stat ids and values as the text shown in game, using
`Metadata/StatDescriptions/stat_descriptions.txt` from the installation
(`--file` selects a sibling file such as `skill_stat_descriptions`; `-l`
selects the language). Values are `id=value` or `id=min..max`.

```sh
$ poe_query translate base_maximum_life=10..20 attack_speed_+%=-8
{
  "lines": [
    "+(10-20) to maximum Life",
    "8% reduced Attack Speed"
  ],
  "unmatched": [],
  "hidden": []
}
```

`unmatched` lists ids no description block knows; `hidden` lists ids the game
intentionally never displays (`no_description`).

## Serve mode

`poe_query serve` indexes the installation once and then answers requests in a
loop: one JSON request per line on stdin, one JSON response per line on stdout
(logs stay on stderr). Decoded data files are cached across requests, so
repeated queries against the same tables skip the startup cost entirely.

```sh
$ echo '{"id": 1, "method": "query", "params": {"query": ".Mods[0].Id"}}' | poe_query serve
{"id":1,"ok":true,"result":"Strength1","timings":{"parse_ms":0,"eval_ms":28}}
```

Methods: `query` (`params.query`), `tables`, `describe` (`params.table`),
`translate` (`params.stats` as `[{"id", "value"}]` or `[{"id", "min", "max"}]`,
optional `params.file`; parsed description files are cached for the session),
and `ping` (version, install path, table count). Failures come back in-band as
`{"ok": false, "error": {"kind", "message", "suggestion"}}` where `kind` is a
stable discriminator (`parse`, `unknown_table`, `unknown_column`, `type_error`,
`missing_data_file`, `schema_mismatch`, `unsupported`, `internal`,
`bad_request`).

# wishlist (TODO)
 - reduce amount of copying of data
 - optional multithreading (HDD vs SSD)
 - darwin release targets
 - offer to download latest spec if not found

The goal is not to be 100% like jq.  
But if you have something you would want implemented a [jqplay](https://jqplay.org/) example would be helpful.