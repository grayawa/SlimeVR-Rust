package dev.slimevr
import dev.slimevr.tracking.trackers.Tracker
import com.jme3.system.NanoTimer
// Explicit clock only for filter update(); no production server or StayAligned is run.
class ReferenceServer {val fpsTimer=NanoTimer();val allTrackers=emptyList<Tracker>()}
object VRServer {val instanceInitialized=true;val instance=ReferenceServer()}
