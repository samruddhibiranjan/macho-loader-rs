# MachO Loader

A reflective loader for Mach-O executables

# Usage
```bash
git clone git@github.com:pauldcs/macho-loader-rs.git
cd macho-loader-rs/example
make
./execvm_example /bin/echo "Hello from memory"
Hello from memory
```

The goal of this project is to implement a function similar to execve, but operating entirely in memory by manually loading and executing a program. Compared to execve, this approach is inferior in almost every way, though it can run most command-line programs on macOS. Objective-C programs are not yet supported, but work on that is ongoing.

Building this project creates a `libexecvm.dylib` that exports a function that you can use to run your
programs.

```C
void execvm(
  uint32_t ac,
  const char *av[],
  const char *ep[],
  const uint8_t *data, // pointer to the start of the Mach-O
  size_t len, // the total size of the image
);
```

If the target program you want to run is not fully static, the loader expects it to be compiled with something like `-macosx-version-min=12.0`. This is because it currently only supports dyld chained fixups for relocations, the default Mach-O relocation format since macOS 12 and iOS 15.
