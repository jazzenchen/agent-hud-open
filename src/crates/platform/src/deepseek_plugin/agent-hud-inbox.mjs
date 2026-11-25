// The second part of Agent HUD's plugin for DeepSeek Harness: what the app
// leaves for this host. It is not a module of its own: the file Harness loads
// is agent-hud.mjs with this one after it (`SCRIPT` in mod.rs), and
// everything it uses comes from its caller there.
//
// In the folder the plugin shares with the app:
// - prompts/<session>/<id>.json: words the app leaves for a session, which
//   are queued as the session's next message, in the model they name, if any,
//   or with mode steer go into its turn in progress (agent-hud-steer.mjs);
// - hosts/<host>.json: this host checks in with its sessions, so the app
//   knows a host is there to start a session in, and with the models Harness
//   offers and the one new sessions start in;
// - starts/<host>/<id>.json: a new session the app asks this host for, in a
//   folder and with its first message, in the model it names, if any. It is
//   created in the folder's Workspace when Harness has one, and the file then
//   says which session it is and how the message went.

/** The rounds that take words and starts, and the host's own check-in. */
function inbox({ fs, path, service, read, write, remove, now, quietly, NAME, folder, host, version, desktop, roots, waiting }) {
  const queues = new Map();
  // The Desktop app's host is preferred for a new session: it has a window
  // the session shows in. The app starts its host from this entry file; the
  // command line's hosts start from cli.js beside it.
  const inDesktop =
    desktop ??
    process.argv.some((arg) => /dsh-desktop-host[\\/]lib[\\/]index\.js$/.test(String(arg)));
  const hostFile = folder("hosts", `${host}.json`);

  // ---- Models: what Harness offers, and the one the app names ----
  const CATALOG_MS = 600_000;
  let catalog;
  let catalogAt = -Infinity;
  let catalogDefault;
  let catalogPending = false;
  const selection = (value) =>
    typeof value?.provider === "string" &&
    value.provider.trim() !== "" &&
    typeof value?.model === "string" &&
    value.model.trim() !== ""
      ? {
          provider: value.provider,
          model: value.model,
          ...(typeof value.reasoningEffort === "string" && value.reasoningEffort.trim() !== ""
            ? { reasoningEffort: value.reasoningEffort.trim() }
            : {}),
        }
      : undefined;
  /** The model new sessions start in: the one the user last chose. */
  const defaultModel = () => {
    try {
      return selection(service("agentDefaultModel")?.currentSelection?.()) ?? catalogDefault;
    } catch {
      return catalogDefault;
    }
  };
  /** Asks Harness for its models now and then: each provider's list. */
  const refreshCatalog = () => {
    const controller = service("sessionController");
    if (catalogPending || now() - catalogAt < CATALOG_MS || typeof controller?.modelCatalog !== "function")
      return;
    catalogPending = true;
    Promise.resolve()
      .then(() => controller.modelCatalog())
      .then(
        (found) => {
          catalogDefault = selection(found?.default);
          catalog = (Array.isArray(found?.groups) ? found.groups : []).map((group) => ({
            id: String(group?.id ?? ""),
            name: String(group?.name ?? ""),
            models: (Array.isArray(group?.models) ? group.models : []).map((model) => ({
              id: String(model?.id ?? ""),
              name: String(model?.name ?? ""),
              ...(model?.reasoning && Array.isArray(model.reasoning.efforts)
                ? {
                    reasoning: {
                      efforts: model.reasoning.efforts
                        .filter((effort) => typeof effort?.id === "string")
                        .map((effort) => ({
                          id: effort.id,
                          name: String(effort.name ?? effort.id),
                          ...(typeof effort.description === "string"
                            ? { description: effort.description }
                            : {}),
                        })),
                      ...(typeof model.reasoning.defaultEffort === "string"
                        ? { defaultEffort: model.reasoning.defaultEffort }
                        : {}),
                    },
                  }
                : {}),
            })),
          }));
          catalogAt = now();
        },
        () => {},
      )
      .finally(() => {
        catalogPending = false;
      });
  };
  const models = () => {
    refreshCatalog();
    return catalog === undefined ? undefined : { default: defaultModel(), groups: catalog };
  };
  /**
   * Checks a model against Harness's live catalog before touching a session.
   * The controller's `selectModel` would also save the choice as the user's
   * default for new sessions, which a start must never do.
   */
  const advertised = async (chosen) => {
    const controller = service("sessionController");
    if (typeof controller?.modelCatalog !== "function")
      throw new Error("This DeepSeek Harness cannot verify a session's model");
    const found = await controller.modelCatalog();
    const group = (Array.isArray(found?.groups) ? found.groups : []).find(
      (entry) => entry?.id === chosen.provider,
    );
    const model = (Array.isArray(group?.models) ? group.models : []).find(
      (entry) => entry?.id === chosen.model,
    );
    if (model === undefined)
      throw new Error(`DeepSeek Harness does not offer ${chosen.provider}/${chosen.model}`);
    if (chosen.reasoningEffort !== undefined) {
      const efforts = Array.isArray(model.reasoning?.efforts) ? model.reasoning.efforts : [];
      if (!efforts.some((effort) => effort?.id === chosen.reasoningEffort))
        throw new Error(
          `DeepSeek Harness does not offer effort ${chosen.reasoningEffort} for ${chosen.provider}/${chosen.model}`,
        );
    }
    return model;
  };
  /**
   * Whether a session-only selector can exist before a session is made. This
   * is a host-interface check, separate from checking the live model catalog:
   * `resolveAgent` and `agents.get` are the two known DSH entry points, and
   * neither requires an existing same-model session.
   */
  const hasSessionSelector = () => {
    const controller = service("sessionController");
    return (
      typeof controller?.resolveAgent === "function" ||
      typeof service("agents")?.get === "function"
    );
  };
  const resolveAgent = async (sessionId) => {
    const controller = service("sessionController");
    let agent;
    if (typeof controller?.resolveAgent === "function") {
      const found = await controller.resolveAgent(sessionId);
      if (found?.error !== undefined)
        throw new Error(found.error.message ?? String(found.error));
      agent = found?.agent;
    }
    return agent ?? service("agents")?.get?.(sessionId);
  };
  const selectSession = async (sessionId, chosen) => {
    const agent = await resolveAgent(sessionId);
    if (typeof agent?.model?.select !== "function")
      throw new Error("This DeepSeek Harness exposes no session-only model selector");
    agent.model.select(chosen);
  };
  /**
   * Switches `sessionId` to `model` before the app's words go in. Harness
   * also saves a session's choice as the model new sessions start in; the
   * app names one for this session only, so the one before is put back.
   */
  const choose = async (sessionId, model) => {
    if (model === undefined || model === null) return;
    const chosen = selection(model);
    if (chosen === undefined) throw new Error("Agent HUD named no model");
    const controller = service("sessionController");
    if (typeof controller?.selectModel !== "function")
      throw new Error("This DeepSeek Harness cannot choose a session's model");
    const defaults = service("agentDefaultModel");
    let before;
    try {
      const current = defaults?.currentSelection?.();
      // Whole, with its reasoning effort, as Harness saved it.
      if (selection(current) !== undefined) before = { ...current };
    } catch {}
    await controller.selectModel({ sessionId, ...chosen });
    if (before !== undefined && typeof defaults?.saveSelection === "function")
      await Promise.resolve()
        .then(() => defaults.saveSelection(before))
        .catch(() => {});
  };

  // ---- Words the app leaves for a session ----
  const steer = steerer({ service, write, now, quietly, roots, waiting });
  const submit = (sessionId, file, prompt) => {
    if (prompt.mode === "steer") return steer(sessionId, file, prompt);
    const controller = service("sessionController");
    if (typeof controller?.prompt !== "function") return;
    const takenAt = now();
    // Claimed before it is submitted, so a reload never submits it twice.
    write(file, { takenAt });
    const became = (what) =>
      quietly(() => write(file, { takenAt, ...what }));
    const previous = queues.get(sessionId) ?? Promise.resolve();
    const next = previous
      .then(() => choose(sessionId, prompt.model))
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

  const starts = sessionStarts({
    fs, path, service, read, write, now, quietly, NAME, folder, host,
    selection, advertised, hasSessionSelector, selectSession,
  });
  return {
    prompts,
    starts,
    checkIn: () => {
      const offered = models();
      write(hostFile, {
        at: now(),
        desktop: inDesktop,
        channelVersion: version,
        ...(offered === undefined ? {} : { models: offered }),
      });
    },
    stop: () => remove(hostFile),
  };
}
