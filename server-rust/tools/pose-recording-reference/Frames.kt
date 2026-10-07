package dev.slimevr.poseframeformat
import io.eiren.util.collections.FastList
import dev.slimevr.poseframeformat.trackerdata.TrackerFrames
class PoseFrames(val frameHolders:FastList<TrackerFrames> = FastList()) { var frameInterval=.02f;val maxFrameCount get()=frameHolders.maxOfOrNull{it.frames.size} ?: 0 }
