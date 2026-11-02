"""Verify source packaging and the installed seven-command sequence workflow."""

import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import shutil
import subprocess
import tarfile

PRODUCT = 'rsomics-seq'
ROOT = Path(__file__).resolve().parents[1]
HOSTS = {'linux-x86_64': 'x86_64-unknown-linux-gnu', 'linux-aarch64': 'aarch64-unknown-linux-gnu',
         'macos-x86_64': 'x86_64-apple-darwin', 'macos-aarch64': 'aarch64-apple-darwin'}


def identity(path):
    body = Path(path).read_bytes()
    return dict(bytes=len(body), sha256=hashlib.sha256(body).hexdigest())


def save(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')


def require(ok, message):
    if not ok:
        raise ValueError(message)


def package_contract(package):
    parent = Path(package['manifest_path']).parent
    result = {k: v for k, v in package.items() if k not in ('id', 'manifest_path', 'targets')}
    result['targets'] = [{**t, 'src_path': str(Path(t['src_path']).relative_to(parent))} for t in package['targets']]
    return result


def extract_package(source, destination, expected, head, candidate=False):
    destination = Path(destination)
    destination.mkdir(parents=True)
    with tarfile.open(source, 'r:gz') as archive:
        members = archive.getmembers()
        require(len(members) == len({m.name for m in members}), 'duplicate source package member')
        require(len(members) == len({m.name.casefold() for m in members}), 'case-colliding source package member')
        bodies = {}
        for member in members:
            path = PurePosixPath(member.name)
            require(not path.is_absolute() and '..' not in path.parts and path.parts[0] == destination.name,
                    'unsafe source package path')
            require(member.isfile() or member.isdir(), 'unsupported source package member')
            if member.isfile():
                name = '/'.join(path.parts[1:])
                mode = expected[name]['mode'] if name in expected else 0o644
                require(name and member.mode == mode, 'source package file mode differs: ' + name)
                bodies[name] = archive.extractfile(member).read()
        require(set(bodies) == set(expected) | {'Cargo.toml.orig', '.cargo_vcs_info.json'},
                'complete source package membership differs')
        require(bodies['Cargo.toml.orig'] == expected['Cargo.toml']['body'], 'original Cargo manifest differs')
        vcs = json.loads(bodies['.cargo_vcs_info.json'])
        require(vcs['git']['sha1'] == head and bool(vcs['git'].get('dirty', False)) == candidate and
                vcs['path_in_vcs'] == '', 'source package VCS identity differs')
        for name, row in expected.items():
            if name != 'Cargo.toml':
                require(bodies[name] == row['body'], 'source package body differs: ' + name)
        for name, body in bodies.items():
            path = destination / name
            path.parent.mkdir(parents=True, exist_ok=True)
            with path.open('xb') as stream:
                stream.write(body)
            path.chmod(expected[name]['mode'] if name in expected else 0o644)
    return dict(**identity(source), members=len(bodies), original_members=len(expected),
                original_bodies_modes_match=True, source_commit=head, clean_vcs=not candidate,
                local_uncommitted_candidate=candidate, immutable_registry_archive_equivalent=False)


def source(candidate=False):
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    metadata = json.loads((delivery / 'package-metadata.json').read_bytes())
    package, = [p for p in metadata['packages'] if p['name'] == PRODUCT]
    prefix = PRODUCT + '-' + package['version']
    archive = Path(metadata['target_directory']) / 'package' / (prefix + '.crate')
    shutil.copy2(archive, delivery / archive.name)
    head = subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=ROOT, text=True).strip()
    require(head == os.environ['GITHUB_SHA'], 'source head differs from actual CI head')
    dirty = subprocess.check_output(['git', 'status', '--porcelain'], cwd=ROOT) != b''
    require(dirty == candidate, 'clean CI source or explicitly dirty local candidate required')
    expected = {}
    for row in subprocess.check_output(['git', 'ls-tree', '-rz', head], cwd=ROOT).rstrip(b'\0').split(b'\0'):
        header, name = row.split(b'\t')
        mode, kind, blob = header.decode().split()
        name = name.decode()
        require(kind == 'blob' and mode in ('100644', '100755'), 'unexpected source Git type/mode: ' + name)
        body = (ROOT / name).read_bytes()
        require(candidate or hashlib.sha1(b'blob ' + str(len(body)).encode() + b'\0' + body).hexdigest() == blob,
                'source differs from exact Git body: ' + name)
        expected[name] = dict(mode=int(mode, 8) & 0o777, body=body)
    if candidate:
        names = (delivery / 'package-files.txt').read_text().splitlines()
        expected = {name: dict(mode=(ROOT / name).stat().st_mode & 0o777, body=(ROOT / name).read_bytes())
                    for name in names if name not in ('Cargo.toml.orig', '.cargo_vcs_info.json')}
    destination = temporary / 'packaged-source' / prefix
    verification = extract_package(archive, destination, expected, head, candidate)
    unpacked = json.loads(subprocess.check_output(['cargo', 'metadata', '--manifest-path', str(destination / 'Cargo.toml'),
                                                 '--locked', '--offline', '--no-deps', '--format-version', '1']))
    installed, = [p for p in unpacked['packages'] if p['name'] == PRODUCT]
    require(package_contract(installed) == package_contract(package), 'normalized source package metadata differs')
    verification['normalized_metadata_match'] = True
    save(delivery / 'package-verification.json', verification)
    metadata_body = subprocess.check_output(['cargo', 'metadata', '--locked', '--format-version', '1'], cwd=ROOT)
    tree_body = subprocess.check_output(['cargo', 'tree', '--locked', '--offline', '--target', os.environ['EXPECTED_HOST'],
                                        '--edges', 'normal,build', '--prefix', 'none', '--format', '{p}'], cwd=ROOT)
    (delivery / 'native-dependencies.metadata.json').write_bytes(metadata_body)
    (delivery / 'native-dependencies.tree.txt').write_bytes(tree_body)
    feature_body = subprocess.check_output(['cargo', 'tree', '--locked', '--offline', '--target',
                                           os.environ['EXPECTED_HOST'], '--edges', 'normal,build,features'], cwd=ROOT)
    (delivery / 'native-features.tree.txt').write_bytes(feature_body)
    by_name = {(p['name'], p['version']): p for p in json.loads(metadata_body)['packages']}
    enabled = set()
    for line in tree_body.decode().splitlines():
        name, version = re.match(r'([^ ]+) v([^ ]+)', line).groups()
        if name != PRODUCT:
            enabled.add(by_name[name, version]['id'])
    inventory = json.loads((ROOT / 'licenses/provenance.json').read_bytes())
    require({str(p.relative_to(ROOT)) for p in (ROOT / 'licenses').rglob('*') if p.is_file()} ==
            {'licenses/provenance.json'} | {r['destination'] for r in inventory['copied_texts'].values()},
            'license directory membership differs from explicit inventory')
    require(enabled == set(inventory['target_graphs'][os.environ['EXPECTED_HOST']]), 'enabled target dependency inventory differs')
    require(identity(ROOT / 'Cargo.lock')['sha256'] == inventory['lock_sha256'], 'notice inventory lock differs')
    for row in inventory['copied_texts'].values():
        require(identity(ROOT / row['destination']) == {k: row[k] for k in ('bytes', 'sha256')}, 'bundled notice body differs')
    save(delivery / 'native-dependencies.json', dict(target=os.environ['EXPECTED_HOST'], enabled_packages=sorted(enabled),
         inventory_sha256=identity(ROOT / 'licenses/provenance.json')['sha256'],
         metadata_sha256=hashlib.sha256(metadata_body).hexdigest(), tree_sha256=hashlib.sha256(tree_body).hexdigest(),
         feature_tree_sha256=hashlib.sha256(feature_body).hexdigest(),
         enabled_normal_build_graph_verified=True, linked_binary_inventory=False))
    with Path(os.environ['GITHUB_ENV']).open('a') as stream:
        stream.write('PACKAGE_DIRECTORY=' + str(destination) + '\n')
        stream.write('PACKAGE_VERSION=' + package['version'] + '\n')
        stream.write('SOURCE_PACKAGE=' + str(delivery / archive.name) + '\n')


