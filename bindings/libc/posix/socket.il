/// POSIX `<sys/socket.h>` + `<netinet/in.h>` + `<arpa/inet.h>`.
///
/// Minimal but practical socket surface — enough to write a TCP or
/// UDP client / server. Wider features (sendmsg ancillary data,
/// SCM_RIGHTS fd passing, AF_PACKET) live in OS-specific headers
/// that aren't bound here.
///
/// Address families:
///   AF_UNIX    1   AF_INET   2   AF_INET6 10
///
/// Socket types:
///   SOCK_STREAM 1  SOCK_DGRAM 2  SOCK_RAW 3  SOCK_SEQPACKET 5
///
/// Protocols (IPPROTO_*):
///   IPPROTO_TCP 6   IPPROTO_UDP 17  IPPROTO_ICMP 1
///
/// Shutdown how:
///   SHUT_RD 0   SHUT_WR 1   SHUT_RDWR 2

@extern(C, "c") {
    /// Generic socket address — fits the largest address family.
    /// 128 bytes covers IPv6 + Unix-domain sockets.
    pub struct sockaddr {
        sa_family: u16
        sa_data:   u8[126]
    }

    /// IPv4 address (32-bit network-byte-order int wrapped for
    /// type identity).
    pub struct in_addr {
        s_addr: u32
    }

    /// `struct sockaddr_in` — IPv4 endpoint.
    pub struct sockaddr_in {
        sin_family: u16
        sin_port:   u16
        sin_addr:   in_addr
        sin_zero:   u8[8]
    }

    pub struct in6_addr {
        s6_addr: u8[16]
    }

    pub struct sockaddr_in6 {
        sin6_family:   u16
        sin6_port:     u16
        sin6_flowinfo: u32
        sin6_addr:     in6_addr
        sin6_scope_id: u32
    }

    /// `getaddrinfo` hints / result list.
    pub struct addrinfo {
        ai_flags:     i32
        ai_family:    i32
        ai_socktype:  i32
        ai_protocol:  i32
        ai_addrlen:   u32
        ai_addr:      *sockaddr
        ai_canonname: *char
        ai_next:      *addrinfo
    }

    // ── Lifecycle ────────────────────────────────────────────

    @lib @optional pub fn socket(domain: i32, type_: i32, protocol: i32): i32
    @lib @optional pub fn socketpair(domain: i32, type_: i32, protocol: i32, sv: *i32): i32
    @lib @optional pub fn bind(sockfd: i32, addr: *const sockaddr, addrlen: u32): i32
    @lib @optional pub fn listen(sockfd: i32, backlog: i32): i32
    @lib @optional pub fn accept(sockfd: i32, addr: *sockaddr, addrlen: *u32): i32
    @lib @optional pub fn connect(sockfd: i32, addr: *const sockaddr, addrlen: u32): i32
    @lib @optional pub fn shutdown(sockfd: i32, how: i32): i32

    // ── Data transfer ───────────────────────────────────────

    @lib @optional pub fn send(sockfd: i32, buf: *const void, len: size_t, flags: i32): i64
    @lib @optional pub fn recv(sockfd: i32, buf: *void, len: size_t, flags: i32): i64

    @lib @optional pub fn sendto(
        sockfd:     i32,
        buf:        *const void,
        len:        size_t,
        flags:      i32,
        dest_addr:  *const sockaddr,
        addrlen:    u32
    ): i64

    @lib @optional pub fn recvfrom(
        sockfd:    i32,
        buf:       *void,
        len:       size_t,
        flags:     i32,
        src_addr:  *sockaddr,
        addrlen:   *u32
    ): i64

    // ── Options ──────────────────────────────────────────────

    @lib @optional pub fn setsockopt(
        sockfd:  i32,
        level:   i32,
        optname: i32,
        optval:  *const void,
        optlen:  u32
    ): i32

    @lib @optional pub fn getsockopt(
        sockfd:  i32,
        level:   i32,
        optname: i32,
        optval:  *void,
        optlen:  *u32
    ): i32

    @lib @optional pub fn getsockname(sockfd: i32, addr: *sockaddr, addrlen: *u32): i32
    @lib @optional pub fn getpeername(sockfd: i32, addr: *sockaddr, addrlen: *u32): i32

    // ── Address conversion ──────────────────────────────────

    /// Convert dotted-quad / colon-hex to network byte order. `af`:
    /// AF_INET 2 / AF_INET6 10. Returns 1 on success, 0 on parse
    /// error, -1 on bad family.
    @lib @optional pub fn inet_pton(af: i32, src: *const char, dst: *void): i32

    /// Inverse — write a printable form to `dst`. `size` is the
    /// capacity of `dst`. Returns `dst` or NULL on error.
    @lib @optional pub fn inet_ntop(
        af:   i32,
        src:  *const void,
        dst:  *char,
        size: u32
    ): *const char

    /// `htons`/`ntohs`/`htonl`/`ntohl` — byte-order helpers. Many
    /// libcs implement these as inline macros; the symbol form is
    /// reliable on glibc + macOS.
    @lib @optional pub fn htons(hostshort: u16): u16
    @lib @optional pub fn ntohs(netshort: u16): u16
    @lib @optional pub fn htonl(hostlong: u32): u32
    @lib @optional pub fn ntohl(netlong: u32): u32

    // ── Name resolution ─────────────────────────────────────

    @lib @optional pub fn getaddrinfo(
        node:    *const char,
        service: *const char,
        hints:   *const addrinfo,
        res:     **addrinfo
    ): i32

    @lib @optional pub fn freeaddrinfo(res: *addrinfo)
    @lib @optional pub fn gai_strerror(errcode: i32): *const char
}
