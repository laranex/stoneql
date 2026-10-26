// /usr/bin/time reports resident bytes on macOS and KiB on Linux.
export function parseMaximumRss(stderr, platform) {
  const pattern = platform === 'darwin' ? /^\s*(\d+)\s+maximum resident set size\s*$/gm
    : platform === 'linux' ? /^\s*Maximum resident set size \(kbytes\):\s*(\d+)\s*$/gm : null;
  if (!pattern) throw Error('Resource measurement requires macOS or Linux');
  const matches = [...stderr.matchAll(pattern)];
  if (matches.length !== 1) throw Error('Missing or ambiguous process RSS measurement');
  const bytes = Number(matches[0][1]) * (platform === 'linux' ? 1024 : 1);
  if (!Number.isSafeInteger(bytes) || bytes <= 0) throw Error('Invalid process RSS measurement');
  return bytes;
}
