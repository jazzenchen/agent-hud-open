import { createHash } from "node:crypto";
import { createWriteStream } from "node:fs";
import { Readable, Transform } from "node:stream";
import { pipeline } from "node:stream/promises";
import { USER_AGENT } from "./releases.mjs";

export async function fetchText(url) {
  const response = await get(url);
  return response.text();
}

// Saves `url` to `file` and returns the file's SHA-256 in hex.
export async function download(url, file, onProgress = () => {}) {
  const response = await get(url);
  const total = Number(response.headers.get("content-length")) || 0;
  const hash = createHash("sha256");
  let received = 0;
  const meter = new Transform({
    transform(chunk, _encoding, done) {
      hash.update(chunk);
      received += chunk.length;
      onProgress(received, total);
      done(null, chunk);
    },
  });
  await pipeline(
    Readable.fromWeb(response.body),
    meter,
    createWriteStream(file),
  );
  return hash.digest("hex");
}

async function get(url) {
  const response = await fetch(url, { headers: { "user-agent": USER_AGENT } });
  if (!response.ok) throw new Error(`HTTP ${response.status} for ${url}`);
  return response;
}
