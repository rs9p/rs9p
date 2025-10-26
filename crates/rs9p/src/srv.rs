//! Asynchronous server side 9P library.
//!
//! # Protocol
//! 9P2000.L

use {
    crate::{
        error::{self, errno::*},
        fcall::*,
        io_err, serialize,
        utils::{self, Result},
    },
    async_trait::async_trait,
    bytes::buf::{Buf, BufMut},
    futures::sink::SinkExt,
    log::{error, info},
    std::{
        collections::HashMap,
        path::{Path, PathBuf},
        sync::{Arc, atomic::Ordering},
    },
    tokio::{
        io::{AsyncRead, AsyncWrite},
        net::{TcpListener, UnixListener},
        sync::{Mutex, RwLock},
    },
    tokio_stream::StreamExt,
    tokio_util::codec::length_delimited::LengthDelimitedCodec,
};

/// Represents a fid of clients holding associated `Filesystem::FId`.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct FId<T> {
    /// Raw client side fid.
    fid: u32,

    /// `Filesystem::FId` associated with this fid.
    /// Changing this value affects the continuous callbacks.
    pub aux: T,
}

impl<T> FId<T> {
    /// Get the raw fid.
    pub fn fid(&self) -> u32 {
        self.fid
    }
}

#[async_trait]
/// Filesystem server trait for implementing 9P2000.L servers.
///
/// Implementors can represent an error condition by returning an `Err`.
/// Otherwise, they must return the appropriate `FCall` response with required fields.
///
/// # Error Handling
/// All methods should return `Err(error::Error::No(errno))` to send an error to the client.
/// Common errno values include:
/// - `ENOENT` - File not found
/// - `EACCES` - Permission denied
/// - `EISDIR` - Is a directory (when file expected)
/// - `ENOTDIR` - Not a directory (when directory expected)
///
/// # Example
/// ```no_run
/// use std::path::PathBuf;
///
/// use rs9p::{error, srv::{Filesystem, FId}, fcall::FCall};
/// use async_trait::async_trait;
///
/// struct MyFs;
/// type Result<T> = ::std::result::Result<T, error::Error>;
///
/// #[async_trait]
/// impl Filesystem for MyFs {
///     type FId = PathBuf;
///
///     async fn rattach(&self,
///                      fid: &FId<Self::FId>,
///                      afid: Option<&FId<Self::FId>>,
///                      uname: &str,
///                      aname: &str,
///                      n_uname: u32,
/// ) -> Result<FCall> {
///         todo!("implementation")
///     }
/// }
/// ```
/// The main trait for implementing a 9P filesystem server.
///
/// This trait provides methods corresponding to 9P protocol operations. Most methods
/// have default implementations that return `EOPNOTSUPP`, allowing you to implement
/// only the operations your filesystem needs to support.
///
/// # Minimum Implementation
///
/// For a basic read-only filesystem, you typically need to implement:
/// - [`rattach`](Self::rattach) - Attach to the filesystem root
/// - [`rwalk`](Self::rwalk) - Navigate the directory tree
/// - [`rlopen`](Self::rlopen) - Open files
/// - [`rread`](Self::rread) - Read file contents
/// - [`rgetattr`](Self::rgetattr) - Get file attributes
/// - [`rreaddir`](Self::rreaddir) - Read directory entries
/// - [`rclunk`](Self::rclunk) - Close files
///
/// For a writable filesystem, additionally implement:
/// - [`rwrite`](Self::rwrite) - Write to files
/// - [`rlcreate`](Self::rlcreate) - Create files
/// - [`rmkdir`](Self::rmkdir) - Create directories
/// - [`rsetattr`](Self::rsetattr) - Modify file attributes
///
/// # FId Management
///
/// The `FId` type represents a file identifier that tracks open files. Each fid
/// can store custom state via the associated `FId` type. Fids are created during
/// `rattach` and `rwalk`, and must be cleaned up in `rclunk`.
pub trait Filesystem: Send {
    /// User defined fid type to be associated with a client's fid.
    ///
    /// This type stores per-fid state such as the current path, open file handle,
    /// or any other metadata needed to service requests on this fid.
    type FId: Send + Sync + Default;

    // 9P2000.L

