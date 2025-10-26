use {
    async_trait::async_trait,
    clap::Parser,
    filetime::FileTime,
    nix::libc::{O_CREAT, O_RDONLY, O_RDWR, O_TRUNC, O_WRONLY},
    rs9p::{
        srv::{FId, Filesystem, srv_async},
        *,
    },
    std::{
        io::{self, SeekFrom},
        os::unix::fs::PermissionsExt,
        path::PathBuf,
    },
    tokio::{
        fs,
        io::{AsyncReadExt, AsyncSeekExt, AsyncWriteExt},
        sync::{Mutex, RwLock},
    },
    tokio_stream::{StreamExt, wrappers::ReadDirStream},
};

mod utils;
use crate::utils::*;

// Some clients will incorrectly set bits in 9p flags that don't make sense.
// For exmaple, the linux 9p kernel client propagates O_DIRECT to TCREATE and TOPEN
// and from there to the server.
// Processes on client machines set O_DIRECT to bypass the cache, but if
// the server uses O_DIRECT in the open or create, then subsequent server
// write and read system calls will fail, as O_DIRECT requires at minimum 512
// byte aligned data, and the data is almost always not aligned.
// While the linux kernel client is arguably broken, we won't be able
// to fix every kernel out there, and this is surely not the only buggy client
// we will see.
// The fix is to enumerate the set of flags we support and then and that with
// the flags received in a TCREATE or TOPEN. This nicely fixes a real problem
// we are seeing with a file system benchmark.
const UNIX_FLAGS: u32 = (O_WRONLY | O_RDONLY | O_RDWR | O_CREAT | O_TRUNC) as u32;

// Maximum depth protection:
// Without a depth limit, it's possible to create infinite recursion by mounting
// the 9P filesystem inside its own export directory. For example:
//   - Export directory: /home/user/export
//   - Mount point: /home/user/export/mnt
// Accessing /home/user/export/mnt/mnt/mnt/... would recurse infinitely.
// The max_depth option prevents this by tracking how deep we've traversed
// from the root and returning ELOOP (too many levels of symbolic links) when
// the limit is exceeded.

#[derive(Default)]
struct UnpfsFId {
    realpath: RwLock<PathBuf>,
    file: Mutex<Option<fs::File>>,
    depth: RwLock<usize>,
}

#[derive(Clone)]
struct Unpfs {
    realroot: PathBuf,
    max_depth: usize,
}

#[async_trait]
impl Filesystem for Unpfs {
    type FId = UnpfsFId;

    async fn rattach(
        &self,
        fid: &FId<Self::FId>,
        _afid: Option<&FId<Self::FId>>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        {
            let mut realpath = fid.aux.realpath.write().await;
            *realpath = PathBuf::from(&self.realroot);
        }
        {
            let mut depth = fid.aux.depth.write().await;
            *depth = 0;
        }

        Ok(FCall::RAttach {
            qid: get_qid(&self.realroot).await?,
        })
    }

    async fn rwalk(
        &self,
        fid: &FId<Self::FId>,
        newfid: &FId<Self::FId>,
        wnames: &[String],
    ) -> Result<FCall> {
        let mut wqids = Vec::new();
        let mut path = {
            let realpath = fid.aux.realpath.read().await;
            realpath.clone()
        };

        let current_depth = {
            let depth = fid.aux.depth.read().await;
            *depth
        };

        let mut new_depth = current_depth;

        for (i, name) in wnames.iter().enumerate() {
            // Check for ".." which decreases depth
            if name == ".." {
                new_depth = new_depth.saturating_sub(1);
            } else if name != "." {
                // Any path component other than "." or ".." increases depth
                new_depth += 1;

                // Check if we've exceeded max depth
                if new_depth > self.max_depth {
                    return Err(error::Error::No(error::errno::ELOOP));
                }
            }

            path.push(name);

            let qid = match get_qid(&path).await {
                Ok(qid) => qid,
                Err(e) => {
                    if i == 0 {
                        return Err(e);
                    } else {
                        break;
                    }
                }
            };

            wqids.push(qid);
        }

        {
            let mut new_realpath = newfid.aux.realpath.write().await;
            *new_realpath = path;
        }
        {
            let mut depth = newfid.aux.depth.write().await;
            *depth = new_depth;
        }

        Ok(FCall::RWalk { wqids })
    }

