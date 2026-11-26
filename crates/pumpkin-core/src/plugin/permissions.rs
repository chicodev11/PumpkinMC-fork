/// Allows the plugin to perform DNS resolution (resolving hostnames to IP addresses).
///
/// Corresponds to the `wasi:sockets/ip-name-lookup` interface.
pub const NETWORK_DNS: &str = "network.dns";

/// Allows the plugin to use TCP sockets.
pub const NETWORK_TCP: &str = "network.tcp";

/// Allows the plugin to use UDP sockets.
pub const NETWORK_UDP: &str = "network.udp";

/// Allows the plugin to initiate TCP connections.
pub const NETWORK_TCP_CONNECT: &str = "network.tcp.connect";

/// Allows the plugin to bind TCP listeners (accept inbound connections).
pub const NETWORK_TCP_BIND: &str = "network.tcp.bind";

/// Allows the plugin to send and receive UDP packets to specific destinations.
pub const NETWORK_UDP_CONNECT: &str = "network.udp.connect";

/// Allows the plugin to bind UDP sockets to local ports.
pub const NETWORK_UDP_BIND: &str = "network.udp.bind";

/// Allows the plugin to send datagram on non-connected UDP socket.
pub const NETWORK_UDP_OUTGOING_DATAGRAM: &str = "network.udp.outgoingdatagram";

/// Restricts all networking permissions to loopback addresses (localhost) only.
pub const NETWORK_LOOPBACK: &str = "network.loopback";

/// Allows the plugin to make outbound TCP/UDP connections.
///
/// This gives the plugin full access to the host's network interfaces.
/// **Warning:** This is a powerful permission and should only be granted to trusted plugins.
pub const NETWORK_OUTBOUND: &str = "network.outbound";

/// Allows the plugin to make outbound HTTP connections.
///
/// This is separate from `network.outbound`. This allows the use of `wasi:http`; the other allows the more powerful `wasi:sockets`.
pub const HTTP_OUTBOUND: &str = "http.outbound";

/// Allows the plugin to read files within its own data folder (`plugins/data/<name>`).
pub const FS_READ_DATA: &str = "fs.read.data";

/// Allows the plugin to write (and read) files within its own data folder (`plugins/data/<name>`).
///
/// Note that this permission also implies `FS_READ_DATA`
pub const FS_WRITE_DATA: &str = "fs.write.data";

/// Allows the plugin to read files anywhere in the server folder, mounted as `server`.
pub const FS_READ_SERVER: &str = "fs.read.server";
/// Allows the plugin to write (and read) files anywhere in the server folder, mounted as
/// `server`. Implies `fs.read.server`.
///
/// **Warning:** this reaches worlds, configs and other plugins; grant it to trusted plugins only.
pub const FS_WRITE_SERVER: &str = "fs.write.server";
/// Allows the plugin to read the world folder (the default world and the worlds plugins made
/// in it), mounted as `worlds`.
pub const FS_READ_WORLDS: &str = "fs.read.worlds";
/// Allows the plugin to write (and read) the world folder, mounted as `worlds`, and to copy
/// and delete unloaded plugin worlds (`server.copy-world`, `delete-world`). Implies
/// `fs.read.worlds`.
pub const FS_WRITE_WORLDS: &str = "fs.write.worlds";
/// Allows the plugin to read all environment variables.
pub const SYS_ENV: &str = "sys.env";

/// Allows the plugin to read specific environment variables.
/// Used with a prefix like "sys.env.PATH".
pub const SYS_ENV_PREFIX: &str = "sys.env.";

/// Allows the plugin to read system information (CPU, Memory, OS).
pub const SYS_INFO: &str = "sys.info";

/// Allows the plugin to read CPU information.
pub const SYS_INFO_CPU: &str = "sys.info.cpu";

/// Allows the plugin to read RAM information.
pub const SYS_INFO_RAM: &str = "sys.info.ram";

/// Allows the plugin to read OS information.
pub const SYS_INFO_OS: &str = "sys.info.os";

/// Every capability permission. A plugin marked `trusted` in the config gets all of them,
/// whether or not it asked. `network.loopback` is a restriction, not a capability, so it is not
/// listed.
pub const TRUSTED_PERMISSIONS: &[&str] = &[
    NETWORK_DNS,
    NETWORK_TCP,
    NETWORK_UDP,
    NETWORK_TCP_CONNECT,
    NETWORK_TCP_BIND,
    NETWORK_UDP_CONNECT,
    NETWORK_UDP_BIND,
    NETWORK_UDP_OUTGOING_DATAGRAM,
    NETWORK_OUTBOUND,
    HTTP_OUTBOUND,
    FS_READ_DATA,
    FS_WRITE_DATA,
    FS_READ_SERVER,
    FS_WRITE_SERVER,
    FS_READ_WORLDS,
    FS_WRITE_WORLDS,
    SYS_ENV,
    SYS_INFO,
];