    /// Get filesystem statistics (9P2000.L).
    ///
    /// Returns information about the filesystem such as block size, total blocks,
    /// free blocks, available blocks, total files, and free files.
    ///
    /// # Arguments
    /// * `fid` - The file identifier for the filesystem root or any file
    ///
    /// # Returns
    /// `FCall::RStatFs` with filesystem statistics, or an error.
    async fn rstatfs(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Open a file (9P2000.L).
    ///
    /// Opens the file represented by the fid with the specified flags. This is one
    /// of the core operations that must be implemented for a functional filesystem.
    ///
    /// # Arguments
    /// * `fid` - The file identifier to open
    /// * `flags` - Open flags (O_RDONLY, O_WRONLY, O_RDWR, etc.)
    ///
    /// # Returns
    /// `FCall::RLOpen` containing a qid and iounit, or an error.
    async fn rlopen(&self, _: &FId<Self::FId>, _flags: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create a new file (9P2000.L).
    ///
    /// Creates a new file with the given name in the directory represented by the fid.
    /// After creation, the fid represents the newly created file.
    ///
    /// # Arguments
    /// * `fid` - The directory fid where the file should be created
    /// * `name` - The name of the file to create
    /// * `flags` - Open flags for the new file
    /// * `mode` - File permissions mode
    /// * `gid` - Group ID for the new file
    ///
    /// # Returns
    /// `FCall::RLCreate` containing a qid and iounit, or an error.
    async fn rlcreate(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _flags: u32,
        _mode: u32,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create a symbolic link (9P2000.L).
    ///
    /// Creates a symbolic link with the specified name in the directory represented
    /// by the fid, pointing to the target path.
    ///
    /// # Arguments
    /// * `fid` - The directory fid where the symlink should be created
    /// * `name` - The name of the symlink to create
    /// * `sym` - The target path the symlink points to
    /// * `gid` - Group ID for the new symlink
    ///
    /// # Returns
    /// `FCall::RSymlink` containing the qid of the new symlink, or an error.
    async fn rsymlink(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _sym: &str,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create a special file (9P2000.L).
    ///
    /// Creates a special file (device node, named pipe, etc.) with the specified name
    /// in the directory represented by the fid.
    ///
    /// # Arguments
    /// * `fid` - The directory fid where the special file should be created
    /// * `name` - The name of the special file to create
    /// * `mode` - File type and permissions (S_IFBLK, S_IFCHR, S_IFIFO, etc.)
    /// * `major` - Major device number (for device nodes)
    /// * `minor` - Minor device number (for device nodes)
    /// * `gid` - Group ID for the new file
    ///
    /// # Returns
    /// `FCall::RMknod` containing the qid of the new file, or an error.
    async fn rmknod(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _mode: u32,
        _major: u32,
        _minor: u32,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Rename a file (9P2000.L).
    ///
    /// Renames the file represented by the first fid to the specified name within
    /// the directory represented by the second fid.
    ///
    /// # Arguments
    /// * `fid` - The file fid to rename
    /// * `dfid` - The destination directory fid
    /// * `name` - The new name for the file
    ///
    /// # Returns
    /// `FCall::RRename` on success, or an error.
    async fn rrename(&self, _: &FId<Self::FId>, _: &FId<Self::FId>, _name: &str) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Read a symbolic link target (9P2000.L).
    ///
    /// Returns the target path that the symbolic link points to.
    ///
    /// # Arguments
    /// * `fid` - The symlink fid to read
    ///
    /// # Returns
    /// `FCall::RReadlink` containing the target path, or an error.
    async fn rreadlink(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Get file attributes (9P2000.L).
    ///
    /// Returns metadata about the file such as mode, size, timestamps, owner, etc.
    /// This is a core operation that must be implemented for most filesystems.
    ///
    /// # Arguments
    /// * `fid` - The file fid to get attributes for
    /// * `req_mask` - Mask indicating which attributes are requested
    ///
    /// # Returns
    /// `FCall::RGetAttr` containing file attributes, or an error.
    async fn rgetattr(&self, _: &FId<Self::FId>, _req_mask: GetAttrMask) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Set file attributes (9P2000.L).
    ///
    /// Modifies metadata about the file such as permissions, timestamps, size, owner, etc.
    /// The `valid` mask indicates which fields in `stat` should be updated.
    ///
    /// # Arguments
    /// * `fid` - The file fid to modify attributes for
    /// * `valid` - Mask indicating which attributes to set
    /// * `stat` - The new attribute values
    ///
    /// # Returns
    /// `FCall::RSetAttr` on success, or an error.
    async fn rsetattr(
        &self,
        _: &FId<Self::FId>,
        _valid: SetAttrMask,
        _stat: &SetAttr,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Walk to an extended attribute (9P2000.L).
    ///
    /// Opens an extended attribute for reading or listing. The new fid will
    /// represent the extended attribute.
    ///
    /// # Arguments
    /// * `fid` - The file fid to access extended attributes on
    /// * `newfid` - The new fid that will represent the extended attribute
    /// * `name` - The name of the extended attribute (empty to list all)
    ///
    /// # Returns
    /// `FCall::RXAttrWalk` containing the size of the attribute, or an error.
    async fn rxattrwalk(
        &self,
        _: &FId<Self::FId>,
        _: &FId<Self::FId>,
        _name: &str,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create or replace an extended attribute (9P2000.L).
    ///
    /// Creates a new extended attribute or replaces an existing one. After this
    /// operation, the fid is used to write the attribute value.
    ///
    /// # Arguments
    /// * `fid` - The file fid to set an extended attribute on
    /// * `name` - The name of the extended attribute
    /// * `attr_size` - The size of the attribute value
    /// * `flags` - Creation flags (XATTR_CREATE or XATTR_REPLACE)
    ///
    /// # Returns
    /// `FCall::RXAttrCreate` on success, or an error.
    async fn rxattrcreate(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _attr_size: u64,
        _flags: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Read directory entries (9P2000.L).
    ///
    /// Returns a list of directory entries starting at the given offset. This is a
    /// core operation that must be implemented for directory traversal.
    ///
    /// # Arguments
    /// * `fid` - The directory fid to read entries from
    /// * `offset` - The offset to start reading from (0 for beginning)
    /// * `count` - Maximum number of bytes to return
    ///
    /// # Returns
    /// `FCall::RReadDir` containing directory entries, or an error.
    async fn rreaddir(&self, _: &FId<Self::FId>, _offset: u64, _count: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Flush file data to stable storage (9P2000.L).
    ///
    /// Ensures that all modified data for the file is written to persistent storage.
    ///
    /// # Arguments
    /// * `fid` - The file fid to sync
    ///
    /// # Returns
    /// `FCall::RFsync` on success, or an error.
    async fn rfsync(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Acquire or release a file lock (9P2000.L).
    ///
    /// Applies an advisory lock on a file or a region of a file.
    ///
    /// # Arguments
    /// * `fid` - The file fid to lock
    /// * `lock` - Lock parameters (type, flags, start, length, etc.)
    ///
    /// # Returns
    /// `FCall::RLock` containing lock status, or an error.
    async fn rlock(&self, _: &FId<Self::FId>, _lock: &Flock) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Test for the existence of a file lock (9P2000.L).
    ///
    /// Checks if a lock can be placed on the file, and returns information about
    /// any conflicting locks.
    ///
    /// # Arguments
    /// * `fid` - The file fid to check locks on
    /// * `lock` - Lock parameters to test
    ///
    /// # Returns
    /// `FCall::RGetLock` containing lock information, or an error.
    async fn rgetlock(&self, _: &FId<Self::FId>, _lock: &Getlock) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create a hard link (9P2000.L).
    ///
    /// Creates a hard link to the file represented by the first fid, with the
    /// specified name in the directory represented by the second fid.
    ///
    /// # Arguments
    /// * `dfid` - The directory fid where the link should be created
    /// * `fid` - The file fid to create a link to
    /// * `name` - The name of the new link
    ///
    /// # Returns
    /// `FCall::RLink` on success, or an error.
    async fn rlink(&self, _: &FId<Self::FId>, _: &FId<Self::FId>, _name: &str) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Create a directory (9P2000.L).
    ///
    /// Creates a new directory with the specified name in the directory represented
    /// by the fid.
    ///
    /// # Arguments
    /// * `fid` - The parent directory fid where the directory should be created
    /// * `name` - The name of the directory to create
    /// * `mode` - Directory permissions mode
    /// * `gid` - Group ID for the new directory
    ///
    /// # Returns
    /// `FCall::RMkdir` containing the qid of the new directory, or an error.
    async fn rmkdir(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _mode: u32,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Rename a file between directories (9P2000.L).
    ///
    /// Renames a file from one directory to another, potentially with a different name.
    ///
    /// # Arguments
    /// * `olddirfid` - The old parent directory fid
    /// * `oldname` - The current name of the file
    /// * `newdirfid` - The new parent directory fid
    /// * `newname` - The new name for the file
    ///
    /// # Returns
    /// `FCall::RRenameAt` on success, or an error.
    async fn rrenameat(
        &self,
        _: &FId<Self::FId>,
        _oldname: &str,
        _: &FId<Self::FId>,
        _newname: &str,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Remove a file or directory (9P2000.L).
    ///
    /// Removes the file or directory with the specified name from the directory
    /// represented by the fid.
    ///
    /// # Arguments
    /// * `dirfid` - The parent directory fid
    /// * `name` - The name of the file or directory to remove
    /// * `flags` - Flags such as AT_REMOVEDIR for directories
    ///
    /// # Returns
    /// `FCall::RUnlinkAt` on success, or an error.
    async fn runlinkat(&self, _: &FId<Self::FId>, _name: &str, _flags: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /*
     * 9P2000.u subset
     */

    /// Authenticate a user (9P2000.u).
    ///
    /// Initiates authentication for a user. The fid will be used for authentication
    /// data exchange. Most filesystems return EOPNOTSUPP if they don't require
    /// authentication.
    ///
    /// # Arguments
    /// * `afid` - The authentication fid to use
    /// * `uname` - The user name
    /// * `aname` - The file tree to access
    /// * `n_uname` - Numeric user ID
    ///
    /// # Returns
    /// `FCall::RAuth` containing an authentication qid, or an error.
    async fn rauth(
        &self,
        _: &FId<Self::FId>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Attach to the filesystem root (9P2000.u).
    ///
    /// This is the first operation performed by a client to connect to the filesystem.
    /// It associates the fid with the root of the filesystem (or a subtree specified
    /// by `aname`). This is a core operation that must be implemented.
    ///
    /// # Arguments
    /// * `fid` - The fid to associate with the filesystem root
    /// * `afid` - Optional authentication fid (if authentication was performed)
    /// * `uname` - The user name
    /// * `aname` - The file tree to access (often "/" or empty)
    /// * `n_uname` - Numeric user ID
    ///
    /// # Returns
    /// `FCall::RAttach` containing the root qid, or an error.
    async fn rattach(
        &self,
        _: &FId<Self::FId>,
        _afid: Option<&FId<Self::FId>>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /*
     * 9P2000 subset
     */

    /// Abort a pending operation (9P2000).
    ///
    /// Requests that the server abandon a pending operation. This is typically used
    /// to cancel long-running requests.
    ///
    /// # Arguments
    /// * `old` - The original request to cancel (if still pending)
    ///
    /// # Returns
    /// `FCall::RFlush` on success, or an error.
    async fn rflush(&self, _old: Option<&FCall>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Walk the directory tree (9P2000).
    ///
    /// Traverses the directory tree from the given fid by following a sequence of
    /// path components. Creates a new fid representing the final destination.
    /// This is a core operation that must be implemented for navigation.
    ///
    /// # Arguments
    /// * `fid` - The starting fid to walk from
    /// * `newfid` - The new fid that will represent the destination
    /// * `wnames` - Array of path component names to traverse
    ///
    /// # Returns
    /// `FCall::RWalk` containing qids for each traversed component, or an error.
    async fn rwalk(
        &self,
        _: &FId<Self::FId>,
        _new: &FId<Self::FId>,
        _wnames: &[String],
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Read data from a file (9P2000).
    ///
    /// Reads data from the file represented by the fid at the specified offset.
    /// This is a core operation that must be implemented for reading files.
    ///
    /// # Arguments
    /// * `fid` - The file fid to read from
    /// * `offset` - The byte offset to start reading from
    /// * `count` - Maximum number of bytes to read
    ///
    /// # Returns
    /// `FCall::RRead` containing the read data, or an error.
    async fn rread(&self, _: &FId<Self::FId>, _offset: u64, _count: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Write data to a file (9P2000).
    ///
    /// Writes data to the file represented by the fid at the specified offset.
    /// Required for writable filesystems.
    ///
    /// # Arguments
    /// * `fid` - The file fid to write to
    /// * `offset` - The byte offset to start writing at
    /// * `data` - The data to write
    ///
    /// # Returns
    /// `FCall::RWrite` containing the number of bytes written, or an error.
    async fn rwrite(&self, _: &FId<Self::FId>, _offset: u64, _data: &Data) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Close a fid and release resources (9P2000).
    ///
    /// Informs the server that the fid is no longer needed. The server should release
    /// any resources associated with the fid. This is a core operation that must be
    /// implemented for proper resource cleanup.
    ///
    /// # Arguments
    /// * `fid` - The fid to close
    ///
    /// # Returns
    /// `FCall::RClunk` on success, or an error.
    async fn rclunk(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Remove a file and clunk the fid (9P2000).
    ///
    /// Removes the file represented by the fid from the filesystem, then clunks the fid.
    /// This is an older operation; prefer `runlinkat` for new implementations.
    ///
    /// # Arguments
    /// * `fid` - The file fid to remove
    ///
    /// # Returns
    /// `FCall::RRemove` on success, or an error.
    async fn rremove(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /// Negotiate protocol version and message size (9P2000).
    ///
    /// The first message in a 9P session. Negotiates the maximum message size and
    /// protocol version to use. The default implementation accepts 9P2000.L and
    /// returns VERSION_UNKNOWN for other versions.
    ///
    /// # Arguments
    /// * `msize` - Maximum message size the client can handle
    /// * `ver` - Protocol version string (e.g., "9P2000.L")
    ///
    /// # Returns
    /// `FCall::RVersion` with the negotiated msize and version.
    async fn rversion(&self, msize: u32, ver: &str) -> Result<FCall> {
        Ok(FCall::RVersion {
            msize,
            version: match ver {
                P92000L => ver.to_owned(),
                _ => VERSION_UNKNOWN.to_owned(),
            },
        })
    }
}

#[rustfmt::skip]
async fn dispatch_once<Fs, FsFId>(
    msg: &Msg,
    fs: Arc<Fs>,
    fsfids: Arc<RwLock<HashMap<u32, FId<FsFId>>>>,
) -> Result<FCall>
where
    Fs: Filesystem<FId = FsFId> + Send + Sync,
    FsFId: Send + Sync + Default,
{
    let newfid = msg.body.newfid().map(|f| FId {
        fid: f,
        aux: Default::default(),
    });

    use crate::FCall::*;
    let response = {
        let fids = fsfids.read().await;
        let get_fid = |fid: &u32| fids.get(fid).ok_or(error::Error::No(EBADF));
        let get_newfid = || newfid.as_ref().ok_or(error::Error::No(EPROTO));

        let fut = match msg.body {
            TStatFs { fid }                                                     => fs.rstatfs(get_fid(&fid)?),
            TlOpen { fid, ref flags }                                           => fs.rlopen(get_fid(&fid)?, *flags),
            TlCreate { fid, ref name, ref flags, ref mode, ref gid }            => fs.rlcreate(get_fid(&fid)?, name, *flags, *mode, *gid),
            TSymlink { fid, ref name, ref symtgt, ref gid }                     => fs.rsymlink(get_fid(&fid)?, name, symtgt, *gid),
            TMkNod { dfid, ref name, ref mode, ref major, ref minor, ref gid }  => fs.rmknod(get_fid(&dfid)?, name, *mode, *major, *minor, *gid),
            TRename { fid, dfid, ref name }                                     => fs.rrename(get_fid(&fid)?, get_fid(&dfid)?, name),
            TReadLink { fid }                                                   => fs.rreadlink(get_fid(&fid)?),
            TGetAttr { fid, ref req_mask }                                      => fs.rgetattr(get_fid(&fid)?, *req_mask),
            TSetAttr { fid, ref valid, ref stat }                               => fs.rsetattr(get_fid(&fid)?, *valid, stat),
            TxAttrWalk { fid, newfid: _, ref name }                             => fs.rxattrwalk(get_fid(&fid)?, get_newfid()?, name),
            TxAttrCreate { fid, ref name, ref attr_size, ref flags }            => fs.rxattrcreate(get_fid(&fid)?, name, *attr_size, *flags),
            TReadDir { fid, ref offset, ref count }                             => fs.rreaddir(get_fid(&fid)?, *offset, *count),
            TFSync { fid }                                                      => fs.rfsync(get_fid(&fid)?),
            TLock { fid, ref flock }                                            => fs.rlock(get_fid(&fid)?, flock),
            TGetLock { fid, ref flock }                                         => fs.rgetlock(get_fid(&fid)?, flock),
            TLink { dfid, fid, ref name }                                       => fs.rlink(get_fid(&dfid)?, get_fid(&fid)?, name),
            TMkDir { dfid, ref name, ref mode, ref gid }                        => fs.rmkdir(get_fid(&dfid)?, name, *mode, *gid),
            TRenameAt { olddirfid, ref oldname, newdirfid, ref newname }        => fs.rrenameat(get_fid(&olddirfid)?, oldname, get_fid(&newdirfid)?, newname),
            TUnlinkAt { dirfd, ref name, ref flags }                            => fs.runlinkat(get_fid(&dirfd)?, name, *flags) ,
            TAuth { afid: _, ref uname, ref aname, ref n_uname }                => fs.rauth(get_newfid()?, uname, aname, *n_uname),
            TAttach { fid: _, afid: _, ref uname, ref aname, ref n_uname }      => fs.rattach(get_newfid()?, None, uname, aname, *n_uname),
            TVersion { ref msize, ref version }                                 => fs.rversion(*msize, version),
            TFlush { oldtag: _ }                                                => fs.rflush(None),
            TWalk { fid, newfid: _, ref wnames }                                => fs.rwalk(get_fid(&fid)?, get_newfid()?, wnames),
            TRead { fid, ref offset, ref count }                                => fs.rread(get_fid(&fid)?, *offset, *count),
            TWrite { fid, ref offset, ref data }                                => fs.rwrite(get_fid(&fid)?, *offset, data),
            TClunk { fid }                                                      => fs.rclunk(get_fid(&fid)?),
            TRemove { fid }                                                     => fs.rremove(get_fid(&fid)?),
            _                                                                   => return Err(error::Error::No(EOPNOTSUPP)),
        };

        fut.await?
    };

    /* Drop the fid which the TClunk contains */
    if let TClunk { fid } = msg.body {
        let mut fids = fsfids.write().await;
        fids.remove(&fid);
    }

    if let Some(newfid) = newfid {
        let mut fids = fsfids.write().await;
        fids.insert(newfid.fid, newfid);
    }

    Ok(response)
}

async fn dispatch<Fs, Reader, Writer>(filesystem: Fs, reader: Reader, writer: Writer) -> Result<()>
where
    Fs: 'static + Filesystem + Send + Sync,
    Reader: 'static + AsyncRead + Send + std::marker::Unpin,
    Writer: 'static + AsyncWrite + Send + std::marker::Unpin,
{
    let fsfids = Arc::new(RwLock::new(HashMap::new()));
    let filesystem = Arc::new(filesystem);

    let mut framedread = LengthDelimitedCodec::builder()
        .length_field_offset(0)
        .length_field_length(4)
        .length_adjustment(-4)
        .little_endian()
        .new_read(reader);
    let framedwrite = LengthDelimitedCodec::builder()
        .length_field_offset(0)
        .length_field_length(4)
        .length_adjustment(-4)
        .little_endian()
        .new_write(writer);
    let framedwrite = Arc::new(Mutex::new(framedwrite));

    while let Some(bytes) = framedread.next().await {
        let bytes = bytes?;

        let msg = serialize::read_msg(&mut bytes.reader())?;
        info!("\t← {:?}", msg);

        let fids = fsfids.clone();
        let fs = filesystem.clone();
        let framedwrite = framedwrite.clone();

        tokio::spawn(async move {
            let response_fcall = dispatch_once(&msg, fs, fids).await.unwrap_or_else(|e| {
                error!("{:?}: Error: \"{}\": {:?}", MsgType::from(&msg.body), e, e);
                FCall::RlError {
                    ecode: e.errno() as u32,
                }
            });

            if MsgType::from(&response_fcall).is_r() {
                let response = Msg {
                    tag: msg.tag,
                    body: response_fcall,
                };

                let mut writer = bytes::BytesMut::with_capacity(4096).writer();
                if let Err(e) = serialize::write_msg(&mut writer, &response) {
                    error!("Failed to serialize response for tag {}: {:?}", msg.tag, e);
                    return;
                }

                let frozen = writer.into_inner().freeze();
                {
                    let mut framedwrite_locked = framedwrite.lock().await;
                    if let Err(e) = framedwrite_locked.send(frozen).await {
                        error!("Failed to send response for tag {}: {:?}", msg.tag, e);
                        return;
                    }
                }
                info!("\t→ {:?}", response);
            }
        });
    }

    Ok(())
}

async fn srv_async_tcp<Fs>(filesystem: Fs, addr: &str) -> Result<()>
where
    Fs: 'static + Filesystem + Send + Sync + Clone,
{
    let listener = TcpListener::bind(addr).await?;

    loop {
        let (stream, peer) = listener.accept().await?;
        info!("accepted: {:?}", peer);

        let fs = filesystem.clone();
        tokio::spawn(async move {
            let (readhalf, writehalf) = stream.into_split();
            let res = dispatch(fs, readhalf, writehalf).await;
            if let Err(e) = res {
                error!("Error: {}: {:?}", e, e);
            }
        });
    }
}

struct DeleteOnDrop {
    path: PathBuf,
    listener: UnixListener,
}

impl DeleteOnDrop {
    fn bind(path: impl AsRef<Path>) -> std::io::Result<Self> {
        let path = path.as_ref().to_owned();
        UnixListener::bind(&path).map(|listener| DeleteOnDrop { path, listener })
    }
}

impl std::ops::Deref for DeleteOnDrop {
    type Target = UnixListener;

    fn deref(&self) -> &Self::Target {
        &self.listener
    }
}

impl std::ops::DerefMut for DeleteOnDrop {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.listener
    }
}

impl Drop for DeleteOnDrop {
    fn drop(&mut self) {
        // There's no way to return a useful error here
        if let Err(e) = std::fs::remove_file(&self.path) {
            eprintln!(
                "Warning: Failed to remove socket file {:?}: {}",
                self.path, e
            );
        }
    }
}

pub async fn srv_async_unix<Fs>(filesystem: Fs, addr: impl AsRef<Path>) -> Result<()>
where
    Fs: 'static + Filesystem + Send + Sync + Clone,
{
    use tokio::signal::unix::{SignalKind, signal};

    let listener = DeleteOnDrop::bind(addr)?;

    let mut sigterm = signal(SignalKind::terminate())?;
    let mut sigint = signal(SignalKind::interrupt())?;

    let running = Arc::new(std::sync::atomic::AtomicBool::new(true));

    {
        let running = running.clone();

        tokio::spawn(async move {
            tokio::select! {
                _ = sigterm.recv() => {
                    info!("Received SIGTERM, shutting down gracefully");
                }
                _ = sigint.recv() => {
                    info!("Received SIGINT, shutting down gracefully");
                }
            }
            running.store(false, Ordering::SeqCst);
        });
    }

    while running.load(Ordering::SeqCst) {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, peer)) => {
                        info!("accepted: {:?}", peer);

                        let fs = filesystem.clone();
                        tokio::spawn(async move {
                            let (readhalf, writehalf) = tokio::io::split(stream);
                            let res = dispatch(fs, readhalf, writehalf).await;
                            if let Err(e) = res {
                                error!("Error: {:?}", e);
                            }
                        });
                    }
                    Err(e) => return Err(e.into()),
                }
            }
            _ = tokio::time::sleep(std::time::Duration::from_millis(100)) => {
                // Allow the server to check the running flag
            }
        }
    }

    info!("Server shutdown complete");
    Ok(())
}

pub async fn srv_async<Fs>(filesystem: Fs, addr: &str) -> Result<()>
where
    Fs: 'static + Filesystem + Send + Sync + Clone,
{
    let (proto, listen_addr) = utils::parse_proto(addr)
        .ok_or_else(|| io_err!(InvalidInput, "Invalid protocol or address"))?;

    match proto {
        "tcp" => srv_async_tcp(filesystem, &listen_addr).await,
        "unix" => srv_async_unix(filesystem, &listen_addr).await,
        _ => Err(From::from(io_err!(InvalidInput, "Protocol not supported"))),
    }
}
