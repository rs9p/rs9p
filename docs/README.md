# rs9p Documentation

This directory contains comprehensive documentation for the rs9p library and its examples.

## Table of Contents

### For Users

- **[Filesystem Trait Implementation Guide](filesystem-trait-guide.md)** - Complete guide to implementing the `Filesystem` trait with examples and best practices
- **[Maximum Depth Protection](max-depth.md)** - Understanding and configuring depth limits to prevent infinite recursion

### Getting Started

If you're new to rs9p, start with these steps:

1. Read the main [README.md](../README.md) for a project overview
2. Review the [Filesystem Trait Implementation Guide](filesystem-trait-guide.md) to understand the core concepts
3. Study the `unpfs` example in `../example/unpfs/` for a complete reference implementation
4. Implement your own filesystem by creating a struct and implementing the `Filesystem` trait

### Quick Example

```rust
use rs9p::{srv::{Filesystem, Fid, srv_async}, Result, Fcall, Qid, QidType};
use async_trait::async_trait;

#[derive(Clone)]
struct MyFs;

#[derive(Default)]
struct MyFid;

#[async_trait]
impl Filesystem for MyFs {
    type Fid = MyFid;

    async fn rattach(
        &self,
        _fid: &Fid<Self::Fid>,
        _afid: Option<&Fid<Self::Fid>>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<Fcall> {
        Ok(Fcall::Rattach {
            qid: Qid {
                typ: QidType::DIR,
                version: 0,
                path: 0,
            }
        })
    }

    // Implement other required methods...
}

#[tokio::main]
async fn main() -> Result<()> {
    srv_async(MyFs, "tcp!127.0.0.1!564").await
}
```

## Documentation Topics

### Core Concepts

- **9P2000.L Protocol**: Extended variant of Plan 9's 9P protocol with Linux-specific features
- **Fids (File Identifiers)**: 32-bit handles clients use to reference files/directories
- **Async Implementation**: Built on tokio for high-performance async I/O
- **Message Flow**: Request/response pattern (Tattach → rattach → Rattach)

### Key Traits and Types

- `Filesystem` - Main trait to implement for creating a 9P server
- `Fid<T>` - Represents a client file identifier with user-defined state `T`
- `Fcall` - Enum of all 9P protocol messages
- `Error` - Error type that maps to errno codes
- `Qid` - Server-side file identifier with type, version, and path

### Server Functions

- `srv_async(filesystem, address)` - Start server on TCP or Unix socket
- `srv_async_tcp(filesystem, address)` - Start TCP server specifically
- `srv_async_unix(filesystem, path)` - Start Unix domain socket server

### Protocol Operations

The `Filesystem` trait defines methods for all 9P operations:

**Essential Operations:**
- `rattach` - Initialize root filesystem connection
- `rwalk` - Navigate directory tree
- `rlopen` - Open files
- `rread`/`rwrite` - Read/write data
- `rclunk` - Close files

**Metadata Operations:**
- `rgetattr`/`rsetattr` - Get/set file attributes
- `rstatfs` - Get filesystem statistics

**Directory Operations:**
- `rreaddir` - Read directory entries
- `rmkdir` - Create directories
- `rlcreate` - Create files

**Advanced Operations:**
- `rsymlink` - Create symbolic links
- `rlink` - Create hard links
- `rmknod` - Create device nodes
- `rlock`/`rgetlock` - File locking
- `rxattrwalk`/`rxattrcreate` - Extended attributes

### Security Considerations

- **Path Validation**: Always validate path components to prevent directory traversal
- **Depth Limits**: Use max depth to prevent infinite recursion (see [max-depth.md](max-depth.md))
- **Authentication**: Default implementation returns `EOPNOTSUPP` - implement `rauth` for production
- **Permission Checks**: Validate user permissions before allowing operations
- **Resource Limits**: Consider limiting open fids, concurrent connections, etc.

## Examples

### `unpfs` - Unix Pass-through Filesystem

Location: `../example/unpfs/`

A complete reference implementation that exports a local directory via 9P. Features:

- Full 9P2000.L operation support
- Depth tracking to prevent infinite recursion
- Proper error handling
- Signal handling (SIGTERM/SIGINT)
- Command-line argument parsing with clap

Run it:
```bash
cd ../example/unpfs
cargo run --release -- --max-depth 100 'tcp!0.0.0.0!564' /path/to/export
```

Mount it:
```bash
sudo mount -t 9p -o version=9p2000.L,trans=tcp,port=564,uname=$USER 127.0.0.1 /mnt/point
```

## Protocol Reference

- [Linux 9P Documentation](https://www.kernel.org/doc/Documentation/filesystems/9p.txt)
- [v9fs Mount Options](https://www.kernel.org/doc/html/latest/filesystems/9p.html)
- [Plan 9 Manual](http://man.cat-v.org/plan_9/)

## API Documentation

Generate and view the API documentation:

```bash
cargo doc --open
```

This will build the rustdoc documentation from inline comments and open it in your browser.

## Contributing

When adding new documentation:

1. Keep examples minimal and focused
2. Explain assumptions and invariants clearly
3. Include error handling examples
4. Cross-reference related documentation
5. Update this README with new topics

## Support

- GitHub Issues: Report bugs or request features
- Discussions: Ask questions or share your implementations
- Examples: Contribute example implementations for common use cases

## License

See [LICENSE](../LICENSE) for details.