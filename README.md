# catj (Rust streaming parser edition&trade;)
Displays JSON files in a flat format.

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
{"todo": "fill this in"}
```

Output:
```txt
.todo = "fill this in"
```

## Why?
* It makes it easier to understand the structure of JSON files.
* The output is valid JavaScript which can be used directly in code.
* It's very helpful when writing queries for tools like [jq](https://stedolan.github.io/jq/manual/).

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
