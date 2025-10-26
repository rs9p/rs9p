# Maximum Depth Protection

## Problem

Without a depth limit, it's possible to create infinite recursion when using `unpfs` if the mount point is placed inside the export directory.

### Example Scenario

```bash
# Export directory
/home/user/export

# Mount the 9P filesystem at
/home/user/export/mnt

# This creates an infinite loop:
/home/user/export/mnt/mnt/mnt/mnt/...
```

When a client attempts to traverse this path, the server will recurse indefinitely, eventually causing:
- Stack overflow
- Out of memory errors
- Server crash
- Denial of service

## Solution

The `--max-depth` option tracks how deep the filesystem has been traversed from the root and returns `ELOOP` (Too many levels of symbolic links) when the limit is exceeded.

## Usage

```bash
# Set a maximum depth of 50 levels
cargo run --release -- --max-depth 50 'tcp!0.0.0.0!564' /exportdir

# Or via Unix socket
cargo run --release -- --max-depth 100 'unix!/tmp/unpfs!0' /exportdir
```

## How It Works

1. **Initialization**: When a filesystem is attached, the depth is set to 0
2. **Path Traversal**: During `rwalk` operations:
   - `.` (current directory) - depth unchanged
   - `..` (parent directory) - depth decreases by 1 (saturating at 0)
   - Any other name - depth increases by 1
3. **Limit Check**: If depth exceeds `max_depth`, return `ELOOP` error
4. **Error Handling**: Client receives error and stops traversal

## Choosing a Value

### Recommended Values

- **Conservative**: `--max-depth 50` - Suitable for most filesystems
- **Permissive**: `--max-depth 100` - Allows deeper nesting
- **Strict**: `--max-depth 20` - For shallow directory structures

### Considerations

- **Real directory depth**: Consider your actual filesystem structure
- **Symbolic links**: Each symlink resolution may increase depth
- **Application needs**: Some applications traverse deep paths
- **Memory usage**: Deeper paths use more memory

### When to Use

✅ **Always use when**:
- Mount point could be inside export directory
- Untrusted clients access the server
- Running in production environments
- Exporting user-controlled directories

❌ **May omit when**:
- Testing in controlled environments
- Export directory is guaranteed separate from mount points
- You have external path validation

## Technical Details

### Depth Tracking

Depth is tracked per-fid in the `UnpfsFid` structure:

```rust
struct UnpfsFid {
    realpath: RwLock<PathBuf>,
    file: Mutex<Option<fs::File>>,
    depth: RwLock<usize>,  // Current depth from root
}
```

### Path Component Handling

```rust
for name in wnames {
    match name.as_str() {
        ".." => depth = depth.saturating_sub(1),
        "." => { /* no change */ },
        _ => {
            depth += 1;
            if let Some(max) = max_depth && depth > max {
                return Err(ELOOP);
            }
        }
    }
}
```

### Error Code

When max depth is exceeded, the server returns:
- **Error Code**: `ELOOP` (errno 40 on Linux)
- **Meaning**: "Too many levels of symbolic links"
- **Standard**: POSIX compliant error code
- **Client Behavior**: Most clients will stop traversal

## Testing

Run the included tests to verify depth tracking:

```bash
cargo test -p unpfs
```

Tests verify:
- ✅ Depth increases with normal paths
- ✅ `..` decreases depth correctly
- ✅ `.` doesn't change depth
- ✅ Saturating subtraction prevents underflow
- ✅ Max depth limit is enforced

## Performance Impact

The depth tracking has minimal performance overhead:
- **Memory**: ~8 bytes per fid (usize in RwLock)
- **CPU**: One integer comparison per path component
- **Locking**: RwLock operations for depth read/write

## Security Considerations

1. **DoS Prevention**: Prevents infinite recursion attacks
2. **Resource Protection**: Limits stack and memory usage
3. **Defense in Depth**: Complements other security measures
4. **No Authentication Bypass**: Depth check happens after authentication

## Limitations

- Does not prevent other forms of path traversal attacks
- Does not validate path components for directory traversal (e.g., `../../../etc/passwd`)
- Does not limit total path length (filesystem limits still apply)
- Shared depth counter across all operations on a fid

## Best Practices

1. **Set appropriate limits**: Choose based on your filesystem structure
2. **Document the limit**: Tell users what value you're using and why
3. **Monitor errors**: Log `ELOOP` errors to detect potential issues
4. **Test scenarios**: Verify legitimate deep paths still work
5. **Defense in depth**: Combine with other security measures

## See Also

- [9P2000.L Protocol Documentation](https://www.kernel.org/doc/Documentation/filesystems/9p.txt)
- [v9fs Mount Options](https://www.kernel.org/doc/html/latest/filesystems/9p.html)
- `man errno` - For ELOOP and other error codes