//! 9P protocol data types and constants.
//!
//! # Protocol
//! 9P2000.L

use std::fs;
use std::mem::{size_of, size_of_val};
use std::os::unix::fs::MetadataExt;

use bitflags::bitflags;
use enum_primitive::*;

/// 9P2000 version string
pub const P92000: &str = "9P2000";

/// 9P2000.L version string
pub const P92000L: &str = "9P2000.L";

/// The version string that comes with RVersion when the server does not understand
/// the client's version string
pub const VERSION_UNKNOWN: &str = "unknown";

/*
 * 9P magic numbers
 */
/// Special tag which `TVersion`/`RVersion` must use as `tag`
pub const NOTAG: u16 = !0;

/// Special value which `TAttach` with no auth must use as `afid`
///
/// If the client does not wish to authenticate the connection, or knows that authentication is
/// not required, the afid field in the attach message should be set to `NOFID`
pub const NOFID: u32 = !0;

/// Special uid which `TAuth`/`TAttach` use as `n_uname` to indicate no uid is specified
pub const NONUNAME: u32 = !0;

/// Ample room for `TWrite`/`RRead` header
///
/// size[4] TRead/TWrite[2] tag[2] fid[4] offset[8] count[4]
pub const IOHDRSZ: u32 = 24;

/// Room for readdir header
pub const READDIRHDRSZ: u32 = 24;

/// v9fs default port
pub const V9FS_PORT: u16 = 564;

/// Old 9P2000 protocol types
///
/// Types in this module are not used 9P2000.L
pub mod p92000 {
    /// The type of I/O
    ///
    /// Open mode to be checked against the permissions for the file.
    pub mod om {
        /// Open for read
        pub const READ: u8 = 0;
        /// Write
        pub const WRITE: u8 = 1;
        /// Read and write
        pub const RDWR: u8 = 2;
        /// Execute, == read but check execute permission
        pub const EXEC: u8 = 3;
        /// Or'ed in (except for exec), truncate file first
        pub const TRUNC: u8 = 16;
        /// Or'ed in, close on exec
        pub const CEXEC: u8 = 32;
        /// Or'ed in, remove on close
        pub const RCLOSE: u8 = 64;
    }

    /// Bits in Stat.mode
    pub mod dm {
        /// Mode bit for directories
        pub const DIR: u32 = 0x80000000;
        /// Mode bit for append only files
        pub const APPEND: u32 = 0x40000000;
        /// Mode bit for exclusive use files
        pub const EXCL: u32 = 0x20000000;
        /// Mode bit for mounted channel
        pub const MOUNT: u32 = 0x10000000;
        /// Mode bit for authentication file
        pub const AUTH: u32 = 0x08000000;
        /// Mode bit for non-backed-up files
        pub const TMP: u32 = 0x04000000;
        /// Mode bit for read permission
        pub const READ: u32 = 0x4;
        /// Mode bit for write permission
        pub const WRITE: u32 = 0x2;
        /// Mode bit for execute permission
        pub const EXEC: u32 = 0x1;
    }

    /// Plan 9 Namespace metadata d(somewhat like a unix fstat)
    ///
    /// NOTE: Defined as `Dir` in libc.h of Plan 9
    #[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub struct Stat {
        /// Server type
        pub typ: u16,
        /// Server subtype
        pub dev: u32,
        /// Unique id from server
        pub qid: super::QId,
        /// Permissions
        pub mode: u32,
        /// Last read time
        pub atime: u32,
        /// Last write time
        pub mtime: u32,
        /// File length
        pub length: u64,
        /// Last element of path
        pub name: String,
        /// Owner name
        pub uid: String,
        /// Group name
        pub gid: String,
        /// Last modifier name
        pub muid: String,
    }

    impl Stat {
        /// Get the current size of the stat
        pub fn size(&self) -> u16 {
            use std::mem::{size_of, size_of_val};
            (size_of_val(&self.typ)
                + size_of_val(&self.dev)
                + size_of_val(&self.qid)
                + size_of_val(&self.mode)
                + size_of_val(&self.atime)
                + size_of_val(&self.mtime)
                + size_of_val(&self.length)
                + (size_of::<u16>() * 4)
                + self.name.len()
                + self.uid.len()
                + self.gid.len()
                + self.muid.len()) as u16
        }
    }
}

