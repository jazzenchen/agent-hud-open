// Start a session through the host, with an optional session-only model selection.
function sessionStarts({
  fs, path, service, read, write, now, quietly, NAME, folder, host,
  selection, advertised, hasSessionSelector, selectSession,
}) {
  // ---- New sessions the app asks this host for ----
  /** The Workspace whose folder is `cwd`, so the session joins its group. */
  const workspaceAt = (cwd) => {
    try {
      const list = service("workspaceRegistry")?.list?.();
      // HUD and Harness can spell the same Windows directory with different separators.
      const target = path.resolve(cwd);
      const found = (Array.isArray(list) ? list : []).find(
        (w) => typeof w?.path === "string" && path.isAbsolute(w.path) && path.resolve(w.path) === target,
      );
      return typeof found?.id === "string" ? found.id : undefined;
    } catch {
      return undefined;
    }
  };
  const create = async (file, request) => {
    const controller = service("sessionController");
    if (typeof controller?.create !== "function" || typeof controller?.prompt !== "function")
      return;
    const takenAt = now();
    // Claimed before the session is made, so a reload never makes it twice.
    write(file, { takenAt });
    const became = (what) => quietly(() => write(file, { takenAt, ...what }));
    // Validate the requested choice before creating anything: the session
    // controller's model selector writes the user's default and is not used.
    const wanted =
      request.model === undefined || request.model === null
        ? undefined
        : selection(request.model);
    if (wanted !== undefined && Object.hasOwn(request.model, "reasoningEffort")
        && wanted.reasoningEffort === undefined) {
      became({ refused: "Agent HUD named no reasoning effort" });
      return;
    }
    if (request.model !== undefined && request.model !== null && wanted === undefined) {
      became({ refused: "Agent HUD named no model" });
      return;
    }
    if (wanted !== undefined) {
      try {
        await advertised(wanted);
      } catch (error) {
        became({ refused: String(error?.message ?? error) });
        return;
      }
      // The live catalog and the host's session-selector interface are two
      // separate checks. Refuse a determinable missing selector before
      // `controller.create`, so no orphan session is made and no default
      // first prompt is sent.
      if (!hasSessionSelector()) {
        became({
          refused: "This DeepSeek Harness exposes no session-only model selector",
        });
        return;
      }
    }
    let sessionId;
    try {
      const workspaceId = workspaceAt(request.cwd);
      const created = await controller.create(
        workspaceId === undefined ? { cwd: request.cwd } : { workspaceId },
      );
      sessionId = created?.sessionId;
      if (typeof sessionId !== "string" || !NAME.test(sessionId))
        return became({ refused: "DeepSeek Harness named no session" });
      became({ sessionId });
      if (wanted !== undefined) await selectSession(sessionId, wanted);
      const result = await controller.prompt(
        {
          requestId: String(request.id),
          sessionId,
          mode: "queue",
          content: [{ type: "text", text: request.text }],
        },
        new AbortController().signal,
      );
      became(
        result?.accepted === false
          ? { sessionId, refused: String(result?.reason ?? "refused") }
          : { sessionId, startedAt: now() },
      );
    } catch (error) {
      became({
        ...(sessionId === undefined ? {} : { sessionId }),
        refused: String(error?.message ?? error),
      });
    }
  };
  const starts = () => {
    const dir = folder("starts", String(host));
    let names;
    try {
      names = fs.readdirSync(dir).filter((entry) => entry.endsWith(".json"));
    } catch {
      return;
    }
    for (const entry of names) {
      const file = path.join(dir, entry);
      const request = read(file);
      if (
        !request ||
        typeof request.text !== "string" ||
        typeof request.cwd !== "string" ||
        !path.isAbsolute(request.cwd) ||
        !NAME.test(String(request.id)) ||
        request.takenAt !== undefined ||
        request.expiredAt !== undefined
      )
        continue;
      quietly(() => {
        if (typeof request.expiresAt === "number" && request.expiresAt <= now())
          return write(file, { expiredAt: now() });
        return create(file, request);
      });
    }
  };

  return starts;
}
