// Only this lookup and UDPDevice's transport dependencies are isolated from VRServer.
// IDs 1..50 are copied from the pinned TrackerPosition enum; no body solving occurs.
package dev.slimevr.tracking.trackers
data class TrackerPosition(val id: Int) {
    companion object { fun getById(id: Int): TrackerPosition? = if (id in 1..50) TrackerPosition(id) else null }
}