bitflags! {
    /// File lock type, Flock.typ
    #[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct LockType: u8 {
        const RDLOCK    = 0;
        const WRLOCK    = 1;
        const UNLOCK    = 2;
    }
}

bitflags! {
    /// File lock flags, Flock.flags
    #[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct LockFlag: u32 {
        #[doc = "Blocking request"]
        const BLOCK     = 1;
        #[doc = "Reserved for future use"]
        const RECLAIM   = 2;
    }
}

bitflags! {
    /// File lock status
    #[derive(Copy, Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct LockStatus: u8 {
        const SUCCESS   = 0;
        const BLOCKED   = 1;
        const ERROR     = 2;
        const GRACE     = 3;
    }
}

bitflags! {
    /// Bits in QId.typ
    ///
    /// QIdType can be constructed from std::fs::FileType via From trait
    ///
    /// # Protocol
    /// 9P2000/9P2000.L
    #[derive(Copy, Debug, Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
    pub struct QIdType: u8 {
        #[doc = "Type bit for directories"]
        const DIR       = 0x80;
        #[doc = "Type bit for append only files"]
        const APPEND    = 0x40;
        #[doc = "Type bit for exclusive use files"]
        const EXCL      = 0x20;
        #[doc = "Type bit for mounted channel"]
        const MOUNT     = 0x10;
        #[doc = "Type bit for authentication file"]
        const AUTH      = 0x08;
        #[doc = "Type bit for not-backed-up file"]
        const TMP       = 0x04;
        #[doc = "Type bits for symbolic links (9P2000.u)"]
        const SYMLINK   = 0x02;
        #[doc = "Type bits for hard-link (9P2000.u)"]
        const LINK      = 0x01;
        #[doc = "Plain file"]
        const FILE      = 0x00;
    }
}

impl From<::std::fs::FileType> for QIdType {
    fn from(typ: ::std::fs::FileType) -> Self {
        From::from(&typ)
    }
}

impl<'a> From<&'a ::std::fs::FileType> for QIdType {
    fn from(typ: &'a ::std::fs::FileType) -> Self {
        let mut qid_type = QIdType::FILE;

        if typ.is_dir() {
            qid_type.insert(QIdType::DIR)
        }

        if typ.is_symlink() {
            qid_type.insert(QIdType::SYMLINK)
        }

        qid_type
    }
}

bitflags! {
    /// Bits in `mask` and `valid` of `TGetAttr` and `RGetAttr`.
    ///
    /// # Protocol
    /// 9P2000.L
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct GetAttrMask: u64 {
        const MODE          = 0x00000001;
        const NLINK         = 0x00000002;
        const UID           = 0x00000004;
        const GID           = 0x00000008;
        const RDEV          = 0x00000010;
        const ATIME         = 0x00000020;
        const MTIME         = 0x00000040;
        const CTIME         = 0x00000080;
        const INO           = 0x00000100;
        const SIZE          = 0x00000200;
        const BLOCKS        = 0x00000400;

        const BTIME         = 0x00000800;
        const GEN           = 0x00001000;
        const DATA_VERSION  = 0x00002000;

        #[doc = "Mask for fields up to BLOCKS"]
        const BASIC         =0x000007ff;
        #[doc = "Mask for All fields above"]
        const ALL           = 0x00003fff;
    }
}

bitflags! {
    /// Bits in `mask` of `TSetAttr`.
    ///
    /// If a time bit is set without the corresponding SET bit, the current
    /// system time on the server is used instead of the value sent in the request.
    ///
    /// # Protocol
    /// 9P2000.L
    #[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
    pub struct SetAttrMask: u32 {
        const MODE      = 0x00000001;
        const UID       = 0x00000002;
        const GID       = 0x00000004;
        const SIZE      = 0x00000008;
        const ATIME     = 0x00000010;
        const MTIME     = 0x00000020;
        const CTIME     = 0x00000040;
        const ATIME_SET = 0x00000080;
        const MTIME_SET = 0x00000100;
    }
}

