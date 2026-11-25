/** Explicit choices only go through a host that binds them to an idle turn. */
function selectedPrompts({ fs, path, crypto, service, folder, roots, read, write, now, NAME }) {
  const leases = new Map();
  const pending = new Set();

  const end = (id) => {
    leases.get(id)?.abort.abort();
    leases.delete(id);
  };

  const generation = (agent) => {
    if (typeof service("sessionController")?.promptWithSelection !== "function") {
      end(agent.id);
      return undefined;
    }
    const found = leases.get(agent.id);
    if (found?.agent === agent) return found.id;
    end(agent.id);
    const lease = { id: crypto.randomUUID(), agent, abort: new AbortController() };
    leases.set(agent.id, lease);
    return lease.id;
  };

  const submit = async (agent, file, prompt) => {
    const lease = leases.get(agent.id);
    const refused = (reason) => write(file, { takenAt: now(), refused: reason });
    if (!lease || lease.agent !== agent || prompt.generation !== lease.id)
      return refused("DeepSeek Harness's Agent HUD plugin stopped or reloaded before taking the message");
    const controller = service("sessionController");
    if (typeof controller?.promptWithSelection !== "function")
      return refused("DeepSeek Harness cannot bind a model or reasoning effort to this turn");
    if (fs.existsSync(file.slice(0, -5) + ".cancel"))
      return write(file, { cancelledAt: now() });
    const remaining = Number(prompt.expiresAt) - now();
    if (!Number.isFinite(remaining) || remaining <= 0)
      return refused("DeepSeek Harness did not take the selected message in time");
    const current = agent.model?.current?.();
    const requested = Object.hasOwn(prompt, "model") ? prompt.model : current;
    if (typeof requested?.provider !== "string" || typeof requested?.model !== "string")
      return refused("DeepSeek Harness cannot confirm the session's current model; name a model");
    if (Object.hasOwn(prompt, "reasoningEffort") &&
        (typeof prompt.reasoningEffort !== "string" || !prompt.reasoningEffort.trim()))
      return refused("reasoningEffort must be a non-empty string");
    if (!prompt.model && !Object.hasOwn(prompt, "reasoningEffort"))
      return refused("The selected message names no model or reasoning effort");
    const choice = {
      provider: requested.provider,
      model: requested.model,
      ...(prompt.reasoningEffort === undefined ? {} : { reasoningEffort: prompt.reasoningEffort }),
    };
    // A timed-out or stopped plugin cannot leave text in Harness's ordinary inbox.
    const signal = AbortSignal.any([lease.abort.signal, AbortSignal.timeout(remaining)]);
    const takenAt = now();
    write(file, { takenAt });
    let outcome;
    try {
      if (fs.existsSync(file.slice(0, -5) + ".cancel"))
        return write(file, { takenAt, cancelledAt: now() });
      signal.throwIfAborted();
      if (now() >= prompt.expiresAt) throw new Error("DeepSeek Harness did not take the selected message in time");
      const result = await controller.promptWithSelection({
        requestId: prompt.id, sessionId: agent.id,
        content: [{ type: "text", text: prompt.text }],
        selection: choice,
      }, signal);
      outcome = result?.accepted === true
        ? { startedAt: now() }
        : { refused: String(result?.reason ?? "DeepSeek Harness refused the selected message") };
    } catch (error) {
      outcome = { refused: String(error?.message ?? error) };
    }
    write(file, { takenAt, ...outcome });
  };

  const poll = () => {
    for (const agent of roots()) {
      const dir = folder("selected", "prompts", agent.id);
      let names;
      try { names = fs.readdirSync(dir); } catch { continue; }
      for (const name of names) {
        if (!name.endsWith(".json")) continue;
        const file = path.join(dir, name);
        const prompt = read(file);
        if (!prompt || typeof prompt.text !== "string" || !prompt.text.trim()
            || typeof prompt.id !== "string" || !NAME.test(prompt.id) || name !== prompt.id + ".json"
            || prompt.takenAt !== undefined || prompt.cancelledAt !== undefined || pending.has(file)) continue;
        pending.add(file);
        // A request whose generation has expired is acknowledged as refused.
        void submit(agent, file, prompt)
          // A failed receipt write after acceptance must remain uncertain.
          .catch(() => {})
          .finally(() => pending.delete(file));
      }
    }
  };

  return { generation, end, poll, stop: () => { for (const id of leases.keys()) end(id); } };
}

/** Session check-ins and selected prompt leases share the same lifetime. */
function checkInSessions({ ctx, NAME, selected, write, folder, now, host, version, quietly, checkInHost }) {
  const checkedIn = new Set();
  const agents = () => {
    try {
      const list = ctx.agents?.list?.();
      return Array.isArray(list) ? list : [];
    } catch {
      return [];
    }
  };
  /** A session the user talks to: a root agent, not a subagent. */
  const root = (agent) =>
    agent && typeof agent.id === "string" && NAME.test(agent.id) && !agent.parent;
  const roots = () => agents().filter(root);
  const checkIn = (agent) => {
    const id = agent.id;
    const selectedGeneration = selected.generation(agent);
    write(folder("sessions", `${id}.json`), {
      at: now(),
      host,
      channelVersion: version,
      ...(selectedGeneration === undefined ? {} : { selectedGeneration }),
    });
    checkedIn.add(id);
  };
  const end = (id) => {
    selected.end(id);
    if (!checkedIn.delete(id)) return;
    quietly(() => write(folder("sessions", `${id}.json`), { ended: now() }));
  };
  const checkIns = () => {
    const open = new Set();
    for (const agent of roots()) {
      open.add(agent.id);
      quietly(() => checkIn(agent));
    }
    for (const id of [...checkedIn]) if (!open.has(id)) end(id);
    quietly(checkInHost);
  };

  return { roots, root, checkIn, end, checkIns, endAll: () => {
    for (const id of [...checkedIn]) end(id);
  } };
}
