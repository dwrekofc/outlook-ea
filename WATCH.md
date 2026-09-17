# Native sender watch

`mea watch --config avengers.toml --since 2026-09-10 --json` searches each
configured sender in the read-only Mail index. Dates start at local midnight;
expiry uses the system date, strictly after `until`. `WATCH_TODAY` is ignored
and explicitly logged as TEST when present. Unit tests inject a clock and
log TEST on their expiry events.

The config filename determines the launchd label (`avengers.toml` becomes
`com.mea.watch-avengers`). Paths support `~/` or resolve relative to the config.
Keep each watch's state/output/log paths distinct. Output is append-only;
seen rowids remain compatible with the legacy watch. An output scan recovers
from interruption before the seen file rename. A file lock prevents overlapping
runs. Wake commands run once per batch with `$HITS` replaced by the count;
failed wakes are counted, with the inbox file as delivery fallback.

Watch runs return zero even when searches fail; inspect `errors` in stdout or
the configured log. Config parse failures go to stderr because no valid log
path is available. Timer administration errors return nonzero.

## After lead review and explicit install order

1. Install the reviewed mea 0.3.1 binary at the FDA-granted binary path.
2. Copy the reviewed `avengers.toml` into
   `~/vault/utilities/ai-productivity/mail/watch/avengers.toml`.
3. Unload `com.mea.avengers-watch` if loaded and remove its LaunchAgents plist.
4. Rename `avengers-watch.sh` to `avengers-watch.sh.legacy` for reference.
5. Run the installed binary's `watch install-timer --config <absolute-config>`.
   Default schedule is 08:00; override with `--at HH:MM`.
6. Run `mea doctor`, then
   `launchctl kickstart -k gui/$(id -u)/com.mea.watch-avengers` and confirm
   `errors=0` in the watch log. Doctor's terminal result alone does not prove
   the launchd process has access.

`mea watch uninstall-timer --config <path>` unloads and removes the native job.
No live job or installed binary is changed by the review branch itself.
