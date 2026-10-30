// These isolated fixtures simulate Cargo output to test the audit runner.
// No Cargo build/test is executed; fixtures remain archived under .scratch.
import test from 'node:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { environmentFingerprint, hash, publicationFindings, redact, reserveAttempt, runCommand, snapshot, snapshotDrift } from './support.mjs';
import { runVerification } from './verify.mjs';
import { runReview } from './review.mjs';

const archive = join(dirname(fileURLToPath(import.meta.url)), '.scratch', `tooling-${Date.now()}-${process.pid}`);
mkdirSync(archive, { recursive: true });
let number = 0;
function fixture() {
  const root = join(archive, `fixture-${++number}`);
  mkdirSync(join(root, 'src'), { recursive: true });
  mkdirSync(join(root, 'audit'));
  writeFileSync(join(root, 'Cargo.toml'), '[package]\nname="fixture"\nversion="0.1.0"\nedition="2024"\n');
  writeFileSync(join(root, 'src', 'lib.rs'), 'pub fn fixture() {}\n');
  writeFileSync(join(root, 'audit', 'fixture.mjs'), '// fixture audit executable\n');
  writeFileSync(join(root, '.gitignore'), 'audit/evidence/\n');
  git(root, ['init', '--quiet', '-b', 'main']);
  git(root, ['add', '.']);
  git(root, ['-c', 'user.name=CWG-audit-tests', '-c', 'user.email=tests@example.invalid', 'commit', '--quiet', '-m', 'fixture']);
  return root;
}
function git(root, args) {
  const result = runCommand('git', args, { cwd: root });
  assert.equal(result.exit_code, 0, result.stderr);
  return result.stdout;
}
const success = stdout => ({ exit_code: 0, stdout, stderr: '', error: null });
function fakeGates(onGate = () => {}, testOutput = 'test result: ok. 1 passed; 0 failed; 0 ignored;') {
  return (command, args, options) => {
    if (command === 'git') return runCommand(command, args, options);
    if (command === 'rustc' || (command === 'cargo' && args[0] === '--version')) return success('SYNTHETIC UNIT TEST VERSION\n');
    assert.equal(command, 'cargo');
    onGate(args, options);
    return success('SYNTHETIC UNIT TEST FIXTURE: no Cargo executed\n' + (args[0] === 'test' ? testOutput : ''));
  };
}
const quiet = () => {};
function assertFixtureSummary(root, result) {
  const saved = JSON.parse(readFileSync(join(root, 'audit', 'evidence', result.phase, 'summary.json'), 'utf8'));
  assert.equal(saved.exit_code, result.exit_code);
  assert.equal(saved.status, result.status);
}

test('new attempts reject repeated and incomplete destinations without replacing bytes', () => {
  const root = fixture(); const attempt = reserveAttempt(root, 'final-1');
  attempt.write('test.txt', 'original');
  assert.throws(() => attempt.write('test.txt', 'replacement'), /EEXIST/);
  assert.throws(() => reserveAttempt(root, 'final-1'), /EEXIST/);
  assert.equal(readFileSync(join(attempt.directory, 'test.txt'), 'utf8'), 'original');
  mkdirSync(join(root, 'audit', 'evidence', 'partial'));
  assert.throws(() => reserveAttempt(root, 'partial'), /EEXIST/);
  assert.throws(() => reserveAttempt(root, '../escape'), /attempt name/);
});

test('existing non-directory parent is refused', () => {
  const root = fixture(); writeFileSync(join(root, 'audit', 'evidence'), 'keep');
  assert.throws(() => reserveAttempt(root, 'review-1'), /ordinary directory/);
  assert.equal(readFileSync(join(root, 'audit', 'evidence'), 'utf8'), 'keep');
});

function linkDirectory(t, target, link) {
  try { symlinkSync(target, link, process.platform === 'win32' ? 'junction' : 'dir'); }
  catch (error) { if (process.platform === 'win32' && ['EPERM', 'EACCES'].includes(error.code)) { t.skip(`Windows refused fixture link creation: ${error.code}`); return false; } throw error; }
  return true;
}

