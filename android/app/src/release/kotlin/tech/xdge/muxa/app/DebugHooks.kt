package tech.xdge.muxa.app

import tech.xdge.muxa.transport.BridgeChannel

/** A release build has no demo bridge: every host is reached over SSH. */
object DebugHooks {
    fun demoBridge(@Suppress("UNUSED_PARAMETER") alerts: Boolean): BridgeChannel? = null
}
