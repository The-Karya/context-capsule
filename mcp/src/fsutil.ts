// context-capsule — mcp: small file helpers (safe JSON reads, atomic writes)
import fs from "node:fs";
import path from "node:path";

export function readJson<T = unknown>(file: string): T | null {
  try {
    return JSON.parse(fs.readFileSync(file, "utf8")) as T;
  } catch {
    return null;
  }
}

/** Write via a temp file + rename so readers never see half a file. */
export function writeFileAtomic(file: string, data: string): void {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  const tmp = `${file}.${process.pid}.${Date.now()}.tmp`;
  fs.writeFileSync(tmp, data);
  fs.renameSync(tmp, file);
}

/** Create a file that must not exist yet. Returns false if it already does. */
export function writeNewFile(file: string, data: string): boolean {
  fs.mkdirSync(path.dirname(file), { recursive: true });
  try {
    fs.writeFileSync(file, data, { flag: "wx" });
    return true;
  } catch (e) {
    if ((e as NodeJS.ErrnoException).code === "EEXIST") return false;
    throw e;
  }
}

/** Write to `dir/base.ext`, or `base-2.ext`, `base-3.ext`... never overwrite. */
export function writeUnique(dir: string, base: string, ext: string, data: string): string {
  for (let n = 1; n < 10000; n++) {
    const file = path.join(dir, n === 1 ? `${base}${ext}` : `${base}-${n}${ext}`);
    if (writeNewFile(file, data)) return file;
  }
  throw new Error(`Could not find a free file name for ${base}${ext} in ${dir}`);
}