/// Server side data type for path tracking
///
/// The server's unique identification for the file being accessed
///
/// # Protocol
/// 9P2000/9P2000.L
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct QId {
    /// Specify whether the file is a directory, append-only file, etc.
    pub typ: QIdType,
    /// Version number for a file; typically, it is incremented every time the file is modified
    pub version: u32,
    /// An integer which is unique among all files in the hierarchy
    pub path: u64,
}

impl QId {
    pub fn size(&self) -> u32 {
        (size_of::<QIdType>() + size_of::<u32>() + size_of::<u64>()) as u32
    }
}

/// Filesystem information corresponding to `struct statfs` of Linux.
///
/// # Protocol
/// 9P2000.L
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct StatFs {
    /// Type of file system
    pub typ: u32,
    /// Optimal transfer block size
    pub bsize: u32,
    /// Total data blocks in file system
    pub blocks: u64,
    /// Free blocks in fs
    pub bfree: u64,
    /// Free blocks avail to non-superuser
    pub bavail: u64,
    /// Total file nodes in file system
    pub files: u64,
    /// Free file nodes in fs
    pub ffree: u64,
    /// Filesystem ID
    pub fsid: u64,
    /// Maximum length of filenames
    pub namelen: u32,
}

impl From<nix::sys::statvfs::Statvfs> for StatFs {
    fn from(buf: nix::sys::statvfs::Statvfs) -> StatFs {
        StatFs {
            typ: 0,
            bsize: buf.block_size() as u32,
            blocks: buf.blocks(),
            bfree: buf.blocks_free(),
            bavail: buf.blocks_available(),
            files: buf.files(),
            ffree: buf.files_free(),
            fsid: buf.filesystem_id(),
            namelen: buf.name_max() as u32,
        }
    }
}

/// Time struct
///
/// # Protocol
/// 9P2000.L
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Time {
    pub sec: u64,
    pub nsec: u64,
}

/// File attributes corresponding to `struct stat` of Linux.
///
/// Stat can be constructed from `std::fs::Metadata` via From trait
///
/// # Protocol
/// 9P2000.L
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stat {
    /// Protection
    pub mode: u32,
    /// User ID of owner
    pub uid: u32,
    /// Group ID of owner
    pub gid: u32,
    /// Number of hard links
    pub nlink: u64,
    /// Device ID (if special file)
    pub rdev: u64,
    /// Total size, in bytes
    pub size: u64,
    /// Blocksize for file system I/O
    pub blksize: u64,
    /// Number of 512B blocks allocated
    pub blocks: u64,
    /// Time of last access
    pub atime: Time,
    /// Time of last modification
    pub mtime: Time,
    /// Time of last status change
    pub ctime: Time,
}

impl From<fs::Metadata> for Stat {
    fn from(attr: fs::Metadata) -> Self {
        From::from(&attr)
    }
}

// Default conversion from metadata of libstd
impl<'a> From<&'a fs::Metadata> for Stat {
    fn from(attr: &'a fs::Metadata) -> Self {
        Stat {
            mode: attr.mode(),
            uid: attr.uid(),
            gid: attr.gid(),
            nlink: attr.nlink(),
            rdev: attr.rdev(),
            size: attr.size(),
            blksize: attr.blksize(),
            blocks: attr.blocks(),
            atime: Time {
                sec: attr.atime() as u64,
                nsec: attr.atime_nsec() as u64,
            },
            mtime: Time {
                sec: attr.mtime() as u64,
                nsec: attr.mtime_nsec() as u64,
            },
            ctime: Time {
                sec: attr.ctime() as u64,
                nsec: attr.ctime_nsec() as u64,
            },
        }
    }
}

/// Subset of `Stat` used for `TSetAttr`
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct SetAttr {
    pub mode: u32,
    pub uid: u32,
    pub gid: u32,
    pub size: u64,
    pub atime: Time,
    pub mtime: Time,
}

