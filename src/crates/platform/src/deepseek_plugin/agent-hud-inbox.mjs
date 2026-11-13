// The second part of Agent HUD's plugin for DeepSeek Harness: what the app
// leaves for this host. It is not a module of its own: the file Harness loads
// is agent-hud.mjs with this one after it (`SCRIPT` in mod.rs), and
// everything it uses comes from its caller there.
//
// In the folder the plugin shares with the app:
// - prompts/<session>/<id>.json: words the app leaves for a session, which
//   are queued as the session's next message;
// - hosts/<host>.json: this host checks in with its sessions, so the app
//   knows a host is there to start a session in;
// - starts/<host>/<id>.json: a new session the app asks this host for, in a
//   folder and with its first message. It is created in the folder's
//   Workspace when Harness has one, and the file then says which session it
//   is and how the message went.

/** The rounds that take words and starts, and the host's own check-in. */
function inbox({ fs, path, service, read, write, remove, now, quietly, NAME, folder, host, version, desktop, roots }) {
  const queues = new Map();
  // The Desktop app's host is preferred for a new session: it has a window
  // the session shows in. The app starts its host from this entry file; the
  // command line's hosts start from cli.js beside it.
  const inDesktop =
    desktop ??
    process.argv.some((arg) => /dsh-desktop-host[\\/]lib[\\/]index\.js$/.test(String(arg)));
  const hostFile = folder("hosts", `${host}.json`);

  // ---- Words the app leaves for a session ----
  const submit = (sessionId, file, prompt) => {
    const controller = service("sessionController");
    if (typeof controller?.prompt !== "function") return;
    const takenAt = now();
    // Claimed before it is submitted, so a reload never submits it twice.
    write(file, { takenAt });
    const became = (what) =>
      quietly(() => write(file, { takenAt, ...what }));
    const previous = queues.get(sessionId) ?? Promise.resolve();
    const next = previous
      .then(() =>
        controller.prompt(
          {
            requestId: String(prompt.id),
            sessionId,
            mode: "queue",
            content: [{ type: "text", text: prompt.text }],
          },
          new AbortController().signal,
        ),
      )
      .then(
        (result) =>
          became(
            result?.accepted === false
              ? { refused: String(result?.reason ?? "refused") }
              : { startedAt: now() },
          ),
        (error) => became({ refused: String(error?.message ?? error) }),
      );
    queues.set(sessionId, next);
    next.then(() => {
      if (queues.get(sessionId) === next) queues.delete(sessionId);
    });
  };
  const prompts = () => {
    for (const agent of roots()) {
        const dir = folder("prompts", agent.id);
        let names;
        try {
          names = fs.readdirSync(dir).filter((entry) => entry.endsWith(".json"));
        } catch {
          continue;
        }
        const pending = [];
        for (const entry of names) {
          const file = path.join(dir, entry);
          const prompt = read(file);
          if (
            !prompt ||
            typeof prompt.text !== "string" ||
            !NAME.test(String(prompt.id)) ||
            prompt.takenAt !== undefined ||
            prompt.cancelledAt !== undefined ||
            prompt.expiredAt !== undefined
          )
            continue;
          pending.push({ file, prompt });
        }
        pending.sort(
          (a, b) =>
            Number(a.prompt.at) - Number(b.prompt.at) ||
            String(a.prompt.id).localeCompare(String(b.prompt.id)),
        );
        for (const { file, prompt } of pending)
          quietly(() => {
            if (fs.existsSync(file.slice(0, -5) + ".cancel"))
              return write(file, { cancelledAt: now() });
            if (typeof prompt.expiresAt === "number" && prompt.expiresAt <= now())
              return write(file, { expiredAt: now() });
            submit(agent.id, file, prompt);
          });
    }
  };

  // ---- New sessions the app asks this host for ----
  /** The Workspace whose folder is `cwd`, so the session joins its group. */
  const workspaceAt = (cwd) => {
    try {
      const list = service("workspaceRegistry")?.list?.();
      const found = (Array.isArray(list) ? list : []).find((w) => w?.path === cwd);
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

  return {
    prompts,
    starts,
    checkIn: () => write(hostFile, { at: now(), desktop: inDesktop, channelVersion: version }),
    stop: () => remove(hostFile),
  };
}
