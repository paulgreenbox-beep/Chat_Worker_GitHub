// Reproduce the requested Cargo gates without a shell or extra npm packages.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readFileSync, writeFileSync, existsSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import os from 'node:os';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const phase = process.argv[2];
if (!/^(baseline|final(?:-[1-9][0-9]*)?|protocol-before|router-before)$/.test(phase ?? '')) throw new Error('Unknown phase');
const directory = resolve(root, 'audit', 'evidence', phase);
if (existsSync(resolve(directory, 'summary.json'))) throw new Error('Preserve existing evidence; use a new final-N attempt');
mkdirSync(directory, { recursive: true });
const hash = value => createHash('sha256').update(value).digest('hex');
const sanitize = value => {
  for (const [path, alias] of [[root, '<workspace>'], [os.homedir(), '<user-home>']]) {
    for (const variant of [path.replaceAll('\\', '\\\\'), path, path.replaceAll('\\', '/')]) {
      value = value.replaceAll(variant, alias);
    }
  }
  return value;
};
const run = (command, args) => {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', windowsHide: true });
  return { exit_code: result.status, stdout: result.stdout ?? '', stderr: result.stderr ?? '',
    error: result.error ? sanitize(String(result.error)) : null };
};
const source = run('git', ['rev-parse', 'HEAD']);
if (source.exit_code !== 0) throw new Error(source.stderr);
const inputs = run('git', ['ls-files', '--cached', '--others', '--exclude-standard', '-z']).stdout.split('\0')
  .filter(path => /(?:\.rs$|(?:^|\/)Cargo\.(?:toml|lock)$)/.test(path));
if (existsSync(resolve(root, 'Cargo.lock')) && !inputs.includes('Cargo.lock')) inputs.push('Cargo.lock');
const inputHashes = () => Object.fromEntries(inputs.sort().map(path => [path, hash(readFileSync(resolve(root, path)))]));
const inputsMatchHead = inputs.every(path => {
  const blob = run('git', ['show', `HEAD:${path}`]);
  return blob.exit_code === 0 && hash(blob.stdout) === hash(readFileSync(resolve(root, path)));
});
if (phase.startsWith('final') && !inputsMatchHead) throw new Error('Commit Rust inputs before final verification');
const summary = {
  source_sha: source.stdout.trim(), phase, started_utc: new Date().toISOString(),
  rust_inputs_match_head: inputsMatchHead,
  platform: process.platform, architecture: process.arch, os_release: os.release(),
  rustc: run('rustc', ['--version', '--verbose']), cargo: run('cargo', ['--version']),
  inputs_before: inputHashes(), checks: [],
};
const gates = phase === 'protocol-before' ? [['test', ['test', '-p', 'cwg-protocol', '--test', 'validation']]]
  : phase === 'router-before' ? [['test', ['test', '-p', 'cwg-router', '--test', 'audit_regressions']]] : [
  ['metadata', ['metadata', '--format-version', '1']],
  ['fmt', ['fmt', '--all', '--', '--check']],
  ['check', ['check', '--workspace', '--all-targets']],
  ['test', ['test', '--workspace', '--all-targets']],
  ['clippy', ['clippy', '--workspace', '--all-targets', '--', '-D', 'warnings']],
  ['debug', ['build', '--workspace']],
  ['release', ['build', '--workspace', '--release']],
];
for (const [name, args] of gates) {
  const result = run('cargo', args);
  const output = sanitize(`command: cargo ${args.join(' ')}\nexit_code: ${result.exit_code}\nstdout:\n${result.stdout}\nstderr:\n${result.stderr}\nerror: ${result.error}\n`);
  const log = `${name}.txt`;
  // Lossless encoding preserves whitespace in a failing rustfmt diff without
  // making the evidence artifact itself fail git diff --check.
  const encoding = /[ \t]+$/m.test(output) ? 'json-envelope' : 'plain-text';
  const published = encoding === 'json-envelope' ? JSON.stringify({transcript: output}, null, 2) + '\n' : output;
  writeFileSync(resolve(directory, log), published);
  const matches = [...result.stdout.matchAll(/test result: \w+\. (\d+) passed; (\d+) failed; (\d+) ignored/g)];
  const tests = name === 'test' ? {
    passed: matches.reduce((sum, match) => sum + Number(match[1]), 0),
    failed: matches.reduce((sum, match) => sum + Number(match[2]), 0),
    ignored: matches.reduce((sum, match) => sum + Number(match[3]), 0),
  } : null;
  const entry = { command: `cargo ${args.join(' ')}`, exit_code: result.exit_code,
    status: result.exit_code === 0 ? 'PASS' : 'FAIL', tests, log, log_encoding: encoding,
    transcript_sha256: hash(output), log_sha256: hash(published) };
  summary.checks.push(entry);
  console.log(JSON.stringify(entry));
}
summary.finished_utc = new Date().toISOString();
// metadata may create the lockfile on the baseline run.
if (existsSync(resolve(root, 'Cargo.lock')) && !inputs.includes('Cargo.lock')) inputs.push('Cargo.lock');
summary.inputs_after = inputHashes();
writeFileSync(resolve(directory, 'summary.json'), sanitize(JSON.stringify(summary, null, 2)) + '\n');
process.exitCode = summary.checks.every(check => check.status === 'PASS') ? 0 : 1;
