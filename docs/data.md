# Keep your data safe

- **Read on your computer**: the records your agents already keep. Usage history stays in a local SQLite database.
- **Read from each vendor**: quotas, balances and Grok Bot's daily tokens, with the agent's existing sign-in or configured key. No read sends a message to a model. Agent HUD stores only what you give it yourself: an API key you type into it, and a sign-in you make on a vendor's site in its own window. Both stay on this computer, in files only your user account can read.
- **Written on your computer**: hooks in your agents' own settings, to receive requests and replies, only while client hooks are on. Turning them off removes them.
- **Uploaded**: nothing, by a build from this source.
- **Access your data from iPhone and Apple Watch**, through the release:
  - **On the same network**, over HTTPS, with the Mac's certificate pinned when the phone pairs.
  - **Anywhere, with Agent HUD Remote**, through your own iCloud. Record contents are end-to-end encrypted with keys in your iCloud Keychain; only the text an alert shows stays outside that encryption, so iOS can display it.
  - **Live Activities of running sessions**, also with Remote, pass through Agent HUD's relay to Apple's push service. The relay keeps only what forwarding needs, each Mac's latest sessions and each phone's card, with no account identity, and deletes an account it hasn't heard from in 30 days.
