---
title: "`world.observe+watch` state is owned by each request"
pull_requests: []
---

Each `world.observe+watch` request now registers its own observer and event buffer, and both are removed when the request's stream closes. Previously, one observer and buffer were shared by every request for the same event and entity and were never removed. Events kept piling up after a client disconnected, and concurrent watchers stole each other's events. Every watcher now receives every event triggered while its stream is open.

To tell requests apart, the server now sets the new `CurrentWatchingRequest` resource before running a watching handler. `process_remote_observe_watching_request` returns an error when it is called outside the server's watching loop, so code that calls it directly (for example in tests) must insert a `CurrentWatchingRequest` naming the request first.
