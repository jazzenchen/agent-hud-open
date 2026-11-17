// The clipboard window's state, as the host sends it with each change
// (`hud:inspect`), and what the window sends to a session with.

export type ClipKind = "text" | "image" | "files" | "page";

/** What a copy holds, as the window shows it: the words' start, a small
 * copy of the picture, the files' paths. The message carries all of it. */
export type InspectContent =
  | { kind: "text"; text: string; length: number }
  | {
      kind: "image";
      thumbnail: string | null;
      width: number;
      height: number;
    }
  | { kind: "files"; paths: string[] }
  | {
      /** An element or an area the browser extension picked on a page. */
      kind: "page";
      url: string;
      title: string;
      host: string;
      area: boolean;
      selector: string | null;
      text: string | null;
      thumbnail?: string | null;
      width?: number;
      height?: number;
    };

/** The latest copy. Its content is read once the bubble is pointed at. */
export interface InspectClip {
  id: string;
  kind: ClipKind;
  /** The app in front when it was copied. */
  app: string | null;
  at: number;
  /** Already in the draft. */
  added: boolean;
  content: InspectContent | null;
}

/** Something the person added to the draft. */
export interface InspectItem {
  id: string;
  kind: ClipKind;
  app: string | null;
  comment: string;
  /** The numbered comments pinned on a marked picture, first first. */
  marks: string[];
  content: InspectContent;
}

export interface InspectView {
  clip: InspectClip | null;
  items: InspectItem[];
  note: string;
  /** The picture open in the markup window, which holds this window open. */
  marking?: string | null;
  /** The last pick from the browser: the HUD brings the window up once for
   * each. */
  presented?: string | null;
}

/** Where the words go: a session there is, or a new one in `folder`
 * (empty: a task folder of its own). */
export type InspectTarget =
  | { kind: "session"; client: string; sessionId: string }
  | { kind: "new"; client: string; folder: string };

export interface InspectSessionRow {
  id: string;
  client: string;
  title: string | null;
  folder: string | null;
  /** The folder as the client's start offer writes it (`~/…`). */
  startFolder: string | null;
  state: "running" | "waitingForApproval" | "idle";
  /** Takes words now; null when only sending finds out. */
  reachable: boolean | null;
  reason: string;
  updatedAt: string;
}

export interface InspectStart {
  client: string;
  folders: string[];
  withoutFolder: boolean;
}

export interface InspectTargets {
  /** Newest first. */
  sessions: InspectSessionRow[];
  starts: InspectStart[];
  /** The place offered first: the last one chosen while it is there. */
  chosen: InspectTarget | null;
}

export interface InspectSendResult {
  state: "queued" | "taken" | "started" | "uncertain" | "refused" | "expired";
  reason?: string | null;
  sessionId?: string | null;
}