/// Directory entry used in `RReadDir`
///
/// # Protocol
/// 9P2000.L
#[derive(Clone, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirEntry {
    /// QId for this directory
    pub qid: QId,
    /// The index of this entry
    pub offset: u64,
    /// Corresponds to `d_type` of `struct dirent`
    ///
    /// Use `0` if you can't set this properly. It might be enough.
    pub typ: u8,
    /// Directory name
    pub name: String,
}

impl DirEntry {
    pub fn size(&self) -> u32 {
        (self.qid.size() as usize
            + size_of_val(&self.offset)
            + size_of_val(&self.typ)
            + size_of::<u16>()
            + self.name.len()) as u32
    }
}

/// Directory entry array
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct DirEntryData {
    pub data: Vec<DirEntry>,
}

impl DirEntryData {
    pub fn new() -> DirEntryData {
        Self::with(Vec::new())
    }

    pub fn with(v: Vec<DirEntry>) -> DirEntryData {
        DirEntryData { data: v }
    }

    pub fn data(&self) -> &[DirEntry] {
        &self.data
    }

    pub fn size(&self) -> u32 {
        self.data.iter().fold(0, |a, e| a + e.size())
    }

    pub fn push(&mut self, entry: DirEntry) {
        self.data.push(entry);
    }
}

impl Default for DirEntryData {
    fn default() -> Self {
        Self::new()
    }
}

/// Data type used in `RRead` and `TWrite`
///
/// # Protocol
/// 9P2000/9P2000.L
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Data(pub Vec<u8>);

/// Similar to Linux `struct flock`
///
/// # Protocol
/// 9P2000.L
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Flock {
    pub typ: LockType,
    pub flags: LockFlag,
    pub start: u64,
    pub length: u64,
    pub proc_id: u32,
    pub client_id: String,
}

/// Getlock structure
///
/// # Protocol
/// 9P2000.L
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Getlock {
    pub typ: LockType,
    pub start: u64,
    pub length: u64,
    pub proc_id: u32,
    pub client_id: String,
}

// Commented out the types not used in 9P2000.L
enum_from_primitive! {
    #[doc = "Message type, 9P operations"]
    #[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
    pub enum MsgType {
        // 9P2000.L
        TlError         = 6,    // Illegal, never used
        RlError,
        TStatFs         = 8,
        RStatFs,
        TlOpen          = 12,
        RlOpen,
        TlCreate        = 14,
        RlCreate,
        TSymlink        = 16,
        RSymlink,
        TMkNod          = 18,
        RMkNod,
        TRename         = 20,
        RRename,
        TReadLink       = 22,
        RReadLink,
        TGetAttr        = 24,
        RGetAttr,
        TSetAttr        = 26,
        RSetAttr,
        TxAttrWalk      = 30,
        RxAttrWalk,
        TxAttrCreate    = 32,
        RxAttrCreate,
        TReadDir        = 40,
        RReadDir,
        TFSync          = 50,
        RFSync,
        TLock           = 52,
        RLock,
        TGetLock        = 54,
        RGetLock,
        TLink           = 70,
        RLink,
        TMkDir          = 72,
        RMkDir,
        TRenameAt       = 74,
        RRenameAt,
        TUnlinkAt       = 76,
        RUnlinkAt,

        // 9P2000
        TVersion        = 100,
        RVersion,
        TAuth           = 102,
        RAuth,
        TAttach         = 104,
        RAttach,
        //TError          = 106,  // Illegal, never used
        //RError,
        TFlush          = 108,
        RFlush,
        TWalk           = 110,
        RWalk,
        //TOpen           = 112,
        //ROpen,
        //TCreate         = 114,
        //RCreate,
        TRead           = 116,
        RRead,
        TWrite          = 118,
        RWrite,
        TClunk          = 120,
        RClunk,
        TRemove         = 122,
        RRemove,
        //TStat           = 124,
        //RStat,
        //TWStat          = 126,
        //RWStat,
    }
}

impl MsgType {
    /// If the message type is T-message
    pub fn is_t(&self) -> bool {
        !self.is_r()
    }

