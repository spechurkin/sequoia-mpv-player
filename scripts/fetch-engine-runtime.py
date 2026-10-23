#!/usr/bin/env python3
"""Fetch pinned engine components without depending on installed graphics drivers or Homebrew."""
import argparse
import hashlib
import json
import os
import shutil
import tarfile
import urllib.request
import zipfile
from pathlib import Path, PurePosixPath


def fetch(asset, cache):
    cache.mkdir(parents=True, exist_ok=True)
    archive = cache / asset['sha256']
    if not archive.is_file() or hashlib.sha256(archive.read_bytes()).hexdigest() != asset['sha256']:
        request = urllib.request.Request(asset['url'], headers={'User-Agent': 'Mozilla/5.0'})
        with urllib.request.urlopen(request, timeout=120) as response, archive.open('wb') as output:
            shutil.copyfileobj(response, output)
    if hashlib.sha256(archive.read_bytes()).hexdigest() != asset['sha256']:
        raise ValueError('Engine runtime checksum mismatch: ' + asset['url'])
    return archive


def extract(asset, archive, destination, windows):
    destination.mkdir(parents=True, exist_ok=True)
    if windows:
        prefix = asset['prefix'] + '/'
        architecture = asset['architecture'] + '/' if asset['architecture'] else ''
        with zipfile.ZipFile(str(archive)) as handle:
            for source, name in [
                (prefix + architecture + 'vulkan-1.dll', 'vulkan-1.dll'),
                (prefix + 'VulkanRT-License.txt', 'VulkanRT-LICENSE'),
            ]:
                with handle.open(source) as content, (destination / name).open('wb') as output:
                    shutil.copyfileobj(content, output)
        provenance = 'vulkan-source-build.json'
    else:
        prefix = asset['prefix'] + '/'
        with tarfile.open(str(archive)) as handle:
            for member in handle.getmembers():
                if not member.name.startswith(prefix) or not member.isfile():
                    continue
                relative = PurePosixPath(member.name[len(prefix):])
                if relative.is_absolute() or '..' in relative.parts:
                    raise ValueError('Unsafe engine archive path: ' + member.name)
                path = destination.joinpath(*relative.parts)
                path.parent.mkdir(parents=True, exist_ok=True)
                with handle.extractfile(member) as content, path.open('wb') as output:
                    shutil.copyfileobj(content, output)
                path.chmod(0o755 if member.mode & 0o111 else 0o644)
        if not (destination / 'mpv').is_file() or not (destination / 'lib').is_dir():
            raise ValueError('Prebuilt mpv is missing its executable or libraries')
        provenance = 'source-build.json'
    (destination / provenance).write_text(json.dumps(asset, indent=2) + '\n', encoding='utf-8')


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--target', required=True)
    parser.add_argument('--destination', required=True, type=Path)
    args = parser.parse_args()
    scripts = Path(__file__).resolve().parent
    asset = json.loads((scripts / 'engine-runtime.json').read_text())[args.target]
    archive = fetch(asset, scripts.parent / 'target' / 'engine-downloads')
    windows = 'windows' in args.target
    extract(asset, archive, args.destination, windows)
    if not windows and os.environ.get('GITHUB_ENV'):
        with open(os.environ['GITHUB_ENV'], 'a', encoding='utf-8') as handle:
            handle.write('SEQUOIA_MPV_DIR=' + str((args.destination / 'mpv').resolve()) + '\n')
    print('Prepared engine runtime: ' + str(args.destination))


if __name__ == '__main__':
    main()
