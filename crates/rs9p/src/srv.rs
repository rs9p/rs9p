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
pub trait Filesystem: Send {
    /// User defined fid type to be associated with a client's fid.
    type FId: Send + Sync + Default;

    // 9P2000.L
    async fn rstatfs(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rlopen(&self, _: &FId<Self::FId>, _flags: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

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

    async fn rsymlink(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _sym: &str,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

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

    async fn rrename(&self, _: &FId<Self::FId>, _: &FId<Self::FId>, _name: &str) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rreadlink(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rgetattr(&self, _: &FId<Self::FId>, _req_mask: GetAttrMask) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rsetattr(
        &self,
        _: &FId<Self::FId>,
        _valid: SetAttrMask,
        _stat: &SetAttr,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rxattrwalk(
        &self,
        _: &FId<Self::FId>,
        _: &FId<Self::FId>,
        _name: &str,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rxattrcreate(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _attr_size: u64,
        _flags: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rreaddir(&self, _: &FId<Self::FId>, _offset: u64, _count: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rfsync(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rlock(&self, _: &FId<Self::FId>, _lock: &Flock) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rgetlock(&self, _: &FId<Self::FId>, _lock: &Getlock) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rlink(&self, _: &FId<Self::FId>, _: &FId<Self::FId>, _name: &str) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rmkdir(
        &self,
        _: &FId<Self::FId>,
        _name: &str,
        _mode: u32,
        _gid: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rrenameat(
        &self,
        _: &FId<Self::FId>,
        _oldname: &str,
        _: &FId<Self::FId>,
        _newname: &str,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn runlinkat(&self, _: &FId<Self::FId>, _name: &str, _flags: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    /*
     * 9P2000.u subset
     */
    async fn rauth(
        &self,
        _: &FId<Self::FId>,
        _uname: &str,
        _aname: &str,
        _n_uname: u32,
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

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
    async fn rflush(&self, _old: Option<&FCall>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rwalk(
        &self,
        _: &FId<Self::FId>,
        _new: &FId<Self::FId>,
        _wnames: &[String],
    ) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rread(&self, _: &FId<Self::FId>, _offset: u64, _count: u32) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rwrite(&self, _: &FId<Self::FId>, _offset: u64, _data: &Data) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rclunk(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

    async fn rremove(&self, _: &FId<Self::FId>) -> Result<FCall> {
        Err(error::Error::No(EOPNOTSUPP))
    }

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
