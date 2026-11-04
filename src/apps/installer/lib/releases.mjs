export const REPOSITORY = "jazzenchen/agent-hud-open";
export const USER_AGENT = "agent-hud-installer";

// Orders X.Y.Z and X.Y.Z-beta.N: a beta comes before its release, and numeric
// parts compare as numbers (beta.10 after beta.9).
export function compareVersions(a, b) {
  const left = split(a);
  const right = split(b);
  for (let i = 0; i < Math.max(left.main.length, right.main.length); i += 1) {
    const difference = (left.main[i] ?? 0) - (right.main[i] ?? 0);
    if (difference) return Math.sign(difference);
  }
  if (!left.pre.length || !right.pre.length) {
    return Math.sign(right.pre.length - left.pre.length);
  }
  for (let i = 0; i < Math.max(left.pre.length, right.pre.length); i += 1) {
    const x = left.pre[i];
    const y = right.pre[i];
    if (x === undefined || y === undefined) return x === undefined ? -1 : 1;
    const numeric = /^\d+$/.test(x) && /^\d+$/.test(y);
    const difference = numeric ? Number(x) - Number(y) : x.localeCompare(y);
    if (difference) return Math.sign(difference);
  }
  return 0;
}

function split(version) {
  const dash = version.indexOf("-");
  const main = dash < 0 ? version : version.slice(0, dash);
  const pre = dash < 0 ? "" : version.slice(dash + 1);
  return { main: main.split(".").map(Number), pre: pre ? pre.split(".") : [] };
}

// The file of a release that installs on `target`. Windows and Linux names
// carry "-unsigned" while those builds are unsigned.
export function assetPattern(version, target) {
  const v = version.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  const arch = target.arch;
  switch (target.os) {
    case "macos":
      return new RegExp(`^Agent-HUD-${v}-${arch}\\.app\\.tar\\.gz$`);
    case "windows":
      return new RegExp(
        `^Agent-HUD-${v}-windows-${arch}(?:-unsigned)?-setup\\.exe$`,
      );
    case "linux":
      return new RegExp(
        `^Agent-HUD-${v}-linux-${arch}(?:-unsigned)?\\.AppImage$`,
      );
    default:
      throw new Error(`Unknown system ${target.os}`);
  }
}

// Picks from GitHub's release list the newest release, or the `wanted` one,
// that has a build for `target` and a SHA256SUMS file to check it with.
export function chooseRelease(releases, target, wanted) {
  return (
    releases
      .filter((release) => !release.draft)
      .map((release) => {
        const version = release.tag_name.replace(/^v/, "");
        const pattern = assetPattern(version, target);
        return {
          version,
          prerelease: Boolean(release.prerelease),
          asset: release.assets.find((asset) => pattern.test(asset.name)),
          sums: release.assets.find((asset) => asset.name === "SHA256SUMS"),
        };
      })
      .filter((release) => !wanted || release.version === wanted)
      .filter((release) => release.asset && release.sums)
      .sort((a, b) => compareVersions(b.version, a.version))[0] ?? null
  );
}

// Reads `sha256sum` output: "<hash>  <file name>" per line.
export function parseSums(text) {
  const sums = new Map();
  for (const line of text.split(/\r?\n/)) {
    const match = line.trim().match(/^([0-9a-fA-F]{64})\s+\*?(.+)$/);
    if (match) sums.set(match[2], match[1].toLowerCase());
  }
  return sums;
}

export async function fetchReleases({ token = process.env.GITHUB_TOKEN } = {}) {
  const headers = {
    accept: "application/vnd.github+json",
    "user-agent": USER_AGENT,
  };
  if (token) headers.authorization = `Bearer ${token}`;
  const response = await fetch(
    `https://api.github.com/repos/${REPOSITORY}/releases?per_page=30`,
    { headers },
  );
  if (response.status === 403 || response.status === 429) {
    throw new Error(
      "GitHub is limiting requests from this address. Try again later, or set GITHUB_TOKEN.",
    );
  }
  if (!response.ok) {
    throw new Error(`GitHub answered HTTP ${response.status} for the releases`);
  }
  return response.json();
}
