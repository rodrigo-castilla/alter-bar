# alter-bar

> **Alpha software.** `alter-bar` works on the author's machine (Omarchy + Hyprland) and has not
> been tested anywhere else. Commands, file formats and defaults may change without notice, and
> there is no crates.io release.

`alter-bar` switches the top bar of an [Omarchy](https://omarchy.org) / Hyprland desktop between
several named bars with one command:

- **`quickshell`**: the stock Omarchy shell bar. Built in.
- **`shoji-bar-4`**: [shoji-bar-4](https://github.com/bea4dev/shoji-bar-4) in `~/.config/shoji-bar-4`.
  Built in.
- **Any other shoji project** (for example the
  [plugin-based fork](https://github.com/rodrigo-castilla/shoji-bar-4-fork)) or any executable,
  registered with `alter-bar set-bar <name> <path>`.

Only one bar runs at a time: choosing a shoji bar stops Quickshell, and choosing Quickshell stops
shoji. A shoji bar has to answer what Omarchy sends to its shell (the fork does this for media keys
and the on-screen display so far, see its `TODO.md`); until a bar covers everything you use, set
`ALTER_BAR_COEXIST=1` and Quickshell is kept alive with only its bar hidden (what earlier versions
always did). The choice is saved and restored at login.

## Requirements

- Omarchy (it uses `omarchy-shell`, `omarchy-toggle-bar`, `omarchy-launch-shell`) with Hyprland.
- A Rust toolchain (edition 2024, so Rust 1.85 or newer) to build it.
- For the shoji bars: a built `shoji-bar-4` (see their READMEs).

## Install (manual)

It is not published on crates.io, so build it from source:

```sh
git clone https://github.com/rodrigo-castilla/alter-bar
cd alter-bar
cargo install --path .        # puts `alter-bar` in ~/.cargo/bin
alter-bar --version
```

or, without `cargo install`:

```sh
cargo build --release
install -Dm755 target/release/alter-bar ~/.local/bin/alter-bar
```

Make sure the directory is in your `PATH` (and in the PATH Hyprland starts programs with).

Then restore the chosen bar at login by adding this to your Hyprland autostart:

```
alter-bar apply
```

(for Omarchy's Lua config, something like `o.launch_on_start(os.getenv("HOME") .. "/.cargo/bin/alter-bar apply")`).

## Usage

```
alter-bar list                     list the bars; `*` marks the chosen one
alter-bar toggle | next            switch to the next bar in the list (rotates)
alter-bar prev                     switch to the previous one
alter-bar set <name>               choose a bar
alter-bar set-bar <name> <path>    register a shoji bar (a project directory or an executable)
alter-bar rename <old> <new>       give a bar another name (`quickshell` keeps its own)
alter-bar remove-bar <name>        remove a registered bar
alter-bar reload                   reload the chosen bar
alter-bar status                   show the chosen bar and what is running
alter-bar apply                    activate the chosen bar (for login)
alter-bar version | --version | -V
```

Bind `alter-bar toggle` to a key in Hyprland to flip between bars.

### Registering a shoji project

```sh
alter-bar set-bar my-bar ~/src/my-shoji-bar     # a directory with a shoji-bar-4 Cargo project
alter-bar set my-bar
```

For a project directory the binary is `target/release/shoji-bar-4` (or `target/fast/shoji-bar-4`
if the project defines a `[profile.fast]`).

### `alter-bar reload`

For a shoji project, `reload` first runs `cargo build` (with the project's `fast` profile when it
has one, `release` otherwise), shows a progress bar, and restarts the bar only if the build
succeeded. If it fails the running bar is left alone and the errors are printed. That makes
editing a [plugin](https://github.com/rodrigo-castilla/shoji-bar-4-fork#plugins) and seeing the result
about a second away.

## Files

| Path | What |
|---|---|
| `~/.local/state/alter-bar/mode` | the chosen bar |
| `~/.config/alter-bar/bars` | registered bars, one `name = path` per line |
| `~/.local/state/alter-bar/build-*` | crate counts of past builds, for the progress bar |

## Notes

- [docs/shoji-bar-4-notes.md](docs/shoji-bar-4-notes.md): practical notes on running shoji-bar-4
  next to Omarchy.

## Status and license

Version `0.4.0-alpha.1`. No license has been chosen yet; until one is added, all rights are
reserved by the author.
