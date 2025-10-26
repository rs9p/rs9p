# rs9p

Tokio-based asynchronous filesystems library using 9P2000.L protocol, an extended variant of 9P from Plan 9.

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

## License

rs9p is distributed under the BSD 3-Clause License.
See LICENSE for details.
