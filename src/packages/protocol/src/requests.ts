// Approvals and questions, their answers and delivery, and island events.

export interface RequestAnswer {
  selected: string[];
  custom?: string | null;
}
export type RequestDecision =
  | { kind: "allow" | "deny" | "leave" }
  | { kind: "always"; suggestion?: unknown }
  | { kind: "answer"; answers: Record<string, RequestAnswer> }
  | { kind: "approve_plan"; acceptEdits: boolean }
  | { kind: "keep_planning"; message?: string };
export interface ApprovalQuestion {
  id: string | null;
  answerKey: string;
  question: string;
  header: string | null;
  options: { label: string; description: string | null }[];
  multiSelect: boolean;
}
export interface Approval {
  id: string;
  title: string;
  source: string;
  toolName: string;
  input: string;
  sessionId: string | null;
  createdAt: string;
  allowsAlways: boolean;
  /** Its client hands the agent the words of a `deny` decision's `message`. */
  deniesWithReply?: boolean;
  kind?: "permission" | "question" | "plan";
  vendor?: string;
  summary?: string;
  detail?: string | null;
  cwd?: string | null;
  path?: string | null;
  removed?: string | null;
  added?: string | null;
  suggestions?: unknown[];
  /** From Core: what always allowing adds, as text (a rule, or a website). */
  alwaysAllowRule?: string | null;
  /** From Core: the allow button's words where they are not "Allow once",
   * such as "Allow this conversation" for a native approval kept per session. */
  allowLabel?: string | null;
  /** From Core: the request's one-word kind where Core names it (APP, WEB). */
  badge?: string | null;
  questions?: ApprovalQuestion[];
  plan?: string | null;
  answersQuestion?: boolean;
  /** From Core: why a question can be answered only in the client's own
   * window, such as a Claude desktop box whose session Agent HUD cannot
   * reach; null when it can be answered here. */
  answerNote?: string | null;
  /** From Core, with `answerNote`: the step that would let Agent HUD answer
   * it here; null when nothing on this Mac would. */
  answerFix?: AnswerFix | null;
  answersPlan?: boolean;
}
/** What lets Agent HUD answer a client's questions: install the client's
 * Agent HUD plugin (Settings → Agent plugins), load it in the session that
 * asks (`/reload-plugins`), or turn answering back on in Settings → Agent
 * plugins. */
export type AnswerFix = "installPlugin" | "reloadPlugins" | "turnOnAnswers";
export interface ClientHooksStatus {
  supported: boolean;
  /** The callback receiver is listening; this alone does not prove client installation. */
  enabled: boolean;
  desired?: boolean;
  installed?: boolean;
  installable?: boolean;
  notice?: string | null;
  clients?: {
    source: string;
    installed: boolean;
    active: boolean;
    notice: string | null;
  }[];
}
export interface RequestDelivery {
  id: string;
  source: string;
  sessionId: string | null;
  state: "confirmed" | "failed" | "uncertain";
  reason: string | null;
  at: string;
}
export interface IslandEvent {
  id: string;
  kind:
    | "completion"
    | "quota-exhaustion"
    | "quota-reset"
    | "reset-credits"
    /** The agent signed out; `message` is what its account reading says. */
    | "sign-in-needed"
    /** The agent skips Agent HUD's hooks until the user allows them in the
     * agent itself; `hooks` names the events, as the agent does. */
    | "hooks-review-needed";
  at: string;
  startedAt?: string | null;
  source: string;
  sessionId?: string;
  turnId?: string;
  title?: string;
  model?: string;
  message?: string | null;
  quotaId?: string;
  accountId?: string;
  usedPercent?: number;
  resetAt?: string;
  crossing?: "critical" | "exhausted" | "forecast";
  timeToExhaustSeconds?: number;
  otherExhaustedWindows?: { id?: string; name?: string }[];
  added?: number;
  availableCount?: number;
  newCredits?: { id?: string; expiresAt?: string | null }[] | number;
  islandAlert?: boolean;
  navigationTarget?: unknown;
  hooks?: string[];
  /** Where in the agent the user allows its hooks. */
  step?: string | null;
}