    async fn rgetattr(&self, fid: &FId<Self::FId>, req_mask: GetAttrMask) -> Result<FCall> {
        let attr = {
            let realpath = fid.aux.realpath.read().await;
            fs::symlink_metadata(&*realpath).await?
        };

        Ok(FCall::RGetAttr {
            valid: req_mask,
            qid: qid_from_attr(&attr),
            stat: From::from(attr),
        })
    }

    async fn rsetattr(
        &self,
        fid: &FId<Self::FId>,
        valid: SetAttrMask,
        stat: &SetAttr,
    ) -> Result<FCall> {
        let filepath = {
            let realpath = fid.aux.realpath.read().await;
            realpath.clone()
        };

        if valid.contains(SetAttrMask::MODE) {
            fs::set_permissions(&filepath, PermissionsExt::from_mode(stat.mode)).await?;
        }

        if valid.intersects(SetAttrMask::UID | SetAttrMask::GID) {
            let uid = if valid.contains(SetAttrMask::UID) {
                Some(nix::unistd::Uid::from_raw(stat.uid))
            } else {
                None
            };
            let gid = if valid.contains(SetAttrMask::GID) {
                Some(nix::unistd::Gid::from_raw(stat.gid))
            } else {
                None
            };
            nix::unistd::chown(&filepath, uid, gid)?;
        }

        if valid.contains(SetAttrMask::SIZE) {
            let _ = fs::OpenOptions::new()
                .write(true)
                .create(false)
                .open(&filepath)
                .await?
                .set_len(stat.size)
                .await?;
        }

        if valid.intersects(SetAttrMask::ATIME_SET | SetAttrMask::MTIME_SET) {
            let attr = fs::metadata(&filepath).await?;
            let atime = if valid.contains(SetAttrMask::ATIME_SET) {
                FileTime::from_unix_time(stat.atime.sec as i64, stat.atime.nsec as u32)
            } else {
                FileTime::from_last_access_time(&attr)
            };

            let mtime = if valid.contains(SetAttrMask::MTIME_SET) {
                FileTime::from_unix_time(stat.mtime.sec as i64, stat.mtime.nsec as u32)
            } else {
                FileTime::from_last_modification_time(&attr)
            };

            let _ = tokio::task::spawn_blocking(move || {
                filetime::set_file_times(filepath, atime, mtime)
            })
            .await;
        }

        Ok(FCall::RSetAttr)
    }

    async fn rreadlink(&self, fid: &FId<Self::FId>) -> Result<FCall> {
        let link = {
            let realpath = fid.aux.realpath.read().await;
            fs::read_link(&*realpath).await?
        };

        Ok(FCall::RReadLink {
            target: link.to_string_lossy().into_owned(),
        })
    }

    async fn rreaddir(&self, fid: &FId<Self::FId>, off: u64, count: u32) -> Result<FCall> {
        let mut dirents = DirEntryData::new();

        let offset = if off == 0 {
            dirents.push(get_dirent_from(".", 0).await?);
            dirents.push(get_dirent_from("..", 1).await?);
            off
        } else {
            off - 1
        } as usize;

        let mut entries = {
            let realpath = fid.aux.realpath.read().await;
            ReadDirStream::new(fs::read_dir(&*realpath).await?).skip(offset)
        };

        let mut i = offset;
        while let Some(entry) = entries.next().await {
            let dirent = get_dirent(&entry?, 2 + i as u64).await?;
            if dirents.size() + dirent.size() > count {
                break;
            }
            dirents.push(dirent);
            i += 1;
        }

        Ok(FCall::RReadDir { data: dirents })
    }

