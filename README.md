# catj (Rust streaming parser edition&trade;)
Displays JSON files in a flat format.

This version doesn't read the whole file into memory for parsing, and has no dependencies other than
the Rust standard library.

## Usage
`catj` reads from standard input and writes to standard output (errors to standard error).

```sh
catj <file.json
```
or
```sh
echo '{"hello": "world"}' | catj
```

## Example
Input:
```json
{
  "mappings": {
    "templates": [
      {
        "fields": {
          "mapping": {
            "norms": false,
            "type": "text",
            "fields": {
              "keyword": {
                "ignore_above": 256,
                "type": "keyword"
              }
            }
          }
        }
      }
    ]
  }
}
```

Output:
```txt
.mappings.templates[0].fields.mapping.norms = false
.mappings.templates[0].fields.mapping.type = "text"
.mappings.templates[0].fields.mapping.fields.keyword.ignore_above = 256
.mappings.templates[0].fields.mapping.fields.keyword.type = "keyword"
```

## Why?
* It makes it easier to understand the structure of JSON files.
* The output is valid JavaScript which can be used directly in code.
* It's very helpful when writing queries for tools like [jq](https://stedolan.github.io/jq/manual/).

## Install
Building this program requires a Rust toolchain, 2018 edition or later.
```sh
cargo install --path .
```

## About
This program is a reimplementation of [Soheil
Rashidi](https://github.com/soheilpro)'s [great
idea](https://github.com/soheilpro/catj). Unlike the original, it does not read
the entire input into memory before parsing it. Instead, this program uses the
[JSON Decoding Algorithm](https://github.com/cheery/json-algorithm) by [Henri
Tuhola](https://github.com/cheery) to print the structure *while* parsing it,
and thus not keeping very much of it in memory. Also as a result it has no
dependencies other than the Rust standard library.

So feel free to use it on [truly huge JSON
files](https://github.com/zemirco/sf-city-lots-json).

## Copyright and License
Copyright 2019-2024 William R. Fraser

Licensed under The MIT License (the "License");
you may not use this work except in compliance with the License.
You may obtain a copy of the License in the LICENSE file, or at:

http://www.opensource.org/licenses/mit-license.php

Unless required by applicable law or agreed to in writing, software
distributed under the License is distributed on an "AS IS" BASIS,
WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
See the License for the specific language governing permissions and
limitations under the License.
