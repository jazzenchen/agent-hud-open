# Quota numbers in the Simple HUD

The number beside each agent normally shows its shortest live quota window
with a known duration. If multiple windows have the same duration, the one
with the least remaining quota wins. Agents without duration data keep their
previous display, including balance-only agents.

Display → Simple → Long quota warning is saved per display. The default is
10% remaining; alternatives are 5%, 15%, 20%, 30%, and Never. A longer window
qualifies only below the selected threshold. The first qualifying reading
for a quota cycle shows a red badge for six seconds. If several windows are
low, the ordinary rotation shows the one with the lowest remaining share
for six seconds after every 24 seconds of the short-window reading. The
tooltip names the current window. Never turns off both the badge and the
rotation.

Number appearance is also saved per display. Colored text uses the status
color for the number and its unit. Grayscale uses a neutral foreground and
adjustable transparency (0–90%). The separate alert badge stays visible.
The expanded quota panel, ring and bar data, and agent artwork are unchanged.
