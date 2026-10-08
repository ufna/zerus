# Install Zerus

[Product overview](../README.md) | [Detailed reference](reference.md#installation)

Zerus runs on Linux and macOS. The primary installation path is currently a source
build. The CLI keeps the name `hgs`; the desktop executable is `hgs-tray`.

## Requirements

For the CLI: Rust 1.85+, a C compiler, Python 3, tmux 3.7+ and OpenSSH. Install the
agents you want to use on the machines where they will run.

The ADE's verified answer/message delivery needs tmux 3.7's input-mode inspection.
Some distributions ship older tmux packages; use a supported upstream release.
Installing a newer binary does not replace an already running tmux server. Check
the server with `tmux display-message -p '#{version}'`; schedule any server
migration deliberately after saving your sessions. Never kill a live server or
native agents as part of a Zerus GUI/package update.
[Official tmux releases](https://github.com/tmux/tmux/releases).

For the desktop: CMake, a C++17 compiler and Qt 6 with Widgets and WebEngine.
Linux also needs KDE Frameworks 6 KStatusNotifierItem and KWindowSystem, a system
tray and user systemd for the standard autostart setup. On macOS, the installer
uses Qt from Homebrew.

## Local installation

```sh
git clone https://github.com/ufna/zerus.git
cd zerus
./install.sh
./tray/install.sh
```

On macOS, install the dependencies first:

```sh
brew install rust python tmux cmake qt qtwebengine
```

On Arch Linux, desktop dependencies include `cmake`, `qt6-base`, `qt6-webengine`, `qt6-svg`,
`kstatusnotifieritem`, `kwindowsystem` and C++ build tools.

Add `~/.local/bin` to `PATH`. The CLI installs to `~/.local/bin/hgs`; SSH calls
use that exact path. The desktop installs to `~/.local/bin/hgs-tray` on Linux or
`~/Applications/hgs-tray.app` on macOS. The installer configures autostart through
systemd or launchd.

Open the workspace from the command line:

```sh
hgs-tray --sessions                         # Linux
~/Applications/hgs-tray.app/Contents/MacOS/hgs-tray --sessions  # macOS
```

The repository's `tmux.conf` is a suggested configuration. If you already have
your own, merge the settings you need manually: `hgs` uses the machine's shared
tmux server.

## Arch package preparation

An Arch system bundle and AUR recipes are prepared, but the packages are **not yet
published**. See [CI and Arch publication](ci-and-aur.md) for candidate builds and
the first-publication requirements. The selected packages are `zerus` (stable sources),
`zerus-git` (upstream main) and `zerus-ade-bin` (stable binary). All include the CLI
and desktop and conflict with one another. The first public product version is
0.37.0. GitHub and AUR publication use a separate manual workflow consuming the
reviewed candidate without rebuilding.

After installing a reviewed pacman package, run `zerus-setup` once as your normal
user to create the `~/.local/bin/hgs` compatibility links used by SSH. Existing
source-install files are preserved and require deliberate migration. Services
and autostart remain opt-in; package installation does not restart native agents.

## Other machines

Add an SSH connection in **Machines**. The agent's machine needs `hgs` installed
at `~/.local/bin/hgs`, the selected agent and the project folder. SSH settings and
keys stay local.

To install on a Mac from another machine:

```sh
./mac-install.sh <ssh-alias>            # CLI and desktop
./mac-install.sh <ssh-alias> --no-tray  # CLI only
```

The CLI configuration lives at `~/.config/hgs/config`. For terminal use, you can
set SSH aliases directly:

```sh
HGS_SELF="workstation"
HGS_PEERS="macbook build-01"
```

Use **Projects** to collect folders from different machines into one project.
The shared project catalog syncs between connected members; local SSH access is
managed in **Machines**.

## Agents

Use **Accounts** to configure accounts and install supported agents.
On Arch, installing Codex or DeepSeek from Accounts requires both the optional
`nodejs` and `npm` packages. For a fresh npm setup, choose a writable user prefix
before installing agents; Zerus already recognizes `~/.local/bin`:

```sh
sudo pacman -S --needed nodejs npm
npm config set prefix "$HOME/.local"
```

Keep an existing working npm prefix if you already manage native agents there.
The current official DeepSeek Harness adapter is pinned to
`@deepseek-ai/dsh@0.2.0-rc.2`:

```sh
npm install -g @deepseek-ai/dsh@0.2.0-rc.2
hgs account login native-dsh
```

Then choose **DeepSeek Harness** in **New session**.
[Adapter capabilities and limitations](reference.md#deepseek).

See the [reference](reference.md) for commands and operational details, and
[CONTRIBUTING.md](../CONTRIBUTING.md) for build checks.
