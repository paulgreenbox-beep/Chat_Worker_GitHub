// Read-only Git ancestry gate. This does not fetch, merge, compile or certify code.
import { spawnSync } from 'node:child_process';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const SHA = /^[0-9a-f]{40}$/;
const SAFE_REF = /^(?:refs\/(?:heads|remotes)\/)?[A-Za-z0-9][A-Za-z0-9_./-]{0,199}$/;
export const DEFAULT_EDGES = Object.freeze([
  Object.freeze(['origin/codex/cwg-build-test-audit', 'origin/codex/cwg-github-inbox']),
  Object.freeze(['origin/codex/cwg-github-inbox', 'origin/codex/cwg-mcp-stdio']),
]);

function validRef(value) {
  return typeof value === 'string' && SAFE_REF.test(value)
    && !value.includes('..') && !value.includes('//')
    && !value.endsWith('/') && !value.endsWith('.lock');
}
function git(repo, argv) {
  // Ignore process-level Git routing/config overrides. No shell or repository
  // scripts/hooks execute: only rev-parse and merge-base are permitted below.
  const env = Object.fromEntries(Object.entries(process.env)
    .filter(([key]) => !/^GIT_/i.test(key)));
  Object.assign(env, { GIT_CONFIG_NOSYSTEM: '1', GIT_CONFIG_GLOBAL: process.platform === 'win32' ? 'NUL' : '/dev/null',
    GIT_TERMINAL_PROMPT: '0', GIT_OPTIONAL_LOCKS: '0', GIT_NO_REPLACE_OBJECTS: '1', GIT_NO_LAZY_FETCH: '1' });
  const result = spawnSync('git', ['--no-replace-objects', '-C', repo, ...argv], {
    env, encoding: 'utf8', timeout: 5000, maxBuffer: 128 * 1024, windowsHide: true, shell: false,
  });
  return result.error || result.signal
    ? { status: null, stdout: '' }
    : { status: result.status, stdout: result.stdout.trim() };
}
function resolve(repo, ref) {
  const result = git(repo, ['rev-parse', '--verify', '--end-of-options', `${ref}^{commit}`]);
  return result.status === 0 && SHA.test(result.stdout) ? result.stdout : null;
}
export function inspectStack(repo, edges = DEFAULT_EDGES) {
  if (typeof repo !== 'string' || !path.isAbsolute(repo) || !Array.isArray(edges)
      || edges.length === 0 || edges.length > 32
      || edges.some(edge => !Array.isArray(edge) || edge.length !== 2 || !edge.every(validRef))) {
    throw new Error('INVALID_STACK_INPUT');
  }
  const repository = git(repo, ['rev-parse', '--is-shallow-repository']);
  if (repository.status !== 0 || repository.stdout !== 'false') {
    return { schema: 'cwg-stack-audit-v1', status: 'BLOCKED', reason: 'FULL_LOCAL_HISTORY_REQUIRED', edges: [] };
  }
  const rows = edges.map(([parent, child]) => {
    const parentSha = resolve(repo, parent);
    const childSha = resolve(repo, child);
    if (!parentSha || !childSha) return { parent, child, parent_sha: parentSha, child_sha: childSha, status: 'BLOCKED', reason: 'REF_UNAVAILABLE' };
    const relation = git(repo, ['merge-base', '--is-ancestor', parentSha, childSha]);
    // A concurrent branch move makes even a successful ancestry result stale.
    if (resolve(repo, parent) !== parentSha || resolve(repo, child) !== childSha) {
      return { parent, child, parent_sha: parentSha, child_sha: childSha, status: 'BLOCKED', reason: 'REF_CHANGED' };
    }
    return { parent, child, parent_sha: parentSha, child_sha: childSha,
      status: relation.status === 0 ? 'PASS' : 'BLOCKED',
      reason: relation.status === 0 ? 'PARENT_INCLUDED' : relation.status === 1 ? 'STALE_DEPENDENCY' : 'GIT_OBSERVATION_FAILED' };
  });
  return { schema: 'cwg-stack-audit-v1', status: rows.every(row => row.status === 'PASS') ? 'PASS' : 'BLOCKED',
    scope: 'local Git ancestry only; remote freshness and runtime checks are separate', edges: rows };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  try {
    const args = process.argv.slice(2);
    if (args.length > 1) throw new Error('USAGE');
    const result = inspectStack(path.resolve(args[0] ?? '.'));
    console.log(JSON.stringify(result, null, 2));
    process.exitCode = result.status === 'PASS' ? 0 : 2;
  } catch {
    console.error('CWG_STACK_AUDIT_REFUSED');
    process.exitCode = 2;
  }
}
