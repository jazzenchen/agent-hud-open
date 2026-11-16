"""Use the installed Agent HUD application's user-local message channel."""

from __future__ import annotations

import itertools
import json
import os
from pathlib import Path
import subprocess
import uuid
from typing import Any

_ENGINE = Path(os.environ.get(
    "AGENT_HUD_ENGINE", "/Applications/Agent HUD.app/Contents/Resources/agent-hud-engine"
))
_CALL_IDS = itertools.count(1)

PAIRED_BOT_ID = os.environ.get("GROK_NATIVE_PAIRED_BOT_ID")
CODEX_THREAD_ID = os.environ.get("GROK_NATIVE_CODEX_THREAD_ID")
def paired_caller() -> dict[str, str]:
    if not PAIRED_BOT_ID or not CODEX_THREAD_ID:
        raise ValueError("Configure the paired Grok Bot and Codex IDs.")
    prefix, separator, bot_id = PAIRED_BOT_ID.rpartition(":")
    if not separator or not prefix.startswith("grok-bot:"):
        raise ValueError("Use the full Grok Bot session ID from Agent HUD.")
    uuid.UUID(bot_id)
    uuid.UUID(CODEX_THREAD_ID)
    return {"client": "grok-bot", "sessionId": PAIRED_BOT_ID}

PAIRED_CALLER = paired_caller()


class NativeError(RuntimeError):
    """Agent HUD could not complete a local RPC."""


def native_call(
    name: str,
    arguments: dict[str, Any],
    caller: dict[str, str] | None = None,
) -> dict[str, Any]:
    request_id = next(_CALL_IDS)
    params: dict[str, Any] = {"name": name, "arguments": arguments}
    if caller is not None:
        params["caller"] = caller
    request = {"id": request_id, "method": "host_tool", "params": params}
    try:
        response = subprocess.run(
            [str(_ENGINE), "--app-channel"],
            input=json.dumps(request) + "\n",
            text=True,
            capture_output=True,
            timeout=20,
            check=True,
        )
    except (OSError, subprocess.SubprocessError) as error:
        raise NativeError("The local Agent HUD request could not be confirmed") from error

    try:
        decoded = json.loads(response.stdout)
    except ValueError as error:
        raise NativeError("Agent HUD returned an invalid local response") from error
    if not isinstance(decoded, dict) or decoded.get("id") != request_id:
        raise NativeError("Agent HUD returned an unmatched local response")
    rpc_error = decoded.get("error")
    if isinstance(rpc_error, dict):
        raise NativeError(rpc_error.get("message") or "Agent HUD rejected the local request")
    result = decoded.get("result")
    if not isinstance(result, dict):
        raise NativeError("Agent HUD returned no result for the local request")
    return result
