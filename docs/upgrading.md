# Upgrading Jam

The app bundle, CLI, and local daemon are distributed together. Linux servers are versioned separately and included in each release.

## Command line

After placing `Jam.app` in `~/Applications`, install its CLI:

```sh
~/Applications/Jam.app/Contents/Resources/bin/jam install-cli
```

Ensure `~/.local/bin` is on your shell's PATH. The symlink continues to work when Jam replaces its app bundle. Installation refuses to replace an unrelated existing command.

```sh
jam upgrade --check         # Read-only release check
jam upgrade --check --json  # Structured output for scripts
jam upgrade                 # Stage, verify, confirm interruptions, apply
jam update                  # Alias for upgrade
```

`--yes` explicitly approves the interruptions for unattended upgrades. Without it, noninteractive use cannot approve stopping work. Development checkouts are not self-updated.

## In the app

Automatic checks run shortly after launch when due, then wait a day after a successful check. Choose **Jam → Check for Updates…** or use **Settings → General** to check immediately. Automatic checks can be disabled in General settings.

The update notification offers **Upgrade…** and **Skip This Version**. A skipped version stays hidden until a newer release appears; a manual check can show it again. Jam downloads and verifies the update before presenting restart confirmation.

If an upgrade was interrupted, Jam offers recovery with fresh confirmation. Recovery does not reuse earlier permission to upgrade remote machines.

## What restarts

Jam downloads and verifies the signed release and required artifacts before asking to interrupt work. Confirmation identifies affected sessions and machines. The application and affected daemons restart; layouts, working directories, and resumable agent conversations are restored. Cancelled and intentionally paused work stays paused. Ordinary running shell commands can end during a daemon restart.

If sessions change after confirmation, Jam refuses stale approval. Installation failures retain diagnostics and restore the previous local installation when safe. Remote failures are tracked separately because several machines cannot be updated as one atomic transaction. Offline machines stay pending and require fresh confirmation when they return.

## Client and server versions

A client never installs its older bundled server over a newer remote. It must upgrade itself before connecting to that machine. If a suitable client release is unavailable, the remote remains unchanged. Protocol compatibility and source fingerprints do not determine version ordering.

Servers without release metadata require an explicit migration. Stop legacy development daemons deliberately before moving to a managed release installation; ordinary reconnect does not perform this migration.

## Release authenticity

The updater verifies an Ed25519 signature against the public key embedded in Jam, then verifies artifact sizes and SHA-256 digests. macOS app signing is checked independently. Initial releases use ad-hoc signatures, so Apple notarization is not provided yet.

See the [release workflow](../.github/workflows/release.yml) and [dependency notices](dependencies.md) for packaging details.

After a successful desktop upgrade, choose **Open Release Notes** in the
confirmation notification to read that version's GitHub release.
