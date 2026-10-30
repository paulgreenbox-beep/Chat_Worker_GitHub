import assert from 'node:assert/strict';
import { test } from 'node:test';
import fs from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { spawnSync } from 'node:child_process';
import { inspectStack } from './verify-stack.mjs';

function fixture(run) {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cwg-stack-test-'));
  const git = (...args) => {
    const out = spawnSync('git', ['-C', root, ...args], { encoding: 'utf8', timeout: 5000, shell: false,
      env: { ...process.env, GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null' } });
    assert.equal(out.status, 0, out.stderr);
    return out.stdout.trim();
  };
  try {
    git('init', '-q');
    git('config', 'user.name', 'CWG synthetic test');
    git('config', 'user.email', 'test@example.invalid');
    git('commit', '-q', '--allow-empty', '-m', 'base');
    const base = git('rev-parse', 'HEAD');
    git('branch', 'parent', base);
    git('checkout', '-q', '-b', 'child');
    git('commit', '-q', '--allow-empty', '-m', 'child');
    run({ root, git, base });
  } finally { fs.rmSync(root, { recursive: true, force: true }); }
}

test('observes included parent at exact local SHAs', () => fixture(({ root }) => {
  const out = inspectStack(root, [['parent', 'child']]);
  assert.equal(out.status, 'PASS');
  assert.match(out.edges[0].parent_sha, /^[0-9a-f]{40}$/);
  assert.equal(out.edges[0].reason, 'PARENT_INCLUDED');
}));
test('refuses a sibling update that is missing from the downstream branch', () => fixture(({ root, git }) => {
  git('checkout', '-q', 'parent'); git('commit', '-q', '--allow-empty', '-m', 'security fix');
  assert.equal(inspectStack(root, [['parent', 'child']]).edges[0].reason, 'STALE_DEPENDENCY');
}));
test('accepts an actual merge without modifying any ref itself', () => fixture(({ root, git }) => {
  git('checkout', '-q', 'parent'); git('commit', '-q', '--allow-empty', '-m', 'fix');
  git('checkout', '-q', 'child'); git('merge', '-q', '--no-ff', 'parent', '-m', 'integrate');
  const before = git('show-ref');
  assert.equal(inspectStack(root, [['parent', 'child']]).status, 'PASS');
  assert.equal(git('show-ref'), before);
}));
test('missing refs block rather than appear equivalent', () => fixture(({ root }) => {
  assert.equal(inspectStack(root, [['parent', 'missing']]).edges[0].reason, 'REF_UNAVAILABLE');
}));
test('nonrepository is blocked', () => {
  const root = fs.mkdtempSync(path.join(os.tmpdir(), 'cwg-not-git-'));
  try { assert.equal(inspectStack(root, [['parent', 'child']]).reason, 'FULL_LOCAL_HISTORY_REQUIRED'); }
  finally { fs.rmSync(root, { recursive: true, force: true }); }
});
test('shallow histories cannot prove dependency completeness', () => fixture(({ root, base }) => {
  fs.writeFileSync(path.join(root, '.git', 'shallow'), `${base}\n`);
  assert.equal(inspectStack(root, [['parent', 'child']]).reason, 'FULL_LOCAL_HISTORY_REQUIRED');
}));
test('option and revision-expression injection is refused before spawning Git', () => {
  for (const ref of ['--help', 'HEAD;whoami', 'parent..child', 'HEAD^{tree}', 'HEAD@{0}', 'x\ny', 'a//b', 'a.lock']) {
    assert.throws(() => inspectStack(path.resolve('.'), [[ref, 'child']]), /INVALID_STACK_INPUT/);
  }
});
test('bounds edge count and refuses relative paths', () => {
  assert.throws(() => inspectStack('relative', [['parent', 'child']]), /INVALID_STACK_INPUT/);
  assert.throws(() => inspectStack(path.resolve('.'), []), /INVALID_STACK_INPUT/);
  assert.throws(() => inspectStack(path.resolve('.'), Array(33).fill(['parent', 'child'])), /INVALID_STACK_INPUT/);
});
test('branch names including slashes and fully qualified names are supported', () => fixture(({ root, git }) => {
  git('branch', 'codex/upstream', 'parent'); git('branch', 'codex/downstream', 'child');
  assert.equal(inspectStack(root, [['refs/heads/codex/upstream', 'codex/downstream']]).status, 'PASS');
}));
test('Git replace objects cannot manufacture a passed ancestry check', () => fixture(({ root, git }) => {
  git('checkout', '-q', 'parent'); git('commit', '-q', '--allow-empty', '-m', 'missing fix');
  const changed = git('rev-parse', 'parent');
  git('replace', changed, git('rev-parse', 'child'));
  assert.equal(inspectStack(root, [['parent', 'child']]).status, 'BLOCKED');
}));
test('multi-edge gate requires every dependency', () => fixture(({ root, git }) => {
  git('branch', 'leaf', 'child');
  git('checkout', '-q', 'parent'); git('commit', '-q', '--allow-empty', '-m', 'fix');
  const out = inspectStack(root, [['parent', 'child'], ['child', 'leaf']]);
  assert.equal(out.status, 'BLOCKED');
  assert.deepEqual(out.edges.map(row => row.status), ['BLOCKED', 'PASS']);
}));