    /// If the message type is R-message
    pub fn is_r(&self) -> bool {
        use crate::MsgType::*;

        matches!(
            *self,
            RlError
                | RStatFs
                | RlOpen
                | RlCreate
                | RSymlink
                | RMkNod
                | RRename
                | RReadLink
                | RGetAttr
                | RSetAttr
                | RxAttrWalk
                | RxAttrCreate
                | RReadDir
                | RFSync
                | RLock
                | RGetLock
                | RLink
                | RMkDir
                | RRenameAt
                | RUnlinkAt
                | RVersion
                | RAuth
                | RAttach
                | RFlush
                | RWalk
                | RRead
                | RWrite
                | RClunk
                | RRemove
        )
    }
}

impl<'a> From<&'a FCall> for MsgType {
    fn from(fcall: &'a FCall) -> MsgType {
        match *fcall {
            FCall::RlError { .. } => MsgType::RlError,
            FCall::TStatFs { .. } => MsgType::TStatFs,
            FCall::RStatFs { .. } => MsgType::RStatFs,
            FCall::TlOpen { .. } => MsgType::TlOpen,
            FCall::RlOpen { .. } => MsgType::RlOpen,
            FCall::TlCreate { .. } => MsgType::TlCreate,
            FCall::RlCreate { .. } => MsgType::RlCreate,
            FCall::TSymlink { .. } => MsgType::TSymlink,
            FCall::RSymlink { .. } => MsgType::RSymlink,
            FCall::TMkNod { .. } => MsgType::TMkNod,
            FCall::RMkNod { .. } => MsgType::RMkNod,
            FCall::TRename { .. } => MsgType::TRename,
            FCall::RRename => MsgType::RRename,
            FCall::TReadLink { .. } => MsgType::TReadLink,
            FCall::RReadLink { .. } => MsgType::RReadLink,
            FCall::TGetAttr { .. } => MsgType::TGetAttr,
            FCall::RGetAttr { .. } => MsgType::RGetAttr,
            FCall::TSetAttr { .. } => MsgType::TSetAttr,
            FCall::RSetAttr => MsgType::RSetAttr,
            FCall::TxAttrWalk { .. } => MsgType::TxAttrWalk,
            FCall::RxAttrWalk { .. } => MsgType::RxAttrWalk,
            FCall::TxAttrCreate { .. } => MsgType::TxAttrCreate,
            FCall::RxAttrCreate => MsgType::RxAttrCreate,
            FCall::TReadDir { .. } => MsgType::TReadDir,
            FCall::RReadDir { .. } => MsgType::RReadDir,
            FCall::TFSync { .. } => MsgType::TFSync,
            FCall::RFSync => MsgType::RFSync,
            FCall::TLock { .. } => MsgType::TLock,
            FCall::RLock { .. } => MsgType::RLock,
            FCall::TGetLock { .. } => MsgType::TGetLock,
            FCall::RGetLock { .. } => MsgType::RGetLock,
            FCall::TLink { .. } => MsgType::TLink,
            FCall::RLink => MsgType::RLink,
            FCall::TMkDir { .. } => MsgType::TMkDir,
            FCall::RMkDir { .. } => MsgType::RMkDir,
            FCall::TRenameAt { .. } => MsgType::TRenameAt,
            FCall::RRenameAt => MsgType::RRenameAt,
            FCall::TUnlinkAt { .. } => MsgType::TUnlinkAt,
            FCall::RUnlinkAt => MsgType::RUnlinkAt,
            FCall::TAuth { .. } => MsgType::TAuth,
            FCall::RAuth { .. } => MsgType::RAuth,
            FCall::TAttach { .. } => MsgType::TAttach,
            FCall::RAttach { .. } => MsgType::RAttach,
            FCall::TVersion { .. } => MsgType::TVersion,
            FCall::RVersion { .. } => MsgType::RVersion,
            FCall::TFlush { .. } => MsgType::TFlush,
            FCall::RFlush => MsgType::RFlush,
            FCall::TWalk { .. } => MsgType::TWalk,
            FCall::RWalk { .. } => MsgType::RWalk,
            FCall::TRead { .. } => MsgType::TRead,
            FCall::RRead { .. } => MsgType::RRead,
            FCall::TWrite { .. } => MsgType::TWrite,
            FCall::RWrite { .. } => MsgType::RWrite,
            FCall::TClunk { .. } => MsgType::TClunk,
            FCall::RClunk => MsgType::RClunk,
            FCall::TRemove { .. } => MsgType::TRemove,
            FCall::RRemove => MsgType::RRemove,
        }
    }
}

