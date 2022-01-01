# mepa

A minimal project that offers a transport primitive backed by TCP.

The main purpose of this project is to abstract all complexity and offer a high level
communication primitive to be used by other projects. Here we are looking forward to offer a
simple TCP communication between a pair of peers, nothing too fancy. First things first,
everything here is heavily based on the [`mini-redis`] implementation by the Tokio team, so,
even when we simplify the components keep in mind that we are pretty much following an already
existent project.

We designed a simple architecture to handle received messages and messages to be sent. The
diagram bellow is a high level view of how the components will interact.

┌────────┐                                         ┌──────────────┐         ┌────────────┐
│        ◄─────────────────poll────────────────────┤              │         │            │
│        │                                         │   Receiver   ◄───Read──┤   Server   ◄────Read────┐
│        │                                         │              │         │            │            │
│        │            ┌─────────────────┐          └──────┬───────┘         └────────────┘    ┌───────┴────────┐
│        │            │                 │                 │                                   │                │
│  User  ├───create───►    Transport    ├─────Create──────┤                                   │   Connection   │
│        │            │                 │                 │                                   │                │
│        │            └─────────────────┘           ┌─────┴──────┐          ┌────────────┐    └───────▲────────┘
│        │                                          │            │          │            │            │
│        ├─────────────────write────────────────────►   Sender   ├──Write───►   Client   ├───Write────┘
└────────┘                                          │            │          │            │
                                                    └────────────┘          └────────────┘

The `User` component represents the final user itself. The main block here is the `Transport`,
through this component the use can create an instance of the [`Receiver`] and [`Sender`], these
structures can be create together or separately.

## Receiving data

The [`Receiver`] will have a two methods exposed to the user, one to retrieve the local address
using the [`SocketAddr`] format and another method to start polling. This polling method is
responsible to accepting incoming connections, it receives a asynchronous function as argument,
this function will be called any time a new message is received. The polling method should be
called just after the creation of the [`Receiver`], this method will block until an error occur
or a CTRL-C signal is received.

Internally, the [`Receiver`] will create a TCP server, identified by the `Server` block. The
server is the responsible for accepting connections, spawning a dedicated handler and
transmitting all received messages from all connections up to the `User` layer.

Once the server starts, it will bind to a port locally and create a [`TCP stream`], then this
stream will be wrapped into a dedicated connection class that will manage the stream decorated
with a [`BufReader`]. At this moment, there is no conception of a frame, so messages can be
split into multiple chunks, depending on the data size and the buffer status.

When the server is shutdown, the [`Receiver`] will try to execute a graceful shutdown, waiting
for all established handlers to finish any processing. We did using the mpsc channels for
notifications.

## Sending data

The other pair is the [`Sender`], which is used only -- as the name suggests -- to send data.
Internally, the [`Sender`] will have a manager handling a connection pool. This pool is used
to avoid always establishing a new connection when sending data, each destination can have at
most 10 connections in memory.

Each connection inside this pool is a wrapper class around the established [`TCP stream`], with
a wrapper dedicated to the writing operations. The stream itself is decorated with a
[`BufWriter`], and after each message the newly written data will be flushed. Since we did not
define a frame format, this method is really direct since no parse is needed.

## Communication

Using both pairs of [`Receiver`] and [`Sender`] is possible to abstract all the communication
and boilerplate needed in order to create both of these structures. Using this approach we also
have the possibility to using only a [`Receiver`] or only the [`Sender`] part, since they can
be created in isolation from each other.

## Problems

The code that is closest to the [`TCP stream`] end could be better. Since we are basically
using the [`mini-redis`] project as a guide, the usage for the buffered decoration maybe
is not the best approach, but at this time I do not know what we could use there.

The lack of frame definition is also something that could be improved. At this time, we only
read what the buffer can handle, and that probably could lead to losing some data. Would be
nice to create a frame definition, so we could parse data correctly and stop publishing data
in chunks to the user layer.

[`Receiver`]: crate::transport::Receiver
[`Sender`]: crate::transport::Sender
[`SocketAddr`]: std::net::SocketAddr
[`TCP stream`]: tokio::net::TcpStream
[`BufReader`]: tokio::io::BufWriter
[`mini-redis`]: https://github.com/tokio-rs/mini-redis
