// Source/dependency observations only; not an authentication or runtime test.
import { spawnSync } from 'node:child_process';
import { readFileSync, writeFileSync, mkdirSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', windowsHide: true });
  if (result.status !== 0) throw new Error(`${command} exited ${result.status}`);
  return result.stdout;
};
const metadata = JSON.parse(run('cargo', ['metadata', '--format-version', '1', '--locked', '--offline']));
const nodes = new Map(metadata.resolve.nodes.map(node => [node.id, node.dependencies]));
const done = new Set();
function visit(id, active = new Set()) {
  if (active.has(id)) throw new Error('Dependency cycle');
  if (done.has(id)) return;
  const next = new Set(active).add(id);
  for (const dependency of nodes.get(id) ?? []) visit(dependency, next);
  done.add(id);
}
for (const id of nodes.keys()) visit(id);
const files = [...new Set(run('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z']).split('\0').filter(Boolean))];
const findings = [];
for (const file of files) {
  const text = readFileSync(resolve(root, file), 'utf8');
  if (/(?:[A-Za-z]:[\\/](?:Users|Program Files)|\/home\/|\/Users\/)/.test(text)) findings.push({file, kind: 'private-host-path'});
  if (/(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)/.test(text)) findings.push({file, kind: 'credential-pattern'});
  if (file.endsWith('.rs') && /\bunsafe\s*(?:\{|fn\b|impl\b|trait\b|extern\b)/.test(text)) findings.push({file, kind: 'unsafe-source'});
}
const report = {
  source_sha: run('git', ['rev-parse', 'HEAD']).trim(),
  rust_inputs_sha256: Object.fromEntries(files.filter(file => /(?:\.rs$|(?:^|\/)Cargo\.(?:toml|lock)$)/.test(file)).sort()
    .map(file => [file, createHash('sha256').update(readFileSync(resolve(root, file))).digest('hex')])),
  command: 'cargo metadata --format-version 1 --locked --offline', exit_code: 0,
  dependency_cycles: 0, publication_files_scanned: files.length, findings,
  workspace: metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id)).map(pkg => ({name: pkg.name, edition: pkg.edition, targets: pkg.targets.map(target => target.kind)})),
  dependencies: metadata.packages.filter(pkg => !metadata.workspace_members.includes(pkg.id)).map(pkg => ({name: pkg.name, version: pkg.version, license: pkg.license, repository: pkg.repository, rust_version: pkg.rust_version})),
  limits: ['Pattern scan is not proof against every possible secret.', 'No advisory database scan or live transport qualification.'],
};
mkdirSync(resolve(root, 'audit/evidence'), { recursive: true });
writeFileSync(resolve(root, 'audit/evidence/source-review.json'), JSON.stringify(report, null, 2) + '\n');
console.log(JSON.stringify(report));
process.exitCode = findings.length === 0 ? 0 : 1;
