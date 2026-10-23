#!/usr/bin/env python3
"""Package a Cargo release with a required, verified playback engine."""
import argparse
import hashlib
import json
import os
import plistlib
import re
import shutil
import tarfile
import zipfile
from pathlib import Path

import bundle_mpv
import bundle_web


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', required=True)
    parser.add_argument('--label', required=True)
    parser.add_argument('--minimum', required=True)
    parser.add_argument('--mpv', required=True, help='mpv executable or Windows mpv directory')
    parser.add_argument('--binary', type=Path, help='Override the Cargo release executable')
    parser.add_argument('--output-name', help='Name of the directory and archive in dist')
    parser.add_argument('--repository', default=os.environ.get('GITHUB_REPOSITORY', ''),
                        help='GitHub owner/repo for updates (defaults to GITHUB_REPOSITORY)')
    parser.add_argument('--refresh-web-tools', action='store_true',
                        help='Refresh stable yt-dlp/Deno release metadata now (otherwise cached for 7 days)')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent.parent
    windows = 'windows' in args.target
    macos = 'apple' in args.target
    binary = args.binary or root / 'target' / args.target / 'release' / ('sequoia.exe' if windows else 'sequoia')
    if not binary.is_file():
        raise FileNotFoundError(binary)
    name = args.output_name or 'Sequoia-{}-{}'.format(args.label, args.target)
    if Path(name).name != name or name in ('.', '..'):
        raise ValueError('Output name must be a single directory name')
    stage = root / 'dist' / name
    # A previous package must never contribute stale files to an archive.
    if stage.exists():
        raise FileExistsError(stage)
    stage.mkdir(parents=True)
    executable_dir = stage
    if macos:
        contents = stage / 'Sequoia.app' / 'Contents'
        executable_dir = contents / 'MacOS'
        executable_dir.mkdir(parents=True)
        with (contents / 'Info.plist').open('wb') as handle:
            plistlib.dump({
                'CFBundleName': 'Sequoia',
                'CFBundleIdentifier': 'org.sequoia.player',
                'CFBundleExecutable': 'sequoia',
                'CFBundlePackageType': 'APPL',
                'CFBundleShortVersionString': package_version(root),
                'CFBundleVersion': package_version(root),
                'LSMinimumSystemVersion': args.minimum,
                'NSHighResolutionCapable': True,
            }, handle)
    executable = executable_dir / ('Sequoia.exe' if windows else 'sequoia')
    shutil.copy2(str(binary), str(executable))
    shutil.copytree(str(root / 'locales'), str(executable_dir / 'locales'))
    if not windows:
        executable.chmod(0o755)
    docs = stage / 'docs'
    docs.mkdir()
    for filename in ['README.md', 'LICENSE', 'THIRD_PARTY.md']:
        shutil.copy2(str(root / filename), str(docs / filename))
    if (root / 'BUILDING.md').is_file():
        shutil.copy2(str(root / 'BUILDING.md'), str(docs / 'BUILDING.md'))
    if windows:
        installers = stage / 'installers'
        installers.mkdir()
        for filename in ['updater.ps1', 'update.cmd', 'install.cmd', 'uninstall.cmd', 'windows-integration.ps1']:
            shutil.copy2(str(root / 'scripts' / filename), str(installers / filename))
        (installers / 'windows-file-types.json').write_text(
            json.dumps(windows_file_types(root), indent=2) + '\n', encoding='utf-8')
    engine = bundle_mpv.bundle(args.mpv, executable_dir / 'mpv', args.target)
    engine_version = bundle_mpv.verify(engine.parent, windows)
    web_tools = bundle_web.bundle(executable_dir / 'yt-dlp', args.target, args.refresh_web_tools)
    collect_dependency_docs(executable_dir, docs)
    (docs / 'build-info.json').write_text(json.dumps({
        'version': package_version(root),
        'update_repository': args.repository,
        'update_asset': 'Sequoia-{}-{}.zip'.format(
            'windows-11-arm' if args.target.startswith('aarch64') else 'windows-2022', args.target
        ) if windows else None,
        'managed_files': sorted([p.relative_to(stage).as_posix() for p in stage.rglob('*')
                                 if p.is_file()] + ['docs/build-info.json']),
        'target': args.target, 'build_environment': args.label,
        'minimum_target': args.minimum, 'mpv_bundled': True,
        'mpv_version': engine_version,
        'mpv_sha256': hashlib.sha256(engine.read_bytes()).hexdigest(),
        'web_tools': web_tools,
    }, indent=2) + '\n', encoding='utf-8')
    if windows:
        archive = stage.with_suffix('.zip')
        with zipfile.ZipFile(str(archive), 'w', zipfile.ZIP_DEFLATED) as handle:
            for path in sorted(stage.rglob('*')):
                if path.is_file():
                    handle.write(str(path), str(path.relative_to(stage.parent)))
    else:
        archive = Path(str(stage) + '.tar.gz')

        def archive_permissions(info):
            info.mode = 0o755 if info.isdir() or info.mode & 0o111 else 0o644
            return info

        with tarfile.open(str(archive), 'w:gz') as handle:
            handle.add(str(stage), arcname=name, filter=archive_permissions)
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    Path(str(archive) + '.sha256').write_text(digest + '  ' + archive.name + '\n', encoding='ascii')
    print(archive)


def collect_dependency_docs(executable_dir, docs):
    for name in ['mpv', 'yt-dlp']:
        dependency = executable_dir / name
        for path in list(dependency.iterdir()):
            if (path.name in ('doc', 'licenses') or
                    path.name.startswith(('LICENSE', 'COPYING')) or
                    path.name.endswith('-LICENSE') or path.name == 'source-build.json'):
                destination = docs / name / path.name
                destination.parent.mkdir(parents=True, exist_ok=True)
                shutil.move(str(path), str(destination))


def package_version(root):
    # Python 3.6 on Ubuntu 18.04 has no tomllib.
    for line in (root / 'Cargo.toml').read_text(encoding='utf-8').splitlines():
        if line.startswith('version = '):
            return line.split('"')[1]
    raise ValueError('Cargo package version missing')


def windows_file_types(root):
    source = (root / 'src' / 'catalog' / 'library.rs').read_text(encoding='utf-8')
    result = {}
    for kind, function, next_function in [('audio', 'audio', 'supported'), ('video', 'supported', 'read_media')]:
        body = source.split('pub fn ' + function + '(', 1)[1].split('pub fn ' + next_function + '(', 1)[0]
        result[kind] = sorted({'.' + ext for ext in re.findall(r'"([a-z0-9]+)"', body)})
        if not result[kind]:
            raise ValueError('No supported ' + kind + ' extensions found')
    return result


if __name__ == '__main__':
    main()
