import { useState } from "react";
import { Button } from "@agent-hud/ui/components/button";
import {
  translator,
  type Language,
  type Translate,
} from "@agent-hud/ui/format";
import type { usePanelServices } from "../PanelContext";
import { FittingName } from "./FittingName";
import type { palette } from "./theme";

type DraftState = NonNullable<
  NonNullable<ReturnType<typeof usePanelServices>>["draft"]
>;

/** Open's PanelDraftBar: a layout or style an agent changed and the person
 * has not saved yet, with Discard and Save. */
export function PanelDraftBar({
  draft,
  language,
  colors,
}: {
  draft?: DraftState;
  language: Language;
  colors: ReturnType<typeof palette>;
}) {
  const [draftError, setDraftError] = useState<string | null>(null);
  if (!draft?.unsaved) return null;
  const t = translator(language);
  return (
    <div
      style={{
        display: "flex",
        gap: 8,
        alignItems: "center",
        fontSize: 11,
        color: colors.secondary,
      }}
    >
      <div style={{ flex: 1, display: "grid", gap: 1, minWidth: 0 }}>
        <strong style={{ fontWeight: 600, color: colors.text }}>
          {draftTitle(draft, t)}
        </strong>
        <FittingName
          {...draftNote(draft, t)}
          style={{ fontSize: 10, color: colors.secondary }}
        />
      </div>
      {draftError && <span role="alert">{draftError}</span>}
      <Button
        size="xs"
        variant="ghost"
        onClick={() =>
          void draft.discard().catch((error) => setDraftError(String(error)))
        }
      >
        {t("Discard")}
      </Button>
      <Button
        size="xs"
        onClick={() =>
          void draft.save().catch((error) => setDraftError(String(error)))
        }
      >
        {t("Save")}
      </Button>
    </div>
  );
}

/** What waits (Open's PanelDraftBar.title): a layout, with or without a
 * style, a style alone, or the app's settings alone. */
function draftTitle(draft: DraftState, t: Translate) {
  if (draft.layout) return t("Unsaved layout");
  if (draft.style) return t("Unsaved HUD style");
  return t("Unsaved app settings");
}

/** When the draft goes, and whose it is (Open's PanelDraftBar.note): this
 * display's alone, or with the app's settings, which are every display's;
 * `short` where `name` does not fit. */
function draftNote(draft: DraftState, t: Translate) {
  const restart = t("Dropped on restart");
  if (draft.app) {
    if (!draft.layout && !draft.style) return { name: restart, short: restart };
    return {
      name: t("With app settings, dropped on restart"),
      short: t("With app settings"),
    };
  }
  if (draft.layout === "shared") return { name: restart, short: restart };
  return {
    name: t("This display only, dropped on restart"),
    short: t("This display only"),
  };
}
