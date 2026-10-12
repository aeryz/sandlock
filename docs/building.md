# Building Sandlock

Run these commands from the repository root. See the
[requirements](../README.md#requirements) for the supported platform.

## Selecting features

The workspace enables `http`, `checkpoint`, `image`, `profiles`, and `cow` by
default. The CLI, FFI, and OCI crates forward these features to `sandlock-core`.

```bash
# All default features
cargo build --release

# No optional features
cargo build --release --no-default-features

# Selected features
cargo build --release --no-default-features --features http,profiles

# Build just the FFI library with checkpoint support
cargo build --release -p sandlock-ffi --no-default-features --features checkpoint
```

Disabling defaults retains Landlock confinement, seccomp supervision, process
tracking, ordinary filesystem access and network policy enforcement.

## Feature behavior

| Feature | Enables | Dependencies and behavior when disabled |
|---------|---------|----------------------------------------|
| `http` | HTTP interception, HTTP access rules and TLS handling | HTTP operations require this feature; ordinary network policy remains available. |
| `checkpoint` | Checkpoint capture, restore and persistence | Enables `bincode` and compilation of the C restore stub. Rust checkpoint APIs and OCI checkpoint commands are omitted when disabled. |
| `image` | Container image extraction and loading | Enables `cow` to protect cached image contents. Registry and Docker daemon transports also require `http`; local OCI layouts and archives work without it. |
| `profiles` | TOML profile loading and writing | Enables TOML support. The profile schema and JSON policy inspection remain available without it. TOML operations return a feature-disabled error. |
| `cow` | Filesystem branches, commit/abort, transactions and recovery | Enables filesystem traversal and hashing dependencies. Requests for a workdir, branch storage, disk limits or nondefault branch actions fail explicitly when disabled. |

CLI learning requires both `profiles` and `cow`: it observes writes in an
isolated layer and discards them afterward. Enabling `profiles` alone supports
TOML configuration without enabling learning.

Checkpoint and image C entry points are compiled and exported only when their
respective features are enabled. The shipped C header describes the default
build with all features; applications linking a reduced library must avoid
entry points for disabled features. CLI image requests fail explicitly when
image support is disabled. Serialized policy fields are retained
across feature configurations; COW configuration is validated again before a
sandbox starts.

## Embedding the Rust library

Disable defaults on the library dependency for a minimal embedder:

```toml
[dependencies]
sandlock-core = { version = "0.8", default-features = false }
```

Add selected features as needed:

```toml
[dependencies]
sandlock-core = { version = "0.8", default-features = false, features = ["checkpoint"] }
```

Cargo combines features requested by dependencies. Another dependency that
enables Sandlock defaults can enable all five features in the resulting build.
The same applies when building multiple workspace crates together.