/// A data type encapsulating the various 9P messages
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum FCall {
    // 9P2000.L
    RlError {
        ecode: u32,
    },
    TStatFs {
        fid: u32,
    },
    RStatFs {
        statfs: StatFs,
    },
    TlOpen {
        fid: u32,
        flags: u32,
    },
    RlOpen {
        qid: QId,
        iounit: u32,
    },
    TlCreate {
        fid: u32,
        name: String,
        flags: u32,
        mode: u32,
        gid: u32,
    },
    RlCreate {
        qid: QId,
        iounit: u32,
    },
    TSymlink {
        fid: u32,
        name: String,
        symtgt: String,
        gid: u32,
    },
    RSymlink {
        qid: QId,
    },
    TMkNod {
        dfid: u32,
        name: String,
        mode: u32,
        major: u32,
        minor: u32,
        gid: u32,
    },
    RMkNod {
        qid: QId,
    },
    TRename {
        fid: u32,
        dfid: u32,
        name: String,
    },
    RRename,
    TReadLink {
        fid: u32,
    },
    RReadLink {
        target: String,
    },
    TGetAttr {
        fid: u32,
        req_mask: GetAttrMask,
    },
    /// Reserved members specified in the protocol are handled in Encodable/Decodable traits.
    RGetAttr {
        valid: GetAttrMask,
        qid: QId,
        stat: Stat,
    },
    TSetAttr {
        fid: u32,
        valid: SetAttrMask,
        stat: SetAttr,
    },
    RSetAttr,
    TxAttrWalk {
        fid: u32,
        newfid: u32,
        name: String,
    },
    RxAttrWalk {
        size: u64,
    },
    TxAttrCreate {
        fid: u32,
        name: String,
        attr_size: u64,
        flags: u32,
    },
    RxAttrCreate,
    TReadDir {
        fid: u32,
        offset: u64,
        count: u32,
    },
    RReadDir {
        data: DirEntryData,
    },
    TFSync {
        fid: u32,
    },
    RFSync,
    TLock {
        fid: u32,
        flock: Flock,
    },
    RLock {
        status: LockStatus,
    },
    TGetLock {
        fid: u32,
        flock: Getlock,
    },
    RGetLock {
        flock: Getlock,
    },
    TLink {
        dfid: u32,
        fid: u32,
        name: String,
    },
    RLink,
    TMkDir {
        dfid: u32,
        name: String,
        mode: u32,
        gid: u32,
    },
    RMkDir {
        qid: QId,
    },
    TRenameAt {
        olddirfid: u32,
        oldname: String,
        newdirfid: u32,
        newname: String,
    },
    RRenameAt,
    TUnlinkAt {
        dirfd: u32,
        name: String,
        flags: u32,
    },
    RUnlinkAt,

    // 9P2000.u
    TAuth {
        afid: u32,
        uname: String,
        aname: String,
        n_uname: u32,
    },
    RAuth {
        aqid: QId,
    },
    TAttach {
        fid: u32,
        afid: u32,
        uname: String,
        aname: String,
        n_uname: u32,
    },
    RAttach {
        qid: QId,
    },

    // 9P2000
    TVersion {
        msize: u32,
        version: String,
    },
    RVersion {
        msize: u32,
        version: String,
    },
    TFlush {
        oldtag: u16,
    },
    RFlush,
    TWalk {
        fid: u32,
        newfid: u32,
        wnames: Vec<String>,
    },
    RWalk {
        wqids: Vec<QId>,
    },
    TRead {
        fid: u32,
        offset: u64,
        count: u32,
    },
    RRead {
        data: Data,
    },
    TWrite {
        fid: u32,
        offset: u64,
        data: Data,
    },
    RWrite {
        count: u32,
    },
    TClunk {
        fid: u32,
    },
    RClunk,
    TRemove {
        fid: u32,
    },
    RRemove,
    // 9P2000 operations not used for 9P2000.L
    //TAuth { afid: u32, uname: String, aname: String },
    //RAuth { aqid: QId },
    //RError { ename: String },
    //TAttach { fid: u32, afid: u32, uname: String, aname: String },
    //RAttach { qid: QId },
    //TOpen { fid: u32, mode: u8 },
    //ROpen { qid: QId, iounit: u32 },
    //TCreate { fid: u32, name: String, perm: u32, mode: u8 },
    //RCreate { qid: QId, iounit: u32 },
    //TStat { fid: u32 },
    //RStat { stat: Stat },
    //TWStat { fid: u32, stat: Stat },
    //RWStat,
}

