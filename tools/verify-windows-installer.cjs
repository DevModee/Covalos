// Run with Node.js on Windows. Uses a temporary install and refuses an existing COVALOS install.
const fs = require('node:fs/promises');
const path = require('node:path');
const os = require('node:os');
const crypto = require('node:crypto');
const { execFile } = require('node:child_process');
const args = process.argv.slice(2);
const option = name => { const i = args.indexOf(name); return i < 0 ? undefined : args[i + 1]; };
const installer = path.resolve(option('--installer') || 'dist/Covalos-0.4.0-Windows-x64-Setup.exe');
const reportPath = path.resolve(option('--report') || 'artifacts/windows-installer/verification.json');
const binaryPath = option('--binary');
const execute = (exe, args, options = {}) => new Promise((resolve, reject) => execFile(exe, args,
  { windowsHide: true, maxBuffer: 4 * 1024 * 1024, timeout: 120000, ...options },
  (error, stdout, stderr) => error ? reject(Object.assign(error, { stdout, stderr })) : resolve(stdout)));
const registry = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\{9734E261-3523-49A9-9571-D2AA32411CF1}_is1';
const appPathKey = 'HKCU\\Software\\Microsoft\\Windows\\CurrentVersion\\App Paths\\covalos.exe';
const contextKeys = ['*', 'Directory', 'Directory\\Background'].map(item => 'HKCU\\Software\\Classes\\' + item + '\\shell\\Covalos');
const query = async key => {
  const exportPath = path.join(os.tmpdir(), 'covalos-reg-' + crypto.randomUUID() + '.reg');
  try {
    await execute('reg.exe', ['export', key, exportPath, '/y', '/reg:64']);
    // reg export uses UTF-16, preserving non-ASCII install paths on every Windows locale.
    return (await fs.readFile(exportPath, 'utf16le')).replaceAll('\\\\', '\\').replaceAll('\\"', '"');
  } finally { await fs.unlink(exportPath).catch(() => {}); }
};
const exists = async key => { try { await query(key); return true; } catch (e) { if (e.code === 1) return false; throw e; } };
const hash = async file => crypto.createHash('sha256').update(await fs.readFile(file)).digest('hex');
const assert = (condition, message) => { if (!condition) throw new Error(message); };
const report = { installer: path.basename(installer), tests: [], passed: false, shortcuts: 'Compiled definitions inspected; /NOICONS avoids modifying the real Start menu and desktop.' };
const check = (name, condition) => { assert(condition, name); report.tests.push(name); };
(async () => {
  if (process.platform !== 'win32') throw new Error('Windows is required.');
  for (const key of [registry, appPathKey, ...contextKeys]) {
    assert(!(await exists(key)), 'Refusing to disturb an existing installation or integration: ' + key);
  }
  const testDir = await fs.mkdtemp(path.join(os.tmpdir(), 'Covalos Install ñ '));
  const appDir = path.join(testDir, 'Program Files', 'Covalos');
  const profileDir = path.join(testDir, 'profile');
  const evidenceDir = path.dirname(reportPath);
  await fs.mkdir(evidenceDir, { recursive: true });
  await fs.mkdir(profileDir, { recursive: true });
  const sentinel = path.join(profileDir, 'keep-preferences.txt');
  await fs.writeFile(sentinel, 'profile survives reinstall and uninstall');
  const setupArgs = ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/NOICONS', '/LANG=spanish', '/TASKS=contextmenu', '/DIR=' + appDir];
  const installedExe = path.join(appDir, 'covalos.exe');
  const uninstaller = path.join(appDir, 'unins000.exe');
  let installed = false;
  try {
    report.installer_sha256 = await hash(installer);
    await execute(installer, [...setupArgs, '/LOG=' + path.join(evidenceDir, 'install.log')]);
    installed = true;
    check('Installed executable and uninstaller exist', !!(await fs.stat(installedExe)) && !!(await fs.stat(uninstaller)));
    if (binaryPath) check('Installed executable matches release binary', await hash(installedExe) === await hash(binaryPath));
    const uninstallInfo = await query(registry);
    check('Registered in Windows Apps with correct name and version', uninstallInfo.includes('"DisplayName"="COVALOS"') && uninstallInfo.includes('"DisplayVersion"="0.4.0"'));
    check('Install directory with spaces and Unicode preserved', uninstallInfo.includes(appDir));
    check('App Paths points to installed executable', (await query(appPathKey)).includes(installedExe));
    for (const key of contextKeys) {
      const command = await query(key + '\\command');
      const parameter = key.includes('Background') ? '%V' : '%1';
      check('Quoted context command: ' + key.split('Classes\\')[1], command.includes('"' + installedExe + '" "' + parameter + '"'));
    }
    const libraryOutput = await execute(installedExe, ['--library', 'sources'], { env: { ...process.env, COVALOS_CONFIG_DIR: profileDir, AEROS_CONFIG_DIR: profileDir } });
    const sources = JSON.parse(libraryOutput);
    check('Installed Library Studio CLI executes and returns JSON', !!sources && typeof sources === 'object');
    const installedHash = await hash(installedExe);
    await fs.writeFile(path.join(appDir, 'user-created-file.txt'), 'must survive uninstall');
    await execute(installer, [...setupArgs, '/LOG=' + path.join(evidenceDir, 'reinstall.log')]);
    check('Reinstall succeeds and retains release binary', await hash(installedExe) === installedHash);
    check('Reinstall keeps preferences', (await fs.readFile(sentinel, 'utf8')).startsWith('profile survives'));
    await execute(uninstaller, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/LOG=' + path.join(evidenceDir, 'uninstall.log')]);
    installed = false;
    // Inno's temporary uninstaller process can finish its final cleanup just after the parent.
    for (let attempt = 0; attempt < 20 && await exists(registry); attempt++) await new Promise(resolve => setTimeout(resolve, 100));
    for (const key of [registry, appPathKey, ...contextKeys]) check('Uninstall removes registration: ' + key, !(await exists(key)));
    check('Uninstall removes the application', !(await fs.stat(installedExe).catch(() => null)));
    check('Uninstall preserves preferences', (await fs.readFile(sentinel, 'utf8')).startsWith('profile survives'));
    check('Uninstall preserves untracked files', (await fs.readFile(path.join(appDir, 'user-created-file.txt'), 'utf8')) === 'must survive uninstall');
    report.passed = true;
  } finally {
    if (installed && await fs.stat(uninstaller).catch(() => null)) {
      await execute(uninstaller, ['/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART']).catch(() => {});
    }
    report.finished_at_utc = new Date().toISOString();
    await fs.writeFile(reportPath, JSON.stringify(report, null, 2) + '\n');
  }
  console.log(JSON.stringify(report, null, 2));
})().catch(error => { console.error(error.message); if (error.stderr) console.error(error.stderr); process.exitCode = 1; });
