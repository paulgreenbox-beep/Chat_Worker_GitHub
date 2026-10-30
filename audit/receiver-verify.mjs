// Native Receiver/State verification. Cargo runs serially and offline; no credentials are logged.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { hash, redact, reserveAttempt, runCommand, snapshot, snapshotDrift, provenanceLimits } from './support.mjs';
const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const phase = process.argv[2];
if (!/^(?:receiver-(?:baseline|final)(?:-[1-9][0-9]*)?|receiver-regression-(?:security|state|github|http|winhttp|retained|commit|workspace)(?:-[1-9][0-9]*)?)$/.test(phase ?? '')) throw Error('Use a fresh named receiver attempt');
const attempt = reserveAttempt(root, phase);
const env = { ...process.env, CARGO_NET_OFFLINE: 'true' };
const clean = value => redact(value, root);
const result = { phase, started_utc: new Date().toISOString(), platform: process.platform, architecture: process.arch,
  os_release: os.release(), cargo_net_offline: true, checks: [], failures: [], provenance_limits: provenanceLimits };
const regressions = {
  security: ['test', '-p', 'cwg-security', '--locked', '--offline', 'cng_', '--', '--nocapture'],
  state: ['test', '-p', 'cwg-state', '--test', 'durable', '--locked', '--offline', 'security_regression_', '--', '--nocapture'],
  github: ['test', '-p', 'cwg-github', '--lib', '--locked', '--offline', 'audit_', '--', '--nocapture'],
  http: ['test', '-p', 'cwg-github', '--lib', '--locked', '--offline', 'http_security_regression_', '--', '--nocapture'],
  winhttp: ['test', '-p', 'cwg-github', '--bin', 'cwg-github', '--locked', '--offline', 'audit_', '--', '--nocapture'],
  retained: ['test', '-p', 'cwg-state', '--test', 'retained_access', '--locked', '--offline', 'security_regression_', '--', '--nocapture'],
  commit: ['test', '-p', 'cwg-state', '--lib', '--locked', '--offline', 'commit_tests', '--', '--nocapture'],
  workspace: ['test', '--workspace', '--all-targets', '--locked', '--offline'],
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
  if (phase.startsWith('receiver-final') && !before.inputs_match_head) throw Error('Commit build/audit inputs before final verification');
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
  if (before) result.failures.push(...snapshotDrift(before, after));
} catch (error) { result.failures.push(clean(`After snapshot: ${error}`)); }
result.finished_utc = new Date().toISOString();
result.status = !result.failures.length && result.checks.length === gates.length && result.checks.every(check => check.status === 'PASS') ? 'PASS' : 'FAIL';
result.exit_code = result.status === 'PASS' ? 0 : 1;
attempt.write('summary.json', clean(JSON.stringify(result, null, 2)) + '\n');
console.log(JSON.stringify({ source_sha: result.source_sha, status: result.status, exit_code: result.exit_code, failures: result.failures }));
process.exitCode = result.exit_code;
