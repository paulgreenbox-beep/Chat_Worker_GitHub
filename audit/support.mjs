// Local audit helpers. Existing-link checks are not an OS security boundary
// against a hostile process racing filesystem operations.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, lstatSync, mkdirSync, readFileSync, readdirSync, realpathSync, writeFileSync } from 'node:fs';
import { delimiter, dirname, isAbsolute, join, parse, relative, resolve } from 'node:path';
import os from 'node:os';
export const hash = value => createHash('sha256').update(value).digest('hex');
const escapeRegex = value => value.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const samePath = (a, b) => process.platform === 'win32' ? a.toLowerCase() === b.toLowerCase() : a === b;

export function redact(value, root, home = os.homedir()) {
  for (const [path, alias] of [[root, '<workspace>'], [home, '<user-home>']]) {
    if (!path) continue;
    const variants = [...new Set([path.replaceAll('\\', '\\\\'), path, path.replaceAll('\\', '/')])].sort((a, b) => b.length - a.length);
    for (const variant of variants) value = value.replace(new RegExp(escapeRegex(variant), /^[A-Za-z]:[\\/]/.test(path) ? 'gi' : 'g'), alias);
  }
  return value;
}

export function publicationFindings(text) {
  const paths = text.replace(/\\{2,}/g, '\\');
  const findings = [];
  if (/(?:\b[A-Za-z]:[\\/](?![\\/])|\/home\/|\/Users\/)/i.test(paths)) findings.push('private-host-path');
  if (/(?:gh[pousr]_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{20,}|-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----)/.test(text)) findings.push('credential-pattern');
  return findings;
}

export function runCommand(command, args, { cwd, env = process.env } = {}) {
  const result = spawnSync(command, args, { cwd, env, encoding: 'utf8', windowsHide: true, maxBuffer: 16 * 1024 * 1024 });
  return { exit_code: result.status, stdout: result.stdout ?? '', stderr: result.stderr ?? '', error: result.error ? String(result.error) : null };
}

