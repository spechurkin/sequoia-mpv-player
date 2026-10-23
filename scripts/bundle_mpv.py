"""Collect the playback engine and validate it without a system mpv on PATH."""
import os
import re
import shutil
import subprocess
import tempfile
import wave
from pathlib import Path


def run(*args, **kwargs):
    return subprocess.check_output(args, universal_newlines=True, **kwargs)


def bundle(source, destination, target):
    source = Path(source).resolve()
    windows = 'windows' in target
    macos = 'apple' in target
    name = 'mpv.exe' if windows else 'mpv'
    binary = source / name if source.is_dir() else source
    if not binary.is_file():
        raise FileNotFoundError('Required bundled mpv: {}'.format(binary))
    destination.mkdir(parents=True)
    executable = destination / name
    shutil.copy2(str(binary), str(executable))
    if windows:
        for dll in binary.parent.glob('*.dll'):
            shutil.copy2(str(dll), str(destination / dll.name))
        if (binary.parent / 'doc').is_dir():
            shutil.copytree(str(binary.parent / 'doc'), str(destination / 'doc'))
        for pattern in ('LICENSE*', 'COPYING*', '*source*.json'):
            for notice in binary.parent.glob(pattern):
                if notice.is_file():
                    shutil.copy2(str(notice), str(destination / notice.name))
    elif macos:
        executable.chmod(0o755)
        prebundled = (binary.parent / 'lib').is_dir()
        if prebundled:
            shutil.copytree(str(binary.parent / 'lib'), str(destination / 'lib'))
            for notice in binary.parent.glob('*source*.json'):
                shutil.copy2(str(notice), str(destination / notice.name))
        else:
            (destination / 'lib').mkdir()
            run('dylibbundler', '-b', '-x', str(executable), '-d',
                str(destination / 'lib'), '-p', '@executable_path/lib/')
        for library in sorted((destination / 'lib').glob('*')):
            run('codesign', '--force', '--sign', '-', str(library))
        run('codesign', '--force', '--sign', '-', str(executable))
        formulas = [] if prebundled else ['mpv'] + run('brew', 'deps', '--formula', 'mpv').splitlines()
        for formula in formulas:
            cellar = Path(run('brew', '--prefix', formula).strip())
            for pattern in ('LICENSE*', 'COPYING*'):
                for notice in cellar.glob(pattern):
                    if notice.is_file():
                        dest = destination / 'licenses' / formula
                        dest.mkdir(parents=True, exist_ok=True)
                        shutil.copy2(str(notice), str(dest / notice.name))
    else:
        libraries = destination / 'lib'
        libraries.mkdir()
        dependencies = run('ldd', str(binary))
        if 'not found' in dependencies:
            raise RuntimeError(dependencies)
        loader = None
        for line in dependencies.splitlines():
            match = re.search(r'(/[^\s]+)\s+\(', line)
            if match:
                library = Path(match.group(1))
                shutil.copy2(str(library), str(libraries / library.name))
                if library.name.startswith(('ld-linux', 'ld-musl')):
                    loader = library.name
        if loader is None:
            raise RuntimeError('ELF loader missing from ldd output')
        executable.rename(destination / 'mpv.bin')
        (destination / 'mpv.bin').chmod(0o755)
        (libraries / loader).chmod(0o755)
        executable.write_text(
            '#!/bin/sh\nbase=$(CDPATH= cd -- "${0%/*}" && pwd)\n'
            'exec "$base/lib/' + loader + '" --library-path "$base/lib" '
                                          '"$base/mpv.bin" "$@"\n', encoding='utf-8')
        executable.chmod(0o755)
        for package in Path('/usr/share/doc').iterdir():
            copyright_file = package / 'copyright'
            if copyright_file.is_file():
                dest = destination / 'licenses' / package.name
                dest.mkdir(parents=True)
                shutil.copy2(str(copyright_file), str(dest / 'copyright'))
    return executable


def verify(engine_directory, windows):
    with tempfile.TemporaryDirectory(prefix='sequoia-engine-') as temporary:
        root = Path(temporary)
        relocated = root / 'engine with spaces'
        shutil.copytree(str(engine_directory), str(relocated))
        executable = relocated / ('mpv.exe' if windows else 'mpv')
        env = dict(os.environ, PATH='')
        env.pop('LD_LIBRARY_PATH', None)
        env.pop('DYLD_LIBRARY_PATH', None)
        version = run(str(executable), '--no-config', '--version',
                      cwd=str(root), env=env, timeout=30).splitlines()[0]
        media = root / 'silence.wav'
        with wave.open(str(media), 'wb') as handle:
            handle.setparams((1, 2, 8000, 0, 'NONE', ''))
            handle.writeframes(b'\0\0' * 800)
        run(str(executable), '--no-config', '--vo=null', '--ao=null',
            '--really-quiet', str(media), cwd=str(root), env=env, timeout=30)
        print('Bundled engine playback PASS: ' + version)
        return version
