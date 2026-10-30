// Final attempts require committed scoped inputs and reject observed source drift.
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';
import { hash, redact, reserveAttempt, runCommand, snapshot, snapshotDrift, provenanceLimits } from './support.mjs';
const rootDirectory = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const gates = [
  ['metadata', ['metadata', '--format-version', '1']],
  ['fmt', ['fmt', '--all', '--', '--check']],
  ['check', ['check', '--workspace', '--all-targets']],
  ['test', ['test', '--workspace', '--all-targets']],
  ['clippy', ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings']],
  ['debug', ['build', '--workspace']],
  ['release', ['build', '--workspace', '--release']],
];
export function runVerification({ root = rootDirectory, phase, run = runCommand, env = process.env, emit = console.log } = {}) {
  if (!/^(baseline|final(?:-[1-9][0-9]*)?|protocol-before|router-before)$/.test(phase ?? '')) throw new Error('Unknown phase');
  const attempt = reserveAttempt(root, phase);
  const clean = value => redact(value, root);
  const summary = { phase, started_utc: new Date().toISOString(), platform: process.platform,
    architecture: process.arch, os_release: os.release(), checks: [], failures: [], provenance_limits: provenanceLimits };
  let before;
  try {
    before = snapshot(root, run, env);
    summary.source_sha = before.head;
    summary.inputs_before = before.inputs;
    summary.external_inputs_before = before.external_inputs;
    summary.environment_before = before.environment;
    summary.scoped_inputs_match_head = before.inputs_match_head;
    if (phase.startsWith('final') && !before.inputs_match_head) throw new Error('Commit scoped build/audit inputs before final verification');
    summary.rustc = run('rustc', ['--version', '--verbose'], { cwd: root, env });
    summary.cargo = run('cargo', ['--version'], { cwd: root, env });
    if (summary.rustc.exit_code !== 0 || summary.cargo.exit_code !== 0) throw new Error('Tool version check failed');
    const selected = phase === 'protocol-before' ? [['test', ['test', '-p', 'cwg-protocol', '--test', 'validation']]]
      : phase === 'router-before' ? [['test', ['test', '-p', 'cwg-router', '--test', 'audit_regressions']]] : gates;
    for (const [name, args] of selected) {
      const result = run('cargo', args, { cwd: root, env });
      const output = clean(`command: cargo ${args.join(' ')}\nexit_code: ${result.exit_code}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}\nerror: ${result.error}\n`);
      const encoding = /[ \t]+$/m.test(output) ? 'json-envelope' : 'plain-text';
      const published = encoding === 'json-envelope' ? JSON.stringify({ transcript: output }, null, 2) + '\n' : output;
      const matches = [...result.stdout.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
      const tests = name === 'test' ? {
        passed: matches.reduce((sum, match) => sum + Number(match[1]), 0),
        failed: matches.reduce((sum, match) => sum + Number(match[2]), 0),
        ignored: matches.reduce((sum, match) => sum + Number(match[3]), 0),
      } : null;
      const verified = result.exit_code === 0 && !result.error && (!tests || (matches.length > 0 && tests.passed > 0 && tests.failed === 0 && tests.ignored === 0));
      const entry = { command: `cargo ${args.join(' ')}`, exit_code: result.exit_code,
        status: verified ? 'PASS' : 'FAIL', tests, log: `${name}.txt`, log_encoding: encoding,
        transcript_sha256: hash(output), log_sha256: hash(published) };
      attempt.write(entry.log, published);
      summary.checks.push(entry);
      emit(JSON.stringify(entry));
    }
  } catch (error) { summary.failures.push(clean(String(error))); }
  try {
    const after = snapshot(root, run, env);
    summary.source_sha_after = after.head;
    summary.inputs_after = after.inputs;
    summary.external_inputs_after = after.external_inputs;
    summary.environment_after = after.environment;
    if (before) summary.failures.push(...snapshotDrift(before, after));
  } catch (error) { summary.failures.push(clean(`Final snapshot failed: ${error}`)); }
  summary.finished_utc = new Date().toISOString();
  summary.status = summary.failures.length === 0 && summary.checks.length > 0 && summary.checks.every(check => check.status === 'PASS') ? 'PASS' : 'FAIL';
  summary.exit_code = summary.status === 'PASS' ? 0 : 1;
  attempt.write('summary.json', clean(JSON.stringify(summary, null, 2)) + '\n');
  emit(JSON.stringify({ status: summary.status, exit_code: summary.exit_code, failures: summary.failures }));
  return summary;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try { process.exitCode = runVerification({ phase: process.argv[2] }).exit_code; }
  catch (error) { console.error(redact(String(error), rootDirectory)); process.exitCode = 1; }
}
