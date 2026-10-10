#!/usr/bin/env python3
"""Carry project terms, source references and retained third-party licenses."""
import argparse
import os
import posixpath
import re
import shutil
from pathlib import Path
from urllib.parse import quote, unquote, urlsplit

ROOT = Path(__file__).resolve().parents[2]
PROJECT_FILES = ('LICENSE', 'LICENSING.md', 'NOTICE', 'THIRD_PARTY_NOTICES.md',
                 'LICENSE-MIT', 'LICENSE-APACHE', 'TRADEMARK.md')
DEFAULT_REPOSITORY = 'grayawa/SlimeVR-Rust'


def copy_documentation(root, destination, revision, extras=()):
    """Bundle current guides with local links and source links for the build revision."""
    root, destination = Path(root).resolve(), Path(destination)
    repository_name = os.environ.get('GITHUB_REPOSITORY', DEFAULT_REPOSITORY)
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository_name):
        raise ValueError('Invalid GITHUB_REPOSITORY for documentation reference')
    reference = revision if re.fullmatch(r'[0-9a-fA-F]{40}', revision) else 'main'
    repository = f'https://github.com/{repository_name}'
    files = [('README.md', 'README.md'), ('CHANGELOG.md', 'CHANGELOG.md')]
    files += [(p.relative_to(root).as_posix(), p.relative_to(root).as_posix())
              for p in sorted((root / 'docs').glob('*.md'))]
    files += list(extras)
    targets = {root / name: name for name in PROJECT_FILES}
    for source, target in files:
        if not (root / source).is_file():
            raise ValueError(f'Missing distribution document: {source}')
        targets.setdefault(root / source, target)

    def render_link(match, source, target):
        href = match.group(2)
        url = urlsplit(href)
        if url.scheme or url.netloc or not url.path:
            return match.group(0)
        resolved = (root / source).parent.joinpath(unquote(url.path)).resolve()
        if resolved in targets:
            href = quote(posixpath.relpath(targets[resolved], posixpath.dirname(target) or '.'), safe='/')
        else:
            path = resolved.relative_to(root).as_posix()
            kind = 'tree' if resolved.is_dir() else 'blob'
            href = f'{repository}/{kind}/{reference}/{quote(path, safe="/")}'
        if url.query:
            href += '?' + url.query
        if url.fragment:
            href += '#' + url.fragment
        return f'{match.group(1)}{href})'

    for source, target in files:
        rendered, fence = [], None
        for line in (root / source).read_text(encoding='utf-8').splitlines(keepends=True):
            marker = re.match(r'^\s*(`{3,}|~{3,})', line)
            if marker:
                if fence is None:
                    fence = marker.group(1)[0]
                elif fence == marker.group(1)[0]:
                    fence = None
                rendered.append(line)
            elif fence:
                rendered.append(line)
            else:
                rendered.append(re.sub(r'(!?\[[^\]\n]*\]\()([^\s)]+)\)',
                                       lambda match: render_link(match, source, target), line))
        path = destination / target
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(''.join(rendered), encoding='utf-8')


def copy_notices(root, destination, revision):
    root, destination = Path(root), Path(destination)
    repository_name = os.environ.get('GITHUB_REPOSITORY', DEFAULT_REPOSITORY)
    if not re.fullmatch(r'[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+', repository_name):
        raise ValueError('Invalid GITHUB_REPOSITORY for source reference')
    repository = f'https://github.com/{repository_name}'
    # Reject an incomplete license bundle before copying any files.
    for name in PROJECT_FILES:
        if not (root / name).is_file():
            raise ValueError(f'Missing project notice: {name}')
    destination.mkdir(parents=True, exist_ok=True)
    for name in PROJECT_FILES:
        shutil.copy2(root / name, destination / name)
    tray_license = root / 'gui-gpui/assets/ksni-UNLICENSE'
    if tray_license.is_file():
        (destination / 'licenses').mkdir(exist_ok=True)
        shutil.copy2(tray_license, destination / 'licenses/ksni-UNLICENSE')
    for source, target, ignore in [
        ('server-rust/licenses', 'licenses/rust-backend', None),
        ('gui-gpui/assets/fonts', 'licenses/fonts', shutil.ignore_patterns('*.ttf')),
        ('licenses/assets', 'licenses/assets', None),
    ]:
        if (root / source).is_dir():
            shutil.copytree(root / source, destination / target,
                            ignore=ignore, dirs_exist_ok=True)
    source = f'{repository}/tree/{revision}' if re.fullmatch(r'[0-9a-fA-F]{40}', revision) else (
        'No exact source commit recorded. Supply the complete source used for this build.'
    )
    (destination / 'SOURCE-CODE.txt').write_text(
        f'SlimeVR-Rust: GPL-3.0-or-later (see LICENSE and LICENSING.md).\n'
        f'Build reference: {revision}\nSource reference: {source}\n'
        f'Repository: {repository}\n\n'
        'Build instructions are in README.md, server-rust/README.zh-CN.md, '
        'gui-gpui/README.zh-CN.md and gui/README.tauri.md.\n'
        'Fetch the recorded commit with recursive submodules; keep the pinned '
        'Cargo.lock / pnpm-lock.yaml and build scripts.\n'
        'If the binary includes local changes, provide those changes as part of '
        'its complete corresponding source, alongside the recorded commit and '
        'build inputs.\n'
        'Reused baseline binaries and third-party components retain their '
        'original terms and provenance; consult BUILD-MANIFEST.json and licenses/.\n',
        encoding='utf-8',
    )


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    parser.add_argument('--revision', required=True)
    parser.add_argument('--backend-exe', type=Path)
    args = parser.parse_args()
    copy_notices(ROOT, args.output, args.revision)
    if args.backend_exe:
        shutil.copy2(args.backend_exe, args.output / 'slimevr-server.exe')


if __name__ == '__main__':
    main()
