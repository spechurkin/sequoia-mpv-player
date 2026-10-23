# Third-party components

Online playback uses yt-dlp (https://github.com/yt-dlp/yt-dlp) and Deno
(https://github.com/denoland/deno). Packaging downloads their latest stable releases, verifies
SHA-256 digests and caches release metadata for seven days. The player never updates these
tools at startup. `--refresh-web-tools` refreshes the build cache explicitly. Packages include
the upstream license texts and `yt-dlp/source-build.json` recording versions and hashes.
The official standalone yt-dlp release includes yt-dlp-ejs; Deno supplies its JavaScript runtime.

Sequoia's Rust application source is MIT licensed. Rust dependencies retain their individual licenses; Cargo.lock
records the exact dependency versions.

mpv is a separate component. Its license and those of its linked FFmpeg components depend on the supplied build. The
Windows mpv build is copied unchanged when creating a portable package. Unix packages include the engine and its shared
libraries; macOS library paths are rewritten for relocation. A bundled binary is not covered by the
Sequoia MIT license.

mpv project, copyright and licensing: https://github.com/mpv-player/mpv

Windows builds and corresponding build recipes: https://github.com/shinchiro/mpv-winbuild-cmake

FFmpeg licensing: https://ffmpeg.org/legal.html

The package script includes the local mpv documentation if present. Before publishing a binary distribution, include the
exact build's license texts, copyright notices and corresponding source / source offer as required by that build's
licenses. The generated local package is for development verification; the script does not produce or verify a complete
source archive for third-party binaries.

Windows packaging uses the shinchiro 20261021 build (mpv v0.41.0-1227-ge471496c5), pinned in scripts/mpv-version.json.
That file records the download URLs and SHA-256 hashes for x86_64 and aarch64, verified against the release metadata
when pinning the version. Packaging does not query the latest-release API. A verified cached archive can be reused.
The package includes a source-build.json with the pinned release, version, download URL, checksum and build-recipe URL.
The build-only tools/unpack-mpv helper extracts the archive using sevenz-rust2 (Apache-2.0), without an external 7-Zip
executable. Its dependencies are recorded separately in tools/unpack-mpv/Cargo.lock; the helper is not shipped to users.
Linux CI takes mpv and its linked libraries from the target Ubuntu distribution and copies distribution copyright
notices. Apple Silicon macOS CI uses Homebrew mpv with dylibbundler and includes installed formula license files where available.
Intel macOS CI uses the prebuilt mpv 0.35.0 bundle from https://laboratory.stolendata.net/~djinn/mpv_osx/,
the upstream-listed distributor, to retain macOS 10.14 compatibility without compiling Homebrew dependencies.
Its archive and SHA-256 are pinned in scripts/engine-runtime.json; its bundled library paths are preserved.
Windows packages also include LunarG's Vulkan Loader 1.4.363.0 alongside mpv, including its license and
download provenance. The x64 and ARM64 runtime archives are pinned in scripts/engine-runtime.json.
Vulkan Loader source and licensing: https://github.com/KhronosGroup/Vulkan-Loader
These build records do not replace the corresponding-source requirements above.

Metadata services: MusicBrainz (https://musicbrainz.org), Cover Art Archive (https://coverartarchive.org). Availability
and cover-art rights are independent of this application's license.
