"""Reuse immutable Intel release bytes only after verifying source and native gates."""
import argparse
import hashlib
import io
import json
import os
import pathlib
import plistlib
import re
import subprocess
import tarfile
import zipfile

parser = argparse.ArgumentParser()
parser.add_argument('--tag', required=True)
parser.add_argument('--run', required=True, type=int)
parser.add_argument('--expected-source', default='')
parser.add_argument('--output', required=True, type=pathlib.Path)
parser.add_argument('--proof', required=True, type=pathlib.Path)
args = parser.parse_args()
assert re.fullmatch(r'v\d+\.\d+\.\d+', args.tag)
REPO = os.environ['GITHUB_REPOSITORY']


def gh(*arguments, binary=False):
    data = subprocess.check_output(['gh', *arguments])
    return data if binary else data.decode('utf8')


def api(endpoint, *fields):
    return json.loads(gh('api', '--method', 'GET', f'repos/{REPO}/{endpoint}', *fields))


def git_blob(commit, filename):
    return subprocess.check_output(['git', 'show', f'{commit}:{filename}'])


subprocess.check_call(['git', 'fetch', 'origin', 'tag', args.tag])
product = subprocess.check_output(['git', 'rev-parse', args.tag + '^{commit}'], encoding='utf8').strip()
if args.expected_source:
    assert product == args.expected_source
config = json.loads(git_blob(product, 'apps/agent/src-tauri/tauri.conf.json'))
version = config['version']
assert args.tag == 'v' + version
run = api(f'actions/runs/{args.run}')
assert run['head_sha'] == product and run['status'] == 'completed' and run['conclusion'] in {'success', 'failure'}
jobs = []
page = 1
while True:
    response = api(f'actions/runs/{args.run}/jobs', '-f', 'filter=all', '-f', 'per_page=100', '-f', f'page={page}')
    jobs.extend(response['jobs'])
    if len(jobs) >= response['total_count']:
        break
    assert response['jobs']
    page += 1
matching = [job for job in jobs if job['name'].startswith('build-macos') and 'x86_64-apple-darwin' in job['name']]
assert matching
job = max(matching, key=lambda item: (item['run_attempt'], item['id']))
assert job['run_id'] == args.run and job['head_sha'] == product
assert job['status'] == 'completed' and job['conclusion'] == 'success'
logs = gh('api', '--allow-escape-sequences', f'repos/{REPO}/actions/jobs/{job["id"]}/logs')
logs = re.sub(r'\x1b\[[0-?]*[ -/]*[@-~]', '', logs)
messages = [match.group(1).strip() for line in logs.splitlines()
            if (match := re.match(r'^\d{4}-\d{2}-\d{2}T\S+Z\s(.*)$', line))]
labels = ['codesign --verify .app', 'stapler validate .app', 'spctl exec .app', 'stapler validate .dmg', 'spctl open .dmg']
assert all(any(re.fullmatch(r'PASS\s+' + re.escape(label), message) for message in messages) for label in labels)
assert 'Developer ID Application signature + secure timestamp confirmed' in messages
assert 'Authority=Developer ID Certification Authority' in messages and 'Authority=Apple Root CA' in messages
assert any(message.startswith('Timestamp=') for message in messages)
submissions = {match.group(1) for message in messages
               if (match := re.fullmatch(r'notarytool id=([0-9a-fA-F-]{36}) status=Accepted', message))}
