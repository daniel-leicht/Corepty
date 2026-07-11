// How a session was launched — kept per-tab so it can be reconnected.
import type { LocalShell } from "./ipc";
import type { ConnForm } from "./dialog";

export type LaunchSpec =
  | { kind: "local"; shell: LocalShell; elevated?: boolean }
  | { kind: "ssh"; form: ConnForm }
  | { kind: "telnet"; form: ConnForm };
