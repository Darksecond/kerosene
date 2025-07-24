# Ports

Ports are an essential component in the actor framework.
The allow an actor to communicate with IO of various kinds.
A port is owned by a single actor. This means that if the actor dies the port is closed as well.

A port will never be migrated to another worker. This means that the port can be `!Send` and `!Sync`.
Ports are asynchronous in nature in that they use messages like actors. A port however is strong typed.
The worker has a separate run queue for ports and each port has an inbox.

When a port closes a `PortExit` message is delivered to it's owning actor.

There are several pieces still under development:
- Add port signals, a port currently has an inbox, but only for messages. We should add port signals, like `Close`, `Message`, etc.
- Expand ports to include real async IO. We can use something like `io_uring` or `mio`.

Right now the following ports are defined:
- `FilePort`: deals with reading and writing to files on the filesystem.
