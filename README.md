# Sequoia

Sequoia is a desktop media player that brings video, animated GIFs, and a music library into one application. Play a
film, organize your albums, or open an online playlist without switching players. Sequoia uses mpv for playback and is
built in Rust with egui.

The Windows package includes the playback engine and tools for opening online media. Extract it and start watching or
listening. You do not need to install Rust, mpv, or a separate codec pack.

## What you can do

- Play local video, music, and animated GIFs. Open several files at once, add a folder with its subfolders, or drag
  files in from Explorer.
- Open links to online video, audio, and playlists. Online playlists expand into the playback queue, with loading
  progress and a cancel option.
- Choose audio tracks and subtitles, load external subtitles, jump between chapters, and change playback speed. Step
  through video frame by frame or save a PNG screenshot with subtitles.
- Browse a searchable music library. Group tracks by artist, album, genre, year, disc, or format, and save your
  preferred grouping.
- Build playlists, rearrange tracks by dragging, and import or export M3U/M3U8 files. Use shuffle, repeat one track, or
  repeat the whole queue.
- Edit tags and cover art for one track or a selection. Look up track information in MusicBrainz and artwork in the
  Cover Art Archive, then review the result before saving.
- Keep listening while the window is hidden in the Windows notification area. Use the tray menu to pause, skip tracks,
  or stop playback.

## 1. Download Sequoia

Open this repository's **Releases** page and choose a release. Under **Assets**, download the archive for your computer,
rather than GitHub's source-code archive.

For Windows:

- On Windows 10 or 11 with an Intel or AMD 64-bit processor, choose `Sequoia-windows-2022-x86_64-pc-windows-msvc.zip`.
- On Windows 11 with an ARM64 processor, choose `Sequoia-windows-11-arm-aarch64-pc-windows-msvc.zip`.

The `windows-2022` part identifies the build environment. That package is intended for Windows 10 and 11.

The build workflow also packages macOS and Ubuntu versions. For macOS, choose the archive matching Apple Silicon (
`aarch64`) or Intel (`x86_64`). For Ubuntu, match both your distribution version and processor architecture. These
packages use `.tar.gz` archives. The Windows registration and update scripts below apply only to Windows.

If the repository has no published release yet, you can build a Windows package from source using the instructions at
the end of this document.

## 2. Extract and launch

1. Extract the entire Windows ZIP archive to a folder you can keep, such as `%LOCALAPPDATA%\Programs\Sequoia`. You can
   paste that path into Explorer's address bar.
2. Open the extracted application folder and run `Sequoia.exe`.
3. Documentation and licenses are in `docs`; Windows installation and update scripts are in `installers`. The only file
   directly in the application folder is `Sequoia.exe`.
4. Keep the bundled `mpv` and `yt-dlp` folders beside the executable. They contain the playback engine and online media
   tools.

You can use Sequoia from this folder without registering it with Windows. Application settings and playlists are stored
separately in your Windows profile, so this is a portable application folder rather than a self-contained user profile.

For a macOS archive, extract the complete package and open `Sequoia.app`. For an Ubuntu archive, extract it and run the
included `sequoia` executable. Keep the bundled folders in their original locations.

## 3. Add Sequoia to Windows

This step is optional. It adds a Start menu shortcut, an entry in installed apps, and Sequoia as a choice in **Open with
** and **Default apps**.

1. Put the extracted application folder in its permanent location.
2. Run `installers\install.cmd` inside that folder. The script registers the files where they are; it does not copy them
   elsewhere.
3. When Windows opens **Default apps**, choose Sequoia for the audio and video formats you want it to open. On Windows
   10, you may need to find Sequoia in the application list first.

Windows asks you to confirm default-app choices through its settings. Running the script does not automatically replace
your current defaults.

If you move the application folder later, run `installers\install.cmd` again from the new location so the shortcut and
file-opening paths point to the right place.

## 4. Play your first file

1. Use the **Open** menu to choose a file or folder, or drag files from Explorer into the window.
2. Use the playback bar to play, pause, seek, or adjust the volume.
3. In video mode, choose the audio track and subtitles you need. You can also load a separate subtitle file.
4. To play online media, choose the option to open a link and paste a video, audio, or playlist URL. On Windows, you can
   also drag a browser link or a `.url` file into the window.

Online playback uses the bundled yt-dlp and Deno tools. Availability depends on the source website and your network
connection.

Useful shortcuts:

- `Space`: play or pause.
- `Left` / `Right`: seek backward or forward.
- `Shift+Left` / `Shift+Right`: move one video frame backward or forward.
- `S`: save a screenshot.
- `M`: mute or unmute.
- `F`: toggle fullscreen; `Escape`: leave fullscreen or close a dialog.
- `Ctrl+O`: open files; `Ctrl+Shift+O`: open a folder.

Over the video, right-click toggles pause and the mouse wheel adjusts volume. You can customize playback bindings in
`%APPDATA%\Sequoia\mpv\input.conf`. Apply changes by reconnecting the engine in settings or restarting Sequoia.

## 5. Set up your music library

1. Switch to music mode. Sequoia uses your system Music folder by default; choose another library folder in settings if
   needed.
2. Scan the library. Music scans include subfolders. If you change tags outside Sequoia, rescan to refresh the catalog.
3. Search for a track or browse the collection tree. Choose a grouping preset or configure up to three grouping levels.
4. Drag tracks or groups into the central playlist. Drag rows to change their order. Use `Ctrl` to select several tracks
   or `Shift` to select a range in a playlist.
