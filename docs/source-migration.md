# Clean source baseline

Zerus starts with a fresh source snapshot and a new Beads database. Previous Git
and issue histories are not part of this repository. Original source, tests,
prototypes and artwork are retained; personal example values and deployment
defaults have been replaced with synthetic data or explicit configuration.

The source baseline retains CLI version 1.46.0 and GUI version 0.36.1. Repository
naming does not migrate runtime state. Existing `hgs` commands, `HGS_*` variables,
`~/.config/hgs` and `~/.local/state/hgs` locations, service/bundle identifiers and
Qt organization name are compatibility interfaces, not private host defaults.

## Deliberately retained identifiers

- `ufna/zerus` is the repository's GitHub owner/name. New Git and tracker
  attribution uses a GitHub no-reply address.
- `com.hgdev.hgs*`, `/org/hgdev/Zerus/LauncherEntry` and the Qt organization name
  retain application identity and preferences on existing installations.
- `arch`, `mac`, `/workspace/`, `/Users/example`, `/Users/test` and reserved example
  domains in fixtures describe synthetic machines and paths.
- Third-party copyright and license attributions are preserved.

## Review boundaries

Source review includes tracked text, deployment defaults, test fixtures, prototype
payloads, asset metadata, secret scanning and fresh tracker content. Local account
stores, SSH files and live session databases are not copied into this repository.
A clean scan is evidence about the reviewed snapshot, not a guarantee about future
commits. Run the checks in `CONTRIBUTING.md` before publishing changes.

The fresh tracker retains verified open engineering work and concise product
constraints. Completed discussions and private operations history stay in the
predecessor archive. Beads is synchronized through its own `refs/dolt/data`; all
future tracker text must be suitable for public collaboration as well.

Public binary packaging and exact dependency notices remain tracked release work.
The repository remains private until the owner explicitly chooses to publish it.
