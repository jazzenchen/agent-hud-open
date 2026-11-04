import assert from "node:assert/strict";
import { test } from "node:test";
import { parseOptions } from "../lib/options.mjs";
import {
  assetPattern,
  chooseRelease,
  compareVersions,
  parseSums,
} from "../lib/releases.mjs";
import { currentTarget } from "../lib/target.mjs";

const asset = (name) => ({
  name,
  browser_download_url: `https://example.invalid/${name}`,
});
const release = (tag, names, extra = {}) => ({
  tag_name: tag,
  draft: false,
  prerelease: tag.includes("-"),
  assets: names.map(asset),
  ...extra,
});

// Shaped like jazzenchen/agent-hud-open's releases: the native 0.5.5 has only
// an arm64 DMG and no SHA256SUMS; the Tauri betas have every build.
const beta = (n) =>
  release(`v0.6.0-beta.${n}`, [
    `Agent-HUD-0.6.0-beta.${n}-arm64.app.tar.gz`,
    `Agent-HUD-0.6.0-beta.${n}-arm64.dmg`,
    `Agent-HUD-0.6.0-beta.${n}-linux-amd64-unsigned.deb`,
    `Agent-HUD-0.6.0-beta.${n}-linux-x86_64-unsigned.AppImage`,
    `Agent-HUD-0.6.0-beta.${n}-windows-x86_64-unsigned-setup.exe`,
    `Agent-HUD-0.6.0-beta.${n}-x86_64.app.tar.gz`,
    `Agent-HUD-0.6.0-beta.${n}-x86_64.dmg`,
    "SHA256SUMS",
  ]);
const releases = [
  beta(6),
  beta(10),
  release("v0.6.0-beta.11", ["SHA256SUMS"], { draft: true }),
  release("v0.5.5", ["Agent-HUD-0.5.5-arm64.dmg", "appcast.xml"]),
  beta(5),
];
const mac = { os: "macos", arch: "arm64" };
const intel = { os: "macos", arch: "x86_64" };
const windows = { os: "windows", arch: "x86_64" };
const linux = { os: "linux", arch: "x86_64" };

test("orders betas before their release and numbers as numbers", () => {
  assert.equal(compareVersions("0.6.0-beta.10", "0.6.0-beta.9"), 1);
  assert.equal(compareVersions("0.6.0", "0.6.0-beta.10"), 1);
  assert.equal(compareVersions("0.6.0-beta.1", "0.5.5"), 1);
  assert.equal(compareVersions("0.10.0", "0.9.9"), 1);
  assert.equal(compareVersions("14.0", "14.0.0"), 0);
  assert.equal(compareVersions("13.6.1", "14.0"), -1);
});

test("picks the newest published release with a build for each system", () => {
  for (const target of [mac, intel, windows, linux]) {
    const chosen = chooseRelease(releases, target);
    assert.equal(chosen.version, "0.6.0-beta.10");
    assert.equal(chosen.prerelease, true);
    assert.equal(chosen.sums.name, "SHA256SUMS");
  }
  assert.equal(
    chooseRelease(releases, mac).asset.name,
    "Agent-HUD-0.6.0-beta.10-arm64.app.tar.gz",
  );
  assert.equal(
    chooseRelease(releases, linux).asset.name,
    "Agent-HUD-0.6.0-beta.10-linux-x86_64-unsigned.AppImage",
  );
});

test("installs a named release, and none that lacks the build", () => {
  assert.equal(
    chooseRelease(releases, mac, "0.6.0-beta.5").version,
    "0.6.0-beta.5",
  );
  assert.equal(chooseRelease(releases, mac, "0.5.5"), null);
  assert.equal(chooseRelease(releases, mac, "9.9.9"), null);
  assert.equal(chooseRelease([], windows), null);
});

test("matches signed and unsigned file names of the release's own version", () => {
  assert.match(
    "Agent-HUD-0.7.0-windows-x86_64-setup.exe",
    assetPattern("0.7.0", windows),
  );
  assert.match(
    "Agent-HUD-0.7.0-linux-x86_64.AppImage",
    assetPattern("0.7.0", linux),
  );
  assert.doesNotMatch("Agent-HUD-0.7.0-arm64.dmg", assetPattern("0.7.0", mac));
  assert.doesNotMatch(
    "Agent-HUD-0.7.01-arm64.app.tar.gz",
    assetPattern("0.7.0", mac),
  );
  assert.doesNotMatch(
    "Agent-HUD-0x7x0-arm64.app.tar.gz",
    assetPattern("0.7.0", mac),
  );
});

test("reads SHA256SUMS lines", () => {
  const hash =
    "3D7BE0ADC34B6E763A404CD2CFC6CC7CADD25DD3926512AE95ACA6069B8BEFEE";
  const sums = parseSums(
    `${hash}  Agent HUD.tar.gz\r\n\nnot a line\n${"0".repeat(64)} *b.exe\n`,
  );
  assert.equal(sums.get("Agent HUD.tar.gz"), hash.toLowerCase());
  assert.equal(sums.get("b.exe"), "0".repeat(64));
  assert.equal(sums.size, 2);
});

test("maps each system and processor to its build", () => {
  const no = () => false;
  const yes = () => true;
  assert.deepEqual(
    currentTarget({ platform: "darwin", arch: "arm64", translated: no }),
    mac,
  );
  assert.deepEqual(
    currentTarget({ platform: "darwin", arch: "x64", translated: no }),
    intel,
  );
  assert.deepEqual(
    currentTarget({ platform: "darwin", arch: "x64", translated: yes }),
    mac,
  );
  assert.deepEqual(
    currentTarget({ platform: "win32", arch: "arm64" }),
    windows,
  );
  assert.deepEqual(currentTarget({ platform: "linux", arch: "x64" }), linux);
  assert.equal(currentTarget({ platform: "linux", arch: "arm64" }), null);
  assert.equal(currentTarget({ platform: "freebsd", arch: "x64" }), null);
});

test("reads the options", () => {
  const options = parseOptions(["--release", "v0.6.0-beta.6", "--no-open"]);
  assert.equal(options.release, "0.6.0-beta.6");
  assert.equal(options.open, false);
  assert.equal(parseOptions([]).open, true);
  assert.throws(() => parseOptions(["--force"]));
});
