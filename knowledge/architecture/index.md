# Architecture

<!-- okf:listing:start (managed by okf.py index — edit prose outside this fence) -->
# Concepts

* [Named terminal peers](agent-peer-bus.md) - How Claude Code on the same Unix host discovers and sends messages to a named terminal pane.
* [Daemon lock order](daemon-locks.md) - Which daemon mutexes are taken before the server state lock and which after it, with a code site for each.
* [End-to-end data flow](data-flow.md) - How terminal frames, browser pixels, chat events and input reach the pane that owns them.
* [zz system overview](overview.md) - How desktop, thin and terminal clients share a daemon, and which process owns each kind of pane.
* [Process and threading model](process-model.md) - Which process owns mux state, terminal actors, native chats and browser runtimes.
<!-- okf:listing:end -->
