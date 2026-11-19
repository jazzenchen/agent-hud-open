// The third part of Agent HUD's plugin for DeepSeek Harness: a steer, words
// the app leaves for a session (prompts/<session>/<id>.json with mode steer)
// to go into its turn in progress. Harness's steer reaches the turn's next
// step. It is refused when no turn runs or the session waits on an approval
// or a question, and is never queued instead; the file then names the turn
// it went into (steeredInto). Like agent-hud-inbox.mjs it is not a module of
// its own: everything it uses comes from its caller there.

/** Steers the words a prompt file holds; `waiting` names a session's asks. */
function steerer({ service, write, now, quietly, roots, waiting }) {
  /** The app's reason for a steer with no turn to go into (NO_TURN_TO_STEER). */
  const NO_TURN = "No turn is in progress to steer";
  /** The turn `agent` runs now, or undefined when it runs none. */
  const runningTurn = (agent) =>
    agent?.status === "running" && Number.isInteger(agent?.phase?.turn) ? agent.phase.turn : undefined;
  /**
   * Steers the words into the session's turn in progress, out of its queue
   * of messages: Harness's steer reaches the turn's next step. With no turn
   * in progress a steer would start one, so it is refused before Harness is
   * asked. One the turn may have missed, because the turn ended or another
   * began meanwhile, names no turn: the app reports it as uncertain.
   */
  const steer = (sessionId, file, prompt) => {
    const controller = service("sessionController");
    const takenAt = now();
    write(file, { takenAt });
    const became = (what) => quietly(() => write(file, { takenAt, ...what }));
    // A host with no controller (`dsh headless`) takes no words: say so at
    // once rather than let the sender wait out the steer's lifetime.
    if (typeof controller?.prompt !== "function")
      return became({ refused: "This DeepSeek Harness host takes no messages" });
    const agent = roots().find((candidate) => candidate.id === sessionId);
    if (waiting?.(sessionId))
      return became({ refused: "The session waits on an approval or a question; a steer would not be read until it is answered" });
    const turn = runningTurn(agent);
    if (turn === undefined) return became({ refused: NO_TURN });
    return Promise.resolve()
      .then(() =>
        controller.prompt(
          {
            requestId: String(prompt.id),
            sessionId,
            mode: "steer",
            content: [{ type: "text", text: prompt.text }],
          },
          new AbortController().signal,
        ),
      )
      .then(
        (result) => {
          if (result?.accepted === false)
            return became({ refused: String(result?.reason ?? "refused") });
          const still = runningTurn(agent) === turn;
          became(still ? { startedAt: now(), steeredInto: String(turn) } : { startedAt: now() });
        },
        (error) => became({ refused: String(error?.message ?? error) }),
      );
  };
  return steer;
}
