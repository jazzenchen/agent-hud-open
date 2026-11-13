/** Components/AgentLogo.swift and Core/Models/Catalog/VendorMark.swift, shared by every UI surface. */
export const ORIGINAL_ARTWORK_FILES = {
  Claude: "claude",
  ChatGPT: "chatgpt",
  Antigravity: "antigravity",
  DeepSeek: "deepseek",
  Grok: "grok",
  "Grok Bot": "grok-bot",
  Cursor: "cursor",
  OpenCode: "opencode",
  "OpenCode-dark": "opencode-dark",
  Kimi: "kimi",
  GLM: "glm",
  Pi: "pi",
  "GitHub Copilot": "copilot",
  OpenClaw: "openclaw",
  Hermes: "hermes",
  ZCode: "zcode",
  CodeBuddy: "codebuddy",
  "CodeBuddy CN": "codebuddy",
  WorkBuddy: "workbuddy",
  Qwen: "qwen",
  Qoder: "qoder",
  "Qoder-dark": "qoder-dark",
  "Qoder CN": "qoder",
  QoderWork: "qoder",
  Devin: "devin",
  MiniMax: "minimax",
  "MiniMax Code": "minimax",
  "Volcengine Ark": "ark",
  "Kilo Code": "kilocode",
  "MiMo Code": "xiaomimimo",
  // Not from the original app: LobeHub's static PNGs 2.6.0 (MIT), `light/`.
  Cline: "cline",
  "Roo Code": "roo-code",
  Goose: "goose",
  TRAE: "trae",
  "TRAE CN": "trae",
  TeleAgent: "teleagent",
} as const;
type OriginalVendor = keyof typeof ORIGINAL_ARTWORK_FILES;
const vendors: Readonly<Record<string, OriginalVendor>> = {
  claude: "Claude",
  openai: "ChatGPT",
  antigravity: "Antigravity",
  deepseek: "DeepSeek",
  grok: "Grok",
  "grok-bot": "Grok Bot",
  cursor: "Cursor",
  opencode: "OpenCode",
  kimi: "Kimi",
  glm: "GLM",
  pi: "Pi",
  copilot: "GitHub Copilot",
  openclaw: "OpenClaw",
  hermes: "Hermes",
  zcode: "ZCode",
  codebuddy: "CodeBuddy",
  "codebuddy-cn": "CodeBuddy CN",
  workbuddy: "WorkBuddy",
  qwen: "Qwen",
  qoder: "Qoder",
  "qoder-cn": "Qoder CN",
  "qoder-work": "QoderWork",
  devin: "Devin",
  minimax: "MiniMax",
  "minimax-code": "MiniMax Code",
  ark: "Volcengine Ark",
  kilo: "Kilo Code",
  mimocode: "MiMo Code",
  cline: "Cline",
  "roo-code": "Roo Code",
  goose: "Goose",
  trae: "TRAE",
  "trae-cn": "TRAE CN",
  teleagent: "TeleAgent",
};
const templates: ReadonlySet<OriginalVendor> = new Set([
  "Grok",
  "Cursor",
  "Kimi",
  "Pi",
  "GitHub Copilot",
  "Hermes",
  "ZCode",
  "Kilo Code",
  "MiMo Code",
  "Cline",
  "Roo Code",
  "Goose",
  "TRAE",
  "TRAE CN",
]);
const normalize = (source: string) =>
  source
    .trim()
    .toLowerCase()
    .replace(/[\s_]+/g, "-");
const aliases: Readonly<Record<string, string>> = {
  "claude-code": "claude",
  anthropic: "claude",
  chatgpt: "openai",
  codex: "openai",
  "codex-cli": "openai",
  "github-copilot": "copilot",
  "grok-cli": "grok",
  "opencode-go": "opencode",
  "opencode-dark": "opencode",
  qoderwork: "qoder-work",
  "qoder-dark": "qoder",
};
export function agentArtworkKey(source: string): string {
  const key = normalize(source);
  return aliases[key] ?? key;
}
/** Vendors with no mark yet, shown with the generic icon: their own names. */
const unmarked: Readonly<Record<string, string>> = {
  droid: "Factory Droid",
};
export function agentVendorName(source: string): string {
  const key = agentArtworkKey(source);
  return key === "openai"
    ? "OpenAI"
    : (vendors[key] ?? unmarked[key] ?? source);
}
const asset = (file: string) =>
  new URL(`./assets/${file}.png`, import.meta.url).href;
export interface AgentArtwork {
  key: string;
  vendor: OriginalVendor;
  file: string;
  url: string;
  template: boolean;
  /** Template foreground or the source's fixed OpenAI brand tint; null preserves every source pixel. */
  tint: string | null;
}
export function agentArtwork(
  source: string,
  light = false,
): AgentArtwork | null {
  const key = agentArtworkKey(source);
  let vendor = vendors[key];
  if (!vendor) return null;
  let file: string = ORIGINAL_ARTWORK_FILES[vendor];
  if (!light && (file === "opencode" || file === "qoder")) {
    file = `${file}-dark`;
    if (vendor === "OpenCode" || vendor === "Qoder") vendor = `${vendor}-dark`;
  }
  const template = templates.has(vendor);
  return {
    key,
    vendor,
    file,
    url: asset(file),
    template,
    tint:
      vendor === "ChatGPT"
        ? "#10a37f"
        : template
          ? light
            ? "#1d1d1f"
            : "#ffffff"
          : null,
  };
}
export const originalArtworkVendors = Object.keys(ORIGINAL_ARTWORK_FILES)
  .filter((vendor) => !vendor.endsWith("-dark"))
  .sort();
