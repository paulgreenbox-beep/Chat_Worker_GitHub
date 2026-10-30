// Native Receiver/State verification. Cargo runs serially and offline; no credentials are logged.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { hash, readRepositoryFile, redact, repositoryFiles, reserveAttempt, runCommand, snapshot, snapshotDrift, provenanceLimits } from './support.mjs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const phase = process.argv[2];
if (!/^(?:receiver-(?:baseline|final)(?:-[1-9][0-9]*)?|receiver-regression-(?:security|state|github|http|winhttp|retained|commit|workspace|fixture|server)(?:-[1-9][0-9]*)?)$/.test(phase ?? '')) throw Error('Use a fresh named receiver attempt');
const attempt = reserveAttempt(root, phase);
const env = { ...process.env, CARGO_NET_OFFLINE: 'true' };
const clean = value => redact(value, root);
const fixtureInputs = () => Object.fromEntries(repositoryFiles(root, runCommand, env)
  .filter(file => /(?:^|\/)tests\/fixtures\//.test(file))
  .map(file => [file, hash(readRepositoryFile(root, file))]));
const result = { phase, started_utc: new Date().toISOString(), platform: process.platform, architecture: process.arch,
  os_release: os.release(), cargo_net_offline: true,
  diagnostic_private_pool: env.CWG_NATIVE_PRIVATE_POOL === '1',
  native_fixture_parent: env.CWG_NATIVE_FIXTURE_PARENT ? { selection: 'explicit test-only parent; no production override', path_sha256: hash(env.CWG_NATIVE_FIXTURE_PARENT) } : { selection: 'default Windows temporary directory' },
  checks: [], failures: [], provenance_limits: provenanceLimits };
const regressions = {
  security: ['test', '-p', 'cwg-security', '--locked', '--offline', 'cng_', '--', '--nocapture'],
  state: ['test', '-p', 'cwg-state', '--test', 'durable', '--locked', '--offline', 'security_regression_', '--', '--nocapture'],
  github: ['test', '-p', 'cwg-github', '--lib', '--locked', '--offline', 'audit_', '--', '--nocapture'],
  http: ['test', '-p', 'cwg-github', '--lib', '--locked', '--offline', 'http_security_regression_', '--', '--nocapture'],
  winhttp: ['test', '-p', 'cwg-github', '--bin', 'cwg-github', '--locked', '--offline', 'audit_', '--', '--nocapture'],
  retained: ['test', '-p', 'cwg-state', '--test', 'retained_access', '--locked', '--offline', 'security_regression_', '--', '--nocapture'],
  commit: ['test', '-p', 'cwg-state', '--lib', '--locked', '--offline', 'commit_tests', '--', '--nocapture'],
  workspace: ['test', '--workspace', '--all-targets', '--locked', '--offline', '--no-fail-fast'],
  fixture: ['test', '-p', 'cwg-state', '--test', 'windows_paths', '--locked', '--offline', 'fixture_fence_', '--', '--nocapture'],
  server: ['test', '-p', 'cwg-github', '--bin', 'cwg-github', '--locked', '--offline', 'audit_fixture_refuses_an_interrupted', '--', '--nocapture'],
};
const regression = phase.startsWith('receiver-regression-') ? regressions[phase.match(/^receiver-regression-([a-z]+)/)[1]] : null;
const gates = regression ? [['test', 'cargo', regression]] : [
  ['metadata', 'cargo', ['metadata', '--format-version', '1']],
  ['fmt', 'cargo', ['fmt', '--all', '--', '--check']],
  ['check', 'cargo', ['check', '--workspace', '--all-targets', '--locked']],
  ['test', 'cargo', ['test', '--workspace', '--all-targets', '--locked']],
  ['clippy', 'cargo', ['clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']],
  ['debug', 'cargo', ['build', '--workspace', '--locked']],
  ['release', 'cargo', ['build', '--workspace', '--release', '--locked']],
  ['offline-test', 'cargo', ['test', '--workspace', '--all-targets', '--locked', '--offline']],
  ['diff-check', 'git', ['diff', '--check']],
];
let before;
try {
  before = snapshot(root, runCommand, env);
  Object.assign(result, { source_sha: before.head, inputs_before: before.inputs, external_inputs_before: before.external_inputs,
    environment_before: before.environment, scoped_inputs_match_head: before.inputs_match_head });
  result.fixture_inputs_before = fixtureInputs();
  for (const [file, digest] of Object.entries(result.fixture_inputs_before)) {
    const saved = runCommand('git', ['show', `${before.head}:${file}`], { cwd: root, env });
    if (saved.exit_code !== 0 || digest !== hash(saved.stdout)) throw Error('Commit raw fixture inputs before verification');
  }
  if (phase.startsWith('receiver-final') && !before.inputs_match_head) throw Error('Commit build/audit inputs before final verification');
  if (phase.startsWith('receiver-final') && env.CWG_NATIVE_PRIVATE_POOL === '1') throw Error('Final gates must use production-equivalent native policy');
  result.rustc = runCommand('rustc', ['--version', '--verbose'], { cwd: root, env });
  result.cargo = runCommand('cargo', ['--version'], { cwd: root, env });
  if (result.rustc.exit_code !== 0 || result.cargo.exit_code !== 0) throw Error('Tool version check failed');
  for (const [name, command, args] of gates) {
    const output = runCommand(command, args, { cwd: root, env });
    const transcript = clean(`command: ${command} ${args.join(' ')}\nCARGO_NET_OFFLINE=true\nexit_code: ${output.exit_code}\nstdout:\n${output.stdout}\nstderr:\n${output.stderr}\nerror: ${output.error}\n`);
    const truncated = Buffer.byteLength(transcript) > 1024 * 1024;
    const saved = truncated ? transcript.slice(0, 250_000) + '\n[EVIDENCE_LIMIT: transcript truncated; attempt FAIL]\n' : transcript;
    const encoding = /[ \t]+$/m.test(saved) ? 'json-envelope' : 'plain-text';
    const published = encoding === 'plain-text' ? saved : JSON.stringify({ transcript: saved }, null, 2) + '\n';
    const summaries = [...output.stdout.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
    const isTest = command === 'cargo' && args[0] === 'test';
    const tests = isTest && summaries.length ? { passed: 0, failed: 0, ignored: 0 } : null;
    if (tests) for (const match of summaries) { tests.passed += +match[1]; tests.failed += +match[2]; tests.ignored += +match[3]; }
    const pass = output.exit_code === 0 && !output.error && !truncated && (!isTest || (tests && tests.passed > 0 && tests.failed === 0 && tests.ignored === 0));
    const check = { command: `${command} ${args.join(' ')}`, exit_code: output.exit_code, status: pass ? 'PASS' : 'FAIL', tests,
      test_execution: isTest ? (summaries.length ? 'OBSERVED' : 'NOT_RUN_NO_TEST_SUMMARY') : 'NOT_APPLICABLE',
      log: `${name}.txt`, log_encoding: encoding, transcript_truncated: truncated, transcript_sha256: hash(transcript), log_sha256: hash(published) };
    attempt.write(check.log, published); result.checks.push(check); console.log(JSON.stringify(check));
  }
} catch (error) { result.failures.push(clean(String(error))); }
try {
  const after = snapshot(root, runCommand, env);
  Object.assign(result, { source_sha_after: after.head, inputs_after: after.inputs, external_inputs_after: after.external_inputs, environment_after: after.environment });
  result.fixture_inputs_after = fixtureInputs();
  if (JSON.stringify(result.fixture_inputs_before) !== JSON.stringify(result.fixture_inputs_after)) result.failures.push('RAW_FIXTURE_DRIFT');
  if (before) result.failures.push(...snapshotDrift(before, after));
} catch (error) { result.failures.push(clean(`After snapshot: ${error}`)); }
result.finished_utc = new Date().toISOString();
result.status = !result.failures.length && result.checks.length === gates.length && result.checks.every(check => check.status === 'PASS') ? 'PASS' : 'FAIL';
result.exit_code = result.status === 'PASS' ? 0 : 1;
attempt.write('summary.json', clean(JSON.stringify(result, null, 2)) + '\n');
console.log(JSON.stringify({ source_sha: result.source_sha, status: result.status, exit_code: result.exit_code, failures: result.failures }));
process.exitCode = result.exit_code;
