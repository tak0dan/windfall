# Snowhush for Hyprland

Snow, autumn leaves, cherry blossom, rain and flowers on your Hyprland desktop. Written from scratch in Rust
for Wayland; inspired by the macOS app of the same name.

* **Effects** – snow, leaves, blossom and rain fall and pile up on window tops, bars and the screen's edge.
  Flowers instead grow in a thick lawn of tiny blooms along the same edges; they sway when the cursor
  brushes them and stretch up while it stays, then settle back.
* **Two renderers** – *smooth* (soft, anti-aliased) and *pixel art* (coarse grid, limited palette, hard edges;
  pixel size and colour count are adjustable). The controller restyles itself to match.
* **Tray icon** (StatusNotifier) – left click opens the controller, middle click toggles, right click has the menu.
* **Controller** (GTK4 + libadwaita) – live preview, themed after the running effect.
* **Bars** – transparent bars (Waybar modules with frames, Quickshell pills…) only hold things on the parts
  that are actually painted. Snowhush looks at the screen (wlr-screencopy) to find them, and re-checks every
  minute. Choose *Whole bar* in the controller (or `snowhushctl bars whole`) to disable that.
* **Hyprland** – window geometry, workspaces, fullscreen, rounded corners and borders come from Hyprland's IPC.
  Snow on a window waits while its workspace is off screen, slides off gradually when the window is moved, and
  drops when it closes. Clicks pass straight through the overlay. Fractional scaling is supported.

## Install

Arch / AUR:

```sh
git clone https://github.com/tak0dan/snowhush-linux && cd snowhush-linux/packaging
makepkg -si            # or, once published: yay -S snowhush-git
```

This installs `snowhushd`, `snowhushctl`, `snowhush-gui`, a system-wide desktop entry
(**Snowhush Controller**), the icon and a systemd user unit.

Start it with your session. Either

```sh
systemctl --user enable --now snowhushd      # needs a systemd-aware Hyprland session (e.g. uwsm)
```

or in `hyprland.conf`: `exec-once = snowhushd` (Lua config: `hl.exec_cmd("snowhushd")`).

## Use

```sh
snowhushctl effect flowers
snowhushctl renderer pixel
snowhushctl intensity 70
snowhushctl bars visible|whole
snowhushctl status --json
```

Settings live in `~/.config/snowhush/config.toml`. The control socket is `$XDG_RUNTIME_DIR/snowhush.sock`
(one JSON request per line; see `crates/core/src/ipc.rs`).

## Notes

* The overlay lives on the compositor's overlay layer, so screenshots and screen sharing include it.
* Needs `wlr-layer-shell` and `wlr-screencopy` (Hyprland has both) and OpenGL ES (Mesa).
* Rotated monitors and mirrored outputs are not handled specially yet.

## Licence

GPL-3.0-or-later. This is an independent implementation; no code from the original project is used.
