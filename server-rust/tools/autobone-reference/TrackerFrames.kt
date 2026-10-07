package dev.slimevr.poseframeformat.trackerdata
import dev.slimevr.tracking.trackers.*
import io.github.axisangles.ktmath.*
class TrackerFrame(val body:TrackerPosition, val rotation:Quaternion?, val position:Vector3?) {
    fun tryGetTrackerPosition()=body
    fun tryGetPosition()=position
    fun tryGetRotation()=rotation
    fun tryGetAcceleration():Vector3?=null
}
class TrackerFrames(val frames:MutableList<TrackerFrame>) {
    fun tryGetFrame(cursor:Int)=frames.getOrNull(cursor)
    fun toTracker()=Tracker(frames.first().body).apply{hasPosition=frames.any{it.position!=null};isHmd=trackerPosition==TrackerPosition.HEAD}
}
