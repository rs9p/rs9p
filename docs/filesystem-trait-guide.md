# Filesystem Trait Implementation Guide

A comprehensive guide to implementing the `Filesystem` trait for building 9P2000.L servers.

## Table of Contents

- [Overview](#overview)
- [Quick Start](#quick-start)
- [FId Management](#fid-management)
- [Method Reference](#method-reference)
- [Error Handling](#error-handling)
- [Best Practices](#best-practices)
- [Common Patterns](#common-patterns)
- [Examples](#examples)

## Overview

The `Filesystem` trait is the core interface for implementing 9P2000.L servers. Each method corresponds to a protocol operation:

- **Client sends**: `TAttach` (T = Transmit)
- **Server calls**: `rattach` (r = response)
- **Server returns**: `RAttach` (R = Response)

### Architecture

```text
┌──────────┐
│  Client  │ (Linux kernel v9fs, qemu, etc.)
└─────┬────┘
      │ 9P2000.L Protocol (TCP/Unix socket)
      ▼
┌─────────────────┐
│   rs9p Server   │
│  - srv_async    │ (handles connections, routing)
│  - dispatch     │ (manages fids, calls your methods)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│ Your Filesystem │ (implement the trait)
│  trait impl     │
└─────────────────┘
```

## Quick Start

### Minimal Example

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

### Stateful Example

```rust
use std::path::PathBuf;
use tokio::sync::RwLock;

#[derive(Default)]
struct MyFId {
    path: RwLock<PathBuf>,
    // Add file handles, permissions, etc.
}

#[async_trait]
impl Filesystem for MyFs {
    type FId = MyFId;

    async fn rattach(
        &self,
        fid: &FId<Self::FId>,
        _afid: Option<&FId<Self::FId>>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        // Initialize fid state
        let mut path = fid.aux.path.write().await;
        *path = PathBuf::from("/");

        Ok(FCall::RAttach {
            qid: QId {
                typ: QIdType::DIR,
                version: 0,
                path: 0,
            }
        })
    }
}
```

## FId Management

### What is a FId?

A **fid** (file identifier) is a 32-bit handle the client uses to reference files or directories. Think of it like a file descriptor, but at the protocol level.

### FId Lifecycle

```rust
// 1. Client attaches (creates root fid)
TAttach { fid: 0, ... } → rattach() → RAttach { qid }

// 2. Client walks to a file (creates new fid)
TWalk { fid: 0, newfid: 1, wnames: ["etc", "passwd"] }
  → rwalk() → RWalk { wqids: [...] }

// 3. Client opens the file
TlOpen { fid: 1, flags: O_RDONLY } → rlopen() → RlOpen { qid, iounit }

// 4. Client reads the file
TRead { fid: 1, offset: 0, count: 4096 } → rread() → RRead { data }

// 5. Client closes the file
TClunk { fid: 1 } → rclunk() → RClunk
// Server automatically removes fid 1 after rclunk returns
```

### Important Invariants

1. **FId uniqueness**: Each fid is unique per connection
2. **FId persistence**: FIds remain valid until clunked
3. **Walk creates new fid**: Original fid is unchanged
4. **Auto-cleanup**: Server removes fid after successful `TClunk`

### Concurrency

- Multiple methods may run concurrently
- Same fid may be accessed from different tasks
- Use `RwLock` or `Mutex` in your `FId` type for thread safety

```rust
#[derive(Default)]
struct MyFId {
    path: RwLock<PathBuf>,      // Multiple readers, one writer
    file: Mutex<Option<File>>,   // Exclusive access needed
}
```

## Method Reference

### Core Operations (Must Implement)

#### `rattach` - Attach to Filesystem Root

```rust
async fn rattach(
    &self,
    fid: &FId<Self::FId>,
    afid: Option<&FId<Self::FId>>,
    uname: &str,
    aname: &str,
    n_uname: u32,
) -> Result<FCall>
```

**Purpose**: Initialize connection and establish root fid.

**Parameters**:
- `fid`: The root fid being initialized (you should set `fid.aux` state)
- `afid`: Authentication fid (usually `None` if no auth)
- `uname`: Username attempting to attach
- `aname`: Name of filesystem to attach (can be used for multiple exports)
- `n_uname`: Numeric user ID

**Returns**: `RAttach { qid }` with root directory's qid

**Example**:
```rust
async fn rattach(&self, fid: &FId<Self::FId>, ...) -> Result<FCall> {
    // Initialize root fid
    let mut path = fid.aux.path.write().await;
    *path = self.root_path.clone();

    // Return root qid
    Ok(FCall::RAttach {
        qid: QId {
            typ: QIdType::DIR,
            version: 0,
            path: 0,  // Root inode
        }
    })
}
```

#### `rwalk` - Navigate Directory Tree

```rust
async fn rwalk(
    &self,
    fid: &FId<Self::FId>,
    newfid: &FId<Self::FId>,
    wnames: &[String],
) -> Result<FCall>
```

**Purpose**: Walk from `fid` through path components in `wnames`, creating `newfid`.

**Parameters**:
- `fid`: Starting directory fid (unchanged by this operation)
- `newfid`: New fid to create (initialize `newfid.aux`)
- `wnames`: Path components to traverse (e.g., `["dir", "subdir", "file"]`)

**Returns**: `RWalk { wqids }` with qid for each successfully traversed component

**Special Cases**:
- `wnames` is empty: Clone `fid` to `newfid`
- Partial walk: Stop at first error, return qids for successful components
- First component fails: Return error

**Example**:
```rust
async fn rwalk(&self, fid: &FId<Self::FId>, newfid: &FId<Self::FId>, wnames: &[String]) -> Result<FCall> {
    let mut current_path = {
        let path = fid.aux.path.read().await;
        path.clone()
    };

    let mut wqids = Vec::new();

    for (i, name) in wnames.iter().enumerate() {
        current_path.push(name);

        match get_qid(&current_path).await {
            Ok(qid) => wqids.push(qid),
            Err(e) => {
                if i == 0 {
                    return Err(e);  // First component must succeed
                } else {
                    break;  // Partial walk is okay
                }
            }
        }
    }

    // Set newfid path
    let mut newfid_path = newfid.aux.path.write().await;
    *newfid_path = current_path;

    Ok(FCall::RWalk { wqids })
}
```

**Important**: Handle `.` and `..` specially to prevent directory traversal attacks!

#### `rlopen` - Open File

```rust
async fn rlopen(&self, fid: &FId<Self::FId>, flags: u32) -> Result<FCall>
```

**Purpose**: Open the file referenced by `fid` with specified flags.

**Parameters**:
- `fid`: FId representing the file to open
- `flags`: Open flags (O_RDONLY, O_WRONLY, O_RDWR, O_TRUNC, etc.)

**Returns**: `RlOpen { qid, iounit }` where iounit is max I/O size (0 = no limit)

**Example**:
```rust
async fn rlopen(&self, fid: &FId<Self::FId>, flags: u32) -> Result<FCall> {
    let path = fid.aux.path.read().await.clone();

    // Open the file
    let file = tokio::fs::OpenOptions::new()
        .read(flags & O_RDONLY != 0)
        .write(flags & O_WRONLY != 0)
        .open(&path)
        .await?;

    // Store file handle in fid
    let mut fid_file = fid.aux.file.lock().await;
    *fid_file = Some(file);

    // Get qid
    let metadata = tokio::fs::metadata(&path).await?;
    let qid = qid_from_metadata(&metadata);

    Ok(FCall::RlOpen { qid, iounit: 0 })
}
```

#### `rread` - Read from File

```rust
async fn rread(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall>
```

**Purpose**: Read data from file.

**Parameters**:
- `fid`: File fid (must be opened first)
- `offset`: Byte offset to start reading
- `count`: Maximum bytes to read

**Returns**: `RRead { data }` with actual data read

**Example**:
```rust
async fn rread(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall> {
    let mut file = fid.aux.file.lock().await;
    let file = file.as_mut().ok_or(error::Error::No(EBADF))?;

    // Seek to offset
    file.seek(SeekFrom::Start(offset)).await?;

    // Read data
    let mut buf = vec![0u8; count as usize];
    let n = file.read(&mut buf).await?;
    buf.truncate(n);

    Ok(FCall::RRead { data: Data(buf) })
}
```

#### `rwrite` - Write to File

```rust
async fn rwrite(&self, fid: &FId<Self::FId>, offset: u64, data: &Data) -> Result<FCall>
```

**Purpose**: Write data to file.

**Parameters**:
- `fid`: File fid (must be opened with write permissions)
- `offset`: Byte offset to start writing
- `data`: Data to write

**Returns**: `RWrite { count }` with number of bytes actually written

**Example**:
```rust
async fn rwrite(&self, fid: &FId<Self::FId>, offset: u64, data: &Data) -> Result<FCall> {
    let mut file = fid.aux.file.lock().await;
    let file = file.as_mut().ok_or(error::Error::No(EBADF))?;

    file.seek(SeekFrom::Start(offset)).await?;
    let count = file.write(&data.0).await? as u32;

    Ok(FCall::RWrite { count })
}
```

#### `rclunk` - Close File

```rust
async fn rclunk(&self, fid: &FId<Self::FId>) -> Result<FCall>
```

**Purpose**: Close file and clean up resources.

**Note**: The server automatically removes the fid after `rclunk` returns successfully. You don't need to manage the fid table.

**Example**:
```rust
async fn rclunk(&self, fid: &FId<Self::FId>) -> Result<FCall> {
    // Clean up resources
    let mut file = fid.aux.file.lock().await;
    *file = None;  // Drop file handle

    Ok(FCall::RClunk)
}
```

### Directory Operations

#### `rreaddir` - Read Directory Entries

```rust
async fn rreaddir(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall>
```

**Purpose**: Read directory entries.

**Parameters**:
- `fid`: Directory fid
- `offset`: Entry offset (0 = start, use previous entry's offset for continuation)
- `count`: Maximum bytes to return

**Returns**: `RReadDir { data }` with directory entry data

**Important**:
- First call should include `.` and `..` entries
- Must return complete entries (don't split entries across calls)
- Empty response indicates end of directory

**Example**:
```rust
async fn rreaddir(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall> {
    let mut dirents = DirEntryData::new();

    // First entries are . and ..
    if offset == 0 {
        dirents.push(DirEntry {
            qid: current_dir_qid(),
            offset: 0,
            typ: 0,
            name: ".".to_string(),
        });
        dirents.push(DirEntry {
            qid: parent_dir_qid(),
            offset: 1,
            typ: 0,
            name: "..".to_string(),
        });
    }

    // Read remaining entries
    let mut entries = read_directory_from_offset(offset).await?;
    for (i, entry) in entries.enumerate() {
        let dirent = DirEntry {
            qid: entry.qid,
            offset: offset + i as u64,
            typ: 0,
            name: entry.name,
        };

        // Don't exceed count
        if dirents.size() + dirent.size() > count {
            break;
        }

        dirents.push(dirent);
    }

    Ok(FCall::RReadDir { data: dirents })
}
```

#### `rlcreate` - Create File

```rust
async fn rlcreate(
    &self,
    fid: &FId<Self::FId>,
    name: &str,
    flags: u32,
    mode: u32,
    gid: u32,
) -> Result<FCall>
```

**Purpose**: Create a new file in the directory referenced by `fid`.

**Important**: The fid transitions from directory to file fid!

**Example**:
```rust
async fn rlcreate(&self, fid: &FId<Self::FId>, name: &str, flags: u32, mode: u32, gid: u32) -> Result<FCall> {
    let parent_path = fid.aux.path.read().await.clone();
    let file_path = parent_path.join(name);

    // Create the file
    let file = tokio::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .mode(mode)
        .open(&file_path)
        .await?;

    // Update fid to point to new file
    let mut fid_path = fid.aux.path.write().await;
    *fid_path = file_path.clone();

    let mut fid_file = fid.aux.file.lock().await;
    *fid_file = Some(file);

    let metadata = tokio::fs::metadata(&file_path).await?;
    let qid = qid_from_metadata(&metadata);

    Ok(FCall::RlCreate { qid, iounit: 0 })
}
```

#### `rmkdir` - Create Directory

```rust
async fn rmkdir(
    &self,
    fid: &FId<Self::FId>,
    name: &str,
    mode: u32,
    gid: u32,
) -> Result<FCall>
```

**Purpose**: Create a directory.

**Example**:
```rust
async fn rmkdir(&self, fid: &FId<Self::FId>, name: &str, mode: u32, gid: u32) -> Result<FCall> {
    let parent_path = fid.aux.path.read().await.clone();
    let dir_path = parent_path.join(name);

    tokio::fs::create_dir(&dir_path).await?;

    let metadata = tokio::fs::metadata(&dir_path).await?;
    let qid = qid_from_metadata(&metadata);

    Ok(FCall::RMkDir { qid })
}
```

### Metadata Operations

#### `rgetattr` - Get File Attributes

```rust
async fn rgetattr(&self, fid: &FId<Self::FId>, req_mask: GetAttrMask) -> Result<FCall>
```

**Purpose**: Get file metadata (like `stat(2)`).

**Example**:
```rust
async fn rgetattr(&self, fid: &FId<Self::FId>, req_mask: GetAttrMask) -> Result<FCall> {
    let path = fid.aux.path.read().await.clone();
    let metadata = tokio::fs::symlink_metadata(&path).await?;

    Ok(FCall::RGetAttr {
        valid: req_mask,
        qid: qid_from_metadata(&metadata),
        stat: Stat::from(&metadata),
    })
}
```

#### `rsetattr` - Set File Attributes

```rust
async fn rsetattr(
    &self,
    fid: &FId<Self::FId>,
    valid: SetAttrMask,
    stat: &SetAttr,
) -> Result<FCall>
```

**Purpose**: Modify file metadata (chmod, chown, truncate, etc.).

**Important**: Only modify fields where `valid` has the corresponding bit set!

**Example**:
```rust
async fn rsetattr(&self, fid: &FId<Self::FId>, valid: SetAttrMask, stat: &SetAttr) -> Result<FCall> {
    let path = fid.aux.path.read().await.clone();

    if valid.contains(SetAttrMask::MODE) {
        tokio::fs::set_permissions(&path, Permissions::from_mode(stat.mode)).await?;
    }

    if valid.contains(SetAttrMask::SIZE) {
        let file = tokio::fs::OpenOptions::new()
            .write(true)
            .open(&path)
            .await?;
        file.set_len(stat.size).await?;
    }

    // Handle other fields...

    Ok(FCall::RSetAttr)
}
```

### Version Negotiation

#### `rversion` - Protocol Version

```rust
async fn rversion(&self, msize: u32, version: &str) -> Result<FCall>
```

**Purpose**: Negotiate protocol version and maximum message size.

**Default implementation**:
```rust
async fn rversion(&self, msize: u32, ver: &str) -> Result<FCall> {
    Ok(FCall::RVersion {
        msize,
        version: match ver {
            P92000L => ver.to_owned(),
            _ => VERSION_UNKNOWN.to_owned(),
        },
    })
}
```

Usually you don't need to override this.

## Error Handling

### Common Error Codes

```rust
use rs9p::error::{self, errno::*};

// File not found
Err(error::Error::No(ENOENT))

// Permission denied
Err(error::Error::No(EACCES))
Err(error::Error::No(EPERM))

// Invalid argument
Err(error::Error::No(EINVAL))

// Is a directory (expected file)
Err(error::Error::No(EISDIR))

// Not a directory (expected directory)
Err(error::Error::No(ENOTDIR))

// File exists
Err(error::Error::No(EEXIST))

// I/O error
Err(error::Error::No(EIO))

// Too many symbolic links / depth exceeded
Err(error::Error::No(ELOOP))

// Operation not supported
Err(error::Error::No(EOPNOTSUPP))

// Bad file descriptor
Err(error::Error::No(EBADF))
```

### Converting from std::io::Error

```rust
use rs9p::error::Error;

// Automatic conversion
let result: Result<FCall> = tokio::fs::read(&path).await.map_err(Error::from)?;

// Or use ? directly
let data = tokio::fs::read(&path).await?;  // Works!
```

## Best Practices

### 1. Use RwLock for Concurrent Access

```rust
#[derive(Default)]
struct MyFId {
    path: RwLock<PathBuf>,     // Multiple readers
    file: Mutex<Option<File>>, // Exclusive access
}
```

### 2. Validate Path Components

```rust
async fn rwalk(&self, ..., wnames: &[String]) -> Result<FCall> {
    for name in wnames {
        // Prevent directory traversal
        if name.contains('/') || name == ".." || name.starts_with('.') {
            return Err(error::Error::No(EINVAL));
        }
    }
    // ...
}
```

### 3. Track Depth to Prevent Infinite Recursion

```rust
#[derive(Default)]
struct MyFId {
    path: RwLock<PathBuf>,
    depth: RwLock<usize>,  // Track directory depth
}

// In rwalk:
if depth > MAX_DEPTH {
    return Err(error::Error::No(ELOOP));
}
```

### 4. Clean Up Resources in rclunk

```rust
async fn rclunk(&self, fid: &FId<Self::FId>) -> Result<FCall> {
    // Close file handles
    let mut file = fid.aux.file.lock().await;
    *file = None;

    // Clear caches
    // Release locks
    // etc.

    Ok(FCall::RClunk)
}
```

### 5. Handle Empty wnames in rwalk

```rust
async fn rwalk(&self, fid: &FId<Self::FId>, newfid: &FId<Self::FId>, wnames: &[String]) -> Result<FCall> {
    if wnames.is_empty() {
        // Clone fid to newfid
        let fid_path = fid.aux.path.read().await;
        let mut newfid_path = newfid.aux.path.write().await;
        *newfid_path = fid_path.clone();

        return Ok(FCall::RWalk { wqids: vec![] });
    }
    // ...
}
```

## Common Patterns

### Read-Only Filesystem

```rust
#[async_trait]
impl Filesystem for ReadOnlyFs {
    type FId = MyFId;

    // Implement read operations
    async fn rread(...) -> Result<FCall> { /* ... */ }
    async fn rgetattr(...) -> Result<FCall> { /* ... */ }
    async fn rreaddir(...) -> Result<FCall> { /* ... */ }

    // Reject write operations
    async fn rwrite(...) -> Result<FCall> {
        Err(error::Error::No(EROFS))  // Read-only filesystem
    }

    async fn rlcreate(...) -> Result<FCall> {
        Err(error::Error::No(EROFS))
    }

    // ... other write operations return EROFS
}
```

### In-Memory Filesystem

```rust
use std::collections::HashMap;
use tokio::sync::RwLock;

struct MemFs {
    files: Arc<RwLock<HashMap<u64, Vec<u8>>>>,
    next_inode: Arc<Mutex<u64>>,
}

#[derive(Default)]
struct MemFId {
    inode: RwLock<u64>,
}

#[async_trait]
impl Filesystem for MemFs {
    type FId = MemFId;

    async fn rread(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall> {
        let inode = *fid.aux.inode.read().await;
        let files = self.files.read().await;

        let data = files.get(&inode)
            .ok_or(error::Error::No(ENOENT))?;

        let start = offset as usize;
        let end = (start + count as usize).min(data.len());
        let slice = &data[start..end];

        Ok(FCall::RRead {
            data: Data(slice.to_vec())
        })
    }

    // ... other operations
}
```

### Pass-Through Filesystem

```rust
struct PassThroughFs {
    root: PathBuf,
}

#[derive(Default)]
struct PassThroughFId {
    path: RwLock<PathBuf>,
    file: Mutex<Option<tokio::fs::File>>,
}

#[async_trait]
impl Filesystem for PassThroughFs {
    type FId = PassThroughFId;

    async fn rlopen(&self, fid: &FId<Self::FId>, flags: u32) -> Result<FCall> {
        let path = fid.aux.path.read().await;
        let real_path = self.root.join(&*path);

        let file = tokio::fs::OpenOptions::new()
            .read(true)
            .write(flags & O_WRONLY != 0)
            .open(&real_path)
            .await?;

        let mut fid_file = fid.aux.file.lock().await;
        *fid_file = Some(file);

        let metadata = tokio::fs::metadata(&real_path).await?;
        let qid = qid_from_metadata(&metadata);

        Ok(FCall::RlOpen { qid, iounit: 0 })
    }

    // ... forward other operations to real filesystem
}
```

## Testing Your Implementation

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_attach() {
        let fs = MyFs::new();
        let fid = FId { fid: 0, aux: MyFId::default() };

        let result = fs.rattach(&fid, None, "user", "fs", 1000).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_read_nonexistent_file() {
        let fs = MyFs::new();
        let fid = FId { fid: 1, aux: MyFId::default() };

        let result = fs.rread(&fid, 0, 100).await;
        assert!(matches!(result, Err(error::Error::No(EBADF))));
    }
}
```

### Integration Tests

```rust
#[tokio::test]
async fn test_full_workflow() {
    let fs = MyFs::new();

    // Attach
    let root_fid = FId { fid: 0, aux: MyFId::default() };
    fs.rattach(&root_fid, None, "user", "fs", 1000).await.unwrap();

    // Walk to file
    let file_fid = FId { fid: 1, aux: MyFId::default() };
    fs.rwalk(&root_fid, &file_fid, &["file.txt".to_string()]).await.unwrap();

    // Open file
    fs.rlopen(&file_fid, O_RDONLY).await.unwrap();

    // Read file
    let result = fs.rread(&file_fid, 0, 100).await.unwrap();
    assert!(matches!(result, FCall::RRead { .. }));

    // Clunk file
    fs.rclunk(&file_fid).await.unwrap();
}
```

## Debugging Tips

### Enable Logging

```rust
// In your main:
env_logger::init();

// In your filesystem:
use log::{debug, info, warn, error};

async fn rread(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall> {
    debug!("rread: fid={}, offset={}, count={}", fid.fid(), offset, count);
    // ...
}
```

### Common Issues

1. **"EBADF on read"**: Did you store the file handle in `rlopen`?
2. **"Partial walks don't work"**: Check that you handle errors correctly (first must fail, rest can succeed)
3. **"FId state is wrong"**: Make sure you're updating both `fid` and `newfid` correctly in `rwalk`
4. **"Concurrent access hangs"**: Deadlock in locks? Use `RwLock` for reads, minimize lock duration

## Additional Resources

- [9P2000.L Protocol Specification](https://www.kernel.org/doc/Documentation/filesystems/9p.txt)
- [Plan 9 Manual - intro(5)](http://man.cat-v.org/plan_9/5/intro)
- [v9fs Linux Kernel Documentation](https://www.kernel.org/doc/html/latest/filesystems/9p.html)
- `unpfs` example in this repository

## Summary Checklist

When implementing `Filesystem`:

- [ ] Define your `FId` associated type with needed state
- [ ] Implement `rattach` to initialize root fid
- [ ] Implement `rwalk` with proper fid cloning and path traversal
- [ ] Implement `rlopen` / `rlcreate` and store file handles
- [ ] Implement `rread` / `rwrite` for I/O
- [ ] Implement `rgetattr` / `rsetattr` for metadata
- [ ] Implement `rreaddir` for directory listing
- [ ] Implement `rclunk` to clean up resources
- [ ] Add proper error handling throughout
- [ ] Use `RwLock` / `Mutex` for concurrent access
- [ ] Validate inputs to prevent security issues
-