/// The permissions a plugin runs with: what it asked for, or everything when the config trusts
/// it, minus what the config blocks. An explicit block always wins over trust.
#[must_use]
pub fn effective_permissions(
    requested: &[String],
    trusted: bool,
    is_blocked: impl Fn(&str) -> bool,
) -> Vec<String> {
    let mut permissions: Vec<String> = requested.to_vec();
    if trusted {
        for permission in TRUSTED_PERMISSIONS {
            if !permissions.iter().any(|p| p == permission) {
                permissions.push((*permission).to_string());
            }
        }
        // Trust lifts the loopback restriction the plugin may have asked for.
        permissions.retain(|p| p != NETWORK_LOOPBACK);
    }
    permissions.retain(|p| !is_blocked(p));
    permissions
}

#[must_use]
pub fn get_permission_description(permission: &str) -> Option<&'static str> {
    match permission {
        NETWORK_DNS => Some(
            "Allows the plugin to perform DNS resolution (resolving hostnames to IP addresses).",
        ),
        NETWORK_TCP => Some("Allows the plugin to use TCP sockets."),
        NETWORK_UDP => Some("Allows the plugin to use UDP sockets."),
        NETWORK_TCP_CONNECT => Some("Allows the plugin to initiate TCP connections."),
        NETWORK_TCP_BIND => {
            Some("Allows the plugin to bind TCP listeners (accept inbound connections).")
        }
        NETWORK_UDP_CONNECT => {
            Some("Allows the plugin to send and receive UDP packets to specific destinations.")
        }
        NETWORK_UDP_BIND => Some("Allows the plugin to bind UDP sockets to local ports."),
        NETWORK_UDP_OUTGOING_DATAGRAM => {
            Some("Allows the plugin to send datagram on non-connected UDP socket.")
        }
        NETWORK_LOOPBACK => {
            Some("Restricts all networking permissions to loopback addresses (localhost) only.")
        }
        NETWORK_OUTBOUND => {
            Some("Allows the plugin to make outbound TCP/UDP connections. (POWERFUL)")
        }
        HTTP_OUTBOUND => {
            Some("Allows the plugin to make outbound HTTP requests (through `wasi:http`)")
        }
        FS_READ_DATA => Some("Allows the plugin to read files within its own data folder."),
        FS_WRITE_DATA => Some(
            "Allows the plugin to write (and read) files within its own data folder. Implies `fs.read.data`.",
        ),
        FS_READ_SERVER => Some("Allows the plugin to read files anywhere in the server folder."),
        FS_WRITE_SERVER => Some(
            "Allows the plugin to write (and read) files anywhere in the server folder, including worlds and configs. (POWERFUL)",
        ),
        FS_READ_WORLDS => Some("Allows the plugin to read the world folder."),
        FS_WRITE_WORLDS => Some(
            "Allows the plugin to write the world folder and to copy and delete unloaded plugin worlds. Implies `fs.read.worlds`.",
        ),
        SYS_ENV => Some("Allows the plugin to read all environment variables."),
        SYS_INFO => Some("Allows the plugin to read system information (CPU, Memory, OS)."),
        SYS_INFO_CPU => Some("Allows the plugin to read CPU information."),
        SYS_INFO_RAM => Some("Allows the plugin to read RAM information."),
        SYS_INFO_OS => Some("Allows the plugin to read OS information."),
        p if p.starts_with(SYS_ENV_PREFIX) => {
            Some("Allows the plugin to read specific environment variables.")
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(ToString::to_string).collect()
    }

    #[test]
    fn untrusted_plugins_get_what_they_asked_minus_blocks() {
        let requested = strings(&[FS_WRITE_DATA, NETWORK_OUTBOUND]);
        let granted = effective_permissions(&requested, false, |p| p == NETWORK_OUTBOUND);
        assert_eq!(granted, strings(&[FS_WRITE_DATA]));
    }

    #[test]
    fn trusted_plugins_get_everything_except_explicit_blocks() {
        let requested = strings(&[FS_READ_DATA, NETWORK_LOOPBACK]);
        let granted = effective_permissions(&requested, true, |p| p == SYS_ENV);
        for permission in TRUSTED_PERMISSIONS {
            assert_eq!(
                granted.iter().any(|p| p == permission),
                *permission != SYS_ENV,
                "{permission}"
            );
        }
        assert!(!granted.iter().any(|p| p == NETWORK_LOOPBACK));
        assert_eq!(granted.iter().filter(|p| *p == FS_READ_DATA).count(), 1);
    }
}
