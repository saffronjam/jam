# 3. Only the daemon answers terminal queries

Status: Accepted, 2026-11-19

Only the authoritative VT instance has a PTY-write callback. Client mirrors consume snapshots and byte events without sending terminal-query responses. User input travels to the daemon separately.

Applications receive one response even with no GUI or multiple observers. Client-dependent terminal geometry must be sent to the daemon before encoding input or query responses. Rendering support must agree with advertised terminal capabilities.
