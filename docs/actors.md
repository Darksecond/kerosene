# Actors

Actors are the central concept the framework is built around.
They can have internal state and although they can be sent to another thread, they are effectively single threaded.
All communication with other actors and ports happens through messages.

Actors have access to various functions available at all times when the actor is running.
These include `send`, `sleep`, `receive`, etc. This is how an actor talks to the outside world.

## Receive

Actors have an internal mailbox wehere messages are append to.
The `receive` macro is used to retrieve messages from the mailbox.
It will return the first matching messages or wait until one arrives.
This does not have to be the first message in the mailbox.

## Linking and trapping exits

An actor can be linked to other actors. This means that if the linked actor dies, so does the linkee.
A link is two-way. An exit reason of 'normal' will not result in the linked actor dying.

An actor can choose to trap exits. This will turn any exit signals from linked actors and ports into messages.
These can then be acted upon at will using the normal receive machinery.

## Notes

An actors mailbox is unbounded, but the first N signals are stored in a fast queue that is lock free.
An actor has a mailbox that contains signals, these are then turned into messages on the message queue as appropriate.
The message queue will use an intrusive linked list in a future version. Messages are already boxed and will be wrapped in an envelope.
