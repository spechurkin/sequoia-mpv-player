"""Cache verified stable yt-dlp and Deno releases; never runs in the player."""
import hashlib
import json
import os
import shutil
import subprocess
import time
import urllib.request
from urllib.parse import urlsplit
import zipfile
from pathlib import Path


def fetch(url):
    request = urllib.request.Request(url, headers={'User-Agent': 'Sequoia-build'})
    parsed = urlsplit(url)
    if parsed.scheme == 'https' and parsed.netloc == 'api.github.com':
        token = os.environ.get('GITHUB_TOKEN') or os.environ.get('GH_TOKEN')
        if token:
            request.add_unredirected_header('Authorization', 'Bearer ' + token)
    with urllib.request.urlopen(request, timeout=120) as response:
        return response.read()


def release(repo, cache, refresh=False):
    record = cache / (repo.replace('/', '-') + '.json')
    if not refresh and record.is_file() and time.time() - record.stat().st_mtime < 7 * 86400:
        return json.loads(record.read_text(encoding='utf-8'))
    data = fetch('https://api.github.com/repos/{}/releases/latest'.format(repo))
    info = json.loads(data.decode('utf-8'))
    if info.get('prerelease') or info.get('draft'):
        raise ValueError('Expected stable release')
    record.write_bytes(data)
    return info


def download(info, name, cache):
    asset = next(a for a in info['assets'] if a['name'] == name)
    digest = asset.get('digest') or ''
    if not digest.startswith('sha256:'):
        sums = next(a for a in info['assets'] if a['name'] == 'SHA2-256SUMS')
        lines = fetch(sums['browser_download_url']).decode('utf-8').splitlines()
        digest = 'sha256:' + next(line.split()[0] for line in lines if line.split()[-1].lstrip('*') == name)
    expected = digest.split(':', 1)[1]
    if len(expected) != 64:
        raise ValueError('Missing SHA-256 for ' + name)
    path = cache / (expected + '-' + name)
    if not path.is_file() or hashlib.sha256(path.read_bytes()).hexdigest() != expected:
        payload = fetch(asset['browser_download_url'])
        if hashlib.sha256(payload).hexdigest() != expected:
            raise ValueError('SHA-256 mismatch: ' + name)
        path.write_bytes(payload)
    return path


def bundle(destination, target, refresh=False):
    cache = Path(__file__).resolve().parent.parent / '.tools' / 'web'
    cache.mkdir(parents=True, exist_ok=True)
    windows, macos = 'windows' in target, 'apple' in target
    arm = target.startswith('aarch64')
    ytdlp = release('yt-dlp/yt-dlp', cache, refresh)
    deno = release('denoland/deno', cache, refresh)
    yt_asset = ('yt-dlp_arm64.exe' if arm else 'yt-dlp.exe') if windows else (
        'yt-dlp_macos' if macos else ('yt-dlp_linux_aarch64' if arm else 'yt-dlp_linux'))
    deno_target = ('aarch64' if arm else 'x86_64') + (
        '-pc-windows-msvc' if windows else '-apple-darwin' if macos else '-unknown-linux-gnu')
    deno_asset = 'deno-' + deno_target + '.zip'
    if windows and arm and not any(a['name'] == deno_asset for a in deno['assets']):
        deno_asset = 'deno-x86_64-pc-windows-msvc.zip'
    destination.mkdir(parents=True)
    yt = destination / ('yt-dlp.exe' if windows else 'yt-dlp')
    js = destination / ('deno.exe' if windows else 'deno')
    shutil.copy2(str(download(ytdlp, yt_asset, cache)), str(yt))
    with zipfile.ZipFile(str(download(deno, deno_asset, cache))) as archive:
        js.write_bytes(archive.read(js.name))
    yt.chmod(0o755)
    js.chmod(0o755)
    versions = {}
    for name, path in [('yt-dlp', yt), ('deno', js)]:
        versions[name] = subprocess.check_output([str(path), '--version'], universal_newlines=True, timeout=30).strip()
    records = {'versions': versions, 'yt_dlp_release': ytdlp['tag_name'], 'deno_release': deno['tag_name'],
               'yt_dlp_asset': yt_asset, 'deno_asset': deno_asset,
               'sha256': {p.name: hashlib.sha256(p.read_bytes()).hexdigest() for p in [yt, js]}}
    (destination / 'source-build.json').write_text(json.dumps(records, indent=2) + '\n', encoding='utf-8')
    for repo, tag, name in [('yt-dlp/yt-dlp', ytdlp['tag_name'], 'yt-dlp'),
                            ('denoland/deno', deno['tag_name'], 'deno')]:
        filename = 'LICENSE.md' if name == 'deno' else 'LICENSE'
        license_text = fetch('https://raw.githubusercontent.com/{}/{}/{}'.format(repo, tag, filename))
        (destination / (name + '-LICENSE')).write_bytes(license_text)
    return records
