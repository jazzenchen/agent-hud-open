import { execFileSync } from "node:child_process";

// The release build this computer runs, or null when there is none for it.
export function currentTarget({
  platform = process.platform,
  arch = process.arch,
  translated = underRosetta,
} = {}) {
  if (platform === "darwin") {
    return {
      os: "macos",
      arch: arch === "arm64" || translated() ? "arm64" : "x86_64",
    };
  }
  // Windows on Arm runs the x64 build.
  if (platform === "win32" && (arch === "x64" || arch === "arm64")) {
    return { os: "windows", arch: "x86_64" };
  }
  if (platform === "linux" && arch === "x64") {
    return { os: "linux", arch: "x86_64" };
  }
  return null;
}

export function describeTarget(target) {
  const os = { macos: "macOS", windows: "Windows", linux: "Linux" }[target.os];
  const arch =
    target.os === "macos"
      ? { arm64: "Apple silicon", x86_64: "Intel" }[target.arch]
      : target.arch;
  return `${os} (${arch})`;
}

// An Intel Node under Rosetta still runs on Apple silicon.
function underRosetta() {
  try {
    const value = execFileSync("sysctl", ["-in", "sysctl.proc_translated"], {
      encoding: "utf8",
    });
    return value.trim() === "1";
  } catch {
    return false;
  }
}
