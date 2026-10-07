package dev.slimevr.tracking.trackers.udp
// Oracle always passes null to parse(). This stub only satisfies compile-time references.
class UDPDevice {
    var lastPacketNumber = 0L
    var lastPacket = 0L
    val trackers = emptyMap<Int, TrackerStub>()
    fun isNextPacket(number: Long): Boolean = error("Oracle must use a null connection")
}
class TrackerStub { fun heartbeat(): Unit = error("Oracle must use a null connection") }
