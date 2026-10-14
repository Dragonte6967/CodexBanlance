import { mkdir, writeFile } from 'node:fs/promises';
// Source-generated Windows icon, embedded as an application resource.
const size = 32, pixels = Buffer.alloc(size * size * 4), mask = Buffer.alloc(size * 4);
for (let y = 0; y < size; y++) for (let x = 0; x < size; x++) {
  const corner = (x < 5 || x > 26) && (y < 5 || y > 26);
  const white = (x >= 7 && x <= 10 && y >= 9 && y <= 22) || (x >= 10 && x <= 23 && ((y >= 7 && y <= 10) || (y >= 21 && y <= 24)));
  const i = ((31 - y) * size + x) * 4;
  pixels.set(corner ? [0,0,0,0] : white ? [246,250,242,255] : [68,88,45,255], i);
  if (corner) mask[(31-y)*4 + (x >> 3)] |= 0x80 >> (x % 8);
}
const dib = Buffer.alloc(40); dib.writeUInt32LE(40); dib.writeInt32LE(size,4); dib.writeInt32LE(size*2,8); dib.writeUInt16LE(1,12); dib.writeUInt16LE(32,14); dib.writeUInt32LE(pixels.length + mask.length,20);
const header = Buffer.alloc(22); header.writeUInt16LE(1,2); header.writeUInt16LE(1,4); header[6]=32; header[7]=32; header.writeUInt16LE(1,10); header.writeUInt16LE(32,12); header.writeUInt32LE(40+pixels.length+mask.length,14); header.writeUInt32LE(22,18);
await mkdir('src-tauri/icons', {recursive:true});
await writeFile('src-tauri/icons/icon.ico', Buffer.concat([header,dib,pixels,mask]));
