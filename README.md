# rs9p

Tokio-based asynchronous filesystems library using 9P2000.L protocol, an extended variant of 9P from Plan 9.

## Features

- 🚀 **Async/Await**: Built on tokio for high-performance async I/O
- 🔒 **Memory Safe**: `#![forbid(unsafe_code)]` - no unsafe code
- 🔌 **Multiple Transports**: TCP and Unix domain sockets
- 📦 **9P2000.L Protocol**: Full support for Linux-extended 9P
- 🛠️ **Easy to Use**: Simple trait-based API for building custom filesystems

## Documentation

- **[API Documentation](https://docs.rs/rs9p)** - Full API reference on docs.rs
- **[Filesystem Trait Guide](docs/filesystem-trait-guide.md)** - Complete guide to implementing custom filesystems
- **[Documentation Index](docs/README.md)** - All available documentation

## Quick Start

### Using as a Library

Add to your `Cargo.toml`:
```toml
[dependencies]
rs9p = "0.5"
async-trait = "0.1"
tokio = { version = "1", features = ["full"] }
```

Implement the `Filesystem` trait:
```rust
use rs9p::{srv::{Filesystem, FId, srv_async}, Result, FCall, QId, QIdType};
use async_trait::async_trait;

#[derive(Clone)]
struct MyFs;

#[derive(Default)]
struct MyFId;

#[async_trait]
impl Filesystem for MyFs {
    type FId = MyFId;

    async fn rattach(
        &self,
        _fid: &FId<Self::FId>,
        _afid: Option<&FId<Self::FId>>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        Ok(FCall::RAttach {
            qid: QId {
                typ: QIdType::DIR,
                version: 0,
                path: 0,
            }
        })
    }

    // Implement other methods...
}

#[tokio::main]
async fn main() -> Result<()> {
    srv_async(MyFs, "tcp!127.0.0.1!564").await
}
```

See the [Filesystem Trait Guide](docs/filesystem-trait-guide.md) for complete examples.

## unpfs - Reference Implementation

## unpfs

`unpfs` is the reference implementation of a file server which exports your filesystem.
You can install unpfs from crates.io:
```bash
cargo install unpfs
```

or build it from source with the following commands:

```bash
git clone https://github.com/rs9p/rs9p
cd rs9p
cargo install crates/unpfs
```

and run unpfs with the following command to export `./testdir`:

```bash
# Unix domain socket:
#  port number is a suffix to the unix domain socket
#  'unix!/tmp/unpfs-socket!n' creates `/tmp/unpfs-socket:n`
unpfs 'unix!/tmp/unpfs-socket!0' testdir
```

You are now ready to import/mount the remote filesystem.
Let's mount it at `./mount`:

```bash
# Unix domain socket
sudo mount -t 9p -o version=9p2000.L,trans=unix,uname=$USER /tmp/unpfs-socket:0 ./mount
```

The default transport is normally TCP, but the port its listening on is in the restricted range,
which requires root permissions. That's why we started with the unix domain socket, because it's
more likely to just work.

But TCP is obviously supported as well. Here's the same commands as above, but using TCP this time:

```bash
sudo unpfs 'tcp!0.0.0.0!564' testdir
```

```bash
# TCP
sudo mount -t 9p -o version=9p2000.L,trans=tcp,port=564,uname=$USER 127.0.0.1 ./mount
```

| Mount option | Value                                              |
| ------------ | -------------------------------------------------- |
| version      | must be "9p2000.L"                                 |
| trans        | an alternative v9fs transport. "tcp" or "unix"     |
| port         | port to connect to on the remote server            |
| uname        | user name to attempt mount as on the remote server |

See [v9fs documentation](https://www.kernel.org/doc/Documentation/filesystems/9p.txt) for more details.

## Protocol Reference

- [Linux Kernel 9P Documentation](https://www.kernel.org/doc/Documentation/filesystems/9p.txt)
- [v9fs Mount Options](https://www.kernel.org/doc/html/latest/filesystems/9p.html)
- [Plan 9 Manual - intro(5)](http://man.cat-v.org/plan_9/5/intro)

## Contributing

Contributions are welcome! Please:

1. Check existing issues or create a new one
2. Fork the repository
3. Create a feature branch
4. Add tests for new functionality
5. Ensure `cargo test`, `cargo clippy`, and `cargo fmt --check` pass
6. Submit a pull request

## Safety and Security

- **Memory Safe**: No unsafe code - all operations use safe Rust
- **Error Handling**: Comprehensive error handling prevents panics
- **Path Validation**: Implement proper validation to prevent directory traversal
- **Authentication**: Default auth returns `EOPNOTSUPP` - implement for production use

See the [Filesystem Trait Guide](docs/filesystem-trait-guide.md) for security best practices.

## License

rs9p is distributed under the BSD 3-Clause License.
See LICENSE for details.