5. Save the queue as a playlist, or create a new playlist and add tracks to it. Saved playlists appear in the sidebar,
   and changes to them are saved automatically. Closing a playlist tab keeps the playlist available in the sidebar.

To correct track information, select one or more tracks and open the tag editor. You can change shared fields, assign
cover art, or number tracks in their displayed order. Unchanged fields keep their existing values. Tag-writing support
depends on the file format; Sequoia can play some formats that the tag editor cannot modify.

MusicBrainz searches send the track title and artist when you request a lookup, or when you open the editor with
automatic lookup enabled in settings. You choose what to save to your files.

## 6. Keep listening in the background

On Windows, closing the window or pressing `Alt+F4` hides Sequoia in the notification area by default. Playback and
queue advancement continue.

Click the tray icon to bring the window back. Right-click it for playback controls or choose the exit command to close
Sequoia completely. You can disable hiding on close in settings. The minimize button sends the window to the taskbar as
usual.

## 7. Update Sequoia

For a Windows package downloaded from a release:

1. Exit Sequoia through the tray menu.
2. Run `installers\update.cmd` in the application folder.
3. Wait for the updater to finish, then launch `Sequoia.exe` again.

The updater downloads the latest stable release for your architecture and verifies its SHA-256 checksum, version, and
architecture before replacing package files. Your library and playlists remain in your profile. It also updates the
bundled mpv, yt-dlp, and Deno versions supplied with that Sequoia release.

To check for an update without installing it, open PowerShell in the application folder and run:

```powershell
.\installers\update.cmd -Check
```

To enable a daily update check and installation at 12:00 local time:

```powershell
.\installers\update.cmd -InstallTask
```

The task runs for the current user without administrator rights. If Sequoia or one of its bundled tools is running, it
waits until a later scheduled run instead of closing the player. The application folder must be writable. To disable the
task:

```powershell
.\installers\update.cmd -RemoveTask
```

If you move the application folder, remove the old task and enable it from the new location. Background update failures
are recorded in `installers\update-error.log`.

Local builds may need an explicit release source. Replace `OWNER/REPOSITORY` with the GitHub repository that publishes
your packages:

```powershell
.\installers\update.cmd -Repository OWNER/REPOSITORY -InstallTask
```

For macOS and Ubuntu packages, download and extract a new release manually.

## Your files and settings

On Windows, Sequoia stores its settings, catalog, and internal playlists in `%APPDATA%\Sequoia\library.json`. Tag
backups are kept in `%APPDATA%\Sequoia\tag-backups` and are not deleted automatically. Restore a backup by copying it
over the corresponding media file after closing playback.

Advanced playback settings live in `%APPDATA%\Sequoia\mpv\mpv.conf`. You can edit this configuration from Sequoia's
settings, add Lua scripts to its `scripts` subfolder, or use the JSON IPC console for mpv commands. These options let
you customize mpv beyond the controls exposed in the player.

To remove Windows registration, run `installers\uninstall.cmd` or uninstall Sequoia from Windows settings. This removes
the shortcut, application registration, and scheduled update task while keeping the application files and your library
data. After exiting the player, delete the application folder manually if you no longer need it. If Sequoia was your
default player, choose another application in Windows settings.

## Build from source

For a Windows build, install Rust 1.88 or newer with the MSVC toolchain and MSVC Build Tools. Python is also required to
create a distributable package.

From the repository folder:

```powershell
cargo run --locked
```

You can pass a media file on the command line:

```powershell
cargo run --locked -- "D:\Media\film.mkv"
```

Development runs need an available mpv executable. Choose its path in settings, place it in `mpv/mpv.exe` beside the
application, or make it available through `PATH`. Online playback also needs yt-dlp and Deno, either in the adjacent
`yt-dlp` folder or on `PATH`.

To build a Windows package with its dependencies bundled:

```powershell
.\scripts\package.ps1
```

The script builds Sequoia and downloads the required playback and online media tools. It writes
`dist/Sequoia-windows-x64.zip`. The Windows mpv download is pinned and verified against the checksum in
`scripts/mpv-version.json`. An installed copy of 7-Zip is not required.

To package an existing local mpv installation instead:

```powershell
.\scripts\package.ps1 -MpvDirectory 'C:\Program Files\mpv'
```

Run the standard Rust tests with:

```powershell
cargo test --locked
```

The repository also contains integration and desktop regression checks in `tests` and fixture generation in
`scripts/fixtures.ps1`. Tests that play media require the corresponding engine and fixtures; a passing unit test suite
alone does not establish desktop playback behavior.

The [build workflow](.github/workflows/build.yml) defines the Windows, macOS, and Ubuntu build targets and release
packaging.

## License and dependencies

Sequoia is released under the [MIT License](LICENSE). Bundled software has its own licenses;
see [THIRD_PARTY.md](THIRD_PARTY.md) for details.

Playback uses [mpv](https://mpv.io/), the desktop interface uses [egui/eframe](https://github.com/emilk/egui), and tag
editing uses [Lofty](https://github.com/Serial-ATA/lofty-rs). Online metadata comes
from [MusicBrainz](https://musicbrainz.org/) and the [Cover Art Archive](https://coverartarchive.org/).