impl FCall {
    /// Get the fids which self contains
    pub fn fids(&self) -> Vec<u32> {
        match *self {
            FCall::TStatFs { fid } => vec![fid],
            FCall::TlOpen { fid, .. } => vec![fid],
            FCall::TlCreate { fid, .. } => vec![fid],
            FCall::TSymlink { fid, .. } => vec![fid],
            FCall::TMkNod { dfid, .. } => vec![dfid],
            FCall::TRename { fid, dfid, .. } => vec![fid, dfid],
            FCall::TReadLink { fid } => vec![fid],
            FCall::TGetAttr { fid, .. } => vec![fid],
            FCall::TSetAttr { fid, .. } => vec![fid],
            FCall::TxAttrWalk { fid, .. } => vec![fid],
            FCall::TxAttrCreate { fid, .. } => vec![fid],
            FCall::TReadDir { fid, .. } => vec![fid],
            FCall::TFSync { fid, .. } => vec![fid],
            FCall::TLock { fid, .. } => vec![fid],
            FCall::TGetLock { fid, .. } => vec![fid],
            FCall::TLink { dfid, fid, .. } => vec![dfid, fid],
            FCall::TMkDir { dfid, .. } => vec![dfid],
            FCall::TRenameAt {
                olddirfid,
                newdirfid,
                ..
            } => vec![olddirfid, newdirfid],
            FCall::TUnlinkAt { dirfd, .. } => vec![dirfd],
            FCall::TAttach { afid, .. } if afid != NOFID => vec![afid],
            FCall::TWalk { fid, .. } => vec![fid],
            FCall::TRead { fid, .. } => vec![fid],
            FCall::TWrite { fid, .. } => vec![fid],
            FCall::TClunk { fid, .. } => vec![fid],
            FCall::TRemove { fid } => vec![fid],
            _ => Vec::new(),
        }
    }

    /// Get the newfid which self contains
    pub fn newfid(&self) -> Option<u32> {
        match *self {
            FCall::TxAttrWalk { newfid, .. } => Some(newfid),
            FCall::TAuth { afid, .. } => Some(afid),
            FCall::TAttach { fid, .. } => Some(fid),
            FCall::TWalk { newfid, .. } => Some(newfid),
            _ => None,
        }
    }

    /// Get the qids which self contains
    pub fn qids(&self) -> Vec<QId> {
        match *self {
            FCall::RlOpen { qid, .. } => vec![qid],
            FCall::RlCreate { qid, .. } => vec![qid],
            FCall::RSymlink { qid } => vec![qid],
            FCall::RMkNod { qid } => vec![qid],
            FCall::RGetAttr { qid, .. } => vec![qid],
            FCall::RMkDir { qid } => vec![qid],
            FCall::RAuth { aqid } => vec![aqid],
            FCall::RAttach { qid } => vec![qid],
            FCall::RWalk { ref wqids } => wqids.clone(),
            _ => Vec::new(),
        }
    }
}

/// Envelope for 9P messages
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Msg {
    /// Chosen and used by the client to identify the message.
    /// The reply to the message will have the same tag
    pub tag: u16,
    /// Message body encapsulating the various 9P messages
    pub body: FCall,
}
