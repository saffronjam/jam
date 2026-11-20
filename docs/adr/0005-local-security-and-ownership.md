# 5. Private local transport and one GUI controller

Status: Accepted, 2026-11-19

Use a private Unix-domain socket and same-user peer credential checks. Hold a socket lease for one GUI controller; concurrent CLI clients remain allowed. No network listener or sharing tokens are exposed in the MVP.

Agent environment variables select panes but confer no additional authority. Agents executing as the owner can control the owner's daemon. Remote sharing or finer pane capabilities require explicit future design; socket permissions alone do not provide those policies.
