import ts from 'typescript';
import { createHash } from 'node:crypto';

/**
 * Create bounded compiler input reading with stable content evidence.
 */
export function trackedInputs() {
  const hashes = new Map();
  let totalBytes = 0;
  /**
   * Read and fingerprint each compiler input while enforcing file and byte limits.
   */
  function read(file) {
    const text = ts.sys.readFile(file);
    if (text !== undefined && !hashes.has(file)) {
      totalBytes += Buffer.byteLength(text);
      if (hashes.size >= 4096 || totalBytes > 64 * 1024 * 1024) throw new Error('Compiler input exceeds 4096 files or 64 MiB');
      hashes.set(file, createHash('sha256').update(text).digest('hex'));
    }
    return text;
  }
  /**
   * Verify that every previously read compiler input still has its recorded content.
   */
  function unchanged() {
    for (const [file, digest] of hashes) {
      const text = ts.sys.readFile(file);
      if (text === undefined || createHash('sha256').update(text).digest('hex') !== digest) return false;
    }
    return true;
  }
  /**
   * Return deterministically sorted input paths and hashes.
   */
  function evidence() {
    return [...hashes].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0).map(([file, sha256]) => ({ path: file, sha256 }));
  }
  return { read, unchanged, evidence };
}