test('existing symlink/junction parent is refused before evidence writes', t => {
  const root = fixture(); const elsewhere = join(archive, `link-target-${number}`); mkdirSync(elsewhere);
  if (!linkDirectory(t, elsewhere, join(root, 'audit', 'evidence'))) return;
  assert.throws(() => reserveAttempt(root, 'final-1'), /links\/junctions/);
  assert.equal(existsSync(join(elsewhere, 'final-1')), false);
});

test('exclusive artifact write cannot follow a pre-existing file link', t => {
  const root = fixture(); const attempt = reserveAttempt(root, 'final-1');
  const target = join(root, 'keep.txt'); writeFileSync(target, 'keep');
  try { symlinkSync(target, join(attempt.directory, 'test.txt'), 'file'); }
  catch (error) { if (process.platform === 'win32' && ['EPERM', 'EACCES'].includes(error.code)) { t.skip(`Windows refused fixture file-link creation: ${error.code}`); return; } throw error; }
  assert.throws(() => attempt.write('test.txt', 'replace'), /EEXIST/);
  assert.equal(readFileSync(target, 'utf8'), 'keep');
});

test('stable committed scoped inputs pass simulated gates and retain test counts', () => {
  const root = fixture(); const result = runVerification({ root, phase: 'final-1', run: fakeGates(), emit: quiet });
  assert.equal(result.status, 'PASS'); assert.equal(result.exit_code, 0);
  assert.equal(result.scoped_inputs_match_head, true);
  assert.equal(result.checks.length, 7); assert.deepEqual(result.checks.find(check => check.tests).tests, { passed: 1, failed: 0, ignored: 0 });
  assertFixtureSummary(root, result);
});

test('source mutation during otherwise-successful gates makes final evidence fail', () => {
  const root = fixture(); let mutated = false;
  const result = runVerification({ root, phase: 'final-1', emit: quiet, run: fakeGates(() => {
    if (!mutated) { writeFileSync(join(root, 'src', 'lib.rs'), 'pub fn changed() {}\n'); mutated = true; }
  }) });
  assert.equal(result.exit_code, 1); assert.equal(result.status, 'FAIL'); assert.ok(result.failures.includes('SOURCE_DRIFT'));
  assert.ok(result.checks.every(check => check.exit_code === 0)); assertFixtureSummary(root, result);
});

test('new source input added during gates is part of the failing after-inventory', () => {
  const root = fixture(); let added = false;
  const result = runVerification({ root, phase: 'final-1', emit: quiet, run: fakeGates(() => {
    if (!added) { writeFileSync(join(root, 'src', 'added.rs'), '// new input\n'); added = true; }
  }) });
  assert.equal(result.exit_code, 1); assert.ok(result.failures.includes('SOURCE_DRIFT'));
  assert.ok(Object.hasOwn(result.inputs_after, 'src/added.rs'));
});

test('HEAD change during gates invalidates the source identity', () => {
  const root = fixture(); let committed = false;
  const result = runVerification({ root, phase: 'final-1', emit: quiet, run: fakeGates(() => {
    if (!committed) { git(root, ['-c', 'user.name=CWG-audit-tests', '-c', 'user.email=tests@example.invalid', 'commit', '--quiet', '--allow-empty', '-m', 'head-drift']); committed = true; }
  }) });
  assert.equal(result.exit_code, 1); assert.ok(result.failures.includes('HEAD_DRIFT'));
});

test('uncommitted Cargo config, toolchain, wrapper and audit tools block final gates', () => {
  for (const file of ['.cargo/config.toml', 'rust-toolchain.toml', 'wrapper.cmd', 'build.rs', 'audit/new.mjs']) {
    const root = fixture(); mkdirSync(dirname(join(root, file)), { recursive: true }); writeFileSync(join(root, file), '# uncommitted input\n');
    let gates = 0; const result = runVerification({ root, phase: 'final-1', emit: quiet, run: fakeGates(() => gates++) });
    assert.equal(result.exit_code, 1, file); assert.equal(gates, 0, file); assert.equal(result.scoped_inputs_match_head, false, file);
  }
});

