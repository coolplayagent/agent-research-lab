#!/usr/bin/env python3
"""Download authenticated distro packages into a private prefix; never install them."""
import argparse, fcntl, hashlib, json, os, pathlib, re, shlex, subprocess, time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('destination', type=pathlib.Path)
parser.add_argument('--timeout', type=int, default=60)
args = parser.parse_args()
root = args.destination.resolve()
root.mkdir(parents=True, exist_ok=True, mode=0o700)
lock = (root / '.prepare.lock').open('a')
fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
archives = root / 'archives'
archives.mkdir(exist_ok=True)
deadline = time.monotonic() + args.timeout

def run(command, **kwargs):
    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise RuntimeError('desktop dependency preparation deadline exceeded; safe to retry')
    return subprocess.run(command, check=True, text=True, stdout=subprocess.PIPE,
                          stderr=subprocess.PIPE, timeout=remaining, **kwargs).stdout

def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def installed(package):
    try:
        value = run(['dpkg-query', '-W', '-f=${Status}\t${Version}', package])
        return value.split('\t', 1)[1] if value.startswith('install ok installed\t') else None
    except subprocess.CalledProcessError:
        return None

existing = root / 'manifest.json'
if existing.exists():
    retained = json.loads(existing.read_text())
    if retained.get('files') and isinstance(retained.get('symlinks'), dict) and all(
            (root / name).is_file() and not (root / name).is_symlink() and digest(root / name) == sha
            for name, sha in retained['files'].items()) and all(
            (root / name).is_symlink() and os.readlink(root / name) == target
            for name, target in retained['symlinks'].items()):
        print(json.dumps({'ready': True, 'cached': True, 'destination': str(root), 'manifest': str(existing)}))
        raise SystemExit(0)
    raise RuntimeError('retained desktop dependency bytes changed; prepare a new private prefix')

# APT's package hashes come from these distributor-signed local index snapshots.
indexes = []
for path in sorted(pathlib.Path('/var/lib/apt/lists').glob('*ubuntu*_InRelease')):
    run(['gpgv', '--keyring', '/usr/share/keyrings/ubuntu-archive-keyring.gpg', str(path)])
    indexes.append({'path': str(path), 'sha256': digest(path)})
if not indexes:
    raise RuntimeError('no verified Ubuntu InRelease index; refresh trusted APT indexes first')

seeds = ['xvfb', 'xdotool', 'scrot', 'openbox', 'xauth', 'x11-utils', 'xfonts-base', 'python3-gi', 'gir1.2-gtk-3.0']
queue = list(seeds)
seen = set()
packages, host_dependencies = [], []
while queue:
    package = queue.pop(0)
    if package in seen:
        continue
    seen.add(package)
    version = installed(package)
    if version and package not in ['xvfb', 'xdotool', 'scrot', 'openbox']:
        host_dependencies.append({'package': package, 'version': version})
        continue
    metadata = run(['apt-cache', 'show', '--no-all-versions', package])
    fields = {}
    for line in metadata.splitlines():
        if line and not line[0].isspace() and ': ' in line:
            key, value = line.split(': ', 1)
            fields.setdefault(key, value)
    version, expected = fields['Version'], fields['SHA256']
    uri_line = run(['apt-get', '-o', 'APT::Get::AllowUnauthenticated=false', '--print-uris',
                    'download', package + '=' + version]).strip().splitlines()[-1]
    uri, filename = shlex.split(uri_line)[:2]
    if not re.match(r'https?://(?:archive|security|ports)\.ubuntu\.com/', uri):
        raise RuntimeError('dependency source is not an approved Ubuntu archive: ' + uri)
    archive = archives / pathlib.Path(filename).name
    if not archive.exists() or digest(archive) != expected:
        run(['apt-get', '-o', 'APT::Get::AllowUnauthenticated=false', 'download',
             package + '=' + version], cwd=archives)
    if digest(archive) != expected:
        raise RuntimeError('download hash differs from authenticated package index: ' + package)
    run(['dpkg-deb', '-x', str(archive), str(root)])
    packages.append({'package': package, 'version': version, 'uri': uri,
                     'archive': str(archive.relative_to(root)), 'sha256': expected})
    dependencies = fields.get('Pre-Depends', '') + ',' + fields.get('Depends', '')
    for group in dependencies.split(','):
        alternatives = [re.split(r'[ (\[]', item.strip(), maxsplit=1)[0].split(':', 1)[0]
                        for item in group.split('|') if item.strip()]
        if not alternatives:
            continue
        selected = next((name for name in alternatives if installed(name)), alternatives[0])
        queue.append(selected)
files = {str(path.relative_to(root)): digest(path) for path in sorted((root / 'usr').rglob('*'))
         if path.is_file() and not path.is_symlink()}
symlinks = {str(path.relative_to(root)): os.readlink(path) for path in sorted((root / 'usr').rglob('*'))
            if path.is_symlink()}
manifest = {'files': files, 'symlinks': symlinks, 'schema_version': 1, 'source': 'authenticated-apt-index',
            'prepared_at_unix': int(time.time()), 'signed_indexes': indexes,
            'packages': packages, 'host_dependencies': host_dependencies}
(root / 'manifest.pending.json').write_text(json.dumps(manifest, indent=2) + '\n')
os.replace(root / 'manifest.pending.json', root / 'manifest.json')
print(json.dumps({'ready': True, 'destination': str(root), 'manifest': str(root / 'manifest.json'),
                  'packages': len(packages), 'host_dependencies': len(host_dependencies)}))
