// What the relay's tools take: their annotations, the draft_hud proposal's schema,
// and the one check the relay makes before the app's own.
import type { StandardSchemaWithJSON } from "@modelcontextprotocol/server";
import * as z from "zod/v4";

export const readOnly = {
  readOnlyHint: true,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
};
export const localWrite = {
  readOnlyHint: false,
  destructiveHint: false,
  idempotentHint: true,
  openWorldHint: false,
};

/** The one check the relay makes before the app's own: the arguments are an object. */
export const objectArguments = <T>(value: unknown) =>
  typeof value === "object" && value !== null && !Array.isArray(value)
    ? { value: value as T }
    : { issues: [{ message: "The arguments are a JSON object." }] };
/** Advertises `schema` to clients and leaves the checking to the app: it refuses a
 * proposal it cannot draw as asked with Agent HUD's own sentences, every reason at
 * once, and reads null as not given, as agent-hud-native-open's draft_hud does. zod would
 * refuse the same calls first, with generic messages. */
export const checkedByTheApp = <S extends z.ZodType>(
  schema: S,
): StandardSchemaWithJSON<z.input<S>, z.output<S>> => ({
  "~standard": {
    ...schema["~standard"],
    validate: objectArguments<z.output<S>>,
  },
});

const keys = z.array(z.string().min(1));
const period = z.enum(["day", "week", "month"]);
const block = z
  .object({
    widget: z.enum(["bar", "rings", "segments", "number", "chart"]).optional(),
    hidden: z.boolean().optional(),
    hiddenRows: keys.optional(),
    hiddenAccounts: keys.optional(),
    rows: keys.optional(),
    accounts: keys.optional(),
    countdown: z.boolean().optional(),
    columns: z.number().int().min(2).max(6).optional(),
    ringSize: z.enum(["small", "medium", "large"]).optional(),
    span: z.enum(["full", "half"]).optional(),
    period: period.optional(),
  })
  .strict();
export const screen = z
  .string()
  .min(1)
  .optional()
  .describe(
    "An attached screen key from get_hud, or all for the shared layout.",
  );
const hud = z
  .object({
    agents: keys.optional(),
    reset: z.boolean().optional(),
    mode: z.enum(["notch", "dynamic", "simple"]).optional(),
    edge: z.enum(["top", "right", "bottom", "left"]).optional(),
    position: z.number().min(0).max(1).optional(),
    logoSize: z.number().min(20).max(60).optional(),
    logoGap: z.number().min(0.1).max(0.6).optional(),
    edgeDistance: z.number().min(0).max(20).optional(),
    showLogos: z.boolean().optional(),
    simpleSize: z.number().min(20).max(60).optional(),
    simpleGap: z.number().min(0.1).max(0.6).optional(),
    ringWidth: z.number().min(1).max(4).optional(),
    simpleStyle: z.enum(["rings", "segments", "bars", "numbers"]).optional(),
    showNumbers: z.boolean().optional(),
    separateAgents: z.boolean().optional(),
    appearance: z.enum(["dark", "light", "system"]).optional(),
    transparency: z.number().min(0).max(1).optional(),
    glow: z
      .object({
        style: z
          .enum(["blur", "dots", "ascii", "blocks", "braille", "binary"])
          .optional(),
        effect: z
          .enum(["breathe", "flow", "scan", "ripple", "shimmer", "boot"])
          .optional(),
        range: z.number().min(0).max(36).optional(),
        blur: z.number().min(0).max(36).optional(),
        brightness: z.number().min(0.2).max(1).optional(),
        speed: z.number().min(1).max(12).optional(),
        idleSpeed: z.number().min(1).max(24).optional(),
        amplitude: z.number().min(0).max(1).optional(),
        outwardOnly: z.boolean().optional(),
        gridPitch: z.number().min(4).max(12).optional(),
        gridDensity: z.number().min(0.5).max(1.5).optional(),
        gridCore: z.number().min(0).max(8).optional(),
        gridFade: z.number().min(0).max(8).optional(),
      })
      .strict()
      .optional(),
  })
  .strict();
const app = z
  .object({
    appearance: z.enum(["dark", "light", "system"]).optional(),
    matchScreen: z.boolean().optional(),
    botLogos: z.boolean().optional(),
    showMenuBarIcon: z.boolean().optional(),
    language: z
      .string()
      .optional()
      .describe("Read only; may repeat the value from get_hud."),
    writtenIn: z.string().optional().describe("Read only; may repeat get_hud."),
    hover: z
      .object({
        pressOptionToOpen: z.boolean().optional(),
        openAtTopEdge: z.boolean().optional(),
        delayMs: z.number().int().min(0).max(1500).multipleOf(50).optional(),
        collapseDelayMs: z
          .number()
          .int()
          .min(0)
          .max(1500)
          .multipleOf(50)
          .optional(),
      })
      .strict()
      .optional(),
  })
  .strict();
/** The panel's own sections; quotaHistory and planValue start hidden. */
const sectionIds = z.enum([
  "blocks",
  "tokens",
  "sessions",
  "quotaHistory",
  "planValue",
]);

export const proposal = z
  .object({
    screen,
    revision: z.number().int().nonnegative().optional(),
    edit: z
      .array(z.record(z.string(), z.unknown()))
      .optional()
      .describe(
        "Steps that change the panel tree get_hud returns as panel, each naming a node by its id: update, insert, move, remove or replace, as get_hud's catalog.edits shows. Everything not named stays.",
      ),
    panel: z
      .record(z.string(), z.unknown())
      .optional()
      .describe(
        "The whole panel tree, as get_hud returns it in panel, changed. Use it or edit, not both, and not with order, sections, blocks, style, sessions, tokens, quotaHistory or planValue.",
      ),
    from: z.enum(["default", "previous", "shared"]).optional(),
    order: keys.optional(),
    sections: z.array(sectionIds).optional(),
    hiddenSections: z.array(sectionIds).optional(),
    countdown: z.boolean().optional(),
    blocks: z.record(z.string(), block).optional(),
    style: z
      .object({
        spacing: z.enum(["compact", "regular", "roomy"]).optional(),
        separators: z.boolean().optional(),
        width: z.number().int().min(480).max(960).optional(),
      })
      .strict()
      .optional(),
    sessions: z
      .object({
        limit: z.number().int().min(1).max(10).optional(),
        period: z.enum(["day", "week"]).optional(),
        agents: keys.optional(),
        projects: keys.optional(),
      })
      .strict()
      .optional(),
    tokens: z
      .object({
        period: period.optional(),
        agents: keys.optional(),
        models: keys.optional(),
      })
      .strict()
      .optional(),
    quotaHistory: z
      .object({
        period: period.optional(),
        agents: keys.optional(),
        hiddenModels: keys.optional(),
      })
      .strict()
      .optional()
      .describe(
        "The quota history section: each window's used share over the period. Shown once sections names it and hiddenSections does not.",
      ),
    planValue: z
      .object({ agents: keys.optional(), hiddenModels: keys.optional() })
      .strict()
      .optional()
      .describe(
        "The plan value section: each plan beside its last 30 days of calls at API list prices. Shown once sections names it and hiddenSections does not.",
      ),
    hud: hud.optional(),
    app: app.optional(),
  })
  .strict();
