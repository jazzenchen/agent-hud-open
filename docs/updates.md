# Beta updates

The distributed Mac app checks release versions by default. Its optional Beta setting adds the Beta channel while retaining the current release channel. Update archives use Sparkle signatures, and Beta installation requires confirmation.

The [Beta feed](../updates/beta.xml) contains the newest eligible Beta and the current GitHub Latest release. Older previews without an appcast are excluded. Each standard Beta states its minimum iPhone version; a Mac-only Beta states that iPhone and Apple Watch sync is unavailable.

Turning off Beta participation limits subsequent checks to release versions. It does not downgrade an installed Beta or cancel an installation that the user has already confirmed. A newer release can replace an installed Beta.
