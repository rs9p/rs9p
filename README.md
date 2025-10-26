# rs9p

Tokio-based asynchronous filesystems library using 9P2000.L protocol, an extended variant of 9P from Plan 9.

## unpfs

`unpfs` is the reference implementation of a file server which exports your filesystem.
You can build unpfs with the following commands below:

```bash
cd example/unpfs/
cargo build --verbose --release
```

and run unpfs with the following command to export `/exportdir`:

```bash
# TCP
cargo run --release -- 'tcp!0.0.0.0!564' /exportdir

# Unix domain socket:
#  port number is a suffix to the unix domain socket
#  'unix!/tmp/unpfs-socket!n' creates `/tmp/unpfs-socket:n`
cargo run --release -- 'unix!/tmp/unpfs-socket!0' /exportdir

# With maximum depth limit (prevents infinite recursion)
cargo run --release -- --max-depth 100 'tcp!0.0.0.0!564' /exportdir
```

**Important:** If you mount the filesystem inside its own export directory, you can create infinite recursion (e.g., exporting `/home/user` and mounting at `/home/user/mnt` creates `/home/user/mnt/mnt/mnt/...`). Use the `--max-depth` option to prevent this:

```bash
cargo run --release -- --max-depth 50 'tcp!0.0.0.0!564' /exportdir
```

You are now ready to import/mount the remote filesystem.
Let's mount it at `/mountdir`:

```bash
# TCP
sudo mount -t 9p -o version=9p2000.L,trans=tcp,port=564,uname=$USER 127.0.0.1 /mountdir
# Unix domain socket
sudo mount -t 9p -o version=9p2000.L,trans=unix,uname=$USER /tmp/unpfs-socket:0 /mountdir
```

| Mount option | Value                                              |
| ------------ | -------------------------------------------------- |
| version      | must be "9p2000.L"                                 |
| trans        | an alternative v9fs transport. "tcp" or "unix"     |
| port         | port to connect to on the remote server            |
| uname        | user name to attempt mount as on the remote server |

See [v9fs documentation](https://www.kernel.org/doc/Documentation/filesystems/9p.txt) for more details.

## License

rs9p is distributed under the BSD 3-Clause License.
See LICENSE for details.
