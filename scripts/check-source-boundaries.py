#!/usr/bin/env python3
"""Check the source distribution for private services and signing material."""
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
paths = subprocess.check_output(
    ["git", "ls-files", "--cached", "--others", "--exclude-standard", "-z"], cwd=ROOT
).decode().split("\0")
private_extensions = {".p8", ".p12", ".pfx", ".key", ".pem", ".mobileprovision", ".provisionprofile", ".entitlements"}
private_directories = {"signing", "Sync", "AgentHUDServices"}
service_patterns = [
    r"\bimport\s+(?:CloudKit|UserNotifications|AgentHUDServices)\b",
    r"\b(?:CKContainer|CKDatabase|ICloudSync|LiveActivitySender|NotificationTracker)\b",
    r"com\.apple\.developer\.(?:icloud|aps|team-identifier)",
    r"\b(?:DEVELOPMENT_TEAM|NOTARY_PROFILE|ICLOUD_PROVISIONING_PROFILE)\b",
    r"iCloud\.",
]
secret_patterns = [
    r"-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----",
    r"\bgh[pousr]_[A-Za-z0-9]{30,}\b",
    r"\bgithub_pat_[A-Za-z0-9_]{40,}\b",
    r"\bAKIA[0-9A-Z]{16}\b",
]
failures = []
for name in sorted(set(filter(None, paths))):
    path = ROOT / name
    if not path.is_file():
        continue
    if path.suffix in private_extensions or private_directories.intersection(Path(name).parts):
        failures.append(f"{name}: private material is not a source asset")
    if (path.name.startswith(".env") and path.name != ".env.example") or ".local." in path.name:
        failures.append(f"{name}: local configuration is not a source asset")
    if path.is_symlink() and not path.resolve().is_relative_to(ROOT):
        failures.append(f"{name}: symlink points outside the package")
    try:
        content = path.read_text()
    except UnicodeDecodeError:
        continue
    patterns = secret_patterns + (service_patterns if path.suffix in {".swift", ".sh", ".plist", ".yml"} else [])
    for pattern in patterns:
        if re.search(pattern, content):
            failures.append(f"{name}: prohibited source content ({pattern})")

manifest = (ROOT / "Package.swift").read_text()
if re.search(r"\.package\s*\(", manifest):
    failures.append("Package.swift: review external package dependencies before including them")

# Verify ignore rules without creating any secret-shaped files.
ignored = [".env", "signing/local.env", "config/service.local.json", "AuthKey_example.p8", "certificate.p12",
           "account.key", "development.mobileprovision", "build/Agent HUD Open.app/Contents/Info.plist", ".build/ignore-check/AgentHUDOpen"]
result = subprocess.run(["git", "check-ignore", "--stdin"], cwd=ROOT,
                        input="\n".join(ignored)+"\n", text=True, capture_output=True)
missing = set(ignored) - set(result.stdout.splitlines())
failures.extend(f".gitignore: missing rule for {name}" for name in sorted(missing))

if failures:
    print("\n".join(failures), file=sys.stderr)
    sys.exit(1)
print("Source boundaries and ignore rules passed.")
