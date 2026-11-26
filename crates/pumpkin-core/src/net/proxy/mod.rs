pub mod bungeecord;
pub mod haproxy;
pub mod velocity;
pub mod vine;

// No shared proxy trait: HAProxy reads a header before the handshake, BungeeCord parses the
// handshake address, Velocity and Vine answer a login plugin request; the steps don't line up.
