// A release's news in the guide's window beside the HUD, with the Star last.
import { Star, X } from "lucide-react";
import type { HUDGuideCardProps } from "./HUDGuideCard";
import { Button } from "@agent-hud/ui/components/button";
import { Actions, Header } from "./hud-guide-parts";
import type { Text } from "./copy";

export function News({ view, t, close, show, star }: HUDGuideCardProps) {
  if (view.kind !== "news") return null;
  return (
    <>
      <Header
        label={t(`New in ${view.version}`, `${view.version} 新增`)}
        close={close}
        t={t}
      />
      {view.items.map((item) => (
        <div
          key={item.title[0]}
          style={{
            display: "flex",
            gap: 10,
            alignItems: "flex-start",
            marginTop: 8,
          }}
        >
          <div style={{ flex: 1, minWidth: 0 }}>
            <div style={{ fontSize: 13.5, fontWeight: 600 }}>
              {t(...item.title)}
            </div>
            {item.body && (
              <div style={{ opacity: 0.72 }}>{t(...item.body)}</div>
            )}
          </div>
          {item.target && (
            <Button
              size="xs"
              variant="ghost"
              className="text-primary"
              onClick={() => show(item.target!)}
            >
              {t("Show me", "看看")}
            </Button>
          )}
        </div>
      ))}
      {view.star && <StarLine t={t} star={star} />}
      <Actions>
        <Button size="sm" onClick={close}>
          {t("Got it", "知道了")}
        </Button>
      </Actions>
    </>
  );
}

export function StarLine({
  t,
  star,
}: {
  t: Text;
  star: (open: boolean) => void;
}) {
  return (
    <div
      style={{
        display: "flex",
        alignItems: "center",
        gap: 7,
        marginTop: 10,
        paddingTop: 9,
        borderTop: "1px solid rgba(128,128,128,0.25)",
        fontSize: 12,
      }}
    >
      <Star size={13} aria-hidden style={{ opacity: 0.6, flexShrink: 0 }} />
      <span style={{ flex: 1, opacity: 0.85 }}>
        {t("Like it? ", "喜欢的话，")}
        <Button
          variant="link"
          onClick={() => star(true)}
          className="h-auto p-0 text-xs"
        >
          {t("Star it on GitHub", "去 GitHub 点个 Star")}
        </Button>
      </span>
      <Button
        size="icon-xs"
        variant="ghost"
        aria-label={t("Don't ask again", "不再提示")}
        onClick={() => star(false)}
        className="opacity-60 hover:opacity-100"
      >
        <X />
      </Button>
    </div>
  );
}
