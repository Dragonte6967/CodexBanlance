import { mkdir, copyFile, readdir, rename } from 'node:fs/promises';
await mkdir('release', { recursive: true });
const existing = await readdir('release');
if (existing.some(name => name !== 'CodexBalance.exe')) throw new Error('release contains unexpected files; refusing to overwrite them.');
const source = 'src-tauri/target/release/codex-balance.exe';
const destination = 'release/CodexBalance.exe';
try {
  await copyFile(source, destination);
} catch (error) {
  if (error.code !== 'EBUSY') throw error;
  // Windows permits renaming a loaded executable. Preserve the old binary in
  // the build directory; never terminate the user's running widget.
  const backup = `src-tauri/target/portable-previous-${Date.now()}.exe`;
  await rename(destination, backup);
  try { await copyFile(source, destination); }
  catch (copyError) { await rename(backup, destination); throw copyError; }
  console.log('An older instance is still running; exit it from the tray before opening the new EXE.');
}
console.log('Portable artifact: release/CodexBalance.exe');
