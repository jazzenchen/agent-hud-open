import { invoke, isTauri } from "@tauri-apps/api/core";
import { MoreHorizontal } from "lucide-react";
import { toast } from "sonner";
import { Button } from "@agent-hud/ui/components/button";
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from "@agent-hud/ui/components/dropdown-menu";
import type { NativeMethod } from "@agent-hud/protocol";
import { emit } from "@tauri-apps/api/event";
import { guideEvents } from "./guide/use-hud-guide";

/** The More options menu of the Settings and Statistics windows. */
export function SourceMenu({
  chinese,
  statistics,
  hudVisible,
  act,
}: {
  chinese: boolean;
  /** The menu is in the Statistics window, so it offers Settings. */
  statistics: boolean;
  hudVisible: boolean;
  act: (
    method: NativeMethod,
    params?: Record<string, unknown>,
  ) => Promise<unknown>;
}) {
  function toggleHUDVisibility() {
    // The menu has no place for a failure, so it shows as a toast.
    void act("set_setting", { key: "hudVisible", value: !hudVisible }).catch(
      (failure) => toast.error(String(failure)),
    );
  }
  async function showStatistics(tab = "tokens") {
    if (isTauri()) {
      await invoke("open_statistics", { tab });
    } else {
      window.open(`?window=statistics`, "_blank");
    }
  }
  return (
    <DropdownMenu>
      <DropdownMenuTrigger asChild>
        <Button
          variant="ghost"
          size="icon"
          data-guide="more-menu"
          aria-label={chinese ? "更多选项" : "More options"}
        >
          <MoreHorizontal size={16} />
        </Button>
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end">
        <DropdownMenuItem
          onClick={() => {
            void showStatistics();
          }}
        >
          {chinese ? "用量统计…" : "Statistics…"}
        </DropdownMenuItem>
        {statistics ? (
          <DropdownMenuItem
            onClick={() => {
              if (isTauri()) void invoke("hud_show_main");
            }}
          >
            {chinese ? "设置…" : "Settings…"}
          </DropdownMenuItem>
        ) : (
          <>
            <DropdownMenuItem
              onClick={() => void emit(guideEvents.open, "tour")}
            >
              {chinese ? "上手引导…" : "Getting Started…"}
            </DropdownMenuItem>
            <DropdownMenuItem
              onClick={() => void emit(guideEvents.open, "news")}
            >
              {chinese ? "新功能…" : "What’s New…"}
            </DropdownMenuItem>
          </>
        )}
        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={toggleHUDVisibility}>
          {hudVisible
            ? chinese
              ? "隐藏 HUD"
              : "Hide HUD"
            : chinese
              ? "显示 HUD"
              : "Show HUD"}
        </DropdownMenuItem>
        {isTauri() && (
          <>
            <DropdownMenuSeparator />
            <DropdownMenuItem
              onClick={() => {
                void invoke("quit_app");
              }}
            >
              {chinese ? "退出" : "Quit"}
            </DropdownMenuItem>
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}
