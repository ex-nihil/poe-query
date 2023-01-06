# Path of Exile Query

[![latest release](https://img.shields.io/github/v/release/ex-nihil/poe-query?label=latest%20release "latest release")](https://github.com/ex-nihil/poe-query/releases/latest)

Perform queries directly against a Path of Exile installation with a [jq](https://stedolan.github.io/jq/) like query format.

The program depends on specifications from this repository [github.com/poe-tool-dev/dat-schema](https://github.com/poe-tool-dev/dat-schema/tree/main/dat-schema).  
Releases are bundled with the latest at the time the release was made, but you might need to update it if there has not been a release for some time.  
The spec should be placed in a `dat-schema` folder in the same directory as the `poe_query` binary.

## Usage / Examples

Iterate over all `Id` fields in `Mods.dat`
```sh 
$ poe_query .Mods[1].Id
"Strength1"
```

Traverse through a foreign key. (`Name` taken from `ModType[364]`)
```sh
$ poe_query .Mods[0].ModTypeKey.Name
"Strength"
```

Get `Id` from the first rows in `Mods.dat` and `Stats.dat`
```sh
$ poe_query '.Mods[0].Id, .Stats[0].Id'
[
  "Strength1",
  "level"
]
```

Construct a JSON object from the wanted fields in the first row of `Mods.dat`
```sh
$ poe_query '.Mods[0] | { foo: .Id, bar: .GenerationType }'
{
  "foo": "Strength1",
  "bar": "SUFFIX"
}
```

Transforming data with `transpose`, `map`, `reduce`.
```sh
$ poe_query '.Mods[0] | [.SpawnWeight_TagsKeys[].Id, .SpawnWeight_Values]'
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

$ poe_query '.Mods[0] | [.SpawnWeight_TagsKeys[].Id, .SpawnWeight_Values] | transpose | map({([0]): [1]}) | reduce .[] as $item ({}; . + $item)
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

# wishlist (TODO)
 - translations
 - reduce amount of copying of data
 - optional multithreading (HDD vs SSD)
 - darwin release targets
 - offer to download latest spec if not found

 # known issues
   - codebase is a crime against humanity
   - `{foo: 1, bar: 2} | .foo, .bar` returns `[1,null]` instead of `[1,2]`

The goal is not to be 100% like jq.  
But if you have something you would want implemented a [jqplay](https://jqplay.org/) example would be helpful.