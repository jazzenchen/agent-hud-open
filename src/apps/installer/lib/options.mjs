import { homedir } from "node:os";
import { resolve } from "node:path";
import { parseArgs } from "node:util";

export const HELP = `Usage: npx @agenthud/install [options]

Installs Agent HUD, or updates it to the newest release, betas included.

  --release <version>  install this release instead, such as 0.6.0-beta.6
  --dir <folder>       install into this folder
                       (macOS: /Applications, Linux: ~/Applications;
                       on Windows the installer asks)
  --no-open            don't open Agent HUD afterwards
  -v, --version        print this installer's version
  -h, --help           show this help

On macOS, Agent HUD then updates itself. On Windows and Linux, run this
again to update. Set GITHUB_TOKEN if GitHub limits your requests.`;

export function parseOptions(argv) {
  const { values } = parseArgs({
    args: argv,
    options: {
      release: { type: "string" },
      dir: { type: "string" },
      "no-open": { type: "boolean" },
      version: { type: "boolean", short: "v" },
      help: { type: "boolean", short: "h" },
    },
  });
  return {
    release: values.release?.replace(/^v/, ""),
    dir: values.dir === undefined ? undefined : folder(values.dir),
    open: !values["no-open"],
    version: Boolean(values.version),
    help: Boolean(values.help),
  };
}

// A shell leaves "~" alone in --dir=~/Apps, so expand it here.
function folder(path) {
  if (path === "~") return homedir();
  if (path.startsWith("~/")) return resolve(homedir(), path.slice(2));
  return resolve(path);
}
