package tech.xdge.muxa.transport

sealed class TransportError(message: String) : Exception(message) {
    class Closed : TransportError("the connection is closed")

    /** The host presented a key that does not match the pinned one, or no key is pinned yet. */
    class HostKeyMismatch(val presented: HostKey) : TransportError("the host key is not the trusted one")

    /** The host refused every login method offered: an unknown key or a wrong password. */
    class NotAuthorized : TransportError("the host refused the login")
    class Failed(message: String) : TransportError(message)
}