function directoryInfo(path) {
  const stat = lstatSync(path, { bigint: true });
  if (stat.isSymbolicLink() || !stat.isDirectory() || !samePath(realpathSync(path), resolve(path))) throw new Error('Evidence path must be an ordinary directory without links/junctions');
  return { dev: stat.dev, ino: stat.ino, born: stat.birthtimeNs };
}
function checkDirectoryChain(path) {
  const absolute = resolve(path);
  let current = parse(absolute).root;
  directoryInfo(current);
  for (const part of relative(current, absolute).split(/[\\/]/).filter(Boolean)) { current = join(current, part); directoryInfo(current); }
}
export function reserveAttempt(root, attempt) {
  if (!/^[a-z][a-z0-9-]{0,63}$/.test(attempt ?? '')) throw new Error('Use a simple, fresh attempt name');
  root = resolve(root); checkDirectoryChain(root);
  let parent = root;
  for (const part of ['audit', 'evidence']) {
    parent = join(parent, part);
    try { mkdirSync(parent); } catch (error) { if (error.code !== 'EEXIST') throw error; }
    checkDirectoryChain(parent);
  }
  const directory = join(parent, attempt);
  mkdirSync(directory); // Atomic reservation: no recursive creation or exists-then-create.
  const identity = directoryInfo(directory);
  return { directory, write(name, content) {
    if (!/^[a-z0-9-]+\.(?:json|txt)$/.test(name)) throw new Error('Invalid evidence filename');
    checkDirectoryChain(directory);
    const current = directoryInfo(directory);
    if (Object.keys(identity).some(key => identity[key] !== current[key])) throw new Error('Evidence directory identity changed');
    writeFileSync(join(directory, name), content, { encoding: 'utf8', flag: 'wx' });
  } };
}
export function git(run, root, args, env) {
  const result = run('git', args, { cwd: root, env });
  if (result.exit_code !== 0 || result.error) throw new Error(`git ${args[0]} failed`);
  return result.stdout;
}
export function repositoryFiles(root, run = runCommand, env = process.env) {
  return [...new Set(git(run, root, ['ls-files', '--cached', '--others', '--exclude-standard', '-z'], env).split('\0').filter(Boolean))].sort();
}
export function readRepositoryFile(root, file) {
  const path = resolve(root, file);
  const within = relative(resolve(root), path);
  if (within.startsWith('..') || isAbsolute(within)) throw new Error('Repository path escapes root');
  checkDirectoryChain(dirname(path));
  const stat = lstatSync(path);
  if (stat.isSymbolicLink() || !stat.isFile()) throw new Error('Publication input must be an ordinary file');
  return readFileSync(path);
}
const scoped = path => !path.startsWith('audit/evidence/') && (
  /(?:\.rs$|(?:^|\/)Cargo\.(?:toml|lock)$|(?:^|\/)rust-toolchain(?:\.toml)?$)/.test(path)
  || /(?:^|\/)\.(?:gitignore|gitattributes|gitmodules)$/.test(path)
  || /(?:^|\/)\.cargo\/(?:config(?:\.toml)?|[^/]+\.toml)$/.test(path)
  || /\.(?:sh|cmd|bat|ps1|py)$/.test(path) || /^audit\/[^/]+\.mjs$/.test(path)
);
const environmentNames = [
  'RUSTC', 'RUSTC_WRAPPER', 'RUSTC_WORKSPACE_WRAPPER', 'RUSTFLAGS', 'CARGO_ENCODED_RUSTFLAGS',
  'RUSTDOC', 'RUSTDOCFLAGS', 'CARGO_ENCODED_RUSTDOCFLAGS', 'RUSTUP_TOOLCHAIN', 'RUSTUP_HOME',
  'CARGO_HOME', 'CARGO_TARGET_DIR', 'CARGO_INCREMENTAL', 'CARGO_BUILD_TARGET', 'CARGO_BUILD_RUSTC',
  'CARGO_BUILD_RUSTC_WRAPPER', 'CARGO_BUILD_RUSTC_WORKSPACE_WRAPPER', 'CARGO_BUILD_RUSTFLAGS',
];
export function environmentFingerprint(env) {
  const names = new Set(environmentNames);
  for (const name of Object.keys(env)) if (/^CARGO_PROFILE_[A-Z0-9_]+$/.test(name) || /^CARGO_TARGET_[A-Z0-9_]+_(?:RUSTFLAGS|LINKER|RUNNER)$/.test(name)) names.add(name);
  // Flags may contain private data; values are never published.
  return Object.fromEntries([...names].sort().map(name => [name, env[name] === undefined ? null : hash(env[name])]));
}
function fileFingerprint(path) {
  if (!existsSync(path)) return null;
  const stat = lstatSync(path);
  if (stat.isSymbolicLink() || !stat.isFile()) throw new Error('Source input must be an ordinary file');
  return hash(readFileSync(path));
}
function resolveWrapper(value, root, env) {
  if (!value) return null;
  if (isAbsolute(value) || /[\\/]/.test(value)) return resolve(root, value);
  for (const directory of (env.PATH ?? '').split(delimiter)) {
    const extensions = process.platform === 'win32' ? ['', ...(env.PATHEXT ?? '.EXE;.CMD;.BAT').split(';')] : [''];
    for (const extension of extensions) { const candidate = resolve(directory, value + extension); if (existsSync(candidate)) return candidate; }
  }
  return null;
}
export function snapshot(root, run = runCommand, env = process.env) {
  const files = new Set(repositoryFiles(root, run, env).filter(scoped));
  for (const name of ['Cargo.lock', 'rust-toolchain', 'rust-toolchain.toml', '.cargo/config', '.cargo/config.toml']) if (existsSync(resolve(root, name))) files.add(name);
  if (existsSync(join(root, 'audit'))) for (const name of readdirSync(join(root, 'audit'))) if (name.endsWith('.mjs')) files.add(`audit/${name}`);
  const inputs = Object.fromEntries([...files].sort().map(file => [file, hash(readRepositoryFile(root, file))]));
  const external = {};
  let ancestor = dirname(resolve(root));
  for (let depth = 1; ; depth++) {
    for (const name of ['config', 'config.toml']) external[`ancestor-${depth}/${name}`] = fileFingerprint(join(ancestor, '.cargo', name));
    if (dirname(ancestor) === ancestor) break;
    ancestor = dirname(ancestor);
  }
  for (const name of ['config', 'config.toml']) external[`cargo-home/${name}`] = fileFingerprint(join(env.CARGO_HOME || join(os.homedir(), '.cargo'), name));
  for (const name of environmentNames.filter(name => name.includes('WRAPPER'))) {
    const wrapper = resolveWrapper(env[name], root, env);
    external[`environment/${name}`] = wrapper ? { path_sha256: hash(wrapper), file_sha256: fileFingerprint(wrapper) } : null;
  }
  const head = git(run, root, ['rev-parse', 'HEAD'], env).trim();
  const headFiles = git(run, root, ['ls-tree', '-r', '--name-only', '-z', head], env).split('\0').filter(scoped).sort();
  const expected = Object.fromEntries(headFiles.map(file => [file, hash(git(run, root, ['show', `${head}:${file}`], env))]));
  return { head, inputs, external_inputs: external, environment: environmentFingerprint(env), inputs_match_head: JSON.stringify(inputs) === JSON.stringify(expected) };
}
export function snapshotDrift(before, after) {
  const reasons = [];
  if (before.head !== after.head) reasons.push('HEAD_DRIFT');
  if (JSON.stringify(before.inputs) !== JSON.stringify(after.inputs)) reasons.push('SOURCE_DRIFT');
  if (JSON.stringify(before.external_inputs) !== JSON.stringify(after.external_inputs)) reasons.push('EXTERNAL_INPUT_DRIFT');
  if (JSON.stringify(before.environment) !== JSON.stringify(after.environment)) reasons.push('ENVIRONMENT_DRIFT');
  return reasons;
}
export const provenanceLimits = [
  'Before/after fingerprints detect net drift, not transient changes restored between snapshots (ABA).',
  'Scope: Rust/manifests/lock, Cargo/toolchain configuration, script wrappers/build scripts and audit executables; not every generated/include input.',
  'Whitelisted compiler/profile environment is fingerprinted without values; this is not complete host/toolchain provenance.',
  'External wrappers referenced only inside Cargo configuration and mutable registry/tool binaries are not fully pinned.',
];
