import hashlib
import importlib.util
import json
import sys
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest.mock import patch

SCRIPTS = Path(__file__).resolve().parents[1] / 'scripts'
sys.path.insert(0, str(SCRIPTS))
spec = importlib.util.spec_from_file_location('package_ci', SCRIPTS / 'package-ci.py')
package = importlib.util.module_from_spec(spec)
spec.loader.exec_module(package)


class PackagingTests(unittest.TestCase):
    def test_windows_update_manifest_and_archive(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scripts = root / 'scripts'
            scripts.mkdir()
            for name in ['updater.ps1', 'update.cmd', 'install.cmd', 'uninstall.cmd', 'windows-integration.ps1']:
                (scripts / name).write_bytes((SCRIPTS / name).read_bytes())
            library = root / 'src' / 'catalog' / 'library.rs'
            library.parent.mkdir(parents=True)
            library.write_bytes((SCRIPTS.parent / 'src' / 'catalog' / 'library.rs').read_bytes())
            for name in ['README.md', 'LICENSE', 'THIRD_PARTY.md']:
                (root / name).write_text(name)
            (root / 'Cargo.toml').write_text('[package]\nversion = "1.2.3"\n')
            binary = root / 'player.exe'
            binary.write_bytes(b'executable fixture')
            locales = root / 'locales'
            locales.mkdir()
            for language in ['en', 'ru', 'de']:
                (locales / (language + '.json')).write_text('{"id":"' + language + '"}')

            def engine(_input, destination, _target):
                destination.mkdir(parents=True)
                result = destination / 'mpv.exe'
                result.write_bytes(b'engine fixture')
                (destination / 'doc').mkdir()
                (destination / 'doc' / 'manual.pdf').write_bytes(b'manual')
                (destination / 'LICENSE').write_text('engine license')
                return result

            def web(destination, *_args):
                destination.mkdir()
                (destination / 'yt-dlp.exe').write_bytes(b'web fixture')
                (destination / 'yt-dlp-LICENSE').write_text('web license')
                return {'fixture': True}

            args = ['package-ci.py', '--target', 'x86_64-pc-windows-msvc',
                    '--label', 'local', '--minimum', '10', '--mpv', 'fixture',
                    '--binary', str(binary), '--repository', 'owner/sequoia-mpv-player']
            with patch.object(package, '__file__', str(scripts / 'package-ci.py')), \
                    patch.object(sys, 'argv', args), \
                    patch.object(package.bundle_mpv, 'bundle', engine), \
                    patch.object(package.bundle_mpv, 'verify', return_value='fixture'), \
                    patch.object(package.bundle_web, 'bundle', web):
                package.main()
            archive = next((root / 'dist').glob('*.zip'))
            with zipfile.ZipFile(archive) as packaged:
                for language in ['en', 'ru', 'de']:
                    self.assertTrue(any(name.endswith('/locales/' + language + '.json')
                                        for name in packaged.namelist()))
            self.assertEqual(Path(str(archive) + '.sha256').read_text().split()[0],
                             hashlib.sha256(archive.read_bytes()).hexdigest())
            with zipfile.ZipFile(archive) as handle:
                prefix = archive.stem + '/'
                info = json.loads(handle.read(prefix + 'docs/build-info.json'))
                files = sorted(name[len(prefix):] for name in handle.namelist())
                self.assertEqual(info['managed_files'], files)
                self.assertEqual(info['version'], '1.2.3')
                self.assertEqual(info['update_repository'], 'owner/sequoia-mpv-player')
                self.assertEqual(info['update_asset'], 'Sequoia-windows-2022-x86_64-pc-windows-msvc.zip')
                self.assertEqual([name for name in files if '/' not in name], ['Sequoia.exe'])
                self.assertIn('installers/updater.ps1', files)
                self.assertIn('installers/update.cmd', files)
                for name in ['install.cmd', 'uninstall.cmd', 'windows-integration.ps1', 'windows-file-types.json']:
                    self.assertIn('installers/' + name, files)
                types = json.loads(handle.read(prefix + 'installers/windows-file-types.json'))
                for name in ['README.md', 'LICENSE', 'THIRD_PARTY.md', 'mpv/doc/manual.pdf', 'mpv/LICENSE',
                             'yt-dlp/yt-dlp-LICENSE']:
                    self.assertIn('docs/' + name, files)
                self.assertIn('.mp3', types['audio'])
                self.assertIn('.mkv', types['video'])
                self.assertNotIn('.m3u', types['audio'] + types['video'])


if __name__ == '__main__':
    unittest.main()
