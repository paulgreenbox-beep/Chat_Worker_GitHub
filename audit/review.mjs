// Source/dependency observations only; not authentication or runtime acceptance.
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import { publicationFindings, provenanceLimits, readRepositoryFile, redact, repositoryFiles, reserveAttempt, runCommand, snapshot, snapshotDrift } from './support.mjs';
const rootDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..');
export function runReview({ root = rootDirectory, phase, run = runCommand, env = process.env, emit = console.log } = {}) {
  if (!/^review(?:-[1-9][0-9]*)?$/.test(phase ?? '')) throw new Error('Use a fresh named attempt, e.g. review-2');
  const attempt = reserveAttempt(root, phase);
  const report = { phase, started_utc: new Date().toISOString(), findings: [], failures: [], provenance_limits: provenanceLimits };
  let before;
  try {
    before = snapshot(root, run, env);
    report.source_sha = before.head;
    report.inputs_before = before.inputs;
    report.external_inputs_before = before.external_inputs;
    report.environment_before = before.environment;
    report.scoped_inputs_match_head = before.inputs_match_head;
    const args = ['metadata', '--format-version', '1', '--locked', '--offline'];
    const result = run('cargo', args, { cwd: root, env });
    report.metadata = { command: `cargo ${args.join(' ')}`, exit_code: result.exit_code };
    if (result.exit_code !== 0 || result.error) throw new Error('Locked metadata failed');
    const metadata = JSON.parse(result.stdout);
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
    report.dependency_cycles = 0;
    const files = repositoryFiles(root, run, env);
    for (const file of files) {
      const text = readRepositoryFile(root, file).toString('utf8');
      for (const kind of publicationFindings(text)) report.findings.push({ file, kind });
      if (file.endsWith('.rs') && /\bunsafe\s*(?:\{|fn\b|impl\b|trait\b|extern\b)/.test(text)) report.findings.push({ file, kind: 'unsafe-source' });
    }
    report.publication_files_scanned = files.length;
    report.workspace = metadata.packages.filter(pkg => metadata.workspace_members.includes(pkg.id)).map(pkg => ({ name: pkg.name, edition: pkg.edition, targets: pkg.targets.map(target => target.kind) }));
    report.dependencies = metadata.packages.filter(pkg => !metadata.workspace_members.includes(pkg.id)).map(pkg => ({ name: pkg.name, version: pkg.version, license: pkg.license, repository: pkg.repository, rust_version: pkg.rust_version }));
  } catch (error) { report.failures.push(redact(String(error), root)); }
  try {
    const after = snapshot(root, run, env);
    report.source_sha_after = after.head;
    report.inputs_after = after.inputs;
    if (before) report.failures.push(...snapshotDrift(before, after));
  } catch (error) { report.failures.push(redact(`Final snapshot failed: ${error}`, root)); }
  report.limits = ['Targeted pattern scan is not proof against every possible secret.', 'No advisory database scan or live transport qualification.'];
  report.finished_utc = new Date().toISOString();
  report.status = report.failures.length ? 'FAIL' : report.findings.length ? 'ISSUES_FOUND' : 'PASS';
  report.exit_code = report.status === 'PASS' ? 0 : 1;
  attempt.write('source-review.json', redact(JSON.stringify(report, null, 2), root) + '\n');
  emit(JSON.stringify(report));
  return report;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runReview({ phase: process.argv[2] }).exit_code; }
  catch (error) { console.error(redact(String(error), rootDirectory)); process.exitCode = 1; }
}