test('profile/compiler environment is hashed and changes invalidate the run', () => {
  const root = fixture(); const env = { ...process.env, RUSTFLAGS: 'private-flag-value', UNRELATED_TEST_SECRET: 'never-publish' };
  let changed = false;
  const result = runVerification({ root, phase: 'final-1', env, emit: quiet, run: fakeGates(() => { if (!changed) { env.RUSTFLAGS = 'different-private-value'; changed = true; } }) });
  assert.equal(result.exit_code, 1); assert.ok(result.failures.includes('ENVIRONMENT_DRIFT'));
  const saved = readFileSync(join(root, 'audit', 'evidence', 'final-1', 'summary.json'), 'utf8');
  assert.equal(saved.includes('private-flag-value'), false); assert.equal(saved.includes('never-publish'), false);
  assert.equal(environmentFingerprint(env).RUSTFLAGS, hash('different-private-value'));
  assert.equal(Object.hasOwn(environmentFingerprint(env), 'UNRELATED_TEST_SECRET'), false);
});

test('resolved environment wrapper bytes and external Cargo config are fingerprinted', () => {
  const root = fixture(); const wrapper = join(archive, `external-wrapper-${number}.cmd`); writeFileSync(wrapper, 'first');
  const cargoHome = join(archive, `cargo-home-${number}`); mkdirSync(cargoHome); writeFileSync(join(cargoHome, 'config.toml'), '# first\n');
  const env = { ...process.env, RUSTC_WRAPPER: wrapper, CARGO_HOME: cargoHome };
  const before = snapshot(root, runCommand, env);
  writeFileSync(wrapper, 'second'); writeFileSync(join(cargoHome, 'config.toml'), '# second\n');
  const after = snapshot(root, runCommand, env);
  assert.ok(snapshotDrift(before, after).includes('EXTERNAL_INPUT_DRIFT'));
  assert.equal(JSON.stringify(after).includes(wrapper), false);
});

test('successful command without observed test summaries is not test PASS', () => {
  const root = fixture(); const result = runVerification({ root, phase: 'final-1', run: fakeGates(() => {}, ''), emit: quiet });
  assert.equal(result.exit_code, 1); assert.equal(result.checks.find(check => check.tests).status, 'FAIL');
});

test('Windows casing and JSON escaping cannot bypass path redaction/scan', () => {
  const home = ['C:', 'Users', 'Fixture'].join('\\'); const root = home + '\\Project';
  for (const path of [root.toUpperCase(), root.toLowerCase(), root.replaceAll('\\', '/'), root.replaceAll('\\', '\\\\')]) {
    const input = `${path}\\src`;
    const redacted = redact(input, root, home);
    assert.ok(redacted.includes('<workspace>')); assert.equal(publicationFindings(redacted).includes('private-host-path'), false);
    assert.ok(publicationFindings(input).includes('private-host-path'));
  }
  const token = ['gh', 'p_', 'a'.repeat(25)].join('');
  assert.ok(publicationFindings(token).includes('credential-pattern'));
});

test('separate source review attempts preserve historical and prior artifacts', () => {
  const root = fixture(); mkdirSync(join(root, 'audit', 'evidence')); writeFileSync(join(root, 'audit', 'evidence', 'source-review.json'), 'historical');
  const metadata = { resolve: { nodes: [] }, workspace_members: [], packages: [] };
  const fake = (command, args, options) => command === 'cargo' ? success(JSON.stringify(metadata)) : runCommand(command, args, options);
  const first = runReview({ root, phase: 'review-1', run: fake, emit: quiet }); assert.equal(first.exit_code, 0);
  assert.throws(() => runReview({ root, phase: 'review-1', run: fake, emit: quiet }), /EEXIST/);
  assert.equal(runReview({ root, phase: 'review-2', run: fake, emit: quiet }).exit_code, 0);
  assert.equal(readFileSync(join(root, 'audit', 'evidence', 'source-review.json'), 'utf8'), 'historical');
});

test('source review records overall failure separately from successful metadata', () => {
  const root = fixture(); const path = ['D:', 'Users', 'Fixture'].join('\\'); writeFileSync(join(root, 'notes.txt'), path);
  const metadata = { resolve: { nodes: [] }, workspace_members: [], packages: [] };
  const fake = (command, args, options) => command === 'cargo' ? success(JSON.stringify(metadata)) : runCommand(command, args, options);
  const result = runReview({ root, phase: 'review-1', run: fake, emit: quiet });
  assert.equal(result.metadata.exit_code, 0); assert.equal(result.exit_code, 1); assert.equal(result.status, 'ISSUES_FOUND');
  assert.ok(result.findings.some(finding => finding.kind === 'private-host-path'));
});
