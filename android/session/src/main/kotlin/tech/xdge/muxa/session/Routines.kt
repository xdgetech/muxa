package tech.xdge.muxa.session

import tech.xdge.muxa.protocol.RoutineInfo

/** `(workspace, name)` identifies a Routine. */
val RoutineInfo.key: String get() = "$workspace/$name"
