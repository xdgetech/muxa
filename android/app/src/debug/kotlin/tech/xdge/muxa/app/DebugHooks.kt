package tech.xdge.muxa.app

import tech.xdge.muxa.demo.DemoBridge
import tech.xdge.muxa.transport.BridgeChannel

/** Debug builds only: the in-app demo bridge, for UI tests and trying the app without a host. */
object DebugHooks {
    fun demoBridge(alerts: Boolean): BridgeChannel? = DemoBridge(alerts)
}