def smoke(binary):
    binary = Path(binary).resolve(strict=True)
    directory = Path(os.environ['RUNNER_TEMP']) / 'delivery/lifecycle'
    directory.mkdir(parents=True)
    shutil.copy2(binary, directory / 'installed-binary')
    import gzip
    records = []

    def run(arguments, *, stdin=None, status=0):
        command = [str(binary), *map(str, arguments)]
        index = len(records)
        row = dict(argv=command, stdin_sha256=None if stdin is None else hashlib.sha256(stdin).hexdigest())
        records.append(row)
        save(directory / 'commands.json', records)
        result = subprocess.run(command, cwd=directory, input=stdin, capture_output=True, timeout=120)
        (directory / f'{index}.stdout').write_bytes(result.stdout)
        (directory / f'{index}.stderr').write_bytes(result.stderr)
        row['returncode'] = result.returncode
        save(directory / 'commands.json', records)
        require(result.returncode == status, 'installed command exit differs: ' + str(command))
        return result.stdout, result.stderr

    out, _ = run(['--version'])
    require(out.strip() == (PRODUCT + ' ' + os.environ['PACKAGE_VERSION']).encode(), 'installed version differs')
    fastq = b'@one description\nACGTAC\n+\n!#%&()\n'
    input_path = directory / 'input.fq'
    input_path.write_bytes(fastq)
    commands = [('stats', []), ('kmers', ['-k', '3']), ('grep', ['-p', 'one']),
                ('convert', ['--to', 'fasta']), ('validate', []),
                ('subseq', ['-r', '2:-2']), ('revcomp', [])]
    for operation, flags in commands:
        out, err = run([operation, *flags, input_path, '--json'])
        result = json.loads(out)
        require(not err and result['tool'] == PRODUCT and result['schema_version'] == '1.0' and
                result['tool_version'] == os.environ['PACKAGE_VERSION'] and result['status'] == 'ok',
                'installed JSON contract differs: ' + operation)
    out, err = run(['kmers', '-k', '3', '--canonical', ROOT / 'tests/golden/kmers.fa'])
    require(not err and out == (ROOT / 'tests/golden/kmers.k3.canonical.tsv').read_bytes(), 'kmer golden differs')
    out, err = run(['grep', '-p', 'one', input_path])
    require(not err and out == fastq, 'grep complete header/quality differs')
    out, err = run(['convert', '--to', 'fasta', input_path])
    require(not err and out == b'>one description\nACGTAC\n', 'conversion differs')
    out, err = run(['subseq', '-r', '2:-2', '-'], stdin=fastq)
    require(not err and out == b'@one description\nCGTA\n+\n#%&(\n', 'subsequence quality differs')
    out, err = run(['revcomp', '-'], stdin=out)
    connected = b'@one description\nTACG\n+\n(&%#\n'
    require(not err and out == connected, 'connected reverse quality differs')
    compressed = directory / 'connected.fq.bgzf'
    run(['revcomp', '-', '-o', compressed], stdin=b'@one description\nCGTA\n+\n#%&(\n')
    body = compressed.read_bytes()
    require(body[:4] == b'\x1f\x8b\x08\x04' and body[12:14] == b'BC' and
            gzip.decompress(body) == connected, 'named BGZF differs')
    run(['validate', compressed])
    protected = directory / 'protected.fq'
    protected.write_bytes(b'previous result\n')
    malformed = directory / 'malformed.fq'
    malformed.write_bytes(fastq + b'@late\nACGT\n+\n!\n')
    for operation, flags in commands:
        out, err = run([operation, *flags, malformed, '-o', protected, '--json'], status=2)
        # JSON plus named output is a configuration failure before input processing.
        require(protected.read_bytes() == b'previous result\n', 'configuration failure replaced output')
        result = subprocess.run([str(binary), operation, *flags, str(malformed), '-o', str(protected)],
                                cwd=directory, capture_output=True, timeout=120)
        (directory / (operation + '-late.stderr')).write_bytes(result.stderr)
        require(result.returncode != 0 and protected.read_bytes() == b'previous result\n',
                'late input failure replaced output: ' + operation)
        result = subprocess.run([str(binary), operation, *flags, str(input_path), '-o', str(input_path)],
                                cwd=directory, capture_output=True, timeout=120)
        (directory / (operation + '-alias.stderr')).write_bytes(result.stderr)
        require(result.returncode != 0 and input_path.read_bytes() == fastq, 'alias changed input: ' + operation)
    save(directory / 'VERIFIED.json', dict(installed_binary=identity(binary), calls=records,
         stable_operations=[x[0] for x in commands], connected_quality_byte_exact=True,
         common_json=True, named_bgzf_decode=True, late_error_preservation=True, alias_preservation=True,
         files={p.name: identity(p) for p in directory.iterdir() if p.is_file()},
         scope='installed seven-operation lifecycle only; no representative performance rerun'))

