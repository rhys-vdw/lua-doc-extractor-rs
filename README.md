# lua-doc-extractor _(lua-doc-extractor-rs)_

[![standard-readme compliant](https://img.shields.io/badge/readme%20style-standard-brightgreen.svg?style=flat-square)](https://github.com/RichardLitt/standard-readme)

Extracts EmmyLua docs from C-style comments.

Document your Lua API in C or C++, where it is defined. The extractor reads
those doc comments and writes a Lua library for
[emmylua-analyzer-rust](https://github.com/EmmyLuaLs/emmylua-analyzer-rust),
allowing EmmyLua to provide API completion, hover docs and type checking.

This repository, `lua-doc-extractor-rs`, is the continuation of
[lua-doc-extractor](https://github.com/rhys-vdw/lua-doc-extractor), rewritten in Rust. It has the same package and executable name.

## Table of Contents

- [Install](#install)
- [Usage](#usage)
  - [Example](#example)
  - [CLI](#cli)
  - [Comments](#comments)
  - [Annotations](#annotations)
- [Contributing](#contributing)
- [License](#license)

## Install

```sh
$ git clone https://github.com/rhys-vdw/lua-doc-extractor-rs
$ cd lua-doc-extractor-rs
$ cargo install --path .
```

## Usage

### Example

#### Input

```cpp
/***
 * Main API
 * @table Api
 * @field Version integer
 */

/***
 * The absolute path to the executable.
 * @global ExecutablePath string
 */

/***
 * @enum NameType
 * @field FirstName 1 First name.
 * @field LastName 2 Last name.
 */

/***
 * @field NameType.FullName 3 Full name, including middle names.
 */

/***
 * Get name by ID
 * @function Api.GetName
 * @param id integer The integer of the person.
 * @param nameType NameType Which name to return.
 * @return string name The full or first name of the person.
 */
int SomeClass::GetName(lua_State *L)
{
```

#### Output

```lua
---@meta

---Main API
Api = {
	---@type integer
	Version = nil
}

---The absolute path to the executable.
---
---@type string
ExecutablePath = nil

---@enum NameType
NameType = {
	---First name.
	FirstName = 1,

	---Last name.
	LastName = 2,

	---Full name, including middle names.
	FullName = 3
}

---Get name by ID
---
---@param id integer The integer of the person.
---@param nameType NameType Which name to return.
---@return string name The full or first name of the person.
function Api.GetName(id, nameType) end
```

### CLI

Process files:

```sh
$ lua-doc-extractor --src some-file.cpp "other-files/*.cpp" --dest output
```

Show usage:

```sh
$ lua-doc-extractor --help
```

To add GitHub source links to the exported library. Provide the `--repo` argument.

```sh
$ lua-doc-extractor --src "**/*.cpp" --dest library --repo https://github.com/beyond-all-reason/spring/blob/62ee0b4/
```

To split the output by the environments a doc's subject is registered in, tag the docs with [`@env`](#env) and route each set of names to a file with `--env`. Docs without a tag are written as before.

```sh
$ lua-doc-extractor --src "**/*.cpp" --dest library --env synced=synced.lua --env unsynced=unsynced.lua --env synced,unsynced=shared.lua
```

To exit with an error code if any errors are reported, provide the `--error` argument.

```sh
$ lua-doc-extractor --src "**/*.cpp" --dest library --error
```

### Comments

Lua docs in document comment blocks (`/*** <docs> */`) will be parsed.

### Annotations

All [emmylua annotations](https://github.com/EmmyLuaLs/emmylua-analyzer-rust/blob/main/docs/emmylua_doc/annotations_EN/README.md) can be used, some with special handling. Some additional annotations are required by lua-doc-extractor.

#### `@global`

```
@global <name> <type> [description]
```

Defines a global variable.

#### `@field`

```
@field <name> <type> [description]
```

Add fields to a table (`@table`, `@class` or `@enum`) by including them in the same comment.

```
@field <table>.<name> <type> [description]
```

You can also add fields to a table defined in a different comment.

#### `@function`

```
@function <name> [description]
```

Defines a global function.

```
@function <table>.<name> [description]
@function <table>:<name> [description]
```

Adds a function to a [`@table`](#table) or [`@class`](#class) defined in a different comment.

Should be paired with `@param`, `@return` and `@generic` attributes.

#### `@table`

```
@table <name>
```

Defines a global table. Fields can be added with [`@field`](#field).

#### `@enum`

```
@enum <name>
```

Defines a global table marked with the `@enum` attribute. Entries can be added with [`@field`](#field).

#### `@class`

```
@class <name>
```

Defines a class. Fields can be added with [`@field`](#field), methods can be added with [`@function`](#function).

#### `@env`

```
@env <name>[, <name>...]
```

Names the environments (Lua states, handles) the doc's subject is registered in. It is not written to the output: it routes the doc to the file given for that exact set of names by `--env`, and is dropped when no `--env` is given. A doc that holds only `@env`, placed before every other doc in a file, tags the whole file; a doc's own `@env` overrides it.

```c
/*** @env unsynced */

/***
 * @function Spring.GetMouseState
 */

/***
 * @function Spring.SendMessage
 * @env synced, unsynced
 */
```

## Contributing

PRs accepted.

## License

[MIT](LICENSE) © Rhys van der Waerden
