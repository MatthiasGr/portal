# Portal

> [!WARNING]
> This project is currently under construction and will not work as advertised here at this point.

A reverse proxy and management layer for minecraft servers.

## Features

Portal aims to provide a simple front end for one or more minecraft servers.
It routes connections to the correct server based on the provided address and port field in the
handshake packet.
If the targeted server is currently offline, Portal can start the server dynamically or just respond
with placeholder status messages.
Other common reverse-proxy features such as load balancing or authentication are not planned at this
time.

## Getting Started

Portal is written in Rust and can be built using the standard cargo commands.
Building and running Portal requires Rust version 1.88.0 or newer.
Other dependencies are not required at this time.
Portal also provides a nix development shell as part of it's flake that can be invoked using
`nix develop`.