assert len(submissions) >= 2
artifacts = api(f'actions/runs/{args.run}/artifacts')['artifacts']
matching = [artifact for artifact in artifacts if artifact['name'] == 'macos-x86_64' and not artifact['expired']]
assert len(matching) == 1
artifact = matching[0]
assert artifact['workflow_run']['id'] == args.run and artifact['workflow_run']['head_sha'] == product
assert artifact['digest'].startswith('sha256:')
payload = gh('api', '--allow-escape-sequences', f'repos/{REPO}/actions/artifacts/{artifact["id"]}/zip', binary=True)
digest = hashlib.sha256(payload).hexdigest()
assert artifact['digest'] == 'sha256:' + digest
base = f'FlowSight.Agent_{version}_x64'
expected = {base + '.dmg', base + '.app.tar.gz', base + '.app.tar.gz.sig', 'latest-fragment.json'}
with zipfile.ZipFile(io.BytesIO(payload)) as archive:
    names = archive.namelist()
    assert len(names) == len(set(names)) and set(names) == expected
    files = {name: archive.read(name) for name in names}
fragment = json.loads(files['latest-fragment.json'])
assert fragment['version'] == version and set(fragment['platforms']) == {'darwin-x86_64'}
notes = git_blob(product, f'docs/releases/{version}.md').decode('utf8').strip()
assert fragment['notes'].strip() == notes
entry = fragment['platforms']['darwin-x86_64']
assert entry['url'] == f'https://github.com/{REPO}/releases/download/{args.tag}/{base}.app.tar.gz'
assert entry['signature'].strip() == files[base + '.app.tar.gz.sig'].decode('utf8').strip()
prefix = 'FlowSight Agent.app/Contents/'
with tarfile.open(fileobj=io.BytesIO(files[base + '.app.tar.gz']), mode='r:gz') as archive:
    members = archive.getmembers()
    names = {member.name for member in members}
    assert len(names) == len(members) and not any(name.endswith('.gguf') for name in names)
    info = plistlib.loads(archive.extractfile(prefix + 'Info.plist').read())
    assert info['CFBundleIdentifier'] == 'ai.flowsight.agent' and info['CFBundleShortVersionString'] == version
    runtime = prefix + 'Resources/local_llm/bin/llama-server'
    binaries = [prefix + 'MacOS/' + info['CFBundleExecutable']]
    binaries += [name for name in names if name.startswith(prefix + 'Resources/local_llm/bin/') and (name.endswith('.dylib') or name == runtime)]
    assert runtime in binaries and len(binaries) > 2
    for name in binaries:
        header = archive.extractfile(name).read(12)
        assert header[:4] == b'\xcf\xfa\xed\xfe' and int.from_bytes(header[4:8], 'little') == 0x01000007, name
    resources = ['manifest.json', 'worker.js', 'music.js', 'options.html', 'options.js', 'blocked.html', 'blocked.js', 'focus.css', 'icon16.png', 'icon48.png', 'icon128.png']
    for name in resources:
        assert archive.extractfile(prefix + 'Resources/browser-extension/' + name).read() == git_blob(product, 'apps/agent/browser-extension/' + name)
args.output.mkdir(parents=True, exist_ok=True)
for name, data in files.items():
    (args.output / name).write_bytes(data)
proof = {'version': version, 'productCommit': product, 'originalRun': args.run,
         'originalWorkflowCommit': run['head_sha'], 'nativeJob': job['id'], 'nativeAttempt': job['run_attempt'],
         'artifact': artifact['id'], 'artifactDigest': artifact['digest'], 'archiveDigestVerified': True,
         'executedNativeGates': labels, 'acceptedNotarySubmissions': sorted(submissions),
         'nativeX86_64Bundle': True, 'browserResourcesMatchSource': True,
         'files': {name: {'size': len(data), 'sha256': hashlib.sha256(data).hexdigest()} for name, data in files.items()}}
args.proof.parent.mkdir(parents=True, exist_ok=True)
args.proof.write_text(json.dumps(proof, indent=2) + '\n', encoding='utf8')
print('REUSED_INTEL_PRODUCT_SOURCE_SHA=' + product)
print('REUSED_INTEL_BUILD_RUN=' + str(args.run))
print('REUSED_INTEL_NATIVE_JOB=' + str(job['id']))
print('REUSED_INTEL_ARTIFACT_SHA256=' + digest)
print(json.dumps(proof, indent=2))
