import hashlib
import importlib.util
import json
import io
import sys
import tarfile
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
runtime_spec = importlib.util.spec_from_file_location('engine_runtime', SCRIPTS / 'fetch-engine-runtime.py')
runtime = importlib.util.module_from_spec(runtime_spec)
runtime_spec.loader.exec_module(runtime)


class PackagingTests(unittest.TestCase):
    def test_vulkan_architecture_and_notices(self):
        assets = json.loads((SCRIPTS / 'engine-runtime.json').read_text())
        for target in ['x86_64-pc-windows-msvc', 'aarch64-pc-windows-msvc']:
            asset = assets[target]
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                archive = root / 'runtime.zip'
                architecture = asset['architecture'] + '/' if asset['architecture'] else ''
                with zipfile.ZipFile(archive, 'w') as handle:
                    handle.writestr(asset['prefix'] + '/' + architecture + 'vulkan-1.dll', target)
                    handle.writestr(asset['prefix'] + '/VulkanRT-License.txt', 'license')
                    handle.writestr(asset['prefix'] + '/x86/vulkan-1.dll', 'wrong architecture')
                destination = root / 'mpv'
                runtime.extract(asset, archive, destination, True)
                self.assertEqual((destination / 'vulkan-1.dll').read_text(), target)
                docs = root / 'docs'
                (root / 'yt-dlp').mkdir()
                package.collect_dependency_docs(root, docs)
                self.assertTrue((docs / 'mpv/VulkanRT-LICENSE').is_file())
                self.assertTrue((docs / 'mpv/vulkan-source-build.json').is_file())

    def test_runtime_rejects_bad_checksum(self):
        with tempfile.TemporaryDirectory() as directory:
            asset = {'sha256': '0' * 64, 'url': 'https://example.invalid/runtime'}
            with patch.object(runtime.urllib.request, 'urlopen', return_value=io.BytesIO(b'corrupt')):
                with self.assertRaisesRegex(ValueError, 'checksum mismatch'):
                    runtime.fetch(asset, Path(directory))

    def test_macos_bundler_destination_exists(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source/mpv'
            source.parent.mkdir()
            source.write_bytes(b'executable')
            destination = root / 'engine'

            def run(*args, **kwargs):
                if args[0] == 'dylibbundler':
                    self.assertTrue(Path(args[args.index('-d') + 1]).is_dir())
                if args[:2] == ('brew', '--prefix'):
                    return str(root / 'formula')
                return ''

            with patch.object(package.bundle_mpv, 'run', side_effect=run) as command:
                package.bundle_mpv.bundle(source, destination, 'aarch64-apple-darwin')
            self.assertTrue(any(call.args[0] == 'dylibbundler' for call in command.call_args_list))

    def test_prebuilt_macos_keeps_libraries_without_homebrew(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            archive = root / 'mpv.tar.gz'
            asset = {'prefix': 'mpv.app/Contents/MacOS'}
            with tarfile.open(archive, 'w:gz') as handle:
                for name, data in [('mpv', b'executable'), ('lib/libavcodec.dylib', b'library')]:
                    member = tarfile.TarInfo(asset['prefix'] + '/' + name)
                    member.size = len(data)
                    member.mode = 0o755
                    handle.addfile(member, io.BytesIO(data))
            runtime.extract(asset, archive, root / 'source', False)
            with patch.object(package.bundle_mpv, 'run') as command:
                package.bundle_mpv.bundle(root / 'source/mpv', root / 'engine', 'x86_64-apple-darwin')
            self.assertEqual((root / 'engine/lib/libavcodec.dylib').read_bytes(), b'library')
            self.assertTrue((root / 'engine/source-build.json').is_file())
            self.assertTrue(all(call.args[0] == 'codesign' for call in command.call_args_list))

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
