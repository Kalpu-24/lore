# Build and run the desktop client

Build the Lore desktop client, `lore-desktop`, from this repository and open a repository instance
in it. Use this guide when you want the graphical client rather than the `lore` CLI, or when you're
working on the client itself. It assumes you have a working tree of the Lore repository and an
instance to open.

## Prerequisites

- The Rust stable toolchain, as [Set up, build and test Lore](../developing/getting-started.md)
  describes.
- On Windows, the Visual Studio C++ build tools.
- On Linux, the windowing headers. On Debian or Ubuntu:

  ```bash
  sudo apt-get install libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev libxkbcommon-dev
  ```

- A repository instance: a working tree created with `lore repository create` or `lore clone`.

## Steps

1. **Build the client.**

   ```bash
   cargo build --release -p lore-desktop
   ```

   The binary is `target/release/lore-desktop`, or `target\release\lore-desktop.exe` on Windows.

2. **Open an instance.** Give the instance's path on the command line:

   ```bash
   target/release/lore-desktop ~/my-project
   ```

   Or start the client with no path, type the path into the **Instance** field and press **Refresh**.

3. **Open more instances.** Press **+** beside the tabs for another tab, and **×** to close one. The
   client remembers the open instances when it closes, and opens them again next time. It keeps them
   in its per-user data directory:

   | Platform | Directory |
   | --- | --- |
   | Windows | `%APPDATA%\Lore\data` |
   | macOS | `~/Library/Application Support/Lore` |
   | Linux | `~/.local/share/lore` |

4. **Run the client's tests** when you change it:

   ```bash
   cargo test --release -p lore-desktop --test unit
   ```

## Result

The client opens on the instance, with its branch, its remote state and its changed files in the
**Changes** view.

## Prebuilt archives

The **Desktop client** workflow builds the client on Linux, Windows and macOS for every push to
`main` and every pull request that changes code. Each run keeps an archive per platform, holding the
binary and the licence, under its **Artifacts**. The archives aren't signed, and the macOS one holds
a bare binary rather than an application bundle.

## See also

- [Work in the desktop client](../tutorials/work-in-the-desktop-client.md) — the daily loop in the
  client, step by step.
- [Open source Rust desktop client](../proposals/2026-11-19-open-source-rust-desktop-client.md) —
  what the client covers and why it's built the way it is.
