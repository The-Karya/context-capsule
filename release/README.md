# Making a release

One version number for the whole product. Releases are tagged `vX.Y.Z` on GitHub. The apps
check one signed file, `latest.json`, and update themselves.

## 1. Bump the version

```sh
release/bump-version.sh 0.2.0
```

Sets it in `capture/build.sh` (Mac), `capture-rs/Cargo.toml` + `Cargo.lock` (Windows, Linux),
`mcp/package.json`, `mcp/extension/manifest.json` and `mcp/src/server.ts`. Commit it.

## 2. Build each platform into `dist/`

- **Claude extension first:** `mcp/extension/build.sh` (the Mac app bundles it).
- **Mac:** `capture/package.sh` → `dist/Context-Capsule-0.2.0-mac.dmg` and `-mac.zip`.
  For a build anyone can open, set `DEVELOPER_ID` ("Developer ID Application: …") and
  `NOTARY_PROFILE` (made once with `xcrun notarytool store-credentials`): it then signs with the
  hardened runtime, notarizes and staples. Without them it uses the local "Context Capsule Dev"
  certificate. Keep using the same one: the app only installs updates signed like itself.
- **Windows:** see [capture-rs/windows/README.md](../capture-rs/windows/README.md); copy the
  result to `dist/Context-Capsule-0.2.0-windows.exe`.
- **Linux:** see [capture-rs/linux/README.md](../capture-rs/linux/README.md); copy the result to
  `dist/Context-Capsule-0.2.0-x86_64.AppImage`.

Any subset is fine: a platform left out simply gets no update.

## 3. Make the release

```sh
NOTES="One or two plain sentences about what's new." release/make-release.sh 0.2.0
```

It writes `dist/latest.json` (download links, SHA-256 and size of each file), signs it into
`latest.json.sig` with the key in `~/.context-capsule-release/update-key.private`, checks the
signature, adds stable-named copies for the download page (`Context-Capsule-mac.dmg`,
`Context-Capsule-windows.exe`, `Context-Capsule-x86_64.AppImage`), and prints the
`gh release create` command. It creates nothing on GitHub: read `latest.json`, then run the
command yourself.

The signing key never goes in the repo. Its public half is built into every app; if the key
is lost, a new one means everyone downloads the next version by hand.

## The update contract

- `latest.json`: `version`, `date`, `notes`, and `mac` / `windows` / `linux`, each
  `{ "url", "sha256", "size" }`. `latest.json.sig`: base64 of the raw 64-byte Ed25519
  signature over the exact bytes of `latest.json`.
- Apps fetch both from `https://github.com/The-Karya/context-capsule/releases/latest/download/`
  (or `update_feed` in `config.json`). They check the signature first, then the version (only
  newer), then download, check size and SHA-256, and install only on "Restart now". Never
  while contextualizing or during a voice note.
- The download page is `site/index.html`; its buttons use the stable file names.

## Check a release by hand

```sh
swift release/sign.swift verify WHTBrAjhTOQIG3S15PKYqHHdj5nbon4J/eEam7LrXd4= dist/latest.json dist/latest.json.sig
"capture/build/Context Capsule.app/Contents/MacOS/ContextCapsule" --test-update \
  dist/latest.json dist/latest.json.sig dist/Context-Capsule-0.2.0-mac.zip
```

The second runs every check a Mac makes (signature, version, size, SHA-256, unzip, same signer)
with an older build, and installs nothing.