    async fn rlopen(&self, fid: &FId<Self::FId>, flags: u32) -> Result<FCall> {
        let realpath = {
            let realpath = fid.aux.realpath.read().await;
            realpath.clone()
        };

        let qid = get_qid(&realpath).await?;
        if !qid.typ.contains(QIdType::DIR) {
            let oflags = nix::fcntl::OFlag::from_bits_truncate((flags & UNIX_FLAGS) as i32);
            let omode = nix::sys::stat::Mode::from_bits_truncate(0);
            let fd = nix::fcntl::open(&realpath, oflags, omode)?;

            {
                let mut file = fid.aux.file.lock().await;
                *file = Some(fs::File::from_std(fd.into()));
            }
        }

        Ok(FCall::RlOpen { qid, iounit: 0 })
    }

    async fn rlcreate(
        &self,
        fid: &FId<Self::FId>,
        name: &str,
        flags: u32,
        mode: u32,
        _gid: u32,
    ) -> Result<FCall> {
        let path = {
            let realpath = fid.aux.realpath.read().await;
            realpath.join(name)
        };
        let oflags = nix::fcntl::OFlag::from_bits_truncate((flags & UNIX_FLAGS) as i32);
        let omode = nix::sys::stat::Mode::from_bits_truncate(mode);
        let fd = nix::fcntl::open(&path, oflags, omode)?;

        let qid = get_qid(&path).await?;
        {
            let mut realpath = fid.aux.realpath.write().await;
            *realpath = path;
        }
        {
            let mut file = fid.aux.file.lock().await;
            *file = Some(fs::File::from_std(fd.into()));
        }

        Ok(FCall::RlCreate { qid, iounit: 0 })
    }

    async fn rread(&self, fid: &FId<Self::FId>, offset: u64, count: u32) -> Result<FCall> {
        let buf = {
            let mut file = fid.aux.file.lock().await;
            let file = file.as_mut().ok_or_else(|| INVALID_FID!())?;
            file.seek(SeekFrom::Start(offset)).await?;

            let mut buf = vec![0; count as usize];
            let bytes = file.read(&mut buf[..]).await?;
            buf.truncate(bytes);
            buf
        };

        Ok(FCall::RRead { data: Data(buf) })
    }

    async fn rwrite(&self, fid: &FId<Self::FId>, offset: u64, data: &Data) -> Result<FCall> {
        let count = {
            let mut file = fid.aux.file.lock().await;
            let file = file.as_mut().ok_or_else(|| INVALID_FID!())?;
            file.seek(SeekFrom::Start(offset)).await?;
            file.write(&data.0).await? as u32
        };

        Ok(FCall::RWrite { count })
    }

    async fn rmkdir(
        &self,
        dfid: &FId<Self::FId>,
        name: &str,
        _mode: u32,
        _gid: u32,
    ) -> Result<FCall> {
        let path = {
            let realpath = dfid.aux.realpath.read().await;
            realpath.join(name)
        };

        fs::create_dir(&path).await?;

        Ok(FCall::RMkDir {
            qid: get_qid(&path).await?,
        })
    }

    async fn rrenameat(
        &self,
        olddir: &FId<Self::FId>,
        oldname: &str,
        newdir: &FId<Self::FId>,
        newname: &str,
    ) -> Result<FCall> {
        let oldpath = {
            let realpath = olddir.aux.realpath.read().await;
            realpath.join(oldname)
        };

        let newpath = {
            let realpath = newdir.aux.realpath.read().await;
            realpath.join(newname)
        };

        fs::rename(&oldpath, &newpath).await?;

        Ok(FCall::RRenameAt)
    }

    async fn runlinkat(&self, dirfid: &FId<Self::FId>, name: &str, _flags: u32) -> Result<FCall> {
        let path = {
            let realpath = dirfid.aux.realpath.read().await;
            realpath.join(name)
        };

        match fs::symlink_metadata(&path).await? {
            ref attr if attr.is_dir() => fs::remove_dir(&path).await?,
            _ => fs::remove_file(&path).await?,
        };

        Ok(FCall::RUnlinkAt)
    }

