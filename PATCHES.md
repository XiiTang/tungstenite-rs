# Boundless patches

Base: upstream v0.29.0 (`c2921718b76f0ed8c69a895397ed981e4a321f6f`).

## Preserve the received Close; normalize only its automatic reply

Boundless #643: a raw Close carrying forbidden code 1005 was reported as a normal
peer end, with a retained 1002 and reason the peer never sent. The host upgrade,
public receive and selected Terminal path reproduced this on Boundless
`eb87987b84108990b6eacc3c45f7e141009c8ab7`. The MQTT WebSocket byte-stream adapter
also needs the original code when retaining its transport failure.

Upstream `WebSocketContext::do_close` changes Active to ClosedByPeer, replaces
a disallowed received code with 1002 / Protocol violation, queues that frame as
the automatic reply, and returns the replacement as the observed Message::Close.
Its ClosedByUs acknowledgement arm already returns the original input.
Upstream master `2f20dc53e2a8d09bba02970d7ba15c5f33fbc17a` still does this.
No public configuration or callback exposes the original code before replacement;
`CloseCode::is_allowed` is sufficient for the consumer once the input survives.

The patch retains `close` and maps its clone only for `Frame::close`. Only the
returned Active-arm observation changes. The state transition, queued automatic
reply, flushing, duplicate-close handling, acknowledgement, codec errors,
cancellation and other error paths remain upstream's implementation. There is
no new API or error variant. Boundless validates before admitting a public Close
item or proposing a normal end, and its existing bounded reader owns reply flush.

Regression: `tests/close_observation.rs` drives actual raw framing in client and
server roles. It asserts original invalid/valid/empty Close observation, the
automatic 1002 response for invalid codes, unchanged own-Close acknowledgement
without an additional response, and malformed-length/UTF-8 failures.

Validation on macOS arm64 (Darwin 27.2.0), local cargo in this clone:
`cargo test --test close_observation` passes 3 of 3 and `cargo test --lib protocol::`
passes 24. Restoring upstream's `do_close` makes `close_observation` fail (exit 101).
An earlier Windows run through the Boundless test pool,
`rt-shared-B-tungstenite-module-10656-1794050398`, passed the protocol-module
library tests and the close_observation binary. No full suite was run.

Consumer: Boundless `execution::interaction` (SocketResource Close classifier)
and `network::websocket` (MQTT WebSocket byte-stream adapter).
