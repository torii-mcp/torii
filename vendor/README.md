# vendor

## libsqlite3-sys: SQLite3 Multiple Ciphers

The audit log is a SQLite database encrypted whole, so that a copied disk reads
nothing, not even dates. `rusqlite` bundles plain SQLite and supports encryption
only through SQLCipher, which brings OpenSSL into the build (a full Perl on
Windows GNU, minutes of build on MSVC, megabytes in the binary).

`libsqlite3-sys/` is therefore `libsqlite3-sys` 0.30.1 (the version `rusqlite`
0.32 uses) as published on crates.io, patched in from the root `Cargo.toml`,
with one change: its bundled build compiles the
[SQLite3 Multiple Ciphers](https://github.com/utelle/SQLite3MultipleCiphers)
amalgamation instead of SQLite. Torii opens the database with its own
ChaCha20-Poly1305 cipher.

What differs from the published crate, all marked `Torii:` in the source:

- `build.rs` compiles `sqlite3mc_src::source_dir()/sqlite3mc_amalgamation.c`
  instead of `sqlite3/sqlite3.c`, and links `advapi32` on Windows, which the
  amalgamation needs for randomness and only MSVC links by itself;
- `Cargo.toml` adds the `sqlite3mc-src` build dependency, pinned exactly;
- the SQLite and SQLCipher amalgamations of the published crate are left out:
  the amalgamation comes, byte for byte as released, from `sqlite3mc-src`,
  whose checksum `Cargo.lock` pins.

The committed bindings (`sqlite3/bindgen_bundled_version*.rs`) describe the
SQLite of `libsqlite3-sys` 0.30.1; the amalgamation wraps a newer SQLite, which
keeps every function those bindings declare.

## Updating

1. A new SQLite3 Multiple Ciphers release: bump the exact `sqlite3mc-src`
   version in `libsqlite3-sys/Cargo.toml`, run the mandatory checks and
   `torii log` against an existing `audit.db` to confirm it still opens.
2. A new `rusqlite` that needs another `libsqlite3-sys`: copy that version from
   the cargo registry (`~/.cargo/registry/src/*/libsqlite3-sys-<version>`)
   without `sqlite3/sqlite3.c`, `sqlite3/sqlite3.h`, `sqlite3/sqlite3ext.h` and
   `sqlcipher/`, then reapply the `Torii:` changes above.
