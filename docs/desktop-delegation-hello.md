# Desktop instance identity challenge

The existing Next single-instance listener remains the owner of the canonical
`hgs-tray` socket. The service never binds, removes or brokers that socket.
`show` and `sessions`, the startup lock, updater discovery and rollback keep
their existing behavior. This additive protocol only identifies a compatible
live frontend. It grants no writer permit, delegation, drain proof or takeover.

Callers first require the exact trimmed help line
`Desktop owner ABI: desktop-delegation-v1` from their selected local CLI before
invoking `hgs desktop owner-status --json`. Older CLIs may interpret an unknown
command as a session launch. The new command makes one read-only socket probe;
it does not start the service, write state, invoke children or retry.

The query is the 59-byte ASCII line `desktop-delegation-v1 NONCE\n`, where
NONCE is a fresh canonical UUID minted for each probe. It fits the listener's
existing 64-byte line budget. Next replies with one JSON line, at most 4096
bytes including LF. Its closed schema carries `abi`, `version:1`, the exact
`nonce`, `application`, `gui`, `build` (version, source_commit, source_dirty),
`instance_epoch`, `writer_epoch`, `configuration`, `live` and `replay`.
`writer_epoch` is currently null; the hello cannot assert effect authority.
The app chooses an instance UUID at startup. This is not a writer epoch.

The accepted identity is the production `com.hgdev.hgs-tray` Tauri app, a known
clean source build, live and not replaying, with an exact configuration hash.
Configuration comes once from a background probe of the selected local hgs:
ordinary `--help` must advertise the exact `Desktop service ABI: desktop-v1`
line before `desktop status --json` is called. Each child has a five-second
owned-child deadline and bounded output; failed/absent capability stays unknown
for the instance. No per-socket-probe child, GUI-thread I/O or automatic retry
is introduced. A second launch that forwards activation never starts this
configuration probe. Replay and nonproduction builds do not probe a CLI.
The listener only reads cached identity. Changing configuration requires a new
instance; later handoff must independently revalidate the selected configuration.

Unix peers authenticate with SO_PEERCRED or getpeereid. The probe uses
nonblocking connect and one total one-second socket deadline, not a timeout
renewed by each byte. A full backlog or a connect that cannot complete within the deadline stays unknown. The
existing Qt path selection is shared with live Next, preserving lexical cleanup
and macOS's per-user temporary directory without expanding `/tmp` symlinks.
Only a missing endpoint is `absent`. A stale, unsafe or unreachable existing
entry is `unknown`; a connected peer ignoring/rejecting the query or returning
an unsolicited, stale, mismatched, newer, dev, replay, malformed, late or
oversized reply is `live_legacy_or_unknown`. These are passive observations,
not service error states. They never establish that no frontend exists.
A valid reply is `live_next`, still with authority, takeover and writers false.
Future effect batches must apply their own bounded scheduling and fresh checks;
this command has no polling loop or retained cache.

Windows currently reports unsupported/unknown. Its permanent instance lock and
native ownership protocol need their separate runtime qualification. No Unix
probe result substitutes for that proof.

## Native frontend policy requests

The selected CLI advertises `Desktop frontend request ABI: desktop-ui-request-v1`.
After its confirmation UI, Next prepares a closed `AutomaticApprovals` request
with a stable request UUID and a fresh attempt UUID. Its native responder reserves
one of eight short-lived slots. The maintained runner binds the original `Child`
before sending the envelope to `hgs desktop frontend-request --json`, and retains
the registration with the child and all pipes. An expired registration supplies
no evidence and cannot release a still-retained resource slot.

The instance listener answers the fixed `desktop-ui-request-v1 ATTEMPT` query
from that cache. The service matches the socket peer's PID, birth, selected CLI
image and direct frontend parent to the actual canonical instance peer and its
installed frontend image. Configuration, request fingerprint and original child
binding must all match. Bounded ancestry inspection refuses agent markers,
agent executables and unavailable ancestry. Native serials prevent a delayed
older setting from overtaking a newer one. A wire role, confirmation flag or
nonce alone supplies no authority. Generic desktop requests cannot change this
policy. Windows refuses this route until its native reply and ancestry adapters
are implemented.

Only the service actor commits policy and read intents, and responses claim
`durable:true` only after that commit. The delivery worker observes successfully
applied service-owned fleet events; passive UI projections cannot authorize an
Answer. Approval policy remains default-off and existing writer/handoff checks
still precede delivery. `delivery_result` is an indexed read-only query available
even while the service is passive after revocation. Retry an unknown interaction
with the same logical request UUID; do not fabricate a new delivery receipt.

Policy mutation also requires an opaque actor-side permit at the delivery API;
passing policy JSON to its generic request method refuses. Ancestry checks reject
advertised loader-injection variables, and Linux requires a readable, zero
`TracerPid`. Other-platform tracing checks remain platform work. Executable
names supplement the session/run markers: a renamed or reparented unmarked
launcher cannot be identified from these checks. This is an application IPC
check, not an OS isolation boundary against arbitrary same-user code. Restart
ordering and a complete native positive-path regression remain cutover work.
