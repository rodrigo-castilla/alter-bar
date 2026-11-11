# shoji-bar-4 notes

Practical notes for running [shoji-bar-4](https://github.com/bea4dev/shoji-bar-4) (or the
[plugin-based fork](https://github.com/rodrigo-castilla/shoji-bar-4-fork)) next to Omarchy, switched
with `alter-bar`.

## Changing settings (scale, colours, dock, clock)

Everything is in `~/.local/state/shoji-bar-4/settings.json`. It is read **at startup**; restart
afterwards with `shoji-bar-4 msg quit && alter-bar set shoji-bar-4`.

| Key | Values |
|---|---|
| `ui_scale` | `1.0` normal · `0.85` compact · `0.75` very compact (range 0.5–2.0, only in builds that support it) |
| `seed` | Base colour `#rrggbb` |
| `palette` | `tonal-spot` · `vibrant` · `expressive` · `fidelity` · `neutral` · `monochrome` |
| `mode` | `dark` · `light` · `auto` |
| `pinned` | `.desktop` IDs of the dock, in order (`ls /usr/share/applications`) |
| `clock_24h`, `clock_seconds` | Clock |

A quick change of scale:

```sh
f=~/.local/state/shoji-bar-4/settings.json
shoji-bar-4 msg quit   # close first: otherwise shoji rewrites the file on exit
jq '.ui_scale=0.8' $f > $f.tmp && mv $f.tmp $f
alter-bar set shoji-bar-4
```

Theme colours on the fly: `shoji-bar-4 msg theme dark|light|auto|toggle`.

## Hyprland keybindings

Add binds that call `alter-bar toggle`, `shoji-bar-4 msg toggle launcher`,
`shoji-bar-4 msg toggle quick-settings`, ... in your Hyprland bindings file, written the way the
existing binds in it are.
