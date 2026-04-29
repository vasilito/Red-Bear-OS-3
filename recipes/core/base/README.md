# Base

Repository containing various system daemons, that are considered fundamental for the OS.

You can see what each component does in the following list:

- audiod : Daemon used to process the sound drivers audio
- bootstrap : First code that the kernel executes, responsible for spawning the init daemon
- daemon : Redox daemon library
- drivers
- init : Daemon used to start most system components and programs
- initfs : Filesystem with the necessary system components to run RedoxFS
- ipcd : Daemon used for inter-process communication
- logd : Daemon used to log system components and daemons
- netstack : Daemon used for networking
- ptyd : Daemon used for pseudo-terminal
- ramfs : RAM filesystem
- randd : Daemon used for random number generation
- zerod : Daemon used to discard all writes and fill read buffers with zero

## How To Contribute

To learn how to contribute you need to read the following document:

- [CONTRIBUTING.md](https://gitlab.redox-os.org/redox-os/redox/-/blob/master/CONTRIBUTING.md)

If you want to contribute to drivers read its [README](drivers/README.md)

## Development

To learn how to do development with these system components inside the Redox build system you need to read the [Build System](https://doc.redox-os.org/book/build-system-reference.html) and [Coding and Building](https://doc.redox-os.org/book/coding-and-building.html) pages.

### How To Build

It is recommended to build this system component via the Redox build system, you can learn how to do it on the [Building Redox](https://doc.redox-os.org/book/podman-build.html) page.

To build and test outside the build system, [install redoxer](https://doc.redox-os.org/book/ci.html) then use `check.sh` script to build or test:
- `./check.sh` - Check build for x86_64
- `./check.sh --arch=ARCH` - Check build for specific ARCH (`aarch64`, `i586`, `riscv64gc`)
- `./check.sh --all` - Check build for all ARCH
- `./check.sh --test` - Check the base system boots up on x86_64

You can also use `make install` to inspect the content on `./sysroot`, or `make test-gui` to test booting with orbital interactively.
