package dev.slimevr.poseframeformat
import dev.slimevr.poseframeformat.trackerdata.TrackerFrames
class PoseFrames(val frameHolders:List<TrackerFrames>) {
    val maxFrameCount get()=frameHolders.maxOfOrNull{it.frames.size} ?: 0
    val maxHmdHeight get()=frameHolders.flatMap{it.frames}.filter{it.tryGetTrackerPosition()==dev.slimevr.tracking.trackers.TrackerPosition.HEAD}.mapNotNull{it.tryGetPosition()?.y}.maxOrNull() ?: 0f
}
