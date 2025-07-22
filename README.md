# Kerosene Actor Framework

Kerosene is a actor framework in rust modeled after Erlang.

It's primary use is to develop robust and fast applications.
Although intended overhead is low, it is not intended to be 0. For example, actors are untyped and thus messages are boxed.

The framework currently has the following features:
- Untyped actors: an actor can receive any type of message.
- Async support: actors are async by nature and should not block.
- Co-operative multi-tasking: async support allows multiplexing many actors on 1 thread.
- in-line receive: modeled after erlang, a `receive!` macro is avialable and can be awaited anywhere in an actor.
  The macro has advanced pattern matching capabilities.

Performance has been an important requirement, spawning an actor does not do many allocations. Sending a message takes approximately 400ns
and receiving a simple message takes about 600ns. These numbers are very rough and will change in the future.

An actor has access to a suite of functions when it is running. They are thread local and can be used forom anywhere in an actor.
These include for example: `spawn`, `sleep`, `send` and `schedule`.

This library is a heavy work in progress and API's will change as work progresses. Amongst the work currently in progress is:
- Rework ports, right now they are tied to an actor. The idea is to change ports to work more like erlang and
  move ownership to the worker. A port will never be migrated to anotehr worker, this means a port can be `!Send` and `!Sync`.
- Add more examples and documentation.
- Add more featuers to the 'standard library'.Right now there is a logger, but it is barely used. A supervisor is also available.
- Remove locks, especially actor has multiple locks that are not needed. They can be removed.

In the longer term it would be nice to add more advanced features like real async IO support using something like `mio` or `io_uring`.

Please reach out if you enjoy playing with my little framework.
I welcome questions and feedback.