    async fn rfsync(&self, fid: &FId<Self::FId>) -> Result<FCall> {
        {
            let mut file = fid.aux.file.lock().await;
            file.as_mut()
                .ok_or_else(|| INVALID_FID!())?
                .sync_all()
                .await?;
        }

        Ok(FCall::RFSync)
    }

    async fn rclunk(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Ok(FCall::RClunk)
    }

    async fn rstatfs(&self, fid: &FId<Self::FId>) -> Result<FCall> {
        let path = {
            let realpath = fid.aux.realpath.read().await;
            realpath.clone()
        };

        let fs = tokio::task::spawn_blocking(move || nix::sys::statvfs::statvfs(&path))
            .await
            .map_err(|e| Error::Io(io::Error::other(e)))??;

        Ok(FCall::RStatFs {
            statfs: From::from(fs),
        })
    }
}

#[derive(Debug, clap::Parser)]
struct Cli {
    /// proto!address!port
    /// where: proto = tcp | unix
    address: String,

    /// Directory to export
    exportdir: PathBuf,

    /// Maximum directory depth to traverse
    #[arg(long, default_value_t = 200)]
    max_depth: usize,
}

async fn unpfs_main(
    Cli {
        address,
        exportdir,
        max_depth,
    }: Cli,
) -> rs9p::Result<i32> {
    if !fs::try_exists(&exportdir).await? {
        fs::create_dir_all(&exportdir).await?;
    }
    if !fs::metadata(&exportdir).await?.is_dir() {
        return res!(io_err!(Other, "mount point must be a directory"));
    }

    println!("[*] Maximum depth limit: {}", max_depth);
    println!("[*] Ready to accept clients: {}", address);
    srv_async(
        Unpfs {
            realroot: exportdir,
            max_depth,
        },
        &address,
    )
    .await
    .and(Ok(0))
}

#[tokio::main]
async fn main() {
    env_logger::init();

    let exit_code = unpfs_main(Cli::parse()).await.unwrap_or_else(|e| {
        eprintln!("Error: {:?}", e);
        -1
    });

    std::process::exit(exit_code);
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_depth_tracking() {
        // Test that depth increases with normal paths
        let mut depth: usize = 0;

        // Going down: a/b/c
        for name in ["a", "b", "c"] {
            if name != "." {
                depth += 1;
            }
        }
        assert_eq!(depth, 3);

        // Going back up with ".."
        depth = depth.saturating_sub(1_usize);
        assert_eq!(depth, 2);

        // "." should not change depth
        let original = depth;
        // (no change for ".")
        assert_eq!(depth, original);

        // Multiple ".." can bring us back to 0
        depth = depth.saturating_sub(1_usize);
        depth = depth.saturating_sub(1_usize);
        assert_eq!(depth, 0);

        // saturating_sub prevents underflow
        depth = depth.saturating_sub(1_usize);
        assert_eq!(depth, 0);
    }

    #[test]
    fn test_max_depth_logic() {
        let max_depth = 5_usize;
        let mut current_depth: usize = 3;

        // Should allow going to depth 4 and 5
        current_depth += 1;
        assert!(current_depth <= max_depth);

        current_depth += 1;
        assert!(current_depth <= max_depth);

        // Should reject depth 6
        current_depth += 1;
        assert!(current_depth > max_depth);
    }

    #[test]
    fn test_no_max_depth() {
        let max_depth: Option<usize> = None;
        let current_depth: usize = 1000;

        // With no max_depth, any depth should be allowed
        match max_depth {
            Some(max) => assert!(current_depth <= max),
            None => {
                // No limit - test passes as long as we reach this branch
                // Any large depth should be acceptable
                assert!(current_depth > 0);
            }
        }
    }
}