def bundle(binary, candidate=False):
    temporary = Path(os.environ['RUNNER_TEMP'])
    delivery = temporary / 'delivery'
    binary = Path(binary).resolve(strict=True)
    native_target = os.environ['NATIVE_TARGET']
    rustc = subprocess.check_output(['rustc', '-vV'], text=True)
    executable_format = subprocess.check_output(['file', str(binary)], text=True).strip()
    system, machine = subprocess.check_output(['uname', '-sm'], text=True).split()
    require((system, machine) == {
        'linux-x86_64': ('Linux', 'x86_64'), 'linux-aarch64': ('Linux', 'aarch64'),
        'macos-x86_64': ('Darwin', 'x86_64'), 'macos-aarch64': ('Darwin', 'arm64')
    }[native_target], 'actual native system/CPU differs')
    require(('ELF' if system == 'Linux' else 'Mach-O') in executable_format and
            ('x86-64' if system == 'Linux' and machine == 'x86_64' else machine) in executable_format,
            'native executable format/CPU differs')
    require(HOSTS[native_target] == os.environ['EXPECTED_HOST'] and
            '\nhost: ' + os.environ['EXPECTED_HOST'] + '\n' in rustc, 'bundle is not from the declared native host')
    stem = PRODUCT + '-' + os.environ['PACKAGE_VERSION'] + '-' + native_target
    folder = delivery / stem
    folder.mkdir()
    shutil.copy2(binary, folder / PRODUCT)
    for name in ('README.md', 'PROVENANCE.md', 'MERGE_RECORD.md', 'PERFORMANCE.md', 'LICENSE-MIT', 'LICENSE-APACHE-2.0', 'THIRD_PARTY_NOTICES.md'):
        shutil.copy2(ROOT / name, folder / name)
    shutil.copytree(ROOT / 'licenses', folder / 'licenses')
    provenance = dict(product=PRODUCT, version=os.environ['PACKAGE_VERSION'], target=native_target,
         head_sha=os.environ['GITHUB_SHA'], run_id=int(os.environ['GITHUB_RUN_ID']),
         run_attempt=int(os.environ['GITHUB_RUN_ATTEMPT']), native_host=os.environ['EXPECTED_HOST'], rustc=rustc,
         machine=subprocess.check_output(['uname', '-sm'], text=True).strip(),
         executable_format=executable_format, schema_version=1, binary=identity(binary),
         production_sources={str(p.relative_to(ROOT)): identity(p) for p in sorted((ROOT / 'src').rglob('*.rs'))},
         lock_sha256=identity(ROOT / 'Cargo.lock')['sha256'],
         source_package=json.loads((delivery / 'package-verification.json').read_bytes()),
         native_dependencies=json.loads((delivery / 'native-dependencies.json').read_bytes()),
         installed_lifecycle=json.loads((delivery / 'lifecycle/VERIFIED.json').read_bytes()),
         accepted_implementation_base='3ba1818e85477573995f6a391522f2ef35054c79',
         historical_performance_source='3ba1818e85477573995f6a391522f2ef35054c79',
         this_binary_remeasured=False, registry_publication=False, signed_release=False,
         immutable_registry_archive_equivalent=False, local_uncommitted_candidate=candidate)
    require(provenance['source_package']['clean_vcs'] == (not candidate) and provenance['source_package']['source_commit'] ==
            os.environ['GITHUB_SHA'], 'native bundle source package is not the clean CI head')
    require(provenance['installed_lifecycle']['installed_binary'] == identity(binary), 'bundle/lifecycle binary differs')
    save(folder / 'BUILD.json', provenance)
    paths = [folder, *folder.rglob('*')]
    require(len(paths) == len({str(p.relative_to(folder)).casefold() for p in paths}), 'case-colliding bundle path')
    for path in paths:
        require(not path.is_symlink() and (path.is_dir() or path.is_file()), 'unsupported bundle member')
        path.chmod(0o755 if path.is_dir() or path.name == PRODUCT else 0o644)
    archive = delivery / (stem + '.tar.gz')
    with tarfile.open(archive, 'w:gz') as output:
        output.add(folder, arcname=stem)
    source_archive = Path(os.environ['SOURCE_PACKAGE'])
    (delivery / 'SHA256SUMS').write_text(''.join(identity(path)['sha256'] + '  ' + path.name + '\n'
                                              for path in (archive, source_archive)))
    print(json.dumps(provenance, indent=2, sort_keys=True))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('action', choices=('source', 'smoke', 'bundle'))
    parser.add_argument('--binary', type=Path)
    parser.add_argument('--candidate', action='store_true', help='explicitly uncommitted local packaging; never a release gate')
    args = parser.parse_args()
    if args.action == 'source':
        source(args.candidate)
    elif args.action == 'smoke':
        smoke(args.binary)
    else:
        bundle(args.binary, args.candidate)


if __name__ == '__main__':
    main()